-- Milestone M6: Asynchronous Snapshot Raids & Revenge, Append-Only Economy & Cosmetics,
-- 30-Player Weekly Leagues, Solo/Co-op Shared-HP Bosses, Weekly Clan Wars, and S-18 Alerts.

-- S-10: Deterministic Monday 04:00 user-local week start calculation (DEFAULT_TZ = 'Asia/Kuala_Lumpur').
CREATE OR REPLACE FUNCTION user_local_week_start(p_ts TIMESTAMPTZ, p_tz TEXT DEFAULT 'Asia/Kuala_Lumpur')
RETURNS DATE
LANGUAGE SQL
IMMUTABLE
STRICT
AS $$
    SELECT date_trunc(
        'week',
        ((p_ts AT TIME ZONE COALESCE(NULLIF(p_tz, ''), 'Asia/Kuala_Lumpur')) - INTERVAL '4 hours')
    )::date;
$$;

-- F-01: Explicit opt-in flag on course membership for private course raids.
ALTER TABLE courses
    ADD COLUMN IF NOT EXISTS raid_opt_in_default BOOLEAN NOT NULL DEFAULT TRUE;

ALTER TABLE course_members
    ADD COLUMN IF NOT EXISTS raid_opt_in BOOLEAN NOT NULL DEFAULT FALSE;

-- S-09: Expand review_logs scope_kind and interaction_source enums.
ALTER TABLE review_logs
    DROP CONSTRAINT IF EXISTS review_logs_scope_kind_check;

ALTER TABLE review_logs
    ADD CONSTRAINT review_logs_scope_kind_check CHECK (
        scope_kind IN ('MATCH', 'STUDY_SESSION', 'RAID', 'BOSS', 'LEAGUE', 'CLAN_WAR')
    );

ALTER TABLE review_logs
    DROP CONSTRAINT IF EXISTS review_logs_interaction_source_check;

ALTER TABLE review_logs
    ADD CONSTRAINT review_logs_interaction_source_check CHECK (
        interaction_source IN (
            'MATCH',
            'STUDY_SESSION',
            'SPEED_DRILL',
            'PRACTICE',
            'TUTORIAL_SANDBOX',
            'SPECTATOR',
            'LIVE_DUEL',
            'GHOST_DUEL',
            'BOT_DUEL',
            'ASYNC_RAID',
            'REVENGE_RAID',
            'SOLO_BOSS',
            'COOP_BOSS',
            'LEAGUE',
            'CLAN_WAR',
            'SOLO_RAID',
            'PVP_DUEL',
            'ASYNC_BASE_RAID',
            'REVENGE_DEFENSE'
        )
    );

CREATE OR REPLACE FUNCTION enforce_review_exposure_and_caps()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    v_tz TEXT;
    v_course_id UUID;
    v_quota_date DATE;
    v_prev_ts TIMESTAMPTZ;
    v_card_count INTEGER;
    v_course_count INTEGER;
BEGIN
    NEW.scope_id := COALESCE(NEW.scope_id, NEW.attempt_id);

    SELECT COALESCE(timezone, 'UTC')
    INTO v_tz
    FROM users
    WHERE id = NEW.user_id;

    IF NEW.course_id IS NULL THEN
        SELECT course_id
        INTO v_course_id
        FROM challenges
        WHERE id = NEW.challenge_id;
        NEW.course_id := v_course_id;
    END IF;

    v_quota_date := user_local_quota_date(COALESCE(NEW.reviewed_at, now()), COALESCE(v_tz, 'UTC'));
    NEW.quota_date := v_quota_date;

    IF NEW.interaction_source IN ('SPECTATOR', 'TUTORIAL_SANDBOX') THEN
        NEW.fsrs_step_eligible := FALSE;
        NEW.reward_eligible := FALSE;
        NEW.score_points := 0;
        NEW.eligibility_blocker := COALESCE(NEW.eligibility_blocker, 'NON_FSRS_MODE');
        RETURN NEW;
    END IF;

    IF NEW.is_void THEN
        NEW.fsrs_step_eligible := FALSE;
        NEW.reward_eligible := FALSE;
        NEW.score_points := 0;
        NEW.eligibility_blocker := COALESCE(NEW.eligibility_blocker, 'VOID_ATTEMPT');
        RETURN NEW;
    END IF;

    -- 30-minute cross-session same-day short-term step guard (SAME_DAY_MIN_GAP)
    SELECT MAX(reviewed_at)
    INTO v_prev_ts
    FROM review_logs
    WHERE user_id = NEW.user_id
      AND challenge_id = NEW.challenge_id
      AND quota_date = v_quota_date
      AND is_void = FALSE
      AND fsrs_step_eligible = TRUE
      AND reviewed_at <= COALESCE(NEW.reviewed_at, now());

    IF v_prev_ts IS NOT NULL
       AND EXTRACT(EPOCH FROM (COALESCE(NEW.reviewed_at, now()) - v_prev_ts)) < 1800 THEN
        NEW.fsrs_step_eligible := FALSE;
        NEW.reward_eligible := FALSE;
        NEW.score_points := 0;
        NEW.eligibility_blocker := 'SAME_DAY_MIN_GAP';
        RETURN NEW;
    END IF;

    IF NEW.interaction_source = 'PRACTICE' THEN
        NEW.reward_eligible := FALSE;
        NEW.score_points := 0;
        NEW.eligibility_blocker := COALESCE(NEW.eligibility_blocker, 'PRACTICE_ONLY_MODE');
        RETURN NEW;
    END IF;

    -- Daily reward caps at local 04:00 rollover: < 3 per card, < 60 per course
    SELECT COUNT(*)::INTEGER
    INTO v_card_count
    FROM review_logs
    WHERE user_id = NEW.user_id
      AND challenge_id = NEW.challenge_id
      AND quota_date = v_quota_date
      AND reward_eligible = TRUE
      AND is_void = FALSE;

    IF v_card_count >= 3 THEN
        NEW.reward_eligible := FALSE;
        NEW.score_points := 0;
        NEW.eligibility_blocker := 'CARD_DAILY_CAP';
        RETURN NEW;
    END IF;

    SELECT COUNT(*)::INTEGER
    INTO v_course_count
    FROM review_logs
    WHERE user_id = NEW.user_id
      AND course_id = NEW.course_id
      AND quota_date = v_quota_date
      AND reward_eligible = TRUE
      AND is_void = FALSE;

    IF v_course_count >= 60 THEN
        NEW.reward_eligible := FALSE;
        NEW.score_points := 0;
        NEW.eligibility_blocker := 'COURSE_DAILY_CAP';
        RETURN NEW;
    END IF;

    RETURN NEW;
END;
$$;

