-- M3 generation tables: content versions, learning unit bundles, verified sandbox
-- simulations, Stage-3 AXIOM packs, clause-concatenated audio briefings, and
-- expanded private asset media types for Opus audio.

CREATE TABLE content_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    course_id UUID NOT NULL,
    version_number INT NOT NULL CHECK (version_number >= 1),
    source_run_id UUID REFERENCES ingestion_runs(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'published', 'archived')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (id, tenant_id, course_id),
    UNIQUE (course_id, version_number),
    FOREIGN KEY (course_id, tenant_id) REFERENCES courses(id, tenant_id) ON DELETE CASCADE
);

CREATE TABLE learning_unit_bundles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    course_id UUID NOT NULL,
    node_id UUID NOT NULL,
    content_version_id UUID NOT NULL,
    archetype TEXT NOT NULL,
    modifier TEXT NOT NULL,
    card_elixir_cost SMALLINT NOT NULL CHECK (card_elixir_cost BETWEEN 1 AND 6),
    world_theme TEXT NOT NULL CHECK (world_theme IN ('BIO_SYSTEM', 'CYBER_GRID', 'MACRO_POLIS', 'ARCANE_ARCHIVE')),
    language TEXT NOT NULL CHECK (language IN ('en', 'ar')),
    grid_x INT NOT NULL CHECK (grid_x >= 0),
    grid_y INT NOT NULL CHECK (grid_y >= 0),
    bundle_json JSONB NOT NULL,
    source_sha256 TEXT NOT NULL CHECK (source_sha256 ~ '^[0-9a-f]{64}$'),
    generator_model_id TEXT NOT NULL CHECK (length(generator_model_id) BETWEEN 1 AND 128),
    generator_revision TEXT NOT NULL CHECK (length(generator_revision) BETWEEN 1 AND 64),
    generator_version TEXT NOT NULL CHECK (length(generator_version) BETWEEN 1 AND 64),
    schema_version TEXT NOT NULL DEFAULT '1.0.0' CHECK (schema_version = '1.0.0'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (id, tenant_id, course_id),
    UNIQUE (node_id, content_version_id),
    UNIQUE (course_id, content_version_id, grid_x, grid_y),
    FOREIGN KEY (node_id, tenant_id, course_id) REFERENCES learning_nodes(id, tenant_id, course_id) ON DELETE CASCADE,
    FOREIGN KEY (content_version_id, tenant_id, course_id) REFERENCES content_versions(id, tenant_id, course_id) ON DELETE CASCADE,
    CONSTRAINT learning_unit_bundles_pair_check CHECK (
        (archetype = 'PIPELINE' AND modifier IN ('REVERSE_ENG', 'BOTTLENECK_RELIEF', 'FAULT_INJECTION')) OR
        (archetype = 'TRADEOFF' AND modifier IN ('OVERLOAD_STRESS', 'BUDGET_CAP', 'PARETO_BALANCE')) OR
        (archetype = 'SPATIAL' AND modifier IN ('BLIND_DIAGNOSIS', 'SURGICAL_REPAIR', 'CROSS_SECTION_SCAN')) OR
        (archetype = 'CONTRAST' AND modifier IN ('STRESS_BENCHMARK', 'BLIND_CHAMBER_ID', 'CROSS_CALIBRATION')) OR
        (archetype = 'TRAP' AND modifier IN ('FORENSIC_AUDIT', 'COUNTEREXAMPLE_PROBE', 'SAFEGUARD_PATCH')) OR
        (archetype = 'AXIOM' AND modifier IN ('SPEED_BLITZ', 'COROLLARY_CHAIN', 'BOUNDARY_TEST'))
    ),
    CONSTRAINT learning_unit_bundles_json_shape CHECK (jsonb_typeof(bundle_json) = 'object')
);

