-- M1 foundation. Roles are created outside migrations by the deployment harness.
-- The migration role owns these objects and must be able to bypass RLS while
-- maintaining the metadata helper functions.

CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT NOT NULL UNIQUE CHECK (email = lower(email) AND length(email) BETWEEN 3 AND 254),
    password_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);

CREATE TABLE sessions (
    token_hash BYTEA PRIMARY KEY CHECK (octet_length(token_hash) = 32),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    expires_at TIMESTAMPTZ NOT NULL DEFAULT (clock_timestamp() + interval '7 days')
);
CREATE INDEX sessions_user ON sessions(user_id);
CREATE INDEX sessions_expiry ON sessions(expires_at);

CREATE TABLE recovery_tokens (
    token_hash BYTEA PRIMARY KEY CHECK (octet_length(token_hash) = 32),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at TIMESTAMPTZ NOT NULL DEFAULT (clock_timestamp() + interval '15 minutes'),
    consumed_at TIMESTAMPTZ
);

-- Only encrypted delivery material is stored. No plaintext recovery token or
-- email address belongs here; a separate delivery process owns the key.
CREATE TABLE recovery_outbox (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    ciphertext BYTEA NOT NULL CHECK (octet_length(ciphertext) BETWEEN 48 AND 4096),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    delivered_at TIMESTAMPTZ
);

CREATE TABLE tenants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id UUID NOT NULL REFERENCES users(id),
    name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 160),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);

CREATE TABLE tenant_members (
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('owner', 'instructor', 'student')),
    active BOOLEAN NOT NULL DEFAULT TRUE,
    PRIMARY KEY (tenant_id, user_id)
);

CREATE TABLE courses (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    created_by UUID NOT NULL,
    name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 160),
    visibility TEXT NOT NULL DEFAULT 'private' CHECK (visibility IN ('private', 'public')),
    published BOOLEAN NOT NULL DEFAULT FALSE,
    rights_attested_at TIMESTAMPTZ,
    published_by UUID REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (id, tenant_id),
    FOREIGN KEY (tenant_id, created_by) REFERENCES tenant_members(tenant_id, user_id),
    CHECK (NOT published OR (visibility = 'public' AND rights_attested_at IS NOT NULL AND published_by IS NOT NULL))
);

-- Public learners need not be tenant staff. Course membership is the study
-- authorization boundary; tenant membership is the authoring/administration one.
CREATE TABLE course_members (
    course_id UUID NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    tenant_id UUID NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('instructor', 'student')),
    active BOOLEAN NOT NULL DEFAULT TRUE,
    PRIMARY KEY (course_id, user_id),
    FOREIGN KEY (course_id, tenant_id) REFERENCES courses(id, tenant_id)
);

CREATE TABLE learning_nodes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    course_id UUID NOT NULL,
    title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 160),
    UNIQUE (id, tenant_id, course_id),
    FOREIGN KEY (course_id, tenant_id) REFERENCES courses(id, tenant_id) ON DELETE CASCADE
);

CREATE TABLE challenges (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    course_id UUID NOT NULL,
    node_id UUID NOT NULL,
    prompt TEXT NOT NULL CHECK (length(prompt) BETWEEN 1 AND 1200),
    options JSONB NOT NULL CHECK (jsonb_typeof(options) = 'array' AND jsonb_array_length(options) BETWEEN 2 AND 6),
    version INT NOT NULL DEFAULT 1 CHECK (version > 0),
    UNIQUE (id, tenant_id, course_id),
    FOREIGN KEY (node_id, tenant_id, course_id) REFERENCES learning_nodes(id, tenant_id, course_id) ON DELETE CASCADE
);

-- Never join this table into a public projection. The grader role is the only
-- runtime role granted SELECT on it.
CREATE TABLE challenge_answers (
    challenge_id UUID PRIMARY KEY REFERENCES challenges(id) ON DELETE CASCADE,
    correct_index SMALLINT NOT NULL CHECK (correct_index BETWEEN 0 AND 5),
    explanation TEXT NOT NULL DEFAULT '' CHECK (length(explanation) <= 2000)
);

