-- M4: Shield & Protection Engine (S-05, S-07, S-13, S-19, S-23, S-24, S-25, DEC-004, DEC-005)
-- Never modifies prior migrations. Supports standard PG16 (with btree_gist) and local pgserver fallback.

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'btree_gist') THEN
        CREATE EXTENSION IF NOT EXISTS btree_gist;
    END IF;
END
$$;

-- S-21 / DEC-005 tenant-scope membership helper for user-wide and course-scoped protection tables.
CREATE OR REPLACE FUNCTION user_has_tenant_scope(target_tenant UUID) RETURNS BOOLEAN
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
    SELECT EXISTS (
        SELECT 1 FROM tenants t
        WHERE t.id = target_tenant AND t.owner_id = app_actor()
    )
    OR EXISTS (
        SELECT 1 FROM tenant_members tm
        WHERE tm.tenant_id = target_tenant AND tm.user_id = app_actor() AND tm.active
    )
    OR EXISTS (
        SELECT 1 FROM course_members cm
        WHERE cm.tenant_id = target_tenant AND cm.user_id = app_actor() AND cm.active
    )
$$;

CREATE OR REPLACE FUNCTION user_account_created_at(target_user_id UUID) RETURNS TIMESTAMPTZ
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
    SELECT created_at
    FROM users
    WHERE id = target_user_id
      AND (
          target_user_id = app_actor()
          OR session_user IN ('campuslingo_worker', 'campuslingo_grader')
      );
$$;

-- Least-privilege projection of a user's own FSRS card stability/reps/last_review inside a course.
-- Preserves M1 isolation preventing direct SELECT on fsrs_cards by campuslingo_app.
CREATE OR REPLACE FUNCTION actor_course_fsrs_card_states(
    target_course_id UUID,
    target_user_id UUID
) RETURNS TABLE (
    challenge_id UUID,
    stability REAL,
    reps INT,
    last_review TIMESTAMPTZ
)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
    SELECT fc.challenge_id, fc.stability, fc.reps, fc.last_review
    FROM fsrs_cards fc
    WHERE fc.course_id = target_course_id
      AND fc.user_id = target_user_id
      AND (
          (
              target_user_id = app_actor()
              AND (can_study_course(target_course_id) OR can_manage_course(target_course_id))
          )
          OR session_user IN ('campuslingo_worker', 'campuslingo_grader')
      );
$$;

-- Content version activation & S-13 quarantine metadata (additive only; never deletes review_logs or fsrs_cards).
ALTER TABLE content_versions
    ADD COLUMN IF NOT EXISTS activated_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS is_quarantined BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS quarantined_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS quarantine_reason TEXT;

CREATE INDEX IF NOT EXISTS idx_content_versions_active_clean
    ON content_versions (course_id, version_number DESC)
    WHERE NOT is_quarantined;

-- Composite enrollment unique constraint for strict S-21 reporting FKs.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'course_members_scope_unique'
    ) THEN
        ALTER TABLE course_members
            ADD CONSTRAINT course_members_scope_unique UNIQUE (course_id, tenant_id, user_id);
    END IF;
END
$$;

-- S-07: Non-destructive versioned card membership & archival.
CREATE TABLE content_version_cards (
    content_version_id UUID        NOT NULL,
    card_id            UUID        NOT NULL,
    tenant_id          UUID        NOT NULL,
    course_id          UUID        NOT NULL,
    added_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    archived_at        TIMESTAMPTZ,
    PRIMARY KEY (content_version_id, card_id),
    FOREIGN KEY (content_version_id, tenant_id, course_id)
        REFERENCES content_versions (id, tenant_id, course_id) ON DELETE CASCADE,
    FOREIGN KEY (card_id, tenant_id, course_id)
        REFERENCES challenges (id, tenant_id, course_id) ON DELETE CASCADE
);

CREATE INDEX idx_content_version_cards_active
    ON content_version_cards (course_id, content_version_id, card_id)
    WHERE archived_at IS NULL;

