-- M1 least-privilege grants. Run as the migration owner after migrations.
-- Roles themselves are created by the deployment harness, never by SQL that
-- could be replayed with default credentials.

REVOKE ALL ON SCHEMA public FROM PUBLIC;
GRANT USAGE ON SCHEMA public TO
    campuslingo_auth,
    campuslingo_app,
    campuslingo_grader,
    campuslingo_worker,
    campuslingo_recovery;

-- Authentication material is isolated from tenants, courses, and grading.
GRANT SELECT, INSERT, UPDATE ON users TO campuslingo_auth;
GRANT SELECT, INSERT, DELETE ON sessions TO campuslingo_auth;
GRANT SELECT, INSERT, UPDATE, DELETE ON recovery_tokens TO campuslingo_auth;
GRANT INSERT ON recovery_outbox TO campuslingo_auth;

-- The separate delivery process can claim and mark outbox rows, but it cannot
-- read users, sessions, passwords, or recovery token hashes.
GRANT SELECT, UPDATE ON recovery_outbox TO campuslingo_recovery;

-- The application role can author content and read public projections, but it
-- never receives SELECT on users or challenge_answers.
GRANT SELECT, INSERT, UPDATE, DELETE ON
    tenants,
    tenant_members,
    courses,
    course_members,
    learning_nodes,
    challenges,
    assets,
    attempts
TO campuslingo_app;
GRANT SELECT, INSERT ON jobs TO campuslingo_app;
GRANT SELECT ON job_results TO campuslingo_app;
GRANT EXECUTE ON FUNCTION add_challenge_answer(UUID, SMALLINT, TEXT) TO campuslingo_app;

-- Grading is the only runtime path to challenge_answers. The grader can move
-- its own attempt/card state and append the review/reward pair, but cannot
-- author courses or inspect users.
GRANT SELECT, UPDATE ON attempts TO campuslingo_grader;
GRANT SELECT ON challenge_answers TO campuslingo_grader;
GRANT SELECT, INSERT, UPDATE ON fsrs_cards TO campuslingo_grader;
GRANT SELECT, INSERT ON review_logs TO campuslingo_grader;
GRANT SELECT, INSERT ON reward_ledger TO campuslingo_grader;

-- Workers own durable job progress. Their credentials never reach the HTTP
-- server, and BYPASSRLS is deliberate because they are not acting as a learner.
GRANT SELECT, UPDATE ON jobs TO campuslingo_worker;
GRANT SELECT, INSERT, UPDATE ON job_results TO campuslingo_worker;
GRANT SELECT, INSERT ON job_events TO campuslingo_worker;
GRANT USAGE, SELECT ON SEQUENCE job_events_id_seq TO campuslingo_worker;

-- Helper functions are invoked both directly and from RLS policy expressions.
-- Revoke the default PUBLIC execute and grant only to roles that evaluate them.
REVOKE ALL ON FUNCTION immutable_audit() FROM PUBLIC;
REVOKE ALL ON FUNCTION app_actor() FROM PUBLIC;
REVOKE ALL ON FUNCTION is_tenant_member(UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION is_tenant_owner(UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION can_read_course(UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION can_manage_course(UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION can_study_course(UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION can_self_enroll(UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION add_challenge_answer(UUID, SMALLINT, TEXT) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION
    immutable_audit(),
    app_actor(),
    is_tenant_member(UUID),
    is_tenant_owner(UUID),
    can_read_course(UUID),
    can_manage_course(UUID),
    can_study_course(UUID),
    can_self_enroll(UUID)
TO campuslingo_app, campuslingo_grader, campuslingo_worker;
GRANT EXECUTE ON FUNCTION add_challenge_answer(UUID, SMALLINT, TEXT) TO campuslingo_app;