CREATE TABLE assets (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    course_id UUID NOT NULL,
    created_by UUID NOT NULL REFERENCES users(id),
    sha256 TEXT NOT NULL CHECK (length(sha256) = 64),
    media_type TEXT NOT NULL CHECK (media_type IN ('application/pdf', 'image/png', 'image/jpeg', 'image/webp', 'text/plain')),
    bytes BIGINT NOT NULL CHECK (bytes > 0 AND bytes <= 83886080),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    FOREIGN KEY (course_id, tenant_id) REFERENCES courses(id, tenant_id) ON DELETE CASCADE
);

CREATE TABLE attempts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    course_id UUID NOT NULL,
    challenge_id UUID NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id),
    option_count SMALLINT NOT NULL CHECK (option_count BETWEEN 2 AND 6),
    selected_index SMALLINT CHECK (selected_index BETWEEN 0 AND 5),
    expires_at TIMESTAMPTZ NOT NULL DEFAULT (clock_timestamp() + interval '2 minutes'),
    answered_at TIMESTAMPTZ,
    response JSONB,
    CHECK ((selected_index IS NULL) = (response IS NULL)),
    CHECK ((answered_at IS NULL) = (response IS NULL)),
    FOREIGN KEY (challenge_id, tenant_id, course_id) REFERENCES challenges(id, tenant_id, course_id) ON DELETE CASCADE
);
CREATE INDEX attempts_owner ON attempts(user_id, challenge_id);

CREATE TABLE fsrs_cards (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    challenge_id UUID NOT NULL,
    tenant_id UUID NOT NULL,
    course_id UUID NOT NULL,
    stability REAL,
    difficulty REAL,
    reps INT NOT NULL DEFAULT 0 CHECK (reps >= 0),
    last_review TIMESTAMPTZ,
    due_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    scheduler_version TEXT NOT NULL DEFAULT 'fsrs-rs-7.0.0/FSRS-6/M1-day-policy',
    PRIMARY KEY (user_id, challenge_id),
    FOREIGN KEY (challenge_id, tenant_id, course_id) REFERENCES challenges(id, tenant_id, course_id) ON DELETE CASCADE
);

-- Review history is append-only. It contains the state transition used for
-- audit and reproducibility, but never the selected/correct answer pair.
CREATE TABLE review_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    attempt_id UUID NOT NULL UNIQUE REFERENCES attempts(id),
    user_id UUID NOT NULL REFERENCES users(id),
    tenant_id UUID NOT NULL,
    course_id UUID NOT NULL,
    challenge_id UUID NOT NULL,
    rating SMALLINT NOT NULL CHECK (rating IN (1, 3)),
    state_before JSONB NOT NULL,
    state_after JSONB NOT NULL,
    reviewed_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    FOREIGN KEY (challenge_id, tenant_id, course_id) REFERENCES challenges(id, tenant_id, course_id)
);