CREATE TABLE node_simulations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    course_id UUID NOT NULL,
    node_id UUID NOT NULL,
    content_version_id UUID NOT NULL,
    spec_json JSONB NOT NULL,
    solution_witness JSONB NOT NULL,
    states_checked INT NOT NULL CHECK (states_checked BETWEEN 1 AND 4096),
    proof_domain TEXT NOT NULL CHECK (proof_domain = 'exhaustive_declared_discrete_grid'),
    verified BOOLEAN NOT NULL CHECK (verified = TRUE),
    verified_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (id, tenant_id, course_id),
    UNIQUE (node_id, content_version_id),
    FOREIGN KEY (node_id, tenant_id, course_id) REFERENCES learning_nodes(id, tenant_id, course_id) ON DELETE CASCADE,
    FOREIGN KEY (content_version_id, tenant_id, course_id) REFERENCES content_versions(id, tenant_id, course_id) ON DELETE CASCADE,
    CONSTRAINT node_simulations_spec_shape CHECK (
        jsonb_typeof(spec_json) = 'object'
        AND jsonb_typeof(spec_json -> 'parameters') = 'array'
        AND jsonb_array_length(spec_json -> 'parameters') BETWEEN 1 AND 4
        AND jsonb_typeof(spec_json -> 'targets') = 'array'
        AND jsonb_array_length(spec_json -> 'targets') BETWEEN 1 AND 8
        AND jsonb_typeof(solution_witness) = 'object'
    )
);

CREATE TABLE axiom_packs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    course_id UUID NOT NULL,
    node_id UUID NOT NULL,
    content_version_id UUID NOT NULL,
    archetype TEXT NOT NULL,
    modifier TEXT NOT NULL,
    w_base_ms INT NOT NULL CHECK (w_base_ms BETWEEN 8000 AND 15000),
    card_elixir_cost SMALLINT NOT NULL CHECK (card_elixir_cost BETWEEN 1 AND 6),
    prompt_count SMALLINT NOT NULL CHECK (prompt_count BETWEEN 2 AND 4),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (id, tenant_id, course_id),
    UNIQUE (node_id, content_version_id),
    FOREIGN KEY (node_id, tenant_id, course_id) REFERENCES learning_nodes(id, tenant_id, course_id) ON DELETE CASCADE,
    FOREIGN KEY (content_version_id, tenant_id, course_id) REFERENCES content_versions(id, tenant_id, course_id) ON DELETE CASCADE,
    CONSTRAINT axiom_packs_stage3_pair_check CHECK (
        (archetype = 'PIPELINE' AND modifier IN ('REVERSE_ENG', 'BOTTLENECK_RELIEF', 'FAULT_INJECTION')) OR
        (archetype = 'TRADEOFF' AND modifier IN ('OVERLOAD_STRESS', 'BUDGET_CAP', 'PARETO_BALANCE')) OR
        (archetype = 'SPATIAL' AND modifier IN ('BLIND_DIAGNOSIS', 'SURGICAL_REPAIR', 'CROSS_SECTION_SCAN')) OR
        (archetype = 'CONTRAST' AND modifier IN ('STRESS_BENCHMARK', 'BLIND_CHAMBER_ID', 'CROSS_CALIBRATION')) OR
        (archetype = 'TRAP' AND modifier IN ('FORENSIC_AUDIT', 'COUNTEREXAMPLE_PROBE', 'SAFEGUARD_PATCH')) OR
        (archetype = 'AXIOM' AND modifier IN ('COROLLARY_CHAIN', 'BOUNDARY_TEST'))
    )
);

CREATE TABLE axiom_pack_items (
    pack_id UUID NOT NULL,
    tenant_id UUID NOT NULL,
    course_id UUID NOT NULL,
    position SMALLINT NOT NULL CHECK (position BETWEEN 0 AND 3),
    card_id UUID NOT NULL REFERENCES challenges(id) ON DELETE CASCADE,
    w_base_ms INT NOT NULL CHECK (w_base_ms BETWEEN 8000 AND 15000),
    PRIMARY KEY (pack_id, position),
    UNIQUE (pack_id, card_id),
    FOREIGN KEY (pack_id, tenant_id, course_id) REFERENCES axiom_packs(id, tenant_id, course_id) ON DELETE CASCADE
);

