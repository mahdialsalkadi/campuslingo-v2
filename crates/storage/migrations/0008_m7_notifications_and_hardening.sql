-- Milestone M7: Notification Devices, Preferences, Outbox with Quiet-Hours Deferral (S-18, F-05),
-- Account Pseudonymization & Versioned Retention Sweep (S-07, S-10, S-15, O-07).

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS pseudonymized_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS erasure_reason TEXT,
    ADD COLUMN IF NOT EXISTS retention_policy_version INTEGER NOT NULL DEFAULT 1;

CREATE TABLE IF NOT EXISTS notification_devices (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    platform TEXT NOT NULL CHECK (platform IN ('WEB_PUSH', 'ANDROID_FCM', 'IOS_APNS')),
    endpoint_or_token TEXT NOT NULL CHECK (length(endpoint_or_token) BETWEEN 8 AND 2048),
    p256dh_key TEXT,
    auth_secret TEXT,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, endpoint_or_token)
);

CREATE INDEX IF NOT EXISTS idx_notification_devices_user_active
    ON notification_devices (user_id)
    WHERE enabled = TRUE;

CREATE TABLE IF NOT EXISTS notification_preferences (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    push_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    raid_alerts_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    shield_digest_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    quiet_hours_start_minute SMALLINT NOT NULL DEFAULT 1320 CHECK (quiet_hours_start_minute = 1320),
    quiet_hours_end_minute SMALLINT NOT NULL DEFAULT 480 CHECK (quiet_hours_end_minute = 480),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS notification_outbox (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    course_id UUID NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    notification_kind TEXT NOT NULL CHECK (notification_kind IN ('RAID_ALERT', 'SHIELD_DIGEST')),
    local_quota_date DATE NOT NULL,
    event_count INTEGER NOT NULL DEFAULT 1 CHECK (event_count >= 1),
    summary_text TEXT NOT NULL CHECK (length(summary_text) BETWEEN 1 AND 320),
    raid_ids UUID[] NOT NULL DEFAULT ARRAY[]::UUID[],
    shield_node_ids UUID[] NOT NULL DEFAULT ARRAY[]::UUID[],
    payload_json JSONB NOT NULL DEFAULT '{"schema_version":"1.0.0"}'::jsonb,
    quiet_hours_deferred BOOLEAN NOT NULL DEFAULT FALSE,
    deliver_after TIMESTAMPTZ NOT NULL,
    delivered_at TIMESTAMPTZ,
    status TEXT NOT NULL DEFAULT 'QUEUED' CHECK (status IN ('QUEUED', 'DELIVERED', 'SUPPRESSED_OPT_OUT')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, notification_kind, local_quota_date)
);

CREATE INDEX IF NOT EXISTS idx_notification_outbox_due
    ON notification_outbox (status, deliver_after, user_id);

CREATE OR REPLACE FUNCTION compute_quiet_hours_delivery(p_ts TIMESTAMPTZ, p_tz TEXT)
RETURNS TABLE(deliver_after TIMESTAMPTZ, quiet_hours_deferred BOOLEAN)
LANGUAGE plpgsql
IMMUTABLE
STRICT
AS $$
DECLARE
    v_tz TEXT := COALESCE(NULLIF(p_tz, ''), 'Asia/Kuala_Lumpur');
    v_local_ts TIMESTAMP;
    v_hour INTEGER;
BEGIN
    v_local_ts := p_ts AT TIME ZONE v_tz;
    v_hour := EXTRACT(HOUR FROM v_local_ts)::INTEGER;
    IF v_hour >= 22 THEN
        deliver_after := ((v_local_ts::date + 1) + TIME '08:00:00') AT TIME ZONE v_tz;
        quiet_hours_deferred := TRUE;
    ELSIF v_hour < 8 THEN
        deliver_after := (v_local_ts::date + TIME '08:00:00') AT TIME ZONE v_tz;
        quiet_hours_deferred := TRUE;
    ELSE
        deliver_after := p_ts;
        quiet_hours_deferred := FALSE;
    END IF;
    RETURN NEXT;
END;
$$;

CREATE OR REPLACE FUNCTION enforce_notification_outbox_daily_cap()
RETURNS TRIGGER
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
DECLARE
    v_existing_count INTEGER;
BEGIN
    SELECT COUNT(*)::INTEGER
    INTO v_existing_count
    FROM notification_outbox
    WHERE user_id = NEW.user_id
      AND local_quota_date = NEW.local_quota_date
      AND id <> NEW.id;

    IF v_existing_count >= 2 THEN
        RAISE EXCEPTION 'MAX_PUSH_NOTIFICATIONS_PER_DAY_EXCEEDED' USING ERRCODE = '23514';
    END IF;

    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_notification_outbox_daily_cap ON notification_outbox;
CREATE TRIGGER trg_notification_outbox_daily_cap
    BEFORE INSERT ON notification_outbox
    FOR EACH ROW EXECUTE FUNCTION enforce_notification_outbox_daily_cap();

CREATE OR REPLACE FUNCTION sync_delivery_ledger_to_outbox()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
    v_tz TEXT;
    v_push_enabled BOOLEAN := TRUE;
    v_raid_enabled BOOLEAN := TRUE;
    v_status TEXT := 'QUEUED';
    v_deferred BOOLEAN := FALSE;
    v_deliver_after TIMESTAMPTZ;
    v_raid_ids UUID[];
    v_event_count INTEGER;
    v_summary TEXT;
BEGIN
    IF NEW.notification_kind <> 'RAID_ALERT' THEN
        RETURN NEW;
    END IF;

    SELECT COALESCE(NULLIF(timezone, ''), 'Asia/Kuala_Lumpur')
    INTO v_tz
    FROM users
    WHERE id = NEW.user_id;

    SELECT np.push_enabled, np.raid_alerts_enabled
    INTO v_push_enabled, v_raid_enabled
    FROM notification_preferences np
    WHERE np.user_id = NEW.user_id;

    IF FOUND AND (NOT v_push_enabled OR NOT v_raid_enabled) THEN
        v_status := 'SUPPRESSED_OPT_OUT';
    END IF;

    SELECT COALESCE(
        ARRAY(
            SELECT DISTINCT elem::uuid
            FROM jsonb_array_elements_text(COALESCE(NEW.payload_json->'raid_ids', '[]'::jsonb)) AS elem
        ),
        ARRAY[]::UUID[]
    ) INTO v_raid_ids;

    v_event_count := GREATEST(1, COALESCE(array_length(v_raid_ids, 1), NEW.raid_count));
    v_summary := CASE
        WHEN v_event_count = 1 THEN 'Raided 1 time today'
        ELSE 'Raided ' || v_event_count::text || ' times today'
    END;

    SELECT q.deliver_after, q.quiet_hours_deferred
    INTO v_deliver_after, v_deferred
    FROM compute_quiet_hours_delivery(NEW.updated_at, COALESCE(v_tz, 'Asia/Kuala_Lumpur')) q;

    -- Keep existing earlier/quiet deliver_after if ledger deliver_after already computed quiet deferral
    IF NEW.deliver_after > NEW.updated_at THEN
        v_deliver_after := NEW.deliver_after;
        v_deferred := TRUE;
    END IF;

    INSERT INTO notification_outbox (
        user_id, course_id, notification_kind, local_quota_date,
        event_count, summary_text, raid_ids, payload_json,
        quiet_hours_deferred, deliver_after, status, created_at, updated_at
    )
    VALUES (
        NEW.user_id, NEW.course_id, 'RAID_ALERT', NEW.local_quota_date,
        v_event_count, v_summary, v_raid_ids,
        jsonb_build_object('schema_version', '1.0.0', 'raid_ids', to_jsonb(v_raid_ids)),
        v_deferred, v_deliver_after, v_status, NEW.created_at, NEW.updated_at
    )
    ON CONFLICT (user_id, notification_kind, local_quota_date) DO UPDATE
    SET raid_ids = ARRAY(
            SELECT DISTINCT u
            FROM unnest(notification_outbox.raid_ids || EXCLUDED.raid_ids) AS u
        ),
        event_count = GREATEST(
            1,
            COALESCE(
                array_length(
                    ARRAY(SELECT DISTINCT u FROM unnest(notification_outbox.raid_ids || EXCLUDED.raid_ids) AS u),
                    1
                ),
                1
            )
        ),
        summary_text = CASE
            WHEN COALESCE(
                array_length(
                    ARRAY(SELECT DISTINCT u FROM unnest(notification_outbox.raid_ids || EXCLUDED.raid_ids) AS u),
                    1
                ),
                1
            ) = 1 THEN 'Raided 1 time today'
            ELSE 'Raided ' || COALESCE(
                array_length(
                    ARRAY(SELECT DISTINCT u FROM unnest(notification_outbox.raid_ids || EXCLUDED.raid_ids) AS u),
                    1
                ),
                1
            )::text || ' times today'
        END,
        payload_json = jsonb_build_object(
            'schema_version', '1.0.0',
            'raid_ids', to_jsonb(ARRAY(SELECT DISTINCT u FROM unnest(notification_outbox.raid_ids || EXCLUDED.raid_ids) AS u))
        ),
        quiet_hours_deferred = notification_outbox.quiet_hours_deferred OR EXCLUDED.quiet_hours_deferred,
        deliver_after = GREATEST(notification_outbox.deliver_after, EXCLUDED.deliver_after),
        status = CASE
            WHEN v_status = 'SUPPRESSED_OPT_OUT' THEN 'SUPPRESSED_OPT_OUT'
            WHEN notification_outbox.status = 'DELIVERED' THEN 'QUEUED'
            ELSE v_status
        END,
        updated_at = EXCLUDED.updated_at;

    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_sync_delivery_ledger_to_outbox ON notification_delivery_ledger;
CREATE TRIGGER trg_sync_delivery_ledger_to_outbox
    AFTER INSERT OR UPDATE ON notification_delivery_ledger
    FOR EACH ROW EXECUTE FUNCTION sync_delivery_ledger_to_outbox();

CREATE OR REPLACE FUNCTION register_notification_device_secure(
    p_platform TEXT,
    p_endpoint_or_token TEXT,
    p_p256dh_key TEXT,
    p_auth_secret TEXT
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
    v_actor UUID := app_actor();
    v_id UUID;
BEGIN
    IF v_actor IS NULL THEN
        RAISE EXCEPTION 'not authorized' USING ERRCODE = '42501';
    END IF;

    INSERT INTO notification_devices (user_id, platform, endpoint_or_token, p256dh_key, auth_secret, enabled, updated_at)
    VALUES (v_actor, p_platform, p_endpoint_or_token, p_p256dh_key, p_auth_secret, TRUE, now())
    ON CONFLICT (user_id, endpoint_or_token) DO UPDATE
    SET platform = EXCLUDED.platform,
        p256dh_key = EXCLUDED.p256dh_key,
        auth_secret = EXCLUDED.auth_secret,
        enabled = TRUE,
        updated_at = now()
    RETURNING id INTO v_id;

    INSERT INTO notification_preferences (user_id)
    VALUES (v_actor)
    ON CONFLICT (user_id) DO NOTHING;

    RETURN v_id;
END;
$$;

CREATE OR REPLACE FUNCTION upsert_notification_preferences_secure(
    p_push_enabled BOOLEAN,
    p_raid_alerts_enabled BOOLEAN,
    p_shield_digest_enabled BOOLEAN
)
RETURNS TABLE(
    push_enabled BOOLEAN,
    raid_alerts_enabled BOOLEAN,
    shield_digest_enabled BOOLEAN,
    quiet_hours_start_minute SMALLINT,
    quiet_hours_end_minute SMALLINT
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
    v_actor UUID := app_actor();
BEGIN
    IF v_actor IS NULL THEN
        RAISE EXCEPTION 'not authorized' USING ERRCODE = '42501';
    END IF;

    INSERT INTO notification_preferences (
        user_id, push_enabled, raid_alerts_enabled, shield_digest_enabled, updated_at
    )
    VALUES (v_actor, p_push_enabled, p_raid_alerts_enabled, p_shield_digest_enabled, now())
    ON CONFLICT (user_id) DO UPDATE
    SET push_enabled = EXCLUDED.push_enabled,
        raid_alerts_enabled = EXCLUDED.raid_alerts_enabled,
        shield_digest_enabled = EXCLUDED.shield_digest_enabled,
        updated_at = now();

    -- Suppress any currently queued notifications whose channel was opted out
    UPDATE notification_outbox
    SET status = 'SUPPRESSED_OPT_OUT',
        updated_at = now()
    WHERE user_id = v_actor
      AND status = 'QUEUED'
      AND (
          NOT p_push_enabled
          OR (notification_kind = 'RAID_ALERT' AND NOT p_raid_alerts_enabled)
          OR (notification_kind = 'SHIELD_DIGEST' AND NOT p_shield_digest_enabled)
      );

    RETURN QUERY
    SELECT np.push_enabled, np.raid_alerts_enabled, np.shield_digest_enabled,
           np.quiet_hours_start_minute, np.quiet_hours_end_minute
    FROM notification_preferences np
    WHERE np.user_id = v_actor;
END;
$$;

CREATE OR REPLACE FUNCTION enqueue_raid_alert_outbox_secure(
    p_defender_user_id UUID,
    p_course_id UUID,
    p_raid_id UUID,
    p_now TIMESTAMPTZ
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
    v_actor UUID := app_actor();
    v_tz TEXT;
    v_quota_date DATE;
    v_deliver_after TIMESTAMPTZ;
    v_deferred BOOLEAN;
    v_push_enabled BOOLEAN := TRUE;
    v_raid_enabled BOOLEAN := TRUE;
    v_status TEXT := 'QUEUED';
    v_outbox_id UUID;
BEGIN
    IF v_actor IS NULL AND session_user NOT IN ('campuslingo_owner', 'campuslingo_worker', 'postgres') THEN
        RAISE EXCEPTION 'not authorized' USING ERRCODE = '42501';
    END IF;

    SELECT COALESCE(NULLIF(timezone, ''), 'Asia/Kuala_Lumpur')
    INTO v_tz
    FROM users
    WHERE id = p_defender_user_id;

    v_quota_date := user_local_quota_date(p_now, COALESCE(v_tz, 'Asia/Kuala_Lumpur'));
    SELECT q.deliver_after, q.quiet_hours_deferred
    INTO v_deliver_after, v_deferred
    FROM compute_quiet_hours_delivery(p_now, COALESCE(v_tz, 'Asia/Kuala_Lumpur')) q;

    SELECT np.push_enabled, np.raid_alerts_enabled
    INTO v_push_enabled, v_raid_enabled
    FROM notification_preferences np
    WHERE np.user_id = p_defender_user_id;

    IF FOUND AND (NOT v_push_enabled OR NOT v_raid_enabled) THEN
        v_status := 'SUPPRESSED_OPT_OUT';
    END IF;

    INSERT INTO notification_outbox (
        user_id, course_id, notification_kind, local_quota_date,
        event_count, summary_text, raid_ids, payload_json,
        quiet_hours_deferred, deliver_after, status, created_at, updated_at
    )
    VALUES (
        p_defender_user_id, p_course_id, 'RAID_ALERT', v_quota_date,
        1, 'Raided 1 time today', ARRAY[p_raid_id],
        jsonb_build_object('schema_version', '1.0.0', 'raid_ids', jsonb_build_array(p_raid_id)),
        v_deferred, v_deliver_after, v_status, p_now, p_now
    )
    ON CONFLICT (user_id, notification_kind, local_quota_date) DO UPDATE
    SET raid_ids = CASE
            WHEN p_raid_id = ANY(notification_outbox.raid_ids) THEN notification_outbox.raid_ids
            ELSE notification_outbox.raid_ids || p_raid_id
        END,
        event_count = CASE
            WHEN p_raid_id = ANY(notification_outbox.raid_ids) THEN COALESCE(array_length(notification_outbox.raid_ids, 1), 1)
            ELSE COALESCE(array_length(notification_outbox.raid_ids, 1), 0) + 1
        END,
        summary_text = CASE
            WHEN (
                CASE
                    WHEN p_raid_id = ANY(notification_outbox.raid_ids) THEN COALESCE(array_length(notification_outbox.raid_ids, 1), 1)
                    ELSE COALESCE(array_length(notification_outbox.raid_ids, 1), 0) + 1
                END
            ) = 1 THEN 'Raided 1 time today'
            ELSE 'Raided ' || (
                CASE
                    WHEN p_raid_id = ANY(notification_outbox.raid_ids) THEN COALESCE(array_length(notification_outbox.raid_ids, 1), 1)
                    ELSE COALESCE(array_length(notification_outbox.raid_ids, 1), 0) + 1
                END
            )::text || ' times today'
        END,
        payload_json = jsonb_build_object(
            'schema_version', '1.0.0',
            'raid_ids', to_jsonb(
                CASE
                    WHEN p_raid_id = ANY(notification_outbox.raid_ids) THEN notification_outbox.raid_ids
                    ELSE notification_outbox.raid_ids || p_raid_id
                END
            )
        ),
        quiet_hours_deferred = notification_outbox.quiet_hours_deferred OR EXCLUDED.quiet_hours_deferred,
        deliver_after = GREATEST(notification_outbox.deliver_after, EXCLUDED.deliver_after),
        status = CASE WHEN v_status = 'SUPPRESSED_OPT_OUT' THEN 'SUPPRESSED_OPT_OUT' ELSE 'QUEUED' END,
        updated_at = p_now
    RETURNING id INTO v_outbox_id;

    RETURN v_outbox_id;
END;
$$;

CREATE OR REPLACE FUNCTION enqueue_shield_digest_outbox_secure(
    p_user_id UUID,
    p_course_id UUID,
    p_node_ids UUID[],
    p_now TIMESTAMPTZ
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
    v_actor UUID := app_actor();
    v_tz TEXT;
    v_quota_date DATE;
    v_deliver_after TIMESTAMPTZ;
    v_deferred BOOLEAN;
    v_push_enabled BOOLEAN := TRUE;
    v_digest_enabled BOOLEAN := TRUE;
    v_status TEXT := 'QUEUED';
    v_dedup_nodes UUID[];
    v_count INTEGER;
    v_summary TEXT;
    v_outbox_id UUID;
BEGIN
    IF v_actor IS NULL AND session_user NOT IN ('campuslingo_owner', 'campuslingo_worker', 'postgres') THEN
        RAISE EXCEPTION 'not authorized' USING ERRCODE = '42501';
    END IF;
    IF p_node_ids IS NULL OR COALESCE(array_length(p_node_ids, 1), 0) = 0 THEN
        RAISE EXCEPTION 'at least one expiring node_id is required' USING ERRCODE = '22023';
    END IF;

    SELECT ARRAY(SELECT DISTINCT n FROM unnest(p_node_ids) AS n) INTO v_dedup_nodes;
    v_count := COALESCE(array_length(v_dedup_nodes, 1), 1);
    v_summary := CASE
        WHEN v_count = 1 THEN '1 shield nearing expiry'
        ELSE v_count::text || ' shields nearing expiry'
    END;

    SELECT COALESCE(NULLIF(timezone, ''), 'Asia/Kuala_Lumpur')
    INTO v_tz
    FROM users
    WHERE id = p_user_id;

    v_quota_date := user_local_quota_date(p_now, COALESCE(v_tz, 'Asia/Kuala_Lumpur'));
    SELECT q.deliver_after, q.quiet_hours_deferred
    INTO v_deliver_after, v_deferred
    FROM compute_quiet_hours_delivery(p_now, COALESCE(v_tz, 'Asia/Kuala_Lumpur')) q;

    SELECT np.push_enabled, np.shield_digest_enabled
    INTO v_push_enabled, v_digest_enabled
    FROM notification_preferences np
    WHERE np.user_id = p_user_id;

    IF FOUND AND (NOT v_push_enabled OR NOT v_digest_enabled) THEN
        v_status := 'SUPPRESSED_OPT_OUT';
    END IF;

    INSERT INTO notification_outbox (
        user_id, course_id, notification_kind, local_quota_date,
        event_count, summary_text, shield_node_ids, payload_json,
        quiet_hours_deferred, deliver_after, status, created_at, updated_at
    )
    VALUES (
        p_user_id, p_course_id, 'SHIELD_DIGEST', v_quota_date,
        v_count, v_summary, v_dedup_nodes,
        jsonb_build_object('schema_version', '1.0.0', 'shield_node_ids', to_jsonb(v_dedup_nodes)),
        v_deferred, v_deliver_after, v_status, p_now, p_now
    )
    ON CONFLICT (user_id, notification_kind, local_quota_date) DO UPDATE
    SET shield_node_ids = ARRAY(
            SELECT DISTINCT n FROM unnest(notification_outbox.shield_node_ids || EXCLUDED.shield_node_ids) AS n
        ),
        event_count = COALESCE(
            array_length(
                ARRAY(SELECT DISTINCT n FROM unnest(notification_outbox.shield_node_ids || EXCLUDED.shield_node_ids) AS n),
                1
            ),
            1
        ),
        summary_text = CASE
            WHEN COALESCE(
                array_length(
                    ARRAY(SELECT DISTINCT n FROM unnest(notification_outbox.shield_node_ids || EXCLUDED.shield_node_ids) AS n),
                    1
                ),
                1
            ) = 1 THEN '1 shield nearing expiry'
            ELSE COALESCE(
                array_length(
                    ARRAY(SELECT DISTINCT n FROM unnest(notification_outbox.shield_node_ids || EXCLUDED.shield_node_ids) AS n),
                    1
                ),
                1
            )::text || ' shields nearing expiry'
        END,
        payload_json = jsonb_build_object(
            'schema_version', '1.0.0',
            'shield_node_ids', to_jsonb(ARRAY(SELECT DISTINCT n FROM unnest(notification_outbox.shield_node_ids || EXCLUDED.shield_node_ids) AS n))
        ),
        quiet_hours_deferred = notification_outbox.quiet_hours_deferred OR EXCLUDED.quiet_hours_deferred,
        deliver_after = GREATEST(notification_outbox.deliver_after, EXCLUDED.deliver_after),
        status = CASE WHEN v_status = 'SUPPRESSED_OPT_OUT' THEN 'SUPPRESSED_OPT_OUT' ELSE 'QUEUED' END,
        updated_at = p_now
    RETURNING id INTO v_outbox_id;

    RETURN v_outbox_id;
END;
$$;

CREATE OR REPLACE FUNCTION dispatch_due_notifications_secure(
    p_user_id UUID,
    p_now TIMESTAMPTZ
)
RETURNS TABLE(
    id UUID,
    notification_kind TEXT,
    summary_text TEXT,
    raid_ids UUID[],
    shield_node_ids UUID[],
    delivered_at TIMESTAMPTZ
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
    v_actor UUID := app_actor();
    v_tz TEXT;
    v_now_quiet BOOLEAN := FALSE;
    v_push_enabled BOOLEAN := TRUE;
    v_raid_enabled BOOLEAN := TRUE;
    v_digest_enabled BOOLEAN := TRUE;
    v_has_device BOOLEAN := FALSE;
BEGIN
    IF v_actor IS NULL OR (v_actor <> p_user_id AND session_user NOT IN ('campuslingo_owner', 'campuslingo_worker', 'postgres')) THEN
        RAISE EXCEPTION 'not authorized' USING ERRCODE = '42501';
    END IF;

    SELECT COALESCE(NULLIF(timezone, ''), 'Asia/Kuala_Lumpur')
    INTO v_tz
    FROM users
    WHERE id = p_user_id;

    SELECT q.quiet_hours_deferred
    INTO v_now_quiet
    FROM compute_quiet_hours_delivery(p_now, COALESCE(v_tz, 'Asia/Kuala_Lumpur')) q;

    IF v_now_quiet THEN
        RETURN;
    END IF;

    SELECT EXISTS (
        SELECT 1 FROM notification_devices nd
        WHERE nd.user_id = p_user_id AND nd.enabled = TRUE
    ) INTO v_has_device;

    IF NOT v_has_device THEN
        RETURN;
    END IF;

    SELECT np.push_enabled, np.raid_alerts_enabled, np.shield_digest_enabled
    INTO v_push_enabled, v_raid_enabled, v_digest_enabled
    FROM notification_preferences np
    WHERE np.user_id = p_user_id;

    IF FOUND AND NOT v_push_enabled THEN
        UPDATE notification_outbox
        SET status = 'SUPPRESSED_OPT_OUT', updated_at = p_now
        WHERE user_id = p_user_id AND status = 'QUEUED';
        RETURN;
    END IF;

    RETURN QUERY
    WITH delivered AS (
        UPDATE notification_outbox nox
        SET status = 'DELIVERED',
            delivered_at = p_now,
            updated_at = p_now
        WHERE nox.user_id = p_user_id
          AND nox.status = 'QUEUED'
          AND nox.deliver_after <= p_now
          AND (
              (nox.notification_kind = 'RAID_ALERT' AND COALESCE(v_raid_enabled, TRUE))
              OR (nox.notification_kind = 'SHIELD_DIGEST' AND COALESCE(v_digest_enabled, TRUE))
          )
        RETURNING nox.id, nox.notification_kind, nox.summary_text, nox.raid_ids, nox.shield_node_ids, nox.delivered_at
    )
    SELECT delivered.id, delivered.notification_kind, delivered.summary_text,
           delivered.raid_ids, delivered.shield_node_ids, delivered.delivered_at
    FROM delivered;
END;
$$;

-- S-07 / O-07: Immutable account erasure & versioned retention sweep audit logs
CREATE TABLE IF NOT EXISTS account_erasure_audit (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id),
    reason TEXT NOT NULL CHECK (length(reason) BETWEEN 1 AND 240),
    retained_review_count INTEGER NOT NULL CHECK (retained_review_count >= 0),
    retained_reward_tx_count INTEGER NOT NULL CHECK (retained_reward_tx_count >= 0),
    retained_match_count INTEGER NOT NULL CHECK (retained_match_count >= 0),
    erased_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);

DROP TRIGGER IF EXISTS immutable_account_erasure_audit ON account_erasure_audit;
CREATE TRIGGER immutable_account_erasure_audit
    BEFORE UPDATE OR DELETE ON account_erasure_audit
    FOR EACH ROW EXECUTE FUNCTION immutable_audit();

CREATE TABLE IF NOT EXISTS retention_sweep_audit (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    course_id UUID NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    archived_versions_count INTEGER NOT NULL CHECK (archived_versions_count >= 0),
    archived_cards_count INTEGER NOT NULL CHECK (archived_cards_count >= 0),
    purged_sessions_count INTEGER NOT NULL CHECK (purged_sessions_count >= 0),
    purged_replays_count INTEGER NOT NULL CHECK (purged_replays_count >= 0),
    swept_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);

DROP TRIGGER IF EXISTS immutable_retention_sweep_audit ON retention_sweep_audit;
CREATE TRIGGER immutable_retention_sweep_audit
    BEFORE UPDATE OR DELETE ON retention_sweep_audit
    FOR EACH ROW EXECUTE FUNCTION immutable_audit();

CREATE OR REPLACE FUNCTION pseudonymize_user_account_secure(
    p_target_user_id UUID,
    p_reason TEXT,
    p_now TIMESTAMPTZ
)
RETURNS TABLE(
    user_id UUID,
    pseudonymized_email TEXT,
    retained_review_count INTEGER,
    retained_reward_tx_count INTEGER,
    retained_match_count INTEGER
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
    v_actor UUID := app_actor();
    v_scrubbed_email TEXT := 'erased-' || replace(p_target_user_id::text, '-', '') || '@erased.invalid';
    v_reviews INTEGER;
    v_rewards INTEGER;
    v_matches INTEGER;
BEGIN
    IF (v_actor IS NULL OR v_actor <> p_target_user_id)
       AND session_user NOT IN ('campuslingo_owner', 'campuslingo_worker', 'postgres') THEN
        RAISE EXCEPTION 'not authorized to pseudonymize user' USING ERRCODE = '42501';
    END IF;

    UPDATE users
    SET email = v_scrubbed_email,
        password_hash = '!ERASED_ACCOUNT!',
        timezone = 'UTC',
        pseudonymized_at = p_now,
        erasure_reason = p_reason
    WHERE id = p_target_user_id;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'user not found';
    END IF;

    DELETE FROM sessions WHERE user_id = p_target_user_id;
    DELETE FROM recovery_tokens WHERE user_id = p_target_user_id;
    DELETE FROM notification_devices WHERE user_id = p_target_user_id;
    DELETE FROM notification_outbox WHERE user_id = p_target_user_id AND status <> 'DELIVERED';

    INSERT INTO notification_preferences (
        user_id, push_enabled, raid_alerts_enabled, shield_digest_enabled, updated_at
    )
    VALUES (p_target_user_id, FALSE, FALSE, FALSE, p_now)
    ON CONFLICT (user_id) DO UPDATE
    SET push_enabled = FALSE,
        raid_alerts_enabled = FALSE,
        shield_digest_enabled = FALSE,
        updated_at = p_now;

    UPDATE course_members
    SET active = FALSE,
        raid_opt_in = FALSE
    WHERE user_id = p_target_user_id;

    SELECT COUNT(*)::INTEGER INTO v_reviews FROM review_logs WHERE user_id = p_target_user_id;
    SELECT COUNT(*)::INTEGER INTO v_rewards FROM reward_transactions WHERE user_id = p_target_user_id;
    SELECT COUNT(*)::INTEGER INTO v_matches
    FROM matches
    WHERE player_a_id = p_target_user_id OR player_b_id = p_target_user_id;

    INSERT INTO account_erasure_audit (
        user_id, reason, retained_review_count, retained_reward_tx_count, retained_match_count, erased_at
    )
    VALUES (p_target_user_id, p_reason, v_reviews, v_rewards, v_matches, p_now);

    RETURN QUERY SELECT p_target_user_id, v_scrubbed_email, v_reviews, v_rewards, v_matches;
END;
$$;

CREATE OR REPLACE FUNCTION apply_versioned_retention_sweep_secure(
    p_course_id UUID,
    p_now TIMESTAMPTZ
)
RETURNS TABLE(
    archived_versions_count INTEGER,
    archived_cards_count INTEGER,
    purged_sessions_count INTEGER,
    purged_replays_count INTEGER
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
    v_actor UUID := app_actor();
    v_arch_versions INTEGER := 0;
    v_arch_cards INTEGER := 0;
    v_purged_sessions INTEGER := 0;
    v_purged_replays INTEGER := 0;
BEGIN
    IF (v_actor IS NULL OR NOT can_manage_course(p_course_id))
       AND session_user NOT IN ('campuslingo_owner', 'campuslingo_worker', 'postgres') THEN
        RAISE EXCEPTION 'not authorized to run retention sweep' USING ERRCODE = '42501';
    END IF;

    -- S-07: Non-destructive archival of superseded content versions without touching review_logs or fsrs_cards
    WITH latest_published AS (
        SELECT MAX(version_number) AS max_ver
        FROM content_versions
        WHERE course_id = p_course_id
          AND status = 'published'
          AND NOT is_quarantined
    ),
    archived_v AS (
        UPDATE content_versions cv
        SET status = 'archived'
        FROM latest_published lp
        WHERE cv.course_id = p_course_id
          AND cv.status = 'published'
          AND lp.max_ver IS NOT NULL
          AND cv.version_number < lp.max_ver
          AND COALESCE(cv.activated_at, cv.created_at) <= p_now - INTERVAL '30 days'
        RETURNING cv.id
    )
    SELECT COUNT(*)::INTEGER INTO v_arch_versions FROM archived_v;

    WITH archived_c AS (
        UPDATE content_version_cards cvc
        SET archived_at = COALESCE(cvc.archived_at, p_now)
        FROM content_versions cv
        WHERE cvc.content_version_id = cv.id
          AND cvc.course_id = p_course_id
          AND cv.status IN ('archived')
          AND cvc.archived_at IS NULL
        RETURNING cvc.card_id
    )
    SELECT COUNT(*)::INTEGER INTO v_arch_cards FROM archived_c;

    WITH del_sessions AS (
        DELETE FROM sessions
        WHERE expires_at <= p_now
        RETURNING token_hash
    )
    SELECT COUNT(*)::INTEGER INTO v_purged_sessions FROM del_sessions;

    WITH del_replays AS (
        DELETE FROM match_replay_events mre
        WHERE mre.course_id = p_course_id
          AND mre.expires_at <= p_now
          AND NOT EXISTS (
              SELECT 1 FROM matches m
              WHERE m.id = mre.match_id
                AND m.status IN ('MATCH_STARTED', 'IN_PROGRESS', 'SUDDEN_DEATH')
          )
        RETURNING mre.id
    )
    SELECT COUNT(*)::INTEGER INTO v_purged_replays FROM del_replays;

    INSERT INTO retention_sweep_audit (
        course_id, archived_versions_count, archived_cards_count,
        purged_sessions_count, purged_replays_count, swept_at
    )
    VALUES (
        p_course_id, v_arch_versions, v_arch_cards,
        v_purged_sessions, v_purged_replays, p_now
    );

    RETURN QUERY SELECT v_arch_versions, v_arch_cards, v_purged_sessions, v_purged_replays;
END;
$$;

-- RLS policies for M7 tables
ALTER TABLE notification_devices ENABLE ROW LEVEL SECURITY;
ALTER TABLE notification_preferences ENABLE ROW LEVEL SECURITY;
ALTER TABLE notification_outbox ENABLE ROW LEVEL SECURITY;
ALTER TABLE account_erasure_audit ENABLE ROW LEVEL SECURITY;
ALTER TABLE retention_sweep_audit ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS notif_devices_owner ON notification_devices;
CREATE POLICY notif_devices_owner ON notification_devices FOR ALL
    USING (user_id = app_actor()) WITH CHECK (user_id = app_actor());

DROP POLICY IF EXISTS notif_prefs_owner ON notification_preferences;
CREATE POLICY notif_prefs_owner ON notification_preferences FOR ALL
    USING (user_id = app_actor()) WITH CHECK (user_id = app_actor());

DROP POLICY IF EXISTS notif_outbox_owner ON notification_outbox;
CREATE POLICY notif_outbox_owner ON notification_outbox FOR SELECT
    USING (user_id = app_actor());

DROP POLICY IF EXISTS account_erasure_owner ON account_erasure_audit;
CREATE POLICY account_erasure_owner ON account_erasure_audit FOR SELECT
    USING (user_id = app_actor());

DROP POLICY IF EXISTS retention_sweep_manager ON retention_sweep_audit;
CREATE POLICY retention_sweep_manager ON retention_sweep_audit FOR SELECT
    USING (can_manage_course(course_id));