CREATE TABLE reward_ledger (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    review_id UUID NOT NULL UNIQUE REFERENCES review_logs(id),
    user_id UUID NOT NULL REFERENCES users(id),
    tenant_id UUID NOT NULL,
    course_id UUID NOT NULL,
    amount INT NOT NULL CHECK (amount = 1),
    kind TEXT NOT NULL DEFAULT 'course_practice_xp' CHECK (kind = 'course_practice_xp'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    FOREIGN KEY (course_id, tenant_id) REFERENCES courses(id, tenant_id)
);
CREATE INDEX reward_owner ON reward_ledger(user_id, course_id);

CREATE TABLE jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    course_id UUID NOT NULL,
    created_by UUID NOT NULL REFERENCES users(id),
    idempotency_key UUID NOT NULL,
    payload JSONB NOT NULL CHECK (octet_length(payload::text) <= 32768),
    payload_hash BYTEA NOT NULL CHECK (octet_length(payload_hash) = 32),
    status TEXT NOT NULL DEFAULT 'queued' CHECK (status IN ('queued', 'running', 'succeeded', 'failed')),
    attempts INT NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts INT NOT NULL DEFAULT 3 CHECK (max_attempts BETWEEN 1 AND 10),
    lease_token UUID,
    lease_until TIMESTAMPTZ,
    progress_done INT NOT NULL DEFAULT 0,
    progress_total INT NOT NULL DEFAULT 1,
    stage TEXT NOT NULL DEFAULT 'queued',
    error_code TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (course_id, created_by, idempotency_key),
    FOREIGN KEY (course_id, tenant_id) REFERENCES courses(id, tenant_id) ON DELETE CASCADE,
    CHECK (progress_done >= 0 AND progress_total >= 1 AND progress_done <= progress_total),
    CHECK ((status = 'running') = (lease_token IS NOT NULL AND lease_until IS NOT NULL))
);
CREATE INDEX jobs_claim ON jobs(status, lease_until, created_at);

CREATE TABLE job_results (
    job_id UUID PRIMARY KEY REFERENCES jobs(id) ON DELETE CASCADE,
    output JSONB NOT NULL CHECK (octet_length(output::text) <= 262144)
);