-- S-05: Same-course prerequisite DAG with cycle rejection and depth <= 16 / closure <= 256 bounds.
CREATE TABLE node_prerequisites (
    prerequisite_node_id UUID        NOT NULL,
    dependent_node_id    UUID        NOT NULL,
    tenant_id            UUID        NOT NULL,
    course_id            UUID        NOT NULL,
    weight               REAL        NOT NULL CHECK (weight >= 0.0 AND weight <= 1.0),
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (prerequisite_node_id, dependent_node_id),
    CHECK (prerequisite_node_id <> dependent_node_id),
    FOREIGN KEY (prerequisite_node_id, tenant_id, course_id)
        REFERENCES learning_nodes (id, tenant_id, course_id) ON DELETE CASCADE,
    FOREIGN KEY (dependent_node_id, tenant_id, course_id)
        REFERENCES learning_nodes (id, tenant_id, course_id) ON DELETE CASCADE
);

CREATE INDEX idx_node_prerequisites_course
    ON node_prerequisites (course_id, dependent_node_id, prerequisite_node_id);

CREATE OR REPLACE FUNCTION enforce_prerequisite_dag_bounds() RETURNS TRIGGER
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
DECLARE
    has_cycle   BOOLEAN;
    max_depth   INT;
    max_closure INT;
BEGIN
    PERFORM pg_advisory_xact_lock(hashtextextended(NEW.course_id::text, 5));

    -- 1. Reject cycles: check whether NEW.dependent_node_id reaches NEW.prerequisite_node_id.
    WITH RECURSIVE reach AS (
        SELECT NEW.dependent_node_id AS node_id, ARRAY[NEW.dependent_node_id] AS visited
        UNION ALL
        SELECT np.dependent_node_id, r.visited || np.dependent_node_id
        FROM reach r
        JOIN node_prerequisites np
          ON np.course_id = NEW.course_id
         AND np.prerequisite_node_id = r.node_id
         AND NOT (TG_OP = 'UPDATE' AND np.prerequisite_node_id = OLD.prerequisite_node_id AND np.dependent_node_id = OLD.dependent_node_id)
        WHERE NOT (np.dependent_node_id = ANY (r.visited))
          AND array_length(r.visited, 1) <= 32
    )
    SELECT EXISTS (SELECT 1 FROM reach WHERE node_id = NEW.prerequisite_node_id)
    INTO has_cycle;

    IF has_cycle THEN
        RAISE EXCEPTION 'prerequisite DAG contains a cycle'
            USING ERRCODE = '23514';
    END IF;

    -- 2. Enforce DAG depth <= 16 and transitive ancestor closure <= 256 across the course.
    WITH RECURSIVE all_edges AS (
        SELECT prerequisite_node_id, dependent_node_id
        FROM node_prerequisites np
        WHERE np.course_id = NEW.course_id
          AND NOT (TG_OP = 'UPDATE' AND np.prerequisite_node_id = OLD.prerequisite_node_id AND np.dependent_node_id = OLD.dependent_node_id)
        UNION ALL
        SELECT NEW.prerequisite_node_id, NEW.dependent_node_id
    ),
    ancestors AS (
        SELECT prerequisite_node_id AS ancestor_id, dependent_node_id AS node_id, 1 AS depth
        FROM all_edges
        UNION ALL
        SELECT a.ancestor_id, e.dependent_node_id, a.depth + 1
        FROM ancestors a
        JOIN all_edges e ON e.prerequisite_node_id = a.node_id
        WHERE a.depth <= 17
    ),
    stats AS (
        SELECT node_id, MAX(depth) AS node_depth, COUNT(DISTINCT ancestor_id) AS closure_size
        FROM ancestors
        GROUP BY node_id
    )
    SELECT COALESCE(MAX(node_depth), 0), COALESCE(MAX(closure_size), 0)
    INTO max_depth, max_closure
    FROM stats;

    IF max_depth > 16 THEN
        RAISE EXCEPTION 'prerequisite DAG depth % exceeds maximum 16', max_depth
            USING ERRCODE = '23514';
    END IF;
    IF max_closure > 256 THEN
        RAISE EXCEPTION 'prerequisite DAG transitive closure % exceeds maximum 256', max_closure
            USING ERRCODE = '23514';
    END IF;

    RETURN NEW;
END;
$$;

CREATE TRIGGER trg_node_prerequisites_bounds
    BEFORE INSERT OR UPDATE ON node_prerequisites
    FOR EACH ROW EXECUTE FUNCTION enforce_prerequisite_dag_bounds();

