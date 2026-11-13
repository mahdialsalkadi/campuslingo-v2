-- Milestone M5: Authoritative 90s 1v1 Duels, Bounded Replays, Exposure Uniqueness,
-- 30-Minute Same-Day Step Guard, Local 04:00 Daily Caps, and Event-Sourced Recovery.

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS timezone TEXT NOT NULL DEFAULT 'UTC';

CREATE OR REPLACE FUNCTION user_local_quota_date(p_ts TIMESTAMPTZ, p_tz TEXT)
RETURNS DATE
LANGUAGE SQL
IMMUTABLE
STRICT
AS $$
    SELECT ((p_ts AT TIME ZONE COALESCE(NULLIF(p_tz, ''), 'UTC')) - INTERVAL '4 hours')::date;
$$;

CREATE TABLE IF NOT EXISTS matches (
    id UUID PRIMARY KEY,
    course_id UUID NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    player_a_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    player_b_id UUID REFERENCES users(id) ON DELETE CASCADE,
    opponent_kind TEXT NOT NULL CHECK (opponent_kind IN ('HUMAN', 'GHOST', 'BOT')),
    ghost_source_match_id UUID REFERENCES matches(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'MATCH_STARTED' CHECK (
        status IN (
            'MATCH_STARTED',
            'IN_PROGRESS',
            'SUDDEN_DEATH',
            'COMPLETED',
            'VOID_SERVER_INTERRUPTION',
            'VOID_ABANDONED'
        )
    ),
    winner_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    is_draw BOOLEAN NOT NULL DEFAULT FALSE,
    void_reason TEXT,
    settlement_key TEXT NOT NULL UNIQUE,
    protocol_version SMALLINT NOT NULL DEFAULT 2,
    content_version INTEGER NOT NULL DEFAULT 1,
    rating_map_version SMALLINT NOT NULL DEFAULT 2,
    score_version SMALLINT NOT NULL DEFAULT 2,
    accommodation_version SMALLINT NOT NULL DEFAULT 1,
    replay_opt_in_a BOOLEAN NOT NULL DEFAULT TRUE,
    replay_opt_in_b BOOLEAN NOT NULL DEFAULT TRUE,
    entry_cost_gems INTEGER NOT NULL DEFAULT 0 CHECK (entry_cost_gems >= 0),
    entry_refunded BOOLEAN NOT NULL DEFAULT FALSE,
    clock_frozen_at TIMESTAMPTZ,
    total_outage_frozen_ms INTEGER NOT NULL DEFAULT 0 CHECK (total_outage_frozen_ms >= 0),
    match_clock_ms INTEGER NOT NULL DEFAULT 0 CHECK (match_clock_ms >= 0 AND match_clock_ms <= 120000),
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ended_at TIMESTAMPTZ,
    UNIQUE (id, course_id),
    CONSTRAINT chk_match_winner_participant CHECK (
        winner_user_id IS NULL
        OR winner_user_id = player_a_id
        OR winner_user_id = player_b_id
    ),
    CONSTRAINT chk_match_terminal_outcome CHECK (
        status <> 'COMPLETED'
        OR (is_draw = TRUE AND winner_user_id IS NULL)
        OR (is_draw = FALSE AND winner_user_id IS NOT NULL)
    ),
    CONSTRAINT chk_match_void_outcome CHECK (
        status NOT LIKE 'VOID_%'
        OR (winner_user_id IS NULL AND is_draw = FALSE AND void_reason IS NOT NULL)
    ),
    CONSTRAINT chk_match_human_has_opponent CHECK (
        opponent_kind <> 'HUMAN'
        OR (player_b_id IS NOT NULL AND player_b_id <> player_a_id)
    )
);

CREATE INDEX IF NOT EXISTS idx_matches_course_status
    ON matches (course_id, status, started_at DESC);

CREATE INDEX IF NOT EXISTS idx_matches_ghost_selection
    ON matches (course_id, started_at DESC)
    WHERE status = 'COMPLETED' AND replay_opt_in_a = TRUE;

CREATE TABLE IF NOT EXISTS match_snapshots (
    match_id UUID NOT NULL,
    course_id UUID NOT NULL,
    snapshot_seq INTEGER NOT NULL CHECK (snapshot_seq >= 0),
    match_clock_ms INTEGER NOT NULL CHECK (match_clock_ms >= 0 AND match_clock_ms <= 120000),
    state_json JSONB NOT NULL CHECK (octet_length(state_json::text) <= 32768),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (match_id, snapshot_seq),
    FOREIGN KEY (match_id, course_id) REFERENCES matches(id, course_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS match_replay_events (
    id BIGSERIAL PRIMARY KEY,
    match_id UUID NOT NULL,
    course_id UUID NOT NULL,
    actor_side TEXT NOT NULL CHECK (actor_side IN ('PLAYER_A', 'PLAYER_B')),
    event_seq INTEGER NOT NULL CHECK (event_seq > 0 AND event_seq <= 512),
    event_kind TEXT NOT NULL CHECK (event_kind IN ('STRIKE', 'SELECT', 'VOID', 'CLOCK_SYNC')),
    elapsed_ms INTEGER NOT NULL CHECK (elapsed_ms >= 0 AND elapsed_ms <= 120000),
    payload_json JSONB NOT NULL CHECK (octet_length(payload_json::text) <= 4096),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL DEFAULT (now() + INTERVAL '30 days'),
    UNIQUE (match_id, actor_side, event_seq),
    FOREIGN KEY (match_id, course_id) REFERENCES matches(id, course_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_match_replay_events_lookup
    ON match_replay_events (match_id, actor_side, elapsed_ms, event_seq);

CREATE INDEX IF NOT EXISTS idx_match_replay_events_ttl
    ON match_replay_events (expires_at);

CREATE TABLE IF NOT EXISTS match_settlements (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id UUID NOT NULL,
    course_id UUID NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    settlement_key TEXT NOT NULL,
    settlement_kind TEXT NOT NULL CHECK (
        settlement_kind IN ('WIN', 'LOSS', 'DRAW', 'VOID_REFUND')
    ),
    score_points INTEGER NOT NULL DEFAULT 0 CHECK (score_points >= 0),
    refunded_gems INTEGER NOT NULL DEFAULT 0 CHECK (refunded_gems >= 0),
    settled_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (match_id, user_id),
    UNIQUE (settlement_key, user_id),
    FOREIGN KEY (match_id, course_id) REFERENCES matches(id, course_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS user_combat_telemetry (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    course_id UUID NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    duels_played INTEGER NOT NULL DEFAULT 0 CHECK (duels_played >= 0),
    duels_won INTEGER NOT NULL DEFAULT 0 CHECK (duels_won >= 0),
    duels_lost INTEGER NOT NULL DEFAULT 0 CHECK (duels_lost >= 0),
    duels_drawn INTEGER NOT NULL DEFAULT 0 CHECK (duels_drawn >= 0),
    duels_voided INTEGER NOT NULL DEFAULT 0 CHECK (duels_voided >= 0),
    overcharges_landed INTEGER NOT NULL DEFAULT 0 CHECK (overcharges_landed >= 0),
    total_score_v2 BIGINT NOT NULL DEFAULT 0 CHECK (total_score_v2 >= 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, course_id)
);

-- Extend review_logs cleanly for rating_map_version = 2, S-08 exposure uniqueness,
-- S-09 interaction_source separation, 30-min SAME_DAY_MIN_GAP, and 04:00 daily caps.
ALTER TABLE review_logs
    DROP CONSTRAINT IF EXISTS review_logs_rating_check;

ALTER TABLE review_logs
    ADD CONSTRAINT review_logs_rating_check CHECK (rating BETWEEN 1 AND 4);

ALTER TABLE review_logs
    ADD COLUMN IF NOT EXISTS course_id UUID REFERENCES courses(id) ON DELETE CASCADE,
    ADD COLUMN IF NOT EXISTS scope_kind TEXT NOT NULL DEFAULT 'STUDY_SESSION'
        CHECK (scope_kind IN ('MATCH', 'STUDY_SESSION')),
    ADD COLUMN IF NOT EXISTS scope_id UUID,
    ADD COLUMN IF NOT EXISTS interaction_source TEXT NOT NULL DEFAULT 'STUDY_SESSION'
        CHECK (interaction_source IN ('MATCH', 'STUDY_SESSION', 'SPEED_DRILL')),
    ADD COLUMN IF NOT EXISTS rating_map_version SMALLINT NOT NULL DEFAULT 2,
    ADD COLUMN IF NOT EXISTS score_version SMALLINT NOT NULL DEFAULT 2,
    ADD COLUMN IF NOT EXISTS score_points INTEGER NOT NULL DEFAULT 0 CHECK (score_points >= 0),
    ADD COLUMN IF NOT EXISTS is_void BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS void_reason TEXT,
    ADD COLUMN IF NOT EXISTS fsrs_step_eligible BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS reward_eligible BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS eligibility_blocker TEXT,
    ADD COLUMN IF NOT EXISTS quota_date DATE;

CREATE UNIQUE INDEX IF NOT EXISTS uq_review_logs_exposure_scope
    ON review_logs (user_id, challenge_id, scope_kind, scope_id)
    WHERE scope_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_review_logs_daily_caps
    ON review_logs (user_id, course_id, quota_date, challenge_id)
    WHERE reward_eligible = TRUE AND is_void = FALSE;

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

DROP TRIGGER IF EXISTS trg_review_logs_exposure_caps ON review_logs;
CREATE TRIGGER trg_review_logs_exposure_caps
    BEFORE INSERT ON review_logs
    FOR EACH ROW
    EXECUTE FUNCTION enforce_review_exposure_and_caps();

-- SECURITY DEFINER match & R-07 recovery helpers restricted to worker / recovery / grader / app roles.
CREATE OR REPLACE FUNCTION create_match_record(
    p_match_id UUID,
    p_course_id UUID,
    p_player_a_id UUID,
    p_player_b_id UUID,
    p_opponent_kind TEXT,
    p_ghost_source_match_id UUID,
    p_settlement_key TEXT,
    p_entry_cost_gems INTEGER,
    p_initial_snapshot JSONB
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    IF session_user NOT IN ('campuslingo_app', 'campuslingo_worker', 'campuslingo_recovery', 'campuslingo_owner') THEN
        RAISE EXCEPTION 'unauthorized role for create_match_record';
    END IF;

    INSERT INTO matches (
        id, course_id, player_a_id, player_b_id, opponent_kind,
        ghost_source_match_id, status, settlement_key, entry_cost_gems
    )
    VALUES (
        p_match_id, p_course_id, p_player_a_id, p_player_b_id, p_opponent_kind,
        p_ghost_source_match_id, 'IN_PROGRESS', p_settlement_key, p_entry_cost_gems
    );

    INSERT INTO match_snapshots (match_id, course_id, snapshot_seq, match_clock_ms, state_json)
    VALUES (p_match_id, p_course_id, 0, 0, p_initial_snapshot);
END;
$$;

CREATE OR REPLACE FUNCTION save_match_snapshot_secure(
    p_match_id UUID,
    p_course_id UUID,
    p_snapshot_seq INTEGER,
    p_match_clock_ms INTEGER,
    p_state_json JSONB
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    IF session_user NOT IN ('campuslingo_app', 'campuslingo_worker', 'campuslingo_recovery', 'campuslingo_owner') THEN
        RAISE EXCEPTION 'unauthorized role for save_match_snapshot_secure';
    END IF;

    INSERT INTO match_snapshots (match_id, course_id, snapshot_seq, match_clock_ms, state_json)
    VALUES (p_match_id, p_course_id, p_snapshot_seq, p_match_clock_ms, p_state_json)
    ON CONFLICT (match_id, snapshot_seq) DO NOTHING;

    UPDATE matches
    SET match_clock_ms = GREATEST(match_clock_ms, p_match_clock_ms)
    WHERE id = p_match_id AND course_id = p_course_id AND status IN ('MATCH_STARTED', 'IN_PROGRESS', 'SUDDEN_DEATH');
END;
$$;

CREATE OR REPLACE FUNCTION append_match_replay_event_secure(
    p_match_id UUID,
    p_course_id UUID,
    p_actor_side TEXT,
    p_event_seq INTEGER,
    p_event_kind TEXT,
    p_elapsed_ms INTEGER,
    p_payload_json JSONB
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    IF session_user NOT IN ('campuslingo_app', 'campuslingo_grader', 'campuslingo_worker', 'campuslingo_recovery', 'campuslingo_owner') THEN
        RAISE EXCEPTION 'unauthorized role for append_match_replay_event_secure';
    END IF;

    INSERT INTO match_replay_events (
        match_id, course_id, actor_side, event_seq, event_kind, elapsed_ms, payload_json
    )
    VALUES (
        p_match_id, p_course_id, p_actor_side, p_event_seq, p_event_kind, p_elapsed_ms, p_payload_json
    )
    ON CONFLICT (match_id, actor_side, event_seq) DO NOTHING;
END;
$$;

CREATE OR REPLACE FUNCTION record_duel_review_secure(
    p_attempt_id UUID,
    p_user_id UUID,
    p_challenge_id UUID,
    p_course_id UUID,
    p_match_id UUID,
    p_interaction_source TEXT,
    p_rating SMALLINT,
    p_score_points INTEGER,
    p_is_void BOOLEAN,
    p_void_reason TEXT,
    p_reviewed_at TIMESTAMPTZ,
    p_next_stability REAL,
    p_next_difficulty REAL,
    p_scheduled_days INTEGER
)
RETURNS TABLE(
    fsrs_step_eligible BOOLEAN,
    reward_eligible BOOLEAN,
    eligibility_blocker TEXT,
    awarded_points INTEGER
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    v_tenant_id UUID;
    v_state_json JSONB;
    v_fsrs_ok BOOLEAN;
    v_reward_ok BOOLEAN;
    v_blocker TEXT;
    v_pts INTEGER;
BEGIN
    IF session_user NOT IN ('campuslingo_grader', 'campuslingo_worker', 'campuslingo_recovery', 'campuslingo_owner') THEN
        RAISE EXCEPTION 'unauthorized role for record_duel_review_secure';
    END IF;

    SELECT ch.tenant_id
    INTO v_tenant_id
    FROM challenges ch
    WHERE ch.id = p_challenge_id AND ch.course_id = p_course_id;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'challenge not found in course';
    END IF;

    v_state_json := jsonb_build_object(
        'stability', GREATEST(0.001, p_next_stability),
        'difficulty', LEAST(10.0, GREATEST(1.0, p_next_difficulty)),
        'scheduled_days', GREATEST(0, p_scheduled_days)
    );

    INSERT INTO attempts (id, tenant_id, course_id, challenge_id, user_id, option_count)
    SELECT p_attempt_id, ch.tenant_id, ch.course_id, ch.id, p_user_id, jsonb_array_length(ch.options)
    FROM challenges ch
    WHERE ch.id = p_challenge_id
      AND NOT EXISTS (SELECT 1 FROM attempts WHERE id = p_attempt_id);

    INSERT INTO review_logs (
        attempt_id, user_id, tenant_id, course_id, challenge_id, rating,
        state_before, state_after, reviewed_at,
        scope_kind, scope_id, interaction_source,
        rating_map_version, score_version, score_points, is_void, void_reason
    )
    VALUES (
        p_attempt_id, p_user_id, v_tenant_id, p_course_id, p_challenge_id, p_rating,
        v_state_json, v_state_json, p_reviewed_at,
        'MATCH', p_match_id, p_interaction_source,
        2, 2, GREATEST(0, p_score_points), p_is_void, p_void_reason
    )
    RETURNING review_logs.fsrs_step_eligible, review_logs.reward_eligible,
              review_logs.eligibility_blocker, review_logs.score_points
    INTO v_fsrs_ok, v_reward_ok, v_blocker, v_pts;

    IF v_fsrs_ok AND NOT p_is_void THEN
        INSERT INTO fsrs_cards (
            user_id, challenge_id, tenant_id, course_id, stability, difficulty,
            reps, last_review, due_at
        )
        VALUES (
            p_user_id, p_challenge_id, v_tenant_id, p_course_id,
            GREATEST(0.001, p_next_stability),
            LEAST(10.0, GREATEST(1.0, p_next_difficulty)),
            1,
            p_reviewed_at,
            p_reviewed_at + make_interval(days => GREATEST(1, p_scheduled_days))
        )
        ON CONFLICT (user_id, challenge_id) DO UPDATE
        SET stability = EXCLUDED.stability,
            difficulty = EXCLUDED.difficulty,
            reps = fsrs_cards.reps + 1,
            last_review = EXCLUDED.last_review,
            due_at = EXCLUDED.due_at;
    END IF;

    RETURN QUERY SELECT v_fsrs_ok, v_reward_ok, v_blocker, v_pts;
END;
$$;

CREATE OR REPLACE FUNCTION settle_match_atomic(
    p_match_id UUID,
    p_course_id UUID,
    p_status TEXT,
    p_winner_user_id UUID,
    p_is_draw BOOLEAN,
    p_void_reason TEXT,
    p_match_clock_ms INTEGER
)
RETURNS BOOLEAN
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    v_match matches%ROWTYPE;
    v_inserted INTEGER;
BEGIN
    IF session_user NOT IN ('campuslingo_app', 'campuslingo_grader', 'campuslingo_worker', 'campuslingo_recovery', 'campuslingo_owner') THEN
        RAISE EXCEPTION 'unauthorized role for settle_match_atomic';
    END IF;

    SELECT * INTO v_match
    FROM matches
    WHERE id = p_match_id AND course_id = p_course_id
    FOR UPDATE;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'match not found';
    END IF;

    IF v_match.status IN ('COMPLETED', 'VOID_SERVER_INTERRUPTION', 'VOID_ABANDONED') THEN
        RETURN FALSE;
    END IF;

    UPDATE matches
    SET status = p_status,
        winner_user_id = p_winner_user_id,
        is_draw = p_is_draw,
        void_reason = p_void_reason,
        entry_refunded = (p_status LIKE 'VOID_%'),
        match_clock_ms = GREATEST(match_clock_ms, p_match_clock_ms),
        ended_at = now()
    WHERE id = p_match_id;

    -- Settle idempotently for player A
    INSERT INTO match_settlements (
        match_id, course_id, user_id, settlement_key, settlement_kind, score_points, refunded_gems
    )
    VALUES (
        p_match_id, p_course_id, v_match.player_a_id, v_match.settlement_key,
        CASE
            WHEN p_status LIKE 'VOID_%' THEN 'VOID_REFUND'
            WHEN p_is_draw THEN 'DRAW'
            WHEN p_winner_user_id = v_match.player_a_id THEN 'WIN'
            ELSE 'LOSS'
        END,
        CASE
            WHEN p_status LIKE 'VOID_%' THEN 0
            WHEN p_is_draw THEN 15
            WHEN p_winner_user_id = v_match.player_a_id THEN 30
            ELSE 10
        END,
        CASE WHEN p_status LIKE 'VOID_%' THEN v_match.entry_cost_gems ELSE 0 END
    )
    ON CONFLICT (match_id, user_id) DO NOTHING;

    GET DIAGNOSTICS v_inserted = ROW_COUNT;

    IF v_match.player_b_id IS NOT NULL AND v_match.opponent_kind = 'HUMAN' THEN
        INSERT INTO match_settlements (
            match_id, course_id, user_id, settlement_key, settlement_kind, score_points, refunded_gems
        )
        VALUES (
            p_match_id, p_course_id, v_match.player_b_id, v_match.settlement_key,
            CASE
                WHEN p_status LIKE 'VOID_%' THEN 'VOID_REFUND'
                WHEN p_is_draw THEN 'DRAW'
                WHEN p_winner_user_id = v_match.player_b_id THEN 'WIN'
                ELSE 'LOSS'
            END,
            CASE
                WHEN p_status LIKE 'VOID_%' THEN 0
                WHEN p_is_draw THEN 15
                WHEN p_winner_user_id = v_match.player_b_id THEN 30
                ELSE 10
            END,
            CASE WHEN p_status LIKE 'VOID_%' THEN v_match.entry_cost_gems ELSE 0 END
        )
        ON CONFLICT (match_id, user_id) DO NOTHING;
    END IF;

    INSERT INTO user_combat_telemetry (user_id, course_id, duels_played, duels_won, duels_lost, duels_drawn, duels_voided)
    VALUES (
        v_match.player_a_id, p_course_id, 1,
        CASE WHEN p_status = 'COMPLETED' AND p_winner_user_id = v_match.player_a_id THEN 1 ELSE 0 END,
        CASE WHEN p_status = 'COMPLETED' AND NOT p_is_draw AND p_winner_user_id <> v_match.player_a_id THEN 1 ELSE 0 END,
        CASE WHEN p_status = 'COMPLETED' AND p_is_draw THEN 1 ELSE 0 END,
        CASE WHEN p_status LIKE 'VOID_%' THEN 1 ELSE 0 END
    )
    ON CONFLICT (user_id, course_id) DO UPDATE
    SET duels_played = user_combat_telemetry.duels_played + 1,
        duels_won = user_combat_telemetry.duels_won + EXCLUDED.duels_won,
        duels_lost = user_combat_telemetry.duels_lost + EXCLUDED.duels_lost,
        duels_drawn = user_combat_telemetry.duels_drawn + EXCLUDED.duels_drawn,
        duels_voided = user_combat_telemetry.duels_voided + EXCLUDED.duels_voided,
        updated_at = now();

    IF v_match.player_b_id IS NOT NULL AND v_match.opponent_kind = 'HUMAN' THEN
        INSERT INTO user_combat_telemetry (user_id, course_id, duels_played, duels_won, duels_lost, duels_drawn, duels_voided)
        VALUES (
            v_match.player_b_id, p_course_id, 1,
            CASE WHEN p_status = 'COMPLETED' AND p_winner_user_id = v_match.player_b_id THEN 1 ELSE 0 END,
            CASE WHEN p_status = 'COMPLETED' AND NOT p_is_draw AND p_winner_user_id <> v_match.player_b_id THEN 1 ELSE 0 END,
            CASE WHEN p_status = 'COMPLETED' AND p_is_draw THEN 1 ELSE 0 END,
            CASE WHEN p_status LIKE 'VOID_%' THEN 1 ELSE 0 END
        )
        ON CONFLICT (user_id, course_id) DO UPDATE
        SET duels_played = user_combat_telemetry.duels_played + 1,
            duels_won = user_combat_telemetry.duels_won + EXCLUDED.duels_won,
            duels_lost = user_combat_telemetry.duels_lost + EXCLUDED.duels_lost,
            duels_drawn = user_combat_telemetry.duels_drawn + EXCLUDED.duels_drawn,
            duels_voided = user_combat_telemetry.duels_voided + EXCLUDED.duels_voided,
            updated_at = now();
    END IF;

    RETURN v_inserted > 0;
END;
$$;

-- RLS policies on M5 tables
ALTER TABLE matches ENABLE ROW LEVEL SECURITY;
ALTER TABLE match_snapshots ENABLE ROW LEVEL SECURITY;
ALTER TABLE match_replay_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE match_settlements ENABLE ROW LEVEL SECURITY;
ALTER TABLE user_combat_telemetry ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS matches_participant_or_course ON matches;
CREATE POLICY matches_participant_or_course ON matches
    FOR SELECT
    USING (
        player_a_id = app_actor()
        OR player_b_id = app_actor()
        OR can_read_course(course_id)
    );

DROP POLICY IF EXISTS match_snapshots_read ON match_snapshots;
CREATE POLICY match_snapshots_read ON match_snapshots
    FOR SELECT
    USING (
        EXISTS (
            SELECT 1 FROM matches m
            WHERE m.id = match_snapshots.match_id
              AND (m.player_a_id = app_actor() OR m.player_b_id = app_actor() OR can_read_course(m.course_id))
        )
    );

DROP POLICY IF EXISTS match_replay_events_read ON match_replay_events;
CREATE POLICY match_replay_events_read ON match_replay_events
    FOR SELECT
    USING (
        EXISTS (
            SELECT 1 FROM matches m
            WHERE m.id = match_replay_events.match_id
              AND (m.player_a_id = app_actor() OR m.player_b_id = app_actor() OR (m.status = 'COMPLETED' AND can_read_course(m.course_id)))
        )
    );

DROP POLICY IF EXISTS match_settlements_read ON match_settlements;
CREATE POLICY match_settlements_read ON match_settlements
    FOR SELECT
    USING (user_id = app_actor());

DROP POLICY IF EXISTS user_combat_telemetry_read ON user_combat_telemetry;
CREATE POLICY user_combat_telemetry_read ON user_combat_telemetry
    FOR SELECT
    USING (user_id = app_actor() OR can_manage_course(course_id));

DROP POLICY IF EXISTS matches_worker_recovery_all ON matches;
CREATE POLICY matches_worker_recovery_all ON matches
    FOR ALL
    TO campuslingo_worker, campuslingo_recovery
    USING (true)
    WITH CHECK (true);

DROP POLICY IF EXISTS match_snapshots_worker_recovery_all ON match_snapshots;
CREATE POLICY match_snapshots_worker_recovery_all ON match_snapshots
    FOR ALL
    TO campuslingo_worker, campuslingo_recovery
    USING (true)
    WITH CHECK (true);

DROP POLICY IF EXISTS match_replay_events_worker_recovery_all ON match_replay_events;
CREATE POLICY match_replay_events_worker_recovery_all ON match_replay_events
    FOR ALL
    TO campuslingo_worker, campuslingo_recovery
    USING (true)
    WITH CHECK (true);

DROP POLICY IF EXISTS match_settlements_worker_recovery_all ON match_settlements;
CREATE POLICY match_settlements_worker_recovery_all ON match_settlements
    FOR ALL
    TO campuslingo_worker, campuslingo_recovery
    USING (true)
    WITH CHECK (true);

DROP POLICY IF EXISTS user_combat_telemetry_worker_recovery_all ON user_combat_telemetry;
CREATE POLICY user_combat_telemetry_worker_recovery_all ON user_combat_telemetry
    FOR ALL
    TO campuslingo_worker, campuslingo_recovery
    USING (true)
    WITH CHECK (true);