-- S-16: Append-only reward_transactions ledger & atomic balances & cosmetics
CREATE TABLE IF NOT EXISTS reward_transactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tenant_id UUID NOT NULL,
    course_id UUID NOT NULL,
    source_kind TEXT NOT NULL CHECK (
        source_kind IN (
            'ASYNC_RAID', 'REVENGE_RAID', 'RAID_PRACTICE', 'RAID_DEFENSE_LOSS',
            'SOLO_BOSS', 'COOP_BOSS', 'LEAGUE', 'CLAN_WAR', 'VOID_REFUND', 'COSMETIC_UNLOCK'
        )
    ),
    source_id UUID,
    settlement_key TEXT NOT NULL,
    gold_delta INTEGER NOT NULL DEFAULT 0,
    trophies_delta INTEGER NOT NULL DEFAULT 0,
    practice_xp_delta INTEGER NOT NULL DEFAULT 0,
    gems_delta INTEGER NOT NULL DEFAULT 0,
    chest_tier TEXT,
    quota_date DATE NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (user_id, source_kind, settlement_key),
    FOREIGN KEY (course_id, tenant_id) REFERENCES courses(id, tenant_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_reward_transactions_user_course_day
    ON reward_transactions (user_id, course_id, quota_date);

DROP TRIGGER IF EXISTS immutable_reward_transactions ON reward_transactions;
CREATE TRIGGER immutable_reward_transactions
    BEFORE UPDATE OR DELETE ON reward_transactions
    FOR EACH ROW EXECUTE FUNCTION immutable_audit();

CREATE TABLE IF NOT EXISTS user_economy_balances (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    course_id UUID NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    gold INTEGER NOT NULL DEFAULT 0 CHECK (gold >= 0),
    trophies INTEGER NOT NULL DEFAULT 0 CHECK (trophies >= 0),
    gems INTEGER NOT NULL DEFAULT 0 CHECK (gems >= 0),
    practice_xp INTEGER NOT NULL DEFAULT 0 CHECK (practice_xp >= 0),
    daily_practice_xp INTEGER NOT NULL DEFAULT 0 CHECK (daily_practice_xp >= 0 AND daily_practice_xp <= 100),
    daily_practice_xp_date DATE NOT NULL DEFAULT current_date,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, course_id)
);

CREATE TABLE IF NOT EXISTS user_cosmetic_inventory (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    course_id UUID NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    cosmetic_key TEXT NOT NULL CHECK (length(cosmetic_key) BETWEEN 1 AND 80),
    rarity TEXT NOT NULL CHECK (rarity IN ('COMMON', 'RARE', 'APEX')),
    source_kind TEXT NOT NULL,
    settlement_key TEXT NOT NULL,
    acquired_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (user_id, course_id, cosmetic_key)
);

DROP TRIGGER IF EXISTS immutable_user_cosmetic_inventory ON user_cosmetic_inventory;
CREATE TRIGGER immutable_user_cosmetic_inventory
    BEFORE UPDATE OR DELETE ON user_cosmetic_inventory
    FOR EACH ROW EXECUTE FUNCTION immutable_audit();

-- F-01 / F-02 / D-03: Snapshot raid sessions
CREATE TABLE IF NOT EXISTS raid_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    course_id UUID NOT NULL,
    attacker_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    defender_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    defender_node_id UUID NOT NULL,
    is_revenge BOOLEAN NOT NULL DEFAULT FALSE,
    revenge_source_raid_id UUID REFERENCES raid_sessions(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'IN_PROGRESS' CHECK (
        status IN (
            'IN_PROGRESS',
            'SETTLED_WIN',
            'SETTLED_LOSS',
            'SETTLED_COOLDOWN_BLOCKED',
            'EXPIRED'
        )
    ),
    pair_user_low UUID GENERATED ALWAYS AS (LEAST(attacker_user_id, defender_user_id)) STORED,
    pair_user_high UUID GENERATED ALWAYS AS (GREATEST(attacker_user_id, defender_user_id)) STORED,
    quota_date DATE NOT NULL,
    defender_snapshot_json JSONB NOT NULL CHECK (octet_length(defender_snapshot_json::text) <= 16384),
    attacker_trophy_gain INTEGER NOT NULL DEFAULT 0 CHECK (attacker_trophy_gain >= 0),
    defender_trophy_loss INTEGER NOT NULL DEFAULT 0 CHECK (defender_trophy_loss >= 0),
    practice_xp_awarded INTEGER NOT NULL DEFAULT 0 CHECK (practice_xp_awarded >= 0),
    gold_awarded INTEGER NOT NULL DEFAULT 0 CHECK (gold_awarded >= 0),
    loot_chest_tier TEXT,
    consumes_defender_inbound_cap BOOLEAN NOT NULL DEFAULT FALSE,
    revenge_window_expires_at TIMESTAMPTZ,
    revenge_consumed BOOLEAN NOT NULL DEFAULT FALSE,
    settlement_key TEXT NOT NULL UNIQUE,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    stage2_started_at TIMESTAMPTZ,
    settled_at TIMESTAMPTZ,
    CHECK (attacker_user_id <> defender_user_id),
    FOREIGN KEY (course_id, tenant_id) REFERENCES courses(id, tenant_id) ON DELETE CASCADE,
    FOREIGN KEY (defender_node_id, tenant_id, course_id) REFERENCES learning_nodes(id, tenant_id, course_id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_raid_revenge_single_use
    ON raid_sessions (revenge_source_raid_id)
    WHERE revenge_source_raid_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_raid_outbound_day
    ON raid_sessions (attacker_user_id, course_id, quota_date);

CREATE INDEX IF NOT EXISTS idx_raid_inbound_day
    ON raid_sessions (defender_user_id, course_id, quota_date, status);

CREATE INDEX IF NOT EXISTS idx_raid_pair_day
    ON raid_sessions (course_id, quota_date, pair_user_low, pair_user_high);

-- S-18 / F-05: Aggregated notification delivery ledger (MAX_PUSH_NOTIFICATIONS_PER_DAY = 2, 22:00-08:00 quiet hours)
CREATE TABLE IF NOT EXISTS notification_delivery_ledger (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    course_id UUID NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    notification_kind TEXT NOT NULL CHECK (notification_kind IN ('RAID_ALERT', 'DAILY_DIGEST')),
    local_quota_date DATE NOT NULL,
    raid_count INTEGER NOT NULL DEFAULT 1 CHECK (raid_count >= 1),
    summary_text TEXT NOT NULL CHECK (length(summary_text) BETWEEN 1 AND 320),
    payload_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    deliver_after TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, notification_kind, local_quota_date)
);

-- S-10 / §11.3: Weekly 30-player league brackets & standings
CREATE TABLE IF NOT EXISTS league_brackets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    league_scope TEXT NOT NULL,
    course_id UUID NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    week_key TEXT NOT NULL CHECK (length(week_key) BETWEEN 4 AND 32),
    week_start_date DATE NOT NULL,
    tier SMALLINT NOT NULL CHECK (tier BETWEEN 1 AND 10),
    late_joiner_shielded BOOLEAN NOT NULL DEFAULT FALSE,
    rolled_over BOOLEAN NOT NULL DEFAULT FALSE,
    rolled_over_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (id, league_scope, week_key)
);

CREATE INDEX IF NOT EXISTS idx_league_brackets_lookup
    ON league_brackets (league_scope, week_key, tier, rolled_over);

CREATE TABLE IF NOT EXISTS league_standings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    bracket_id UUID NOT NULL,
    league_scope TEXT NOT NULL,
    course_id UUID NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    week_key TEXT NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    timezone TEXT NOT NULL DEFAULT 'Asia/Kuala_Lumpur',
    tier_before SMALLINT NOT NULL CHECK (tier_before BETWEEN 1 AND 10),
    tier_after SMALLINT CHECK (tier_after BETWEEN 1 AND 10),
    trophies_earned INTEGER NOT NULL DEFAULT 0 CHECK (trophies_earned >= 0),
    eligible_mastery_recovery_count INTEGER NOT NULL DEFAULT 0 CHECK (eligible_mastery_recovery_count >= 0),
    last_trophy_earned_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    is_dormant BOOLEAN NOT NULL DEFAULT FALSE,
    returned_from_dormancy_this_week BOOLEAN NOT NULL DEFAULT FALSE,
    qualifying_sessions_since_return INTEGER NOT NULL DEFAULT 0 CHECK (qualifying_sessions_since_return >= 0),
    consecutive_dormant_weeks SMALLINT NOT NULL DEFAULT 0 CHECK (consecutive_dormant_weeks >= 0),
    league_status TEXT NOT NULL DEFAULT 'ACTIVE' CHECK (league_status IN ('ACTIVE', 'DORMANT')),
    rollover_outcome TEXT CHECK (
        rollover_outcome IN ('PROMOTED', 'HELD', 'DEMOTED', 'DORMANT_PRESERVED', 'DORMANT_DEMOTED')
    ),
    final_rank INTEGER CHECK (final_rank IS NULL OR final_rank >= 1),
    UNIQUE (league_scope, week_key, user_id),
    FOREIGN KEY (bracket_id, league_scope, week_key)
        REFERENCES league_brackets(id, league_scope, week_key) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_league_standings_bracket
    ON league_standings (bracket_id, trophies_earned DESC, eligible_mastery_recovery_count DESC, last_trophy_earned_at ASC);