-- S-19: Versioned 21-weight FSRS-6 parameter persistence (REAL[] round-trip discipline).
CREATE TABLE user_fsrs_parameters (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tenant_id  UUID        NOT NULL REFERENCES tenants(id),
    version    INT         NOT NULL CHECK (version > 0),
    weights    REAL[]      NOT NULL CHECK (array_length(weights, 1) = 21),
    is_active  BOOLEAN     NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, version)
);

CREATE UNIQUE INDEX idx_user_fsrs_parameters_active
    ON user_fsrs_parameters (user_id)
    WHERE is_active;

-- S-23 + DEC-005: One row per user (PK user_id preserved so cross-tenant enrollment cannot reset bubble/dormancy),
-- anchored to a validated tenant_id.
CREATE TABLE user_protection_state (
    user_id                      UUID        PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    tenant_id                    UUID        NOT NULL REFERENCES tenants(id),
    last_qualifying_study_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    is_dormant                   BOOLEAN     NOT NULL DEFAULT FALSE,
    return_study_at              TIMESTAMPTZ,
    resume_grace_until           TIMESTAMPTZ,
    last_resume_grace_granted_at TIMESTAMPTZ,
    new_player_bubble_until      TIMESTAMPTZ NOT NULL,
    eligible_transition_count    INT         NOT NULL DEFAULT 0 CHECK (eligible_transition_count >= 0),
    consecutive_dormant_weeks    SMALLINT    NOT NULL DEFAULT 0 CHECK (consecutive_dormant_weeks >= 0),
    accommodation_multiplier     REAL        NOT NULL DEFAULT 1.0 CHECK (accommodation_multiplier IN (1.0, 1.25, 1.5)),
    accommodation_version        INT         NOT NULL DEFAULT 1 CHECK (accommodation_version >= 1),
    updated_at                   TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, tenant_id)
);

-- S-24 + DEC-005: Historical dormancy intervals; non-overlapping enforced by trigger (23P01) + EXCLUDE USING gist when available.
CREATE TABLE user_dormancy_periods (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tenant_id  UUID        NOT NULL REFERENCES tenants(id),
    started_at TIMESTAMPTZ NOT NULL,
    ended_at   TIMESTAMPTZ CHECK (ended_at IS NULL OR ended_at >= started_at),
    FOREIGN KEY (user_id, tenant_id)
        REFERENCES user_protection_state (user_id, tenant_id) ON DELETE CASCADE
);

CREATE INDEX idx_user_dormancy_lookup ON user_dormancy_periods (user_id, started_at, ended_at);
CREATE INDEX idx_user_dormancy_range ON user_dormancy_periods USING gist (tstzrange(started_at, ended_at, '[)'));

CREATE OR REPLACE FUNCTION enforce_non_overlapping_dormancy() RETURNS TRIGGER
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
BEGIN
    PERFORM pg_advisory_xact_lock(hashtextextended(NEW.user_id::text, 24));
    IF EXISTS (
        SELECT 1
        FROM user_dormancy_periods p
        WHERE p.user_id = NEW.user_id
          AND p.id <> NEW.id
          AND tstzrange(p.started_at, p.ended_at, '[)') && tstzrange(NEW.started_at, NEW.ended_at, '[)')
    ) THEN
        RAISE EXCEPTION 'overlapping user dormancy interval for user %', NEW.user_id
            USING ERRCODE = '23P01';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trg_user_dormancy_no_overlap
    BEFORE INSERT OR UPDATE ON user_dormancy_periods
    FOR EACH ROW EXECUTE FUNCTION enforce_non_overlapping_dormancy();

-- S-24 + DEC-005: Node-level timed protections ONLY.
CREATE TABLE user_node_protection (
    user_id         UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    node_id         UUID        NOT NULL,
    tenant_id       UUID        NOT NULL,
    course_id       UUID        NOT NULL,
    kind            VARCHAR(24) NOT NULL CHECK (kind IN ('NEW_NODE_GRACE', 'VERSION_GRACE', 'POST_RAID_COOLDOWN')),
    protected_until TIMESTAMPTZ NOT NULL,
    source_event_id UUID,
    computed_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, node_id, kind),
    FOREIGN KEY (node_id, tenant_id, course_id)
        REFERENCES learning_nodes (id, tenant_id, course_id) ON DELETE CASCADE
);

