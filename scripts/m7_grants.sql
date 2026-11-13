-- Least-privilege role grants for Milestone M7 Notifications, Pseudonymization, and Retention.

REVOKE ALL ON
    notification_devices,
    notification_preferences,
    notification_outbox,
    account_erasure_audit,
    retention_sweep_audit
FROM PUBLIC, campuslingo_auth, campuslingo_grader;

GRANT SELECT, INSERT, UPDATE, DELETE ON
    notification_devices,
    notification_preferences
TO campuslingo_app;

GRANT SELECT ON
    notification_outbox,
    account_erasure_audit,
    retention_sweep_audit
TO campuslingo_app;

GRANT SELECT, INSERT, UPDATE, DELETE ON
    notification_devices,
    notification_preferences,
    notification_outbox
TO campuslingo_worker, campuslingo_recovery;

GRANT SELECT, INSERT ON
    account_erasure_audit,
    retention_sweep_audit
TO campuslingo_worker;

GRANT EXECUTE ON FUNCTION compute_quiet_hours_delivery(TIMESTAMPTZ, TEXT)
    TO campuslingo_app, campuslingo_worker, campuslingo_recovery;
GRANT EXECUTE ON FUNCTION register_notification_device_secure(TEXT, TEXT, TEXT, TEXT)
    TO campuslingo_app;
GRANT EXECUTE ON FUNCTION upsert_notification_preferences_secure(BOOLEAN, BOOLEAN, BOOLEAN)
    TO campuslingo_app;
GRANT EXECUTE ON FUNCTION enqueue_raid_alert_outbox_secure(UUID, UUID, UUID, TIMESTAMPTZ)
    TO campuslingo_app, campuslingo_worker;
GRANT EXECUTE ON FUNCTION enqueue_shield_digest_outbox_secure(UUID, UUID, UUID[], TIMESTAMPTZ)
    TO campuslingo_app, campuslingo_worker;
GRANT EXECUTE ON FUNCTION dispatch_due_notifications_secure(UUID, TIMESTAMPTZ)
    TO campuslingo_app, campuslingo_worker;
GRANT EXECUTE ON FUNCTION pseudonymize_user_account_secure(UUID, TEXT, TIMESTAMPTZ)
    TO campuslingo_app, campuslingo_worker;
GRANT EXECUTE ON FUNCTION apply_versioned_retention_sweep_secure(UUID, TIMESTAMPTZ)
    TO campuslingo_app, campuslingo_worker;
