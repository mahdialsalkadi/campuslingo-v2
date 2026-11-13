-- M2 only. Existing migrations are immutable. Provision pgvector as a DB
-- administrator before this migration; no extension/bootstrap DDL at startup.
CREATE EXTENSION IF NOT EXISTS vector;
ALTER TABLE assets ADD CONSTRAINT assets_scope_unique UNIQUE (id, tenant_id, course_id);
ALTER TABLE jobs ADD CONSTRAINT jobs_scope_unique UNIQUE (id, tenant_id, course_id);
ALTER TABLE jobs DROP CONSTRAINT jobs_status_check;
ALTER TABLE jobs ADD CONSTRAINT jobs_status_check
    CHECK (status IN ('queued','running','needs_review','succeeded','failed'));

CREATE TABLE ingestion_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    course_id UUID NOT NULL,
    job_id UUID NOT NULL UNIQUE,
    source_asset_id UUID NOT NULL,
    input_sha256 TEXT NOT NULL CHECK (input_sha256 ~ '^[0-9a-f]{64}$'),
    pipeline_version TEXT NOT NULL CHECK (length(pipeline_version) BETWEEN 1 AND 128),
    provenance JSONB NOT NULL CHECK (provenance @> '{"schema_version":1}'::jsonb),
    source_pages INT NOT NULL CHECK (source_pages BETWEEN 1 AND 200),
    status TEXT NOT NULL DEFAULT 'running' CHECK (status IN ('running','needs_review','succeeded','failed')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (id,tenant_id,course_id),
    FOREIGN KEY (job_id,tenant_id,course_id) REFERENCES jobs(id,tenant_id,course_id),
    FOREIGN KEY (source_asset_id,tenant_id,course_id) REFERENCES assets(id,tenant_id,course_id)
);

CREATE TABLE ingestion_pages (
    run_id UUID NOT NULL,
    tenant_id UUID NOT NULL,
    course_id UUID NOT NULL,
    page_number INT NOT NULL CHECK (page_number BETWEEN 1 AND 200),
    artifact_sha256 TEXT NOT NULL CHECK (artifact_sha256 ~ '^[0-9a-f]{64}$'),
    status TEXT NOT NULL CHECK (status IN ('ready','needs_review')),
    artifact JSONB NOT NULL CHECK (artifact @> '{"schema_version":1}'::jsonb),
    checkpointed_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (run_id,page_number),
    UNIQUE (run_id,page_number,tenant_id,course_id),
    FOREIGN KEY (run_id,tenant_id,course_id) REFERENCES ingestion_runs(id,tenant_id,course_id)
);

CREATE TABLE ingestion_crops (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    course_id UUID NOT NULL,
    run_id UUID NOT NULL,
    page_number INT NOT NULL,
    asset_id UUID NOT NULL UNIQUE,
    region_index INT NOT NULL CHECK (region_index >= 0),
    dpi INT NOT NULL CHECK (dpi = 250),
    width INT NOT NULL CHECK (width BETWEEN 8 AND 8192),
    height INT NOT NULL CHECK (height BETWEEN 8 AND 8192),
    bbox JSONB NOT NULL CHECK (bbox @> '{"schema_version":1}'::jsonb),
    UNIQUE (run_id,page_number,region_index),
    UNIQUE (id,tenant_id,course_id,run_id),
    CHECK (width::BIGINT * height <= 24000000),
    FOREIGN KEY (run_id,page_number,tenant_id,course_id) REFERENCES ingestion_pages(run_id,page_number,tenant_id,course_id),
    FOREIGN KEY (asset_id,tenant_id,course_id) REFERENCES assets(id,tenant_id,course_id)
);