-- S-25 + DEC-005: Once-ever per-card VERSION_GRACE record.
CREATE TABLE user_card_version_grace (
    user_id            UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    card_id            UUID        NOT NULL,
    tenant_id          UUID        NOT NULL,
    course_id          UUID        NOT NULL,
    content_version_id UUID        NOT NULL,
    granted_at         TIMESTAMPTZ NOT NULL,
    grace_until        TIMESTAMPTZ NOT NULL,
    consumed_at        TIMESTAMPTZ,
    PRIMARY KEY (user_id, card_id),
    FOREIGN KEY (card_id, tenant_id, course_id)
        REFERENCES challenges (id, tenant_id, course_id) ON DELETE CASCADE,
    FOREIGN KEY (content_version_id, tenant_id, course_id)
        REFERENCES content_versions (id, tenant_id, course_id) ON DELETE CASCADE
);

-- S-25 + DEC-005: Append-only non-overlapping <= 14d/30d node grace ledger.
CREATE TABLE user_node_grace_ledger (
    id                 UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id            UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    node_id            UUID        NOT NULL,
    tenant_id          UUID        NOT NULL,
    course_id          UUID        NOT NULL,
    content_version_id UUID        NOT NULL,
    started_at         TIMESTAMPTZ NOT NULL,
    ended_at           TIMESTAMPTZ NOT NULL CHECK (ended_at > started_at),
    FOREIGN KEY (node_id, tenant_id, course_id)
        REFERENCES learning_nodes (id, tenant_id, course_id) ON DELETE CASCADE,
    FOREIGN KEY (content_version_id, tenant_id, course_id)
        REFERENCES content_versions (id, tenant_id, course_id) ON DELETE CASCADE
);

CREATE INDEX idx_node_grace_ledger_window ON user_node_grace_ledger (user_id, node_id, ended_at);
CREATE INDEX idx_node_grace_ledger_range ON user_node_grace_ledger USING gist (tstzrange(started_at, ended_at, '[)'));

CREATE OR REPLACE FUNCTION enforce_non_overlapping_node_grace() RETURNS TRIGGER
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
BEGIN
    PERFORM pg_advisory_xact_lock(hashtextextended(NEW.user_id::text || ':' || NEW.node_id::text, 25));
    IF EXISTS (
        SELECT 1
        FROM user_node_grace_ledger g
        WHERE g.user_id = NEW.user_id
          AND g.node_id = NEW.node_id
          AND g.id <> NEW.id
          AND tstzrange(g.started_at, g.ended_at, '[)') && tstzrange(NEW.started_at, NEW.ended_at, '[)')
    ) THEN
        RAISE EXCEPTION 'overlapping node grace interval for user % node %', NEW.user_id, NEW.node_id
            USING ERRCODE = '23P01';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trg_node_grace_ledger_no_overlap
    BEFORE INSERT OR UPDATE ON user_node_grace_ledger
    FOR EACH ROW EXECUTE FUNCTION enforce_non_overlapping_node_grace();

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'btree_gist') THEN
        ALTER TABLE user_dormancy_periods
            ADD CONSTRAINT user_dormancy_periods_no_overlap_excl
            EXCLUDE USING gist (user_id WITH =, tstzrange(started_at, ended_at, '[)') WITH &&);
        ALTER TABLE user_node_grace_ledger
            ADD CONSTRAINT user_node_grace_ledger_no_overlap_excl
            EXCLUDE USING gist (user_id WITH =, node_id WITH =, tstzrange(started_at, ended_at, '[)') WITH &&);
    END IF;
END
$$;

-- Version-transition course seen & node engagement tracking (§9.2).
CREATE TABLE user_course_version_seen (
    user_id                    UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    course_id                  UUID        NOT NULL,
    content_version_id         UUID        NOT NULL,
    tenant_id                  UUID        NOT NULL,
    first_opened_at            TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (user_id, course_id, content_version_id),
    FOREIGN KEY (content_version_id, tenant_id, course_id)
        REFERENCES content_versions (id, tenant_id, course_id) ON DELETE CASCADE
);

CREATE TABLE user_node_version_engagement (
    user_id            UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    node_id            UUID        NOT NULL,
    content_version_id UUID        NOT NULL,
    tenant_id          UUID        NOT NULL,
    course_id          UUID        NOT NULL,
    engaged_at         TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (user_id, node_id, content_version_id),
    FOREIGN KEY (node_id, tenant_id, course_id)
        REFERENCES learning_nodes (id, tenant_id, course_id) ON DELETE CASCADE,
    FOREIGN KEY (content_version_id, tenant_id, course_id)
        REFERENCES content_versions (id, tenant_id, course_id) ON DELETE CASCADE
);

