-- Runtime grants for M3 content generation tables.
-- Run as campuslingo_owner after sqlx migrate run.

GRANT SELECT, INSERT, UPDATE, DELETE ON
    content_versions,
    learning_unit_bundles,
    node_simulations,
    axiom_packs,
    axiom_pack_items,
    node_audio_briefings
    TO campuslingo_app;

GRANT SELECT, INSERT, UPDATE, DELETE ON
    content_versions,
    learning_unit_bundles,
    node_simulations,
    axiom_packs,
    axiom_pack_items,
    node_audio_briefings
    TO campuslingo_worker;