CREATE TABLE node_audio_briefings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    course_id UUID NOT NULL,
    node_id UUID NOT NULL,
    content_version_id UUID NOT NULL,
    asset_id UUID NOT NULL REFERENCES assets(id) ON DELETE RESTRICT,
    language TEXT NOT NULL CHECK (language IN ('en', 'ar')),
    codec TEXT NOT NULL CHECK (codec = 'opus'),
    alignment_method TEXT NOT NULL CHECK (alignment_method IN ('CLAUSE_CONCAT', 'model_durations', 'forced_alignment', 'asr_estimate')),
    duration_ms INT NOT NULL CHECK (duration_ms > 0),
    manifest_json JSONB NOT NULL,
    director_tracks_json JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (id, tenant_id, course_id),
    UNIQUE (node_id, content_version_id, language),
    FOREIGN KEY (node_id, tenant_id, course_id) REFERENCES learning_nodes(id, tenant_id, course_id) ON DELETE CASCADE,
    FOREIGN KEY (content_version_id, tenant_id, course_id) REFERENCES content_versions(id, tenant_id, course_id) ON DELETE CASCADE,
    CONSTRAINT node_audio_briefings_json_shapes CHECK (
        jsonb_typeof(manifest_json) = 'object'
        AND jsonb_typeof(director_tracks_json) = 'array'
    )
);

DO $$
DECLARE
    c_name TEXT;
BEGIN
    SELECT con.conname INTO c_name
    FROM pg_constraint con
    JOIN pg_class rel ON rel.oid = con.conrelid
    WHERE rel.relname = 'assets'
      AND con.contype = 'c'
      AND pg_get_constraintdef(con.oid) LIKE '%media_type%';
    IF c_name IS NOT NULL THEN
        EXECUTE format('ALTER TABLE assets DROP CONSTRAINT %I', c_name);
    END IF;
END $$;

ALTER TABLE assets
    ADD CONSTRAINT assets_media_type_check CHECK (
        media_type IN (
            'application/pdf',
            'image/png',
            'image/jpeg',
            'image/webp',
            'text/plain',
            'audio/ogg',
            'audio/opus'
        )
    );

ALTER TABLE content_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE content_versions FORCE ROW LEVEL SECURITY;
ALTER TABLE learning_unit_bundles ENABLE ROW LEVEL SECURITY;
ALTER TABLE learning_unit_bundles FORCE ROW LEVEL SECURITY;
ALTER TABLE node_simulations ENABLE ROW LEVEL SECURITY;
ALTER TABLE node_simulations FORCE ROW LEVEL SECURITY;
ALTER TABLE axiom_packs ENABLE ROW LEVEL SECURITY;
ALTER TABLE axiom_packs FORCE ROW LEVEL SECURITY;
ALTER TABLE axiom_pack_items ENABLE ROW LEVEL SECURITY;
ALTER TABLE axiom_pack_items FORCE ROW LEVEL SECURITY;
ALTER TABLE node_audio_briefings ENABLE ROW LEVEL SECURITY;
ALTER TABLE node_audio_briefings FORCE ROW LEVEL SECURITY;

CREATE POLICY content_versions_select ON content_versions
    FOR SELECT USING (can_read_course(course_id));
CREATE POLICY content_versions_manage ON content_versions
    FOR ALL USING (can_manage_course(course_id))
    WITH CHECK (can_manage_course(course_id));

CREATE POLICY learning_unit_bundles_select ON learning_unit_bundles
    FOR SELECT USING (can_read_course(course_id));
CREATE POLICY learning_unit_bundles_manage ON learning_unit_bundles
    FOR ALL USING (can_manage_course(course_id))
    WITH CHECK (can_manage_course(course_id));

CREATE POLICY node_simulations_select ON node_simulations
    FOR SELECT USING (can_read_course(course_id));
CREATE POLICY node_simulations_manage ON node_simulations
    FOR ALL USING (can_manage_course(course_id))
    WITH CHECK (can_manage_course(course_id));

CREATE POLICY axiom_packs_select ON axiom_packs
    FOR SELECT USING (can_read_course(course_id));
CREATE POLICY axiom_packs_manage ON axiom_packs
    FOR ALL USING (can_manage_course(course_id))
    WITH CHECK (can_manage_course(course_id));

CREATE POLICY axiom_pack_items_select ON axiom_pack_items
    FOR SELECT USING (can_read_course(course_id));
CREATE POLICY axiom_pack_items_manage ON axiom_pack_items
    FOR ALL USING (can_manage_course(course_id))
    WITH CHECK (can_manage_course(course_id));

CREATE POLICY node_audio_briefings_select ON node_audio_briefings
    FOR SELECT USING (can_read_course(course_id));
CREATE POLICY node_audio_briefings_manage ON node_audio_briefings
    FOR ALL USING (can_manage_course(course_id))
    WITH CHECK (can_manage_course(course_id));