-- DEC-004: Materialized cached shield & tier state per (user_id, node_id).
CREATE TABLE user_node_shield_state (
    user_id            UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    node_id            UUID        NOT NULL,
    tenant_id          UUID        NOT NULL,
    course_id          UUID        NOT NULL,
    content_version_id UUID,
    state              TEXT        NOT NULL CHECK (state IN ('INACTIVE', 'ACTIVE', 'INVALID')),
    tier               SMALLINT    CHECK (tier IS NULL OR tier BETWEEN 0 AND 3),
    e_u_ticks          INTEGER     CHECK (e_u_ticks IS NULL OR e_u_ticks BETWEEN 0 AND 10000),
    i_u_ticks          INTEGER     CHECK (i_u_ticks IS NULL OR i_u_ticks BETWEEN 0 AND 10000),
    s_u_ticks          INTEGER     CHECK (s_u_ticks IS NULL OR s_u_ticks >= 0),
    is_brownout        BOOLEAN,
    unlocked_at        TIMESTAMPTZ,
    raid_immune        BOOLEAN     NOT NULL DEFAULT TRUE,
    propagation_exempt BOOLEAN     NOT NULL DEFAULT TRUE,
    active_protections TEXT[]      NOT NULL DEFAULT '{}',
    computed_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, node_id),
    FOREIGN KEY (node_id, tenant_id, course_id)
        REFERENCES learning_nodes (id, tenant_id, course_id) ON DELETE CASCADE,
    CONSTRAINT chk_user_node_shield_dec004_nulls CHECK (
        (state = 'ACTIVE' AND tier IS NOT NULL AND e_u_ticks IS NOT NULL AND i_u_ticks IS NOT NULL AND s_u_ticks IS NOT NULL AND is_brownout IS NOT NULL)
        OR
        (state IN ('INACTIVE', 'INVALID') AND tier IS NULL AND e_u_ticks IS NULL AND i_u_ticks IS NULL AND s_u_ticks IS NULL AND is_brownout IS NULL)
    )
);

CREATE INDEX idx_user_node_shield_course ON user_node_shield_state (user_id, course_id, computed_at);

-- Async shield recompute queue (§9.6: enqueued when cached state >= 15 min stale).
CREATE TABLE shield_recompute_queue (
    user_id      UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    course_id    UUID        NOT NULL,
    tenant_id    UUID        NOT NULL,
    requested_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    PRIMARY KEY (user_id, course_id),
    FOREIGN KEY (course_id, tenant_id) REFERENCES courses (id, tenant_id) ON DELETE CASCADE
);

-- S-13: Version-scoped reporting & anti-brigading quarantine.
CREATE TABLE node_reports (
    id                        UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id                   UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    node_id                   UUID        NOT NULL,
    content_version_id        UUID        NOT NULL,
    tenant_id                 UUID        NOT NULL,
    course_id                 UUID        NOT NULL,
    reason                    TEXT        NOT NULL CHECK (length(trim(reason)) BETWEEN 1 AND 240),
    details                   TEXT        CHECK (details IS NULL OR length(details) <= 2000),
    reporter_cluster          TEXT        NOT NULL CHECK (length(trim(reporter_cluster)) BETWEEN 1 AND 128),
    is_eligible               BOOLEAN     NOT NULL DEFAULT TRUE,
    local_replacement_card_id UUID,
    refund_applied            BOOLEAN     NOT NULL DEFAULT TRUE,
    created_at                TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, node_id, content_version_id),
    FOREIGN KEY (course_id, tenant_id, user_id)
        REFERENCES course_members (course_id, tenant_id, user_id) ON DELETE CASCADE,
    FOREIGN KEY (node_id, tenant_id, course_id)
        REFERENCES learning_nodes (id, tenant_id, course_id) ON DELETE CASCADE,
    FOREIGN KEY (content_version_id, tenant_id, course_id)
        REFERENCES content_versions (id, tenant_id, course_id) ON DELETE CASCADE
);

CREATE INDEX idx_node_reports_version_eligible
    ON node_reports (course_id, content_version_id, reporter_cluster)
    WHERE is_eligible;