CREATE TABLE job_events (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    stage TEXT NOT NULL CHECK (length(stage) BETWEEN 1 AND 64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);

CREATE FUNCTION immutable_audit() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'append-only audit records cannot be modified';
END
$$;
CREATE TRIGGER immutable_reviews BEFORE UPDATE OR DELETE ON review_logs FOR EACH ROW EXECUTE FUNCTION immutable_audit();
CREATE TRIGGER immutable_rewards BEFORE UPDATE OR DELETE ON reward_ledger FOR EACH ROW EXECUTE FUNCTION immutable_audit();
CREATE TRIGGER immutable_job_events BEFORE UPDATE OR DELETE ON job_events FOR EACH ROW EXECUTE FUNCTION immutable_audit();

CREATE FUNCTION app_actor() RETURNS UUID LANGUAGE SQL STABLE AS $$
    SELECT nullif(current_setting('campuslingo.actor', true), '')::uuid
$$;

-- SECURITY DEFINER metadata predicates avoid recursive RLS: their owner is the
-- migration role, which owns the metadata tables and can read them directly.
CREATE FUNCTION is_tenant_member(target UUID) RETURNS BOOLEAN
LANGUAGE SQL STABLE SECURITY DEFINER SET search_path = public, pg_temp AS $$
    SELECT EXISTS (
        SELECT 1 FROM tenant_members m
        WHERE m.tenant_id = target AND m.user_id = app_actor() AND m.active
    )
$$;

CREATE FUNCTION is_tenant_owner(target UUID) RETURNS BOOLEAN
LANGUAGE SQL STABLE SECURITY DEFINER SET search_path = public, pg_temp AS $$
    SELECT EXISTS (
        SELECT 1 FROM tenants t
        WHERE t.id = target AND t.owner_id = app_actor()
    )
$$;

CREATE FUNCTION can_read_course(target UUID) RETURNS BOOLEAN
LANGUAGE SQL STABLE SECURITY DEFINER SET search_path = public, pg_temp AS $$
    SELECT EXISTS (
        SELECT 1 FROM courses c
        JOIN tenants t ON t.id = c.tenant_id
        WHERE c.id = target AND (
            (c.visibility = 'public' AND c.published)
            OR t.owner_id = app_actor()
            OR EXISTS (
                SELECT 1 FROM course_members m
                WHERE m.course_id = c.id AND m.user_id = app_actor() AND m.active
            )
        )
    )
$$;

CREATE FUNCTION can_manage_course(target UUID) RETURNS BOOLEAN
LANGUAGE SQL STABLE SECURITY DEFINER SET search_path = public, pg_temp AS $$
    SELECT EXISTS (
        SELECT 1 FROM courses c
        JOIN tenants t ON t.id = c.tenant_id
        WHERE c.id = target AND (
            t.owner_id = app_actor()
            OR EXISTS (
                SELECT 1 FROM course_members m
                WHERE m.course_id = c.id AND m.user_id = app_actor()
                  AND m.active AND m.role = 'instructor'
            )
        )
    )
$$;

CREATE FUNCTION can_study_course(target UUID) RETURNS BOOLEAN
LANGUAGE SQL STABLE SECURITY DEFINER SET search_path = public, pg_temp AS $$
    SELECT EXISTS (
        SELECT 1 FROM course_members m
        WHERE m.course_id = target AND m.user_id = app_actor() AND m.active
    )
$$;

CREATE FUNCTION can_self_enroll(target UUID) RETURNS BOOLEAN
LANGUAGE SQL STABLE SECURITY DEFINER SET search_path = public, pg_temp AS $$
    SELECT EXISTS (
        SELECT 1 FROM courses c
        WHERE c.id = target AND c.visibility = 'public' AND c.published
    )
$$;

-- Writes the grading secret only after re-checking course management. The app
-- role can call this but never SELECT challenge_answers directly.
CREATE FUNCTION add_challenge_answer(target UUID, correct SMALLINT, explanation TEXT)
RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path = public, pg_temp AS $$
DECLARE
    target_course UUID;
BEGIN
    SELECT course_id INTO target_course FROM challenges WHERE id = target;
    IF target_course IS NULL OR NOT can_manage_course(target_course) THEN
        RAISE EXCEPTION 'not authorized' USING ERRCODE = '42501';
    END IF;
    IF correct < 0 OR correct > 5 THEN
        RAISE EXCEPTION 'invalid correct index' USING ERRCODE = '22023';
    END IF;
    INSERT INTO challenge_answers (challenge_id, correct_index, explanation)
    VALUES (target, correct, explanation);
END
$$;

ALTER TABLE tenants ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_read ON tenants FOR SELECT USING (owner_id = app_actor() OR is_tenant_member(id));
CREATE POLICY tenant_insert ON tenants FOR INSERT WITH CHECK (owner_id = app_actor());
CREATE POLICY tenant_update ON tenants FOR UPDATE USING (owner_id = app_actor());

ALTER TABLE tenant_members ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_member_read ON tenant_members FOR SELECT USING (is_tenant_member(tenant_id));
CREATE POLICY tenant_member_insert ON tenant_members FOR INSERT WITH CHECK (is_tenant_owner(tenant_id));
CREATE POLICY tenant_member_update ON tenant_members FOR UPDATE USING (is_tenant_owner(tenant_id));
CREATE POLICY tenant_member_delete ON tenant_members FOR DELETE USING (is_tenant_owner(tenant_id));

ALTER TABLE courses ENABLE ROW LEVEL SECURITY;
-- The direct creator predicate is also what makes INSERT ... RETURNING work:
-- a STABLE policy function cannot see the row being inserted in its snapshot.
CREATE POLICY course_read ON courses FOR SELECT USING (
    created_by = app_actor() OR can_read_course(id)
);
CREATE POLICY course_insert ON courses FOR INSERT WITH CHECK (created_by = app_actor() AND is_tenant_owner(tenant_id));
CREATE POLICY course_update ON courses FOR UPDATE USING (can_manage_course(id));

ALTER TABLE course_members ENABLE ROW LEVEL SECURITY;
CREATE POLICY course_member_read ON course_members FOR SELECT USING (user_id = app_actor() OR can_manage_course(course_id));
CREATE POLICY course_member_insert ON course_members FOR INSERT WITH CHECK (
    (can_manage_course(course_id) AND role IN ('instructor', 'student'))
    OR (user_id = app_actor() AND role = 'student' AND can_self_enroll(course_id))
);
CREATE POLICY course_member_update ON course_members FOR UPDATE USING (can_manage_course(course_id));
CREATE POLICY course_member_self_update ON course_members FOR UPDATE
    USING (user_id = app_actor() AND can_self_enroll(course_id))
    WITH CHECK (user_id = app_actor() AND role = 'student');
CREATE POLICY course_member_delete ON course_members FOR DELETE USING (user_id = app_actor() OR can_manage_course(course_id));

ALTER TABLE learning_nodes ENABLE ROW LEVEL SECURITY;
CREATE POLICY node_read ON learning_nodes FOR SELECT USING (can_read_course(course_id));
CREATE POLICY node_insert ON learning_nodes FOR INSERT WITH CHECK (can_manage_course(course_id));

ALTER TABLE challenges ENABLE ROW LEVEL SECURITY;
CREATE POLICY challenge_read ON challenges FOR SELECT USING (can_read_course(course_id));
CREATE POLICY challenge_insert ON challenges FOR INSERT WITH CHECK (can_manage_course(course_id));

ALTER TABLE challenge_answers ENABLE ROW LEVEL SECURITY;
ALTER TABLE challenge_answers FORCE ROW LEVEL SECURITY;
CREATE POLICY answer_grader ON challenge_answers FOR SELECT USING (current_user = 'campuslingo_grader');

ALTER TABLE assets ENABLE ROW LEVEL SECURITY;
CREATE POLICY asset_read ON assets FOR SELECT USING (can_read_course(course_id));
CREATE POLICY asset_insert ON assets FOR INSERT WITH CHECK (created_by = app_actor() AND can_manage_course(course_id));

ALTER TABLE attempts ENABLE ROW LEVEL SECURITY;
CREATE POLICY attempt_owner ON attempts FOR ALL
    USING (user_id = app_actor() AND can_study_course(course_id))
    WITH CHECK (user_id = app_actor() AND can_study_course(course_id));

ALTER TABLE fsrs_cards ENABLE ROW LEVEL SECURITY;
CREATE POLICY card_owner ON fsrs_cards FOR ALL
    USING (user_id = app_actor() AND can_study_course(course_id))
    WITH CHECK (user_id = app_actor() AND can_study_course(course_id));

ALTER TABLE review_logs ENABLE ROW LEVEL SECURITY;
CREATE POLICY review_owner ON review_logs FOR SELECT USING (user_id = app_actor() AND can_study_course(course_id));
CREATE POLICY review_append ON review_logs FOR INSERT WITH CHECK (user_id = app_actor() AND can_study_course(course_id));

ALTER TABLE reward_ledger ENABLE ROW LEVEL SECURITY;
CREATE POLICY reward_owner ON reward_ledger FOR SELECT USING (user_id = app_actor() AND can_study_course(course_id));
CREATE POLICY reward_append ON reward_ledger FOR INSERT WITH CHECK (user_id = app_actor() AND can_study_course(course_id));

ALTER TABLE jobs ENABLE ROW LEVEL SECURITY;
CREATE POLICY job_read ON jobs FOR SELECT USING (can_manage_course(course_id));
CREATE POLICY job_create ON jobs FOR INSERT WITH CHECK (created_by = app_actor() AND can_manage_course(course_id));

ALTER TABLE job_results ENABLE ROW LEVEL SECURITY;
CREATE POLICY result_read ON job_results FOR SELECT USING (
    EXISTS (SELECT 1 FROM jobs j WHERE j.id = job_id)
);

ALTER TABLE job_events ENABLE ROW LEVEL SECURITY;
CREATE POLICY event_read ON job_events FOR SELECT USING (
    EXISTS (SELECT 1 FROM jobs j WHERE j.id = job_id)
);
