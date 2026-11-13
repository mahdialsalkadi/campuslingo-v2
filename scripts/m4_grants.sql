-- Apply after M4 migration; worker role stays separate from HTTP roles.
REVOKE ALL ON FUNCTION
    user_has_tenant_scope(UUID),
    user_account_created_at(UUID),
    actor_course_fsrs_card_states(UUID, UUID),
    is_report_cluster_eligible(UUID, UUID, TEXT),
    quarantine_content_version_if_threshold(UUID, UUID, INT)
    FROM PUBLIC;

GRANT EXECUTE ON FUNCTION
    user_has_tenant_scope(UUID),
    user_account_created_at(UUID),
    actor_course_fsrs_card_states(UUID, UUID),
    is_report_cluster_eligible(UUID, UUID, TEXT),
    quarantine_content_version_if_threshold(UUID, UUID, INT)
    TO campuslingo_app, campuslingo_grader, campuslingo_worker;

GRANT SELECT, INSERT, UPDATE ON
    content_version_cards,
    user_fsrs_parameters,
    user_protection_state,
    user_dormancy_periods,
    user_node_protection,
    user_card_version_grace,
    user_node_shield_state,
    shield_recompute_queue
    TO campuslingo_app, campuslingo_worker;

GRANT SELECT, INSERT, UPDATE, DELETE ON node_prerequisites
    TO campuslingo_app, campuslingo_worker;

GRANT SELECT, INSERT ON
    user_node_grace_ledger,
    user_course_version_seen,
    user_node_version_engagement,
    node_reports
    TO campuslingo_app;

GRANT SELECT, INSERT, UPDATE ON
    user_node_grace_ledger,
    user_course_version_seen,
    user_node_version_engagement,
    node_reports
    TO campuslingo_worker;

-- Worker & harness quarantine can mark versions/chunks quarantined without deleting review_logs.
GRANT SELECT ON learning_nodes, challenges TO campuslingo_worker;
GRANT UPDATE ON content_versions, ingestion_chunks TO campuslingo_worker;
GRANT SELECT, UPDATE ON user_protection_state, user_dormancy_periods TO campuslingo_grader;