-- Privacy-preserving anti-brigading check: checks cluster eligibility across all reports
-- without exposing other students' node_reports rows to RLS callers.
CREATE OR REPLACE FUNCTION is_report_cluster_eligible(
    target_course_id UUID,
    target_version_id UUID,
    target_cluster TEXT
) RETURNS BOOLEAN
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
    SELECT (can_study_course(target_course_id) OR session_user = 'campuslingo_worker')
       AND NOT EXISTS (
           SELECT 1 FROM node_reports nr
           WHERE nr.course_id = target_course_id
             AND nr.content_version_id = target_version_id
             AND nr.reporter_cluster = target_cluster
             AND nr.is_eligible = TRUE
       );
$$;

CREATE OR REPLACE FUNCTION quarantine_content_version_if_threshold(
    target_course_id UUID,
    target_version_id UUID,
    min_independent_reports INT DEFAULT 3
) RETURNS BOOLEAN
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    distinct_clusters INT;
BEGIN
    SELECT COUNT(DISTINCT nr.reporter_cluster)
    INTO distinct_clusters
    FROM node_reports nr
    JOIN course_members cm
      ON cm.course_id = nr.course_id
     AND cm.tenant_id = nr.tenant_id
     AND cm.user_id = nr.user_id
    WHERE nr.course_id = target_course_id
      AND nr.content_version_id = target_version_id
      AND nr.is_eligible = TRUE
      AND cm.active = TRUE;

    IF distinct_clusters >= min_independent_reports THEN
        UPDATE content_versions
        SET is_quarantined = TRUE,
            quarantined_at = COALESCE(quarantined_at, now()),
            quarantine_reason = COALESCE(
                quarantine_reason,
                format('S-13 quarantine: %s independent enrolled reports', distinct_clusters)
            )
        WHERE id = target_version_id
          AND course_id = target_course_id;

        -- Also quarantine matching ingestion chunks linked via bundle source hashes (S-04) without touching review_logs.
        UPDATE ingestion_chunks ic
        SET is_quarantined = TRUE
        FROM learning_unit_bundles lub
        WHERE lub.content_version_id = target_version_id
          AND lub.course_id = target_course_id
          AND ic.course_id = lub.course_id
          AND ic.text_sha256 = lub.source_sha256;

        RETURN TRUE;
    END IF;
    RETURN FALSE;
END;
$$;

-- RLS policies (separate SELECT / INSERT / UPDATE / DELETE policies; FORCE RLS everywhere).
ALTER TABLE content_version_cards ENABLE ROW LEVEL SECURITY;
ALTER TABLE content_version_cards FORCE ROW LEVEL SECURITY;
CREATE POLICY content_version_cards_select ON content_version_cards
    FOR SELECT USING (can_read_course(course_id));
CREATE POLICY content_version_cards_insert ON content_version_cards
    FOR INSERT WITH CHECK (can_manage_course(course_id));
CREATE POLICY content_version_cards_update ON content_version_cards
    FOR UPDATE USING (can_manage_course(course_id)) WITH CHECK (can_manage_course(course_id));

ALTER TABLE node_prerequisites ENABLE ROW LEVEL SECURITY;
ALTER TABLE node_prerequisites FORCE ROW LEVEL SECURITY;
CREATE POLICY node_prerequisites_select ON node_prerequisites
    FOR SELECT USING (can_read_course(course_id));
CREATE POLICY node_prerequisites_insert ON node_prerequisites
    FOR INSERT WITH CHECK (can_manage_course(course_id));
CREATE POLICY node_prerequisites_update ON node_prerequisites
    FOR UPDATE USING (can_manage_course(course_id)) WITH CHECK (can_manage_course(course_id));
CREATE POLICY node_prerequisites_delete ON node_prerequisites
    FOR DELETE USING (can_manage_course(course_id));

ALTER TABLE user_fsrs_parameters ENABLE ROW LEVEL SECURITY;
ALTER TABLE user_fsrs_parameters FORCE ROW LEVEL SECURITY;
CREATE POLICY user_fsrs_parameters_select ON user_fsrs_parameters
    FOR SELECT USING (user_id = app_actor() AND user_has_tenant_scope(tenant_id));
CREATE POLICY user_fsrs_parameters_insert ON user_fsrs_parameters
    FOR INSERT WITH CHECK (user_id = app_actor() AND user_has_tenant_scope(tenant_id));