-- S-17 / §11.4: Solo / Async Co-op / Live Lobby Shared-HP Bosses
CREATE TABLE IF NOT EXISTS chapter_bosses (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    course_id UUID NOT NULL,
    node_id UUID NOT NULL,
    title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 160),
    mode TEXT NOT NULL CHECK (mode IN ('SOLO', 'ASYNC_COOP', 'LIVE_COOP')),
    max_hp INTEGER NOT NULL CHECK (max_hp BETWEEN 100 AND 1000000),
    current_hp INTEGER NOT NULL CHECK (current_hp >= 0 AND current_hp <= max_hp),
    reward_tier TEXT NOT NULL DEFAULT 'BOSS_CHEST_APEX',
    gold_reward INTEGER NOT NULL DEFAULT 150 CHECK (gold_reward >= 0),
    cosmetic_reward_key TEXT,
    status TEXT NOT NULL DEFAULT 'ACTIVE' CHECK (status IN ('ACTIVE', 'DEFEATED', 'EXPIRED')),
    expires_at TIMESTAMPTZ NOT NULL DEFAULT (now() + INTERVAL '24 hours'),
    defeated_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (id, course_id),
    FOREIGN KEY (course_id, tenant_id) REFERENCES courses(id, tenant_id) ON DELETE CASCADE,
    FOREIGN KEY (node_id, tenant_id, course_id) REFERENCES learning_nodes(id, tenant_id, course_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS boss_lobbies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    boss_id UUID NOT NULL,
    course_id UUID NOT NULL,
    host_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    member_user_ids UUID[] NOT NULL DEFAULT ARRAY[]::UUID[],
    max_players SMALLINT NOT NULL DEFAULT 4 CHECK (max_players BETWEEN 2 AND 4),
    status TEXT NOT NULL DEFAULT 'OPEN' CHECK (status IN ('OPEN', 'IN_PROGRESS', 'COMPLETED')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (boss_id, course_id) REFERENCES chapter_bosses(id, course_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS boss_contributions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    boss_id UUID NOT NULL,
    course_id UUID NOT NULL,
    lobby_id UUID REFERENCES boss_lobbies(id) ON DELETE SET NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL CHECK (sequence >= 1),
    rating SMALLINT NOT NULL CHECK (rating BETWEEN 1 AND 4),
    overcharged BOOLEAN NOT NULL DEFAULT FALSE,
    raw_damage INTEGER NOT NULL CHECK (raw_damage >= 0),
    applied_damage INTEGER NOT NULL CHECK (applied_damage >= 0),
    overkill_damage INTEGER NOT NULL CHECK (overkill_damage >= 0),
    settlement_key TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (boss_id, user_id, sequence),
    FOREIGN KEY (boss_id, course_id) REFERENCES chapter_bosses(id, course_id) ON DELETE CASCADE
);

DROP TRIGGER IF EXISTS immutable_boss_contributions ON boss_contributions;
CREATE TRIGGER immutable_boss_contributions
    BEFORE UPDATE OR DELETE ON boss_contributions
    FOR EACH ROW EXECUTE FUNCTION immutable_audit();

CREATE TABLE IF NOT EXISTS boss_chest_claims (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    boss_instance_id UUID NOT NULL,
    course_id UUID NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    reward_tier TEXT NOT NULL CHECK (length(reward_tier) BETWEEN 1 AND 64),
    gold_awarded INTEGER NOT NULL DEFAULT 0 CHECK (gold_awarded >= 0),
    cosmetic_key TEXT,
    settlement_key TEXT NOT NULL UNIQUE,
    claimed_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (user_id, boss_instance_id, reward_tier),
    FOREIGN KEY (boss_instance_id, course_id) REFERENCES chapter_bosses(id, course_id) ON DELETE CASCADE
);

DROP TRIGGER IF EXISTS immutable_boss_chest_claims ON boss_chest_claims;
CREATE TRIGGER immutable_boss_chest_claims
    BEFORE UPDATE OR DELETE ON boss_chest_claims
    FOR EACH ROW EXECUTE FUNCTION immutable_audit();

-- §11.4: Weekly Clan Wars with immutable week-start active_members_snapshot
CREATE TABLE IF NOT EXISTS clans (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    course_id UUID NOT NULL,
    name TEXT NOT NULL CHECK (length(name) BETWEEN 2 AND 80),
    created_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (id, course_id),
    UNIQUE (course_id, name),
    FOREIGN KEY (course_id, tenant_id) REFERENCES courses(id, tenant_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS clan_memberships (
    clan_id UUID NOT NULL,
    course_id UUID NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'MEMBER' CHECK (role IN ('LEADER', 'MEMBER')),
    joined_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (clan_id, user_id),
    UNIQUE (course_id, user_id),
    FOREIGN KEY (clan_id, course_id) REFERENCES clans(id, course_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS clan_wars (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    course_id UUID NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    clan_id UUID NOT NULL,
    week_key TEXT NOT NULL CHECK (length(week_key) BETWEEN 4 AND 32),
    week_start_date DATE NOT NULL,
    war_roster_member_ids UUID[] NOT NULL,
    active_members_snapshot INTEGER NOT NULL CHECK (active_members_snapshot >= 0),
    total_raw_points BIGINT NOT NULL DEFAULT 0 CHECK (total_raw_points >= 0),
    total_normalized_points BIGINT NOT NULL DEFAULT 0 CHECK (total_normalized_points >= 0),
    status TEXT NOT NULL DEFAULT 'ACTIVE' CHECK (status IN ('ACTIVE', 'COMPLETED')),
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (clan_id, week_key),
    FOREIGN KEY (clan_id, course_id) REFERENCES clans(id, course_id) ON DELETE CASCADE
);

CREATE OR REPLACE FUNCTION protect_clan_war_snapshot()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.war_roster_member_ids IS DISTINCT FROM OLD.war_roster_member_ids
       OR NEW.active_members_snapshot IS DISTINCT FROM OLD.active_members_snapshot THEN
        RAISE EXCEPTION 'clan war week-start roster snapshot is immutable';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_clan_war_snapshot_immutable ON clan_wars;
CREATE TRIGGER trg_clan_war_snapshot_immutable
    BEFORE UPDATE ON clan_wars
    FOR EACH ROW EXECUTE FUNCTION protect_clan_war_snapshot();

CREATE TABLE IF NOT EXISTS clan_war_contributions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    war_id UUID NOT NULL REFERENCES clan_wars(id) ON DELETE CASCADE,
    clan_id UUID NOT NULL,
    course_id UUID NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    contribution_kind TEXT NOT NULL CHECK (contribution_kind IN ('STABILITY', 'RAID_WIN', 'REVENGE_WIN')),
    raw_points INTEGER NOT NULL CHECK (raw_points >= 0),
    normalized_points INTEGER NOT NULL CHECK (normalized_points >= 0),
    relative_gain_bps INTEGER NOT NULL DEFAULT 0 CHECK (relative_gain_bps >= 0),
    settlement_key TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    FOREIGN KEY (clan_id, course_id) REFERENCES clans(id, course_id) ON DELETE CASCADE
);

DROP TRIGGER IF EXISTS immutable_clan_war_contributions ON clan_war_contributions;
CREATE TRIGGER immutable_clan_war_contributions
    BEFORE UPDATE OR DELETE ON clan_war_contributions
    FOR EACH ROW EXECUTE FUNCTION immutable_audit();

-- Security-definer M6 helper procedures preserving strict role isolation & RLS.
CREATE OR REPLACE FUNCTION set_course_raid_opt_in_secure(p_course_id UUID, p_opt_in BOOLEAN)
RETURNS BOOLEAN
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    IF app_actor() IS NULL OR NOT can_study_course(p_course_id) THEN
        RAISE EXCEPTION 'not authorized' USING ERRCODE = '42501';
    END IF;

    UPDATE course_members
    SET raid_opt_in = p_opt_in
    WHERE course_id = p_course_id AND user_id = app_actor();

    RETURN p_opt_in;
END;
$$;

CREATE OR REPLACE FUNCTION list_raid_candidates_secure(p_course_id UUID, p_now TIMESTAMPTZ)
RETURNS TABLE(
    defender_user_id UUID,
    defender_node_id UUID,
    node_title TEXT,
    node_tier SMALLINT,
    e_u_ticks INTEGER,
    defender_raid_opt_in BOOLEAN,
    raid_immune BOOLEAN,
    immunity_reason TEXT,
    has_revenge_window BOOLEAN,
    revenge_source_raid_id UUID,
    revenge_expires_at TIMESTAMPTZ
)
LANGUAGE plpgsql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    v_actor UUID := app_actor();
    v_is_private BOOLEAN;
BEGIN
    IF v_actor IS NULL OR NOT can_study_course(p_course_id) THEN
        RAISE EXCEPTION 'not authorized' USING ERRCODE = '42501';
    END IF;

    SELECT (c.visibility = 'private') INTO v_is_private
    FROM courses c WHERE c.id = p_course_id;

    -- F-04 allowlist: exposes only target user, node title/tier, public defense band & immunity;
    -- NEVER exposes personal stability, reps, due dates, or review logs.
    RETURN QUERY
    SELECT
        cm.user_id,
        ln.id,
        ln.title,
        COALESCE(sh.tier, 0)::SMALLINT,
        COALESCE(sh.e_u_ticks, 0)::INTEGER,
        cm.raid_opt_in,
        (
            COALESCE(sh.state, 'INACTIVE') <> 'ACTIVE'
            OR COALESCE(sh.tier, 0) = 0
            OR COALESCE(ups.is_dormant, FALSE)
            OR (ups.resume_grace_until IS NOT NULL AND p_now < ups.resume_grace_until)
            OR (p_now < ups.new_player_bubble_until AND ups.eligible_transition_count < 20)
            OR EXISTS (
                SELECT 1 FROM user_node_protection unp
                WHERE unp.user_id = cm.user_id
                  AND unp.node_id = ln.id
                  AND p_now < unp.protected_until
            )
            OR (v_is_private AND NOT cm.raid_opt_in)
        ) AS raid_immune,
        CASE
            WHEN v_is_private AND NOT cm.raid_opt_in THEN 'PRIVATE_COURSE_OPT_IN_REQUIRED'
            WHEN COALESCE(sh.state, 'INACTIVE') <> 'ACTIVE' OR COALESCE(sh.tier, 0) = 0 THEN 'NODE_INACTIVE_OR_TIER0'
            WHEN COALESCE(ups.is_dormant, FALSE) THEN 'DORMANT'
            WHEN ups.resume_grace_until IS NOT NULL AND p_now < ups.resume_grace_until THEN 'RESUME_GRACE'
            WHEN p_now < ups.new_player_bubble_until AND ups.eligible_transition_count < 20 THEN 'NEW_PLAYER_BUBBLE'
            WHEN EXISTS (
                SELECT 1 FROM user_node_protection unp
                WHERE unp.user_id = cm.user_id AND unp.node_id = ln.id AND unp.kind = 'POST_RAID_COOLDOWN' AND p_now < unp.protected_until
            ) THEN 'POST_RAID_COOLDOWN'
            WHEN EXISTS (
                SELECT 1 FROM user_node_protection unp
                WHERE unp.user_id = cm.user_id AND unp.node_id = ln.id AND p_now < unp.protected_until
            ) THEN 'GRACE_ACTIVE'
            ELSE NULL
        END AS immunity_reason,
        (rev.id IS NOT NULL) AS has_revenge_window,
        rev.id AS revenge_source_raid_id,
        rev.revenge_window_expires_at
    FROM course_members cm
    JOIN learning_nodes ln ON ln.course_id = cm.course_id
    LEFT JOIN user_node_shield_state sh
        ON sh.user_id = cm.user_id AND sh.node_id = ln.id
    LEFT JOIN user_protection_state ups
        ON ups.user_id = cm.user_id
    LEFT JOIN LATERAL (
        SELECT rs.id, rs.revenge_window_expires_at
        FROM raid_sessions rs
        WHERE rs.course_id = p_course_id
          AND rs.attacker_user_id = cm.user_id
          AND rs.defender_user_id = v_actor
          AND rs.status = 'SETTLED_WIN'
          AND rs.revenge_consumed = FALSE
          AND rs.revenge_window_expires_at IS NOT NULL
          AND p_now <= rs.revenge_window_expires_at
        ORDER BY rs.settled_at DESC
        LIMIT 1
    ) rev ON TRUE
    WHERE cm.course_id = p_course_id
      AND cm.active = TRUE
      AND cm.user_id <> v_actor;
END;
$$;

CREATE OR REPLACE FUNCTION start_raid_session_secure(
    p_raid_id UUID,
    p_course_id UUID,
    p_defender_user_id UUID,
    p_defender_node_id UUID,
    p_is_revenge BOOLEAN,
    p_revenge_source_raid_id UUID,
    p_settlement_key TEXT,
    p_now TIMESTAMPTZ
)
RETURNS TABLE(
    raid_id UUID,
    is_revenge BOOLEAN,
    quota_date DATE,
    defender_snapshot_json JSONB
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
    v_actor UUID := app_actor();
    v_tenant_id UUID;
    v_is_private BOOLEAN;
    v_att_opt_in BOOLEAN;
    v_def_opt_in BOOLEAN;
    v_tz TEXT;
    v_quota_date DATE;
    v_out_count INTEGER;
    v_in_count INTEGER;
    v_pair_count INTEGER;
    v_node_state TEXT;
    v_node_tier SMALLINT;
    v_def_dormant BOOLEAN;
    v_def_resume_grace TIMESTAMPTZ;
    v_def_bubble TIMESTAMPTZ;
    v_def_trans INTEGER;
    v_prot_active BOOLEAN;
    v_snap JSONB;
BEGIN
    IF v_actor IS NULL OR NOT can_study_course(p_course_id) THEN
        RAISE EXCEPTION 'not authorized' USING ERRCODE = '42501';
    END IF;
    IF v_actor = p_defender_user_id THEN
        RAISE EXCEPTION 'self raid forbidden' USING ERRCODE = '42501';
    END IF;

    SELECT c.tenant_id, (c.visibility = 'private')
    INTO v_tenant_id, v_is_private
    FROM courses c WHERE c.id = p_course_id;

    SELECT cm.raid_opt_in INTO v_att_opt_in
    FROM course_members cm WHERE cm.course_id = p_course_id AND cm.user_id = v_actor AND cm.active = TRUE;

    SELECT cm.raid_opt_in INTO v_def_opt_in
    FROM course_members cm WHERE cm.course_id = p_course_id AND cm.user_id = p_defender_user_id AND cm.active = TRUE;

    IF v_att_opt_in IS NULL OR v_def_opt_in IS NULL THEN
        RAISE EXCEPTION 'not course member' USING ERRCODE = '42501';
    END IF;
    IF v_is_private AND (NOT v_att_opt_in OR NOT v_def_opt_in) THEN
        RAISE EXCEPTION 'PRIVATE_COURSE_OPT_IN_REQUIRED' USING ERRCODE = '42501';
    END IF;

    SELECT COALESCE(timezone, 'UTC') INTO v_tz FROM users WHERE id = v_actor;
    v_quota_date := user_local_quota_date(p_now, COALESCE(v_tz, 'UTC'));

    SELECT COUNT(*)::INTEGER INTO v_out_count
    FROM raid_sessions
    WHERE attacker_user_id = v_actor AND course_id = p_course_id AND quota_date = v_quota_date;
    IF v_out_count >= 10 THEN
        RAISE EXCEPTION 'OUTBOUND_DAILY_CAP_REACHED' USING ERRCODE = '23514';
    END IF;

    SELECT COUNT(*)::INTEGER INTO v_pair_count
    FROM raid_sessions
    WHERE course_id = p_course_id
      AND quota_date = v_quota_date
      AND pair_user_low = LEAST(v_actor, p_defender_user_id)
      AND pair_user_high = GREATEST(v_actor, p_defender_user_id);
    IF v_pair_count >= 1 THEN
        RAISE EXCEPTION 'UNORDERED_PAIR_DAILY_CAP_REACHED' USING ERRCODE = '23514';
    END IF;

    SELECT COUNT(*)::INTEGER INTO v_in_count
    FROM raid_sessions
    WHERE defender_user_id = p_defender_user_id
      AND course_id = p_course_id
      AND quota_date = v_quota_date
      AND (status = 'IN_PROGRESS' OR consumes_defender_inbound_cap = TRUE);
    IF v_in_count >= 3 THEN
        RAISE EXCEPTION 'INBOUND_DAILY_CAP_REACHED' USING ERRCODE = '23514';
    END IF;

    IF p_is_revenge THEN
        IF p_revenge_source_raid_id IS NULL OR NOT EXISTS (
            SELECT 1 FROM raid_sessions sr
            WHERE sr.id = p_revenge_source_raid_id
              AND sr.course_id = p_course_id
              AND sr.attacker_user_id = p_defender_user_id
              AND sr.defender_user_id = v_actor
              AND sr.status = 'SETTLED_WIN'
              AND sr.revenge_consumed = FALSE
              AND sr.revenge_window_expires_at IS NOT NULL
              AND p_now <= sr.revenge_window_expires_at
        ) THEN
            RAISE EXCEPTION 'REVENGE_WINDOW_INVALID' USING ERRCODE = '23514';
        END IF;
    END IF;

    SELECT COALESCE(sh.state, 'INACTIVE'), sh.tier
    INTO v_node_state, v_node_tier
    FROM learning_nodes ln
    LEFT JOIN user_node_shield_state sh
      ON sh.user_id = p_defender_user_id AND sh.node_id = ln.id
    WHERE ln.id = p_defender_node_id AND ln.course_id = p_course_id;

    SELECT ups.is_dormant, ups.resume_grace_until, ups.new_player_bubble_until, ups.eligible_transition_count
    INTO v_def_dormant, v_def_resume_grace, v_def_bubble, v_def_trans
    FROM user_protection_state ups
    WHERE ups.user_id = p_defender_user_id;

    SELECT EXISTS (
        SELECT 1 FROM user_node_protection unp
        WHERE unp.user_id = p_defender_user_id
          AND unp.node_id = p_defender_node_id
          AND p_now < unp.protected_until
    ) INTO v_prot_active;

    IF COALESCE(v_node_state, 'INACTIVE') <> 'ACTIVE'
       OR COALESCE(v_node_tier, 0) = 0
       OR COALESCE(v_def_dormant, FALSE)
       OR (v_def_resume_grace IS NOT NULL AND p_now < v_def_resume_grace)
       OR (v_def_bubble IS NOT NULL AND p_now < v_def_bubble AND COALESCE(v_def_trans, 0) < 20)
       OR v_prot_active THEN
        RAISE EXCEPTION 'DEFENDER_NODE_RAID_IMMUNE' USING ERRCODE = '23514';
    END IF;

    SELECT jsonb_build_object(
        'schema_version', '1.0.0',
        'node_id', ln.id,
        'node_title', ln.title,
        'node_tier', v_node_tier,
        'challenge_count', (SELECT COUNT(*) FROM challenges ch WHERE ch.node_id = ln.id),
        'snapshot_taken_at', p_now
    )
    INTO v_snap
    FROM learning_nodes ln
    WHERE ln.id = p_defender_node_id;

    INSERT INTO raid_sessions (
        id, tenant_id, course_id, attacker_user_id, defender_user_id, defender_node_id,
        is_revenge, revenge_source_raid_id, status, quota_date, defender_snapshot_json,
        settlement_key, started_at, stage2_started_at
    )
    VALUES (
        p_raid_id, v_tenant_id, p_course_id, v_actor, p_defender_user_id, p_defender_node_id,
        p_is_revenge, p_revenge_source_raid_id, 'IN_PROGRESS', v_quota_date, v_snap,
        p_settlement_key, p_now, p_now
    );

    RETURN QUERY SELECT p_raid_id, p_is_revenge, v_quota_date, v_snap;
END;
$$;

CREATE OR REPLACE FUNCTION settle_raid_session_secure(
    p_raid_id UUID,
    p_course_id UUID,
    p_sandbox_solved BOOLEAN,
    p_drills_passed SMALLINT,
    p_base_score_points INTEGER,
    p_now TIMESTAMPTZ
)
RETURNS TABLE(
    status TEXT,
    concurrent_cooldown_blocked BOOLEAN,
    attacker_trophy_gain INTEGER,
    defender_trophy_loss INTEGER,
    practice_xp_awarded INTEGER,
    gold_awarded INTEGER,
    loot_chest_tier TEXT,
    consumes_defender_inbound_cap BOOLEAN
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
    v_actor UUID := app_actor();
    v_raid raid_sessions%ROWTYPE;
    v_elapsed_ms INTEGER;
    v_def_immune BOOLEAN;
    v_def_loss_today INTEGER;
    v_att_xp_today INTEGER;
    v_def_tz TEXT;
    v_def_quota DATE;
    v_quiet_deliver TIMESTAMPTZ;
    v_local_ts TIMESTAMP;
    v_stake INTEGER;
    v_att_gain INTEGER := 0;
    v_def_loss INTEGER := 0;
    v_xp INTEGER := 0;
    v_gold INTEGER := 0;
    v_chest TEXT := NULL;
    v_status TEXT;
    v_consume_in BOOLEAN := FALSE;
    v_revenge_exp TIMESTAMPTZ := NULL;
    v_blocked BOOLEAN := FALSE;
BEGIN
    IF v_actor IS NULL THEN
        RAISE EXCEPTION 'not authorized' USING ERRCODE = '42501';
    END IF;

    SELECT * INTO v_raid
    FROM raid_sessions
    WHERE id = p_raid_id AND course_id = p_course_id
    FOR UPDATE;

    IF NOT FOUND OR v_raid.attacker_user_id <> v_actor THEN
        RAISE EXCEPTION 'raid session not found or unauthorized' USING ERRCODE = '42501';
    END IF;

    IF v_raid.status <> 'IN_PROGRESS' THEN
        RETURN QUERY SELECT v_raid.status, (v_raid.status = 'SETTLED_COOLDOWN_BLOCKED'),
            v_raid.attacker_trophy_gain, v_raid.defender_trophy_loss,
            v_raid.practice_xp_awarded, v_raid.gold_awarded, v_raid.loot_chest_tier,
            v_raid.consumes_defender_inbound_cap;
        RETURN;
    END IF;

    -- Serialize concurrent settlements targeting the same defender in this course
    PERFORM pg_advisory_xact_lock(hashtext(v_raid.defender_user_id::text || ':' || p_course_id::text));

    v_elapsed_ms := GREATEST(0, (EXTRACT(EPOCH FROM (p_now - COALESCE(v_raid.stage2_started_at, v_raid.started_at))) * 1000)::INTEGER);

    SELECT COALESCE(ups.is_dormant, FALSE)
           OR (ups.resume_grace_until IS NOT NULL AND p_now < ups.resume_grace_until)
           OR EXISTS (
               SELECT 1 FROM user_node_protection unp
               WHERE unp.user_id = v_raid.defender_user_id
                 AND unp.node_id = v_raid.defender_node_id
                 AND p_now < unp.protected_until
           )
    INTO v_def_immune
    FROM (SELECT 1) dummy
    LEFT JOIN user_protection_state ups ON ups.user_id = v_raid.defender_user_id;

    SELECT COALESCE(SUM(rs.defender_trophy_loss), 0)::INTEGER
    INTO v_def_loss_today
    FROM raid_sessions rs
    WHERE rs.defender_user_id = v_raid.defender_user_id
      AND rs.course_id = p_course_id
      AND rs.quota_date = v_raid.quota_date;

    INSERT INTO user_economy_balances (user_id, course_id, daily_practice_xp_date)
    VALUES (v_raid.attacker_user_id, p_course_id, v_raid.quota_date)
    ON CONFLICT (user_id, course_id) DO NOTHING;

    INSERT INTO user_economy_balances (user_id, course_id, daily_practice_xp_date)
    VALUES (v_raid.defender_user_id, p_course_id, v_raid.quota_date)
    ON CONFLICT (user_id, course_id) DO NOTHING;

    SELECT CASE WHEN daily_practice_xp_date = v_raid.quota_date THEN daily_practice_xp ELSE 0 END
    INTO v_att_xp_today
    FROM user_economy_balances
    WHERE user_id = v_raid.attacker_user_id AND course_id = p_course_id;

    IF v_elapsed_ms > 120000 THEN
        v_status := 'EXPIRED';
        v_consume_in := FALSE;
    ELSIF v_def_immune THEN
        v_status := 'SETTLED_COOLDOWN_BLOCKED';
        v_blocked := TRUE;
        v_xp := LEAST(15, GREATEST(0, 100 - v_att_xp_today));
        v_consume_in := FALSE;
    ELSIF p_sandbox_solved AND p_drills_passed >= 2 THEN
        v_status := 'SETTLED_WIN';
        v_stake := CASE WHEN v_raid.is_revenge THEN 22 ELSE 15 END;
        v_def_loss := LEAST(v_stake, GREATEST(0, 45 - v_def_loss_today));
        v_att_gain := v_stake;
        v_gold := LEAST(10000, GREATEST(10, p_base_score_points) * 2);
        v_chest := CASE WHEN p_drills_passed = 3 THEN 'APEX_RAID_CHEST' ELSE 'STANDARD_RAID_CHEST' END;
        v_consume_in := TRUE;
        IF NOT v_raid.is_revenge THEN
            v_revenge_exp := p_now + INTERVAL '48 hours';
        ELSE
            UPDATE raid_sessions
            SET revenge_consumed = TRUE
            WHERE id = v_raid.revenge_source_raid_id;
        END IF;

        -- Apply 6h POST_RAID_COOLDOWN immediately so concurrent settlements see defender_immune = TRUE
        INSERT INTO user_node_protection (user_id, node_id, tenant_id, course_id, kind, protected_until, source_event_id)
        VALUES (
            v_raid.defender_user_id,
            v_raid.defender_node_id,
            v_raid.tenant_id,
            p_course_id,
            'POST_RAID_COOLDOWN',
            p_now + INTERVAL '6 hours',
            v_raid.id
        )
        ON CONFLICT (user_id, node_id, kind) DO UPDATE
        SET protected_until = GREATEST(user_node_protection.protected_until, EXCLUDED.protected_until),
            source_event_id = EXCLUDED.source_event_id,
            computed_at = p_now;

        -- S-18 / F-05: Record or coalesce aggregated RAID_ALERT notification (respecting 22:00-08:00 quiet hours)
        SELECT COALESCE(timezone, 'Asia/Kuala_Lumpur') INTO v_def_tz FROM users WHERE id = v_raid.defender_user_id;
        v_def_quota := user_local_quota_date(p_now, COALESCE(v_def_tz, 'Asia/Kuala_Lumpur'));
        v_local_ts := p_now AT TIME ZONE COALESCE(v_def_tz, 'Asia/Kuala_Lumpur');
        IF EXTRACT(HOUR FROM v_local_ts) >= 22 THEN
            v_quiet_deliver := ((v_local_ts::date + 1) + TIME '08:00') AT TIME ZONE COALESCE(v_def_tz, 'Asia/Kuala_Lumpur');
        ELSIF EXTRACT(HOUR FROM v_local_ts) < 8 THEN
            v_quiet_deliver := (v_local_ts::date + TIME '08:00') AT TIME ZONE COALESCE(v_def_tz, 'Asia/Kuala_Lumpur');
        ELSE
            v_quiet_deliver := p_now;
        END IF;

        INSERT INTO notification_delivery_ledger (
            user_id, course_id, notification_kind, local_quota_date, raid_count, summary_text, payload_json, deliver_after
        )
        VALUES (
            v_raid.defender_user_id,
            p_course_id,
            'RAID_ALERT',
            v_def_quota,
            1,
            'Raided 1 time today',
            jsonb_build_object('schema_version', '1.0.0', 'raid_ids', jsonb_build_array(v_raid.id)),
            v_quiet_deliver
        )
        ON CONFLICT (user_id, notification_kind, local_quota_date) DO UPDATE
        SET raid_count = notification_delivery_ledger.raid_count + 1,
            summary_text = 'Raided ' || (notification_delivery_ledger.raid_count + 1)::text || ' times today',
            payload_json = jsonb_set(
                notification_delivery_ledger.payload_json,
                '{raid_ids}',
                COALESCE(notification_delivery_ledger.payload_json->'raid_ids', '[]'::jsonb) || to_jsonb(v_raid.id)
            ),
            updated_at = p_now;
    ELSE
        v_status := 'SETTLED_LOSS';
        v_xp := LEAST(5, GREATEST(0, 100 - v_att_xp_today));
        v_consume_in := TRUE;
    END IF;

    UPDATE raid_sessions
    SET status = v_status,
        attacker_trophy_gain = v_att_gain,
        defender_trophy_loss = v_def_loss,
        practice_xp_awarded = v_xp,
        gold_awarded = v_gold,
        loot_chest_tier = v_chest,
        consumes_defender_inbound_cap = v_consume_in,
        revenge_window_expires_at = v_revenge_exp,
        settled_at = p_now
    WHERE id = v_raid.id;

    -- Append-only economy ledger + balances update
    IF v_att_gain > 0 OR v_gold > 0 OR v_xp > 0 THEN
        INSERT INTO reward_transactions (
            user_id, tenant_id, course_id, source_kind, source_id, settlement_key,
            gold_delta, trophies_delta, practice_xp_delta, chest_tier, quota_date
        )
        VALUES (
            v_raid.attacker_user_id, v_raid.tenant_id, p_course_id,
            CASE
                WHEN v_status = 'SETTLED_COOLDOWN_BLOCKED' OR v_status = 'SETTLED_LOSS' THEN 'RAID_PRACTICE'
                WHEN v_raid.is_revenge THEN 'REVENGE_RAID'
                ELSE 'ASYNC_RAID'
            END,
            v_raid.id, v_raid.settlement_key,
            v_gold, v_att_gain, v_xp, v_chest, v_raid.quota_date
        )
        ON CONFLICT (user_id, source_kind, settlement_key) DO NOTHING;

        UPDATE user_economy_balances
        SET gold = gold + v_gold,
            trophies = trophies + v_att_gain,
            practice_xp = practice_xp + v_xp,
            daily_practice_xp = CASE
                WHEN daily_practice_xp_date = v_raid.quota_date THEN LEAST(100, daily_practice_xp + v_xp)
                ELSE LEAST(100, v_xp)
            END,
            daily_practice_xp_date = v_raid.quota_date,
            updated_at = p_now
        WHERE user_id = v_raid.attacker_user_id AND course_id = p_course_id;
    END IF;

    IF v_def_loss > 0 THEN
        INSERT INTO reward_transactions (
            user_id, tenant_id, course_id, source_kind, source_id, settlement_key,
            trophies_delta, quota_date
        )
        VALUES (
            v_raid.defender_user_id, v_raid.tenant_id, p_course_id,
            'RAID_DEFENSE_LOSS', v_raid.id, v_raid.settlement_key,
            -v_def_loss, v_raid.quota_date
        )
        ON CONFLICT (user_id, source_kind, settlement_key) DO NOTHING;

        UPDATE user_economy_balances
        SET trophies = GREATEST(0, trophies - v_def_loss),
            updated_at = p_now
        WHERE user_id = v_raid.defender_user_id AND course_id = p_course_id;
    END IF;

    RETURN QUERY SELECT v_status, v_blocked, v_att_gain, v_def_loss, v_xp, v_gold, v_chest, v_consume_in;
END;
$$;

CREATE OR REPLACE FUNCTION apply_boss_strike_secure(
    p_boss_id UUID,
    p_course_id UUID,
    p_lobby_id UUID,
    p_sequence INTEGER,
    p_rating SMALLINT,
    p_overcharged BOOLEAN,
    p_base_damage INTEGER,
    p_settlement_key TEXT
)
RETURNS TABLE(
    raw_damage INTEGER,
    applied_damage INTEGER,
    overkill_damage INTEGER,
    boss_hp_after INTEGER,
    boss_status TEXT
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
    v_actor UUID := app_actor();
    v_boss chapter_bosses%ROWTYPE;
    v_scale INTEGER;
    v_raw INTEGER;
    v_applied INTEGER;
    v_overkill INTEGER;
    v_hp_after INTEGER;
    v_new_status TEXT;
BEGIN
    IF v_actor IS NULL OR NOT can_study_course(p_course_id) THEN
        RAISE EXCEPTION 'not authorized' USING ERRCODE = '42501';
    END IF;

    SELECT * INTO v_boss
    FROM chapter_bosses
    WHERE id = p_boss_id AND course_id = p_course_id
    FOR UPDATE;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'boss not found';
    END IF;

    v_scale := CASE
        WHEN p_rating = 1 THEN 0
        WHEN p_rating = 2 THEN 75
        WHEN p_rating = 3 THEN 100
        WHEN p_rating = 4 AND p_overcharged THEN 200
        WHEN p_rating = 4 THEN 125
        ELSE 0
    END;
    v_raw := (GREATEST(1, p_base_damage) * v_scale) / 100;
    v_applied := LEAST(v_raw, v_boss.current_hp);
    v_overkill := v_raw - v_applied;
    v_hp_after := v_boss.current_hp - v_applied;
    v_new_status := CASE WHEN v_hp_after = 0 THEN 'DEFEATED' ELSE v_boss.status END;

    INSERT INTO boss_contributions (
        boss_id, course_id, lobby_id, user_id, sequence, rating, overcharged,
        raw_damage, applied_damage, overkill_damage, settlement_key
    )
    VALUES (
        p_boss_id, p_course_id, p_lobby_id, v_actor, p_sequence, p_rating, p_overcharged,
        v_raw, v_applied, v_overkill, p_settlement_key
    );

    UPDATE chapter_bosses
    SET current_hp = v_hp_after,
        status = v_new_status,
        defeated_at = CASE WHEN v_hp_after = 0 AND defeated_at IS NULL THEN now() ELSE defeated_at END
    WHERE id = p_boss_id;

    RETURN QUERY SELECT v_raw, v_applied, v_overkill, v_hp_after, v_new_status;
END;
$$;

CREATE OR REPLACE FUNCTION claim_boss_chest_secure(
    p_boss_id UUID,
    p_course_id UUID,
    p_settlement_key TEXT
)
RETURNS TABLE(
    claimed_now BOOLEAN,
    reward_tier TEXT,
    gold_awarded INTEGER,
    cosmetic_key TEXT
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
    v_actor UUID := app_actor();
    v_boss chapter_bosses%ROWTYPE;
    v_contributed BOOLEAN;
    v_inserted INTEGER;
    v_quota DATE;
BEGIN
    IF v_actor IS NULL OR NOT can_study_course(p_course_id) THEN
        RAISE EXCEPTION 'not authorized' USING ERRCODE = '42501';
    END IF;

    SELECT * INTO v_boss
    FROM chapter_bosses
    WHERE id = p_boss_id AND course_id = p_course_id
    FOR UPDATE;

    IF NOT FOUND OR v_boss.status <> 'DEFEATED' THEN
        RAISE EXCEPTION 'boss not defeated' USING ERRCODE = '23514';
    END IF;

    SELECT EXISTS (
        SELECT 1 FROM boss_contributions bc
        WHERE bc.boss_id = p_boss_id AND bc.user_id = v_actor AND bc.applied_damage > 0
    ) INTO v_contributed;

    IF NOT v_contributed THEN
        RAISE EXCEPTION 'player has zero applied contribution on defeated boss' USING ERRCODE = '42501';
    END IF;

    INSERT INTO boss_chest_claims (
        boss_instance_id, course_id, user_id, reward_tier, gold_awarded, cosmetic_key, settlement_key
    )
    VALUES (
        p_boss_id, p_course_id, v_actor, v_boss.reward_tier, v_boss.gold_reward,
        v_boss.cosmetic_reward_key, p_settlement_key
    )
    ON CONFLICT (user_id, boss_instance_id, reward_tier) DO NOTHING;

    GET DIAGNOSTICS v_inserted = ROW_COUNT;

    IF v_inserted > 0 THEN
        v_quota := user_local_quota_date(now(), 'UTC');
        INSERT INTO reward_transactions (
            user_id, tenant_id, course_id, source_kind, source_id, settlement_key,
            gold_delta, chest_tier, quota_date
        )
        VALUES (
            v_actor, v_boss.tenant_id, p_course_id,
            CASE WHEN v_boss.mode = 'SOLO' THEN 'SOLO_BOSS' ELSE 'COOP_BOSS' END,
            p_boss_id, p_settlement_key, v_boss.gold_reward, v_boss.reward_tier, v_quota
        )
        ON CONFLICT (user_id, source_kind, settlement_key) DO NOTHING;

        INSERT INTO user_economy_balances (user_id, course_id, gold, daily_practice_xp_date)
        VALUES (v_actor, p_course_id, v_boss.gold_reward, v_quota)
        ON CONFLICT (user_id, course_id) DO UPDATE
        SET gold = user_economy_balances.gold + v_boss.gold_reward,
            updated_at = now();

        IF v_boss.cosmetic_reward_key IS NOT NULL THEN
            INSERT INTO user_cosmetic_inventory (
                user_id, course_id, cosmetic_key, rarity, source_kind, settlement_key
            )
            VALUES (
                v_actor, p_course_id, v_boss.cosmetic_reward_key, 'APEX',
                v_boss.mode || '_BOSS', p_settlement_key
            )
            ON CONFLICT (user_id, course_id, cosmetic_key) DO NOTHING;
        END IF;
    END IF;

    RETURN QUERY SELECT (v_inserted > 0), v_boss.reward_tier,
        CASE WHEN v_inserted > 0 THEN v_boss.gold_reward ELSE 0 END,
        v_boss.cosmetic_reward_key;
END;
$$;

CREATE OR REPLACE FUNCTION record_clan_war_contribution_secure(
    p_war_id UUID,
    p_course_id UUID,
    p_contribution_kind TEXT,
    p_raw_points INTEGER,
    p_normalized_points INTEGER,
    p_relative_gain_bps INTEGER,
    p_settlement_key TEXT
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
    v_actor UUID := app_actor();
    v_war clan_wars%ROWTYPE;
BEGIN
    IF v_actor IS NULL OR NOT can_study_course(p_course_id) THEN
        RAISE EXCEPTION 'not authorized' USING ERRCODE = '42501';
    END IF;

    SELECT * INTO v_war FROM clan_wars WHERE id = p_war_id AND course_id = p_course_id FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'clan war not found';
    END IF;
    IF NOT (v_actor = ANY(v_war.war_roster_member_ids)) THEN
        RAISE EXCEPTION 'USER_NOT_IN_FROZEN_WAR_ROSTER' USING ERRCODE = '42501';
    END IF;

    INSERT INTO clan_war_contributions (
        war_id, clan_id, course_id, user_id, contribution_kind,
        raw_points, normalized_points, relative_gain_bps, settlement_key
    )
    VALUES (
        p_war_id, v_war.clan_id, p_course_id, v_actor, p_contribution_kind,
        p_raw_points, p_normalized_points, p_relative_gain_bps, p_settlement_key
    );

    UPDATE clan_wars
    SET total_raw_points = total_raw_points + p_raw_points,
        total_normalized_points = total_normalized_points + p_normalized_points
    WHERE id = p_war_id;
END;
$$;

-- RLS policies for M6 tables
ALTER TABLE reward_transactions ENABLE ROW LEVEL SECURITY;
ALTER TABLE user_economy_balances ENABLE ROW LEVEL SECURITY;
ALTER TABLE user_cosmetic_inventory ENABLE ROW LEVEL SECURITY;
ALTER TABLE raid_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE notification_delivery_ledger ENABLE ROW LEVEL SECURITY;
ALTER TABLE league_brackets ENABLE ROW LEVEL SECURITY;
ALTER TABLE league_standings ENABLE ROW LEVEL SECURITY;
ALTER TABLE chapter_bosses ENABLE ROW LEVEL SECURITY;
ALTER TABLE boss_lobbies ENABLE ROW LEVEL SECURITY;
ALTER TABLE boss_contributions ENABLE ROW LEVEL SECURITY;
ALTER TABLE boss_chest_claims ENABLE ROW LEVEL SECURITY;
ALTER TABLE clans ENABLE ROW LEVEL SECURITY;
ALTER TABLE clan_memberships ENABLE ROW LEVEL SECURITY;
ALTER TABLE clan_wars ENABLE ROW LEVEL SECURITY;
ALTER TABLE clan_war_contributions ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS reward_tx_owner ON reward_transactions;
CREATE POLICY reward_tx_owner ON reward_transactions FOR SELECT USING (user_id = app_actor());

DROP POLICY IF EXISTS econ_bal_owner ON user_economy_balances;
CREATE POLICY econ_bal_owner ON user_economy_balances FOR SELECT USING (user_id = app_actor());

DROP POLICY IF EXISTS cosmetics_owner ON user_cosmetic_inventory;
CREATE POLICY cosmetics_owner ON user_cosmetic_inventory FOR SELECT USING (user_id = app_actor());

DROP POLICY IF EXISTS raid_sessions_participant ON raid_sessions;
CREATE POLICY raid_sessions_participant ON raid_sessions FOR SELECT
    USING (attacker_user_id = app_actor() OR defender_user_id = app_actor());

DROP POLICY IF EXISTS notif_owner ON notification_delivery_ledger;
CREATE POLICY notif_owner ON notification_delivery_ledger FOR SELECT USING (user_id = app_actor());

DROP POLICY IF EXISTS league_brackets_course ON league_brackets;
CREATE POLICY league_brackets_course ON league_brackets FOR SELECT USING (can_read_course(course_id));
CREATE POLICY league_brackets_write ON league_brackets FOR INSERT WITH CHECK (can_study_course(course_id));
CREATE POLICY league_brackets_update ON league_brackets FOR UPDATE USING (can_study_course(course_id));

DROP POLICY IF EXISTS league_standings_course ON league_standings;
CREATE POLICY league_standings_course ON league_standings FOR SELECT USING (can_read_course(course_id));
CREATE POLICY league_standings_insert ON league_standings FOR INSERT WITH CHECK (can_study_course(course_id));
CREATE POLICY league_standings_update ON league_standings FOR UPDATE USING (can_study_course(course_id));

DROP POLICY IF EXISTS chapter_bosses_course ON chapter_bosses;
CREATE POLICY chapter_bosses_course ON chapter_bosses FOR SELECT USING (can_read_course(course_id));
CREATE POLICY chapter_bosses_insert ON chapter_bosses FOR INSERT WITH CHECK (can_study_course(course_id));

DROP POLICY IF EXISTS boss_lobbies_course ON boss_lobbies;
CREATE POLICY boss_lobbies_course ON boss_lobbies FOR ALL
    USING (can_study_course(course_id)) WITH CHECK (can_study_course(course_id));

DROP POLICY IF EXISTS boss_contributions_course ON boss_contributions;
CREATE POLICY boss_contributions_course ON boss_contributions FOR SELECT USING (can_study_course(course_id));

DROP POLICY IF EXISTS boss_chest_claims_owner ON boss_chest_claims;
CREATE POLICY boss_chest_claims_owner ON boss_chest_claims FOR SELECT USING (user_id = app_actor());

DROP POLICY IF EXISTS clans_course ON clans;
CREATE POLICY clans_course ON clans FOR ALL
    USING (can_read_course(course_id)) WITH CHECK (can_study_course(course_id));

DROP POLICY IF EXISTS clan_memberships_course ON clan_memberships;
CREATE POLICY clan_memberships_course ON clan_memberships FOR ALL
    USING (can_study_course(course_id)) WITH CHECK (can_study_course(course_id));

DROP POLICY IF EXISTS clan_wars_course ON clan_wars;
CREATE POLICY clan_wars_course ON clan_wars FOR SELECT USING (can_study_course(course_id));
CREATE POLICY clan_wars_insert ON clan_wars FOR INSERT WITH CHECK (can_study_course(course_id));

DROP POLICY IF EXISTS clan_war_contrib_course ON clan_war_contributions;
CREATE POLICY clan_war_contrib_course ON clan_war_contributions FOR SELECT USING (can_study_course(course_id));
