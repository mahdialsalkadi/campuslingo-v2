-- Least-privilege role grants for Milestone M5 duel, replay, audit, and R-07 recovery tables.

REVOKE ALL ON matches, match_snapshots, match_replay_events, match_settlements, user_combat_telemetry FROM PUBLIC;
REVOKE ALL ON matches, match_snapshots, match_replay_events, match_settlements, user_combat_telemetry FROM campuslingo_auth;

GRANT SELECT ON matches, match_snapshots, match_replay_events, match_settlements, user_combat_telemetry TO campuslingo_app;
GRANT SELECT ON matches, match_snapshots, match_replay_events, match_settlements, user_combat_telemetry TO campuslingo_grader;

GRANT SELECT, INSERT, UPDATE, DELETE ON matches, match_snapshots, match_replay_events, match_settlements, user_combat_telemetry TO campuslingo_worker;
GRANT USAGE, SELECT ON SEQUENCE match_replay_events_id_seq TO campuslingo_worker;

GRANT SELECT, INSERT, UPDATE ON matches, match_snapshots, match_replay_events, match_settlements, user_combat_telemetry TO campuslingo_recovery;
GRANT USAGE, SELECT ON SEQUENCE match_replay_events_id_seq TO campuslingo_recovery;

GRANT EXECUTE ON FUNCTION user_local_quota_date(TIMESTAMPTZ, TEXT) TO campuslingo_app, campuslingo_grader, campuslingo_worker, campuslingo_recovery;
GRANT EXECUTE ON FUNCTION create_match_record(UUID, UUID, UUID, UUID, TEXT, UUID, TEXT, INTEGER, JSONB) TO campuslingo_app, campuslingo_worker, campuslingo_recovery;
GRANT EXECUTE ON FUNCTION save_match_snapshot_secure(UUID, UUID, INTEGER, INTEGER, JSONB) TO campuslingo_app, campuslingo_worker, campuslingo_recovery;
GRANT EXECUTE ON FUNCTION append_match_replay_event_secure(UUID, UUID, TEXT, INTEGER, TEXT, INTEGER, JSONB) TO campuslingo_app, campuslingo_grader, campuslingo_worker, campuslingo_recovery;
GRANT EXECUTE ON FUNCTION record_duel_review_secure(UUID, UUID, UUID, UUID, UUID, TEXT, SMALLINT, INTEGER, BOOLEAN, TEXT, TIMESTAMPTZ, REAL, REAL, INTEGER) TO campuslingo_grader, campuslingo_worker, campuslingo_recovery;
GRANT EXECUTE ON FUNCTION settle_match_atomic(UUID, UUID, TEXT, UUID, BOOLEAN, TEXT, INTEGER) TO campuslingo_app, campuslingo_grader, campuslingo_worker, campuslingo_recovery;