CREATE POLICY user_fsrs_parameters_update ON user_fsrs_parameters
    FOR UPDATE USING (user_id = app_actor() AND user_has_tenant_scope(tenant_id))
    WITH CHECK (user_id = app_actor() AND user_has_tenant_scope(tenant_id));

ALTER TABLE user_protection_state ENABLE ROW LEVEL SECURITY;
ALTER TABLE user_protection_state FORCE ROW LEVEL SECURITY;
CREATE POLICY user_protection_state_select ON user_protection_state
    FOR SELECT USING (user_id = app_actor() AND user_has_tenant_scope(tenant_id));
CREATE POLICY user_protection_state_insert ON user_protection_state
    FOR INSERT WITH CHECK (user_id = app_actor() AND user_has_tenant_scope(tenant_id));
CREATE POLICY user_protection_state_update ON user_protection_state
    FOR UPDATE USING (user_id = app_actor() AND user_has_tenant_scope(tenant_id))
    WITH CHECK (user_id = app_actor() AND user_has_tenant_scope(tenant_id));

ALTER TABLE user_dormancy_periods ENABLE ROW LEVEL SECURITY;
ALTER TABLE user_dormancy_periods FORCE ROW LEVEL SECURITY;
CREATE POLICY user_dormancy_periods_select ON user_dormancy_periods
    FOR SELECT USING (user_id = app_actor() AND user_has_tenant_scope(tenant_id));
CREATE POLICY user_dormancy_periods_insert ON user_dormancy_periods
    FOR INSERT WITH CHECK (user_id = app_actor() AND user_has_tenant_scope(tenant_id));
CREATE POLICY user_dormancy_periods_update ON user_dormancy_periods
    FOR UPDATE USING (user_id = app_actor() AND user_has_tenant_scope(tenant_id))
    WITH CHECK (user_id = app_actor() AND user_has_tenant_scope(tenant_id));

ALTER TABLE user_node_protection ENABLE ROW LEVEL SECURITY;
ALTER TABLE user_node_protection FORCE ROW LEVEL SECURITY;
CREATE POLICY user_node_protection_select ON user_node_protection
    FOR SELECT USING (user_id = app_actor() AND user_has_tenant_scope(tenant_id) AND (can_study_course(course_id) OR can_manage_course(course_id)));
CREATE POLICY user_node_protection_insert ON user_node_protection
    FOR INSERT WITH CHECK (user_id = app_actor() AND user_has_tenant_scope(tenant_id) AND (can_study_course(course_id) OR can_manage_course(course_id)));
CREATE POLICY user_node_protection_update ON user_node_protection
    FOR UPDATE USING (user_id = app_actor() AND user_has_tenant_scope(tenant_id) AND (can_study_course(course_id) OR can_manage_course(course_id)))
    WITH CHECK (user_id = app_actor() AND user_has_tenant_scope(tenant_id) AND (can_study_course(course_id) OR can_manage_course(course_id)));

ALTER TABLE user_card_version_grace ENABLE ROW LEVEL SECURITY;
ALTER TABLE user_card_version_grace FORCE ROW LEVEL SECURITY;
CREATE POLICY user_card_version_grace_select ON user_card_version_grace
    FOR SELECT USING (user_id = app_actor() AND user_has_tenant_scope(tenant_id) AND (can_study_course(course_id) OR can_manage_course(course_id)));
CREATE POLICY user_card_version_grace_insert ON user_card_version_grace
    FOR INSERT WITH CHECK (user_id = app_actor() AND user_has_tenant_scope(tenant_id) AND (can_study_course(course_id) OR can_manage_course(course_id)));
CREATE POLICY user_card_version_grace_update ON user_card_version_grace
    FOR UPDATE USING (user_id = app_actor() AND user_has_tenant_scope(tenant_id) AND (can_study_course(course_id) OR can_manage_course(course_id)))
    WITH CHECK (user_id = app_actor() AND user_has_tenant_scope(tenant_id) AND (can_study_course(course_id) OR can_manage_course(course_id)));

ALTER TABLE user_node_grace_ledger ENABLE ROW LEVEL SECURITY;
ALTER TABLE user_node_grace_ledger FORCE ROW LEVEL SECURITY;
CREATE POLICY user_node_grace_ledger_select ON user_node_grace_ledger
    FOR SELECT USING (user_id = app_actor() AND user_has_tenant_scope(tenant_id) AND (can_study_course(course_id) OR can_manage_course(course_id)));