CREATE TABLE ingestion_chunks (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    course_id UUID NOT NULL,
    run_id UUID NOT NULL,
    page_number INT NOT NULL,
    chunk_order INT NOT NULL CHECK (chunk_order >= 0),
    source_text TEXT NOT NULL CHECK (length(source_text) BETWEEN 1 AND 8388608),
    normalized_candidate TEXT NOT NULL,
    normalization_version TEXT NOT NULL,
    text_sha256 TEXT NOT NULL CHECK (text_sha256 ~ '^[0-9a-f]{64}$'),
    structural_fingerprint TEXT NOT NULL CHECK (structural_fingerprint ~ '^[0-9a-f]{64}$'),
    critical_tokens JSONB NOT NULL CHECK (critical_tokens @> '{"schema_version":1}'::jsonb),
    language_code TEXT NOT NULL DEFAULT 'en',
    archetype TEXT NOT NULL DEFAULT 'UNCLASSIFIED',
    modifier TEXT NOT NULL DEFAULT 'NONE',
    generator_version TEXT NOT NULL DEFAULT 'extraction-only',
    is_quarantined BOOLEAN NOT NULL DEFAULT FALSE,
    duplicate_of UUID,
    embedding vector(1024),
    embedding_model_id TEXT,
    embedding_revision TEXT,
    embedding_dim INT,
    preprocess_version TEXT,
    UNIQUE (id,tenant_id,course_id,run_id),
    UNIQUE (run_id,page_number,chunk_order),
    FOREIGN KEY (run_id,page_number,tenant_id,course_id) REFERENCES ingestion_pages(run_id,page_number,tenant_id,course_id),
    FOREIGN KEY (duplicate_of,tenant_id,course_id,run_id) REFERENCES ingestion_chunks(id,tenant_id,course_id,run_id) DEFERRABLE INITIALLY DEFERRED,
    CHECK ((embedding IS NULL AND embedding_model_id IS NULL AND embedding_revision IS NULL AND embedding_dim IS NULL AND preprocess_version IS NULL)
        OR (embedding IS NOT NULL AND embedding_model_id IS NOT NULL AND embedding_revision IS NOT NULL AND embedding_dim IS NOT NULL AND embedding_dim = 1024 AND preprocess_version IS NOT NULL))
);
CREATE INDEX ingestion_fingerprint_scope ON ingestion_chunks
    (tenant_id,course_id,language_code,archetype,modifier,generator_version,structural_fingerprint,text_sha256)
    WHERE NOT is_quarantined;
CREATE INDEX ingestion_embedding_hnsw ON ingestion_chunks USING hnsw (embedding vector_cosine_ops)
    WHERE NOT is_quarantined AND embedding IS NOT NULL;

CREATE TABLE ingestion_chunk_crops (
    tenant_id UUID NOT NULL,
    course_id UUID NOT NULL,
    run_id UUID NOT NULL,
    chunk_id UUID NOT NULL,
    crop_id UUID NOT NULL,
    PRIMARY KEY (chunk_id,crop_id),
    FOREIGN KEY (chunk_id,tenant_id,course_id,run_id) REFERENCES ingestion_chunks(id,tenant_id,course_id,run_id),
    FOREIGN KEY (crop_id,tenant_id,course_id,run_id) REFERENCES ingestion_crops(id,tenant_id,course_id,run_id)
);

-- Draft/raw private derivatives do not automatically become public merely
-- because somebody later publishes the course shell.
CREATE FUNCTION can_read_ingestion(target UUID) RETURNS BOOLEAN
LANGUAGE SQL STABLE SECURITY DEFINER SET search_path=public,pg_temp AS $$
    SELECT can_manage_course(target) OR EXISTS (
        SELECT 1 FROM course_members WHERE course_id=target AND user_id=app_actor() AND active
    )
$$;
CREATE FUNCTION ingestion_asset_allowed(target UUID, target_course UUID) RETURNS BOOLEAN
LANGUAGE SQL STABLE SECURITY DEFINER SET search_path=public,pg_temp AS $$
    SELECT NOT EXISTS (SELECT 1 FROM ingestion_crops WHERE asset_id=target)
        OR can_read_ingestion(target_course)
$$;
DROP POLICY asset_read ON assets;
CREATE POLICY asset_read ON assets FOR SELECT USING (
    can_read_course(course_id) AND ingestion_asset_allowed(id,course_id)
);

ALTER TABLE ingestion_runs ENABLE ROW LEVEL SECURITY;
ALTER TABLE ingestion_pages ENABLE ROW LEVEL SECURITY;
ALTER TABLE ingestion_crops ENABLE ROW LEVEL SECURITY;
ALTER TABLE ingestion_chunks ENABLE ROW LEVEL SECURITY;
ALTER TABLE ingestion_chunk_crops ENABLE ROW LEVEL SECURITY;
CREATE POLICY ingestion_run_read ON ingestion_runs FOR SELECT USING (can_manage_course(course_id));
CREATE POLICY ingestion_page_read ON ingestion_pages FOR SELECT USING (can_manage_course(course_id));
CREATE POLICY ingestion_crop_read ON ingestion_crops FOR SELECT USING (can_read_ingestion(course_id));
CREATE POLICY ingestion_chunk_read ON ingestion_chunks FOR SELECT USING (can_manage_course(course_id));
CREATE POLICY ingestion_link_read ON ingestion_chunk_crops FOR SELECT USING (can_manage_course(course_id));
REVOKE ALL ON FUNCTION can_read_ingestion(UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION ingestion_asset_allowed(UUID,UUID) FROM PUBLIC;