CREATE POLICY user_node_grace_ledger_insert ON user_node_grace_ledger
    FOR INSERT WITH CHECK (user_id = app_actor() AND user_has_tenant_scope(tenant_id) AND (can_study_course(course_id) OR can_manage_course(course_id)));

ALTER TABLE user_course_version_seen ENABLE ROW LEVEL SECURITY;
ALTER TABLE user_course_version_seen FORCE ROW LEVEL SECURITY;
CREATE POLICY user_course_version_seen_select ON user_course_version_seen
    FOR SELECT USING (user_id = app_actor() AND user_has_tenant_scope(tenant_id) AND (can_study_course(course_id) OR can_manage_course(course_id)));
CREATE POLICY user_course_version_seen_insert ON user_course_version_seen
    FOR INSERT WITH CHECK (user_id = app_actor() AND user_has_tenant_scope(tenant_id) AND (can_study_course(course_id) OR can_manage_course(course_id)));

ALTER TABLE user_node_version_engagement ENABLE ROW LEVEL SECURITY;
ALTER TABLE user_node_version_engagement FORCE ROW LEVEL SECURITY;
CREATE POLICY user_node_version_engagement_select ON user_node_version_engagement
    FOR SELECT USING (user_id = app_actor() AND user_has_tenant_scope(tenant_id) AND (can_study_course(course_id) OR can_manage_course(course_id)));
CREATE POLICY user_node_version_engagement_insert ON user_node_version_engagement
    FOR INSERT WITH CHECK (user_id = app_actor() AND user_has_tenant_scope(tenant_id) AND (can_study_course(course_id) OR can_manage_course(course_id)));

ALTER TABLE user_node_shield_state ENABLE ROW LEVEL SECURITY;
ALTER TABLE user_node_shield_state FORCE ROW LEVEL SECURITY;
CREATE POLICY user_node_shield_state_select ON user_node_shield_state
    FOR SELECT USING (user_id = app_actor() AND user_has_tenant_scope(tenant_id) AND (can_study_course(course_id) OR can_manage_course(course_id)));
CREATE POLICY user_node_shield_state_insert ON user_node_shield_state
    FOR INSERT WITH CHECK (user_id = app_actor() AND user_has_tenant_scope(tenant_id) AND (can_study_course(course_id) OR can_manage_course(course_id)));
CREATE POLICY user_node_shield_state_update ON user_node_shield_state
    FOR UPDATE USING (user_id = app_actor() AND user_has_tenant_scope(tenant_id) AND (can_study_course(course_id) OR can_manage_course(course_id)))
    WITH CHECK (user_id = app_actor() AND user_has_tenant_scope(tenant_id) AND (can_study_course(course_id) OR can_manage_course(course_id)));

ALTER TABLE shield_recompute_queue ENABLE ROW LEVEL SECURITY;
ALTER TABLE shield_recompute_queue FORCE ROW LEVEL SECURITY;
CREATE POLICY shield_recompute_queue_select ON shield_recompute_queue
    FOR SELECT USING (user_id = app_actor() AND user_has_tenant_scope(tenant_id) AND (can_study_course(course_id) OR can_manage_course(course_id)));
CREATE POLICY shield_recompute_queue_insert ON shield_recompute_queue
    FOR INSERT WITH CHECK (user_id = app_actor() AND user_has_tenant_scope(tenant_id) AND (can_study_course(course_id) OR can_manage_course(course_id)));
CREATE POLICY shield_recompute_queue_update ON shield_recompute_queue
    FOR UPDATE USING (user_id = app_actor() AND user_has_tenant_scope(tenant_id) AND (can_study_course(course_id) OR can_manage_course(course_id)))
    WITH CHECK (user_id = app_actor() AND user_has_tenant_scope(tenant_id) AND (can_study_course(course_id) OR can_manage_course(course_id)));

ALTER TABLE node_reports ENABLE ROW LEVEL SECURITY;
ALTER TABLE node_reports FORCE ROW LEVEL SECURITY;
CREATE POLICY node_reports_select ON node_reports
    FOR SELECT USING ((user_id = app_actor() AND can_study_course(course_id)) OR can_manage_course(course_id));
CREATE POLICY node_reports_insert ON node_reports
    FOR INSERT WITH CHECK (user_id = app_actor() AND can_study_course(course_id) AND user_has_tenant_scope(tenant_id));
