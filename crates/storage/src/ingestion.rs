//! Fenced, transactional M2 checkpoints. Worker methods derive tenant/course
//! scope from the locked durable job, never from a request or a filesystem path.
use crate::{Result, StorageError, begin_as};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgConnection, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunInput {
    pub source_asset_id: Uuid,
    pub input_sha256: String,
    pub pipeline_version: String,
    pub provenance: Value,
    pub source_pages: i32,
}

#[derive(Debug, FromRow, Serialize)]
pub struct RunView {
    pub id: Uuid,
    pub job_id: Uuid,
    pub source_pages: i32,
    pub status: String,
    pub pipeline_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CropCheckpoint {
    pub id: Uuid,
    pub region_index: i32,
    pub dpi: i32,
    pub width: i32,
    pub height: i32,
    pub bbox: Value,
    pub bytes: i64,
    pub sha256: String,
}

pub const EMBEDDING_DIM: usize = 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmbeddingVector {
    pub values: Vec<f32>,
    pub model_id: String,
    pub revision: String,
    pub preprocess_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingScope {
    pub language_code: String,
    pub model_id: String,
    pub revision: String,
    pub preprocess_version: String,
    pub limit: i64,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct SimilarChunk {
    pub id: Uuid,
    pub page_number: i32,
    pub source_text: String,
    pub normalized_candidate: String,
    pub text_sha256: String,
    pub structural_fingerprint: String,
    pub critical_tokens: Value,
    pub cosine_similarity: f64,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkCheckpoint {
    pub id: Uuid,
    pub order: i32,
    pub source_text: String,
    pub normalized_candidate: String,
    pub text_sha256: String,
    pub structural_fingerprint: String,
    pub critical_tokens: Value,
    pub duplicate_of: Option<Uuid>,
    pub crop_ids: Vec<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language_code: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_quarantined: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<EmbeddingVector>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageCheckpoint {
    pub page_number: i32,
    pub ready: bool,
    pub artifact: Value,
    pub crops: Vec<CropCheckpoint>,
    pub chunks: Vec<ChunkCheckpoint>,
}

fn digest_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn versioned(value: &Value) -> bool {
    value.as_object().is_some() && value["schema_version"] == 1
}

fn valid_label(value: &str, max_len: usize) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= max_len
        && !value.chars().any(|c| c == '\0')
}

fn valid_language_code(value: &str) -> bool {
    (2..=16).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub fn validate_embedding_vector(embedding: &EmbeddingVector) -> Result<()> {
    if embedding.values.len() != EMBEDDING_DIM
        || embedding.values.iter().any(|v| !v.is_finite())
        || !valid_label(&embedding.model_id, 128)
        || !valid_label(&embedding.revision, 128)
        || !valid_label(&embedding.preprocess_version, 64)
    {
        return Err(StorageError::Invalid(
            "invalid 1024-d embedding or provenance",
        ));
    }
    let norm_sq: f64 = embedding
        .values
        .iter()
        .map(|&v| f64::from(v) * f64::from(v))
        .sum();
    if !norm_sq.is_finite() || norm_sq <= 1e-12 {
        return Err(StorageError::Invalid(
            "embedding vector norm must be nonzero",
        ));
    }
    Ok(())
}

pub fn format_pgvector(values: &[f32]) -> Result<String> {
    if values.len() != EMBEDDING_DIM || values.iter().any(|v| !v.is_finite()) {
        return Err(StorageError::Invalid(
            "vector literal requires 1024 finite floats",
        ));
    }
    let mut out = String::with_capacity(values.len() * 12);
    out.push('[');
    for (idx, value) in values.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{value}");
    }
    out.push(']');
    Ok(out)
}

/// Object key order cannot make a retry conflict after a JSONB round trip.
pub fn canonical_digest(value: &Value) -> Result<String> {
    fn sorted(value: &Value, depth: usize) -> Result<Value> {
        if depth > 32 {
            return Err(StorageError::Invalid("artifact nesting is too deep"));
        }
        Ok(match value {
            Value::Object(object) => {
                let mut keys = object.keys().collect::<Vec<_>>();
                keys.sort();
                let mut result = serde_json::Map::new();
                for key in keys {
                    result.insert(key.clone(), sorted(&object[key], depth + 1)?);
                }
                Value::Object(result)
            }
            Value::Array(values) => Value::Array(
                values
                    .iter()
                    .map(|v| sorted(v, depth + 1))
                    .collect::<Result<_>>()?,
            ),
            value => value.clone(),
        })
    }
    let bytes = serde_json::to_vec(&sorted(value, 0)?)
        .map_err(|_| StorageError::Invalid("invalid artifact"))?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(StorageError::Invalid("artifact exceeds 8 MiB"));
    }
    Ok(hex::encode(Sha256::digest(bytes)))
}

async fn fence(
    connection: &mut PgConnection,
    job_id: Uuid,
    lease: Uuid,
) -> Result<(Uuid, Uuid, Uuid)> {
    sqlx::query_as(
        "SELECT tenant_id,course_id,created_by FROM jobs WHERE id=$1 AND lease_token=$2
        AND status='running' AND lease_until>clock_timestamp() FOR UPDATE",
    )
    .bind(job_id)
    .bind(lease)
    .fetch_optional(connection)
    .await?
    .ok_or(StorageError::Conflict)
}

pub async fn start_run(pool: &PgPool, job_id: Uuid, lease: Uuid, input: &RunInput) -> Result<Uuid> {
    if !digest_valid(&input.input_sha256)
        || input.pipeline_version.is_empty()
        || input.pipeline_version.len() > 128
        || !(1..=200).contains(&input.source_pages)
        || !versioned(&input.provenance)
    {
        return Err(StorageError::Invalid("invalid run provenance"));
    }
    let mut tx = pool.begin().await?;
    let (tenant, course, _) = fence(&mut tx, job_id, lease).await?;
    let source: Option<(String, String)> = sqlx::query_as(
        "SELECT sha256,media_type FROM assets WHERE id=$1 AND tenant_id=$2 AND course_id=$3",
    )
    .bind(input.source_asset_id)
    .bind(tenant)
    .bind(course)
    .fetch_optional(&mut *tx)
    .await?;
    if source != Some((input.input_sha256.clone(), "application/pdf".into())) {
        return Err(StorageError::Invalid(
            "source asset scope or digest mismatch",
        ));
    }
    let proposed = Uuid::new_v4();
    sqlx::query("INSERT INTO ingestion_runs (id,tenant_id,course_id,job_id,source_asset_id,input_sha256,pipeline_version,provenance,source_pages)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT (job_id) DO NOTHING")
        .bind(proposed).bind(tenant).bind(course).bind(job_id).bind(input.source_asset_id).bind(&input.input_sha256)
        .bind(&input.pipeline_version).bind(&input.provenance).bind(input.source_pages).execute(&mut *tx).await?;
    let (id,asset,hash,version,provenance,pages):(Uuid,Uuid,String,String,Value,i32)=sqlx::query_as(
        "SELECT id,source_asset_id,input_sha256,pipeline_version,provenance,source_pages FROM ingestion_runs WHERE job_id=$1")
        .bind(job_id).fetch_one(&mut *tx).await?;
    if asset != input.source_asset_id
        || hash != input.input_sha256
        || version != input.pipeline_version
        || pages != input.source_pages
        || canonical_digest(&provenance)? != canonical_digest(&input.provenance)?
    {
        return Err(StorageError::Conflict);
    }
    tx.commit().await?;
    Ok(id)
}

pub async fn checkpoint_page(
    pool: &PgPool,
    job_id: Uuid,
    lease: Uuid,
    run_id: Uuid,
    page: &PageCheckpoint,
) -> Result<bool> {
    if !(1..=200).contains(&page.page_number)
        || !versioned(&page.artifact)
        || page.crops.len() > 60
        || page.chunks.len() > 4096
    {
        return Err(StorageError::Invalid("invalid page checkpoint"));
    }
    for crop in &page.crops {
        if crop.dpi != 250
            || !(8..=8192).contains(&crop.width)
            || !(8..=8192).contains(&crop.height)
            || i64::from(crop.width) * i64::from(crop.height) > 24000000
            || !(1..=8 * 1024 * 1024).contains(&crop.bytes)
            || crop.region_index < 0
            || !versioned(&crop.bbox)
            || !digest_valid(&crop.sha256)
        {
            return Err(StorageError::Invalid("invalid crop checkpoint"));
        }
    }
    for chunk in &page.chunks {
        if chunk.order < 0
            || chunk.source_text.is_empty()
            || !digest_valid(&chunk.text_sha256)
            || !digest_valid(&chunk.structural_fingerprint)
            || !versioned(&chunk.critical_tokens)
            || !chunk
                .language_code
                .as_deref()
                .is_none_or(valid_language_code)
        {
            return Err(StorageError::Invalid("invalid chunk checkpoint"));
        }
        if let Some(embedding) = &chunk.embedding {
            validate_embedding_vector(embedding)?;
        }
    }
    // Bind the complete child set too; a retry may not alter children while
    // presenting an identical page artifact.
    let mut stable =
        serde_json::to_value(page).map_err(|_| StorageError::Invalid("invalid checkpoint"))?;
    fn strip_runtime(value: &mut Value) {
        match value {
            Value::Object(object) => {
                object.remove("duration_ms");
                object.remove("model_peak_rss_bytes");
                for value in object.values_mut() {
                    strip_runtime(value)
                }
            }
            Value::Array(values) => {
                for value in values {
                    strip_runtime(value)
                }
            }
            _ => {}
        }
    }
    strip_runtime(&mut stable);
    let fingerprint = canonical_digest(&stable)?;
    let mut tx = pool.begin().await?;
    let (tenant, course, creator) = fence(&mut tx, job_id, lease).await?;
    let pages:Option<i32>=sqlx::query_scalar("SELECT source_pages FROM ingestion_runs WHERE id=$1 AND job_id=$2 AND tenant_id=$3 AND course_id=$4")
        .bind(run_id).bind(job_id).bind(tenant).bind(course).fetch_optional(&mut *tx).await?;
    if pages.is_none_or(|n| page.page_number > n) {
        return Err(StorageError::Conflict);
    }
    if let Some(existing) = sqlx::query_scalar::<_, String>(
        "SELECT artifact_sha256 FROM ingestion_pages WHERE run_id=$1 AND page_number=$2",
    )
    .bind(run_id)
    .bind(page.page_number)
    .fetch_optional(&mut *tx)
    .await?
    {
        if existing != fingerprint {
            return Err(StorageError::Conflict);
        }
        tx.commit().await?;
        return Ok(false);
    }
    sqlx::query("INSERT INTO ingestion_pages (run_id,tenant_id,course_id,page_number,artifact_sha256,status,artifact) VALUES ($1,$2,$3,$4,$5,$6,$7)")
        .bind(run_id).bind(tenant).bind(course).bind(page.page_number).bind(fingerprint)
        .bind(if page.ready{"ready"}else{"needs_review"}).bind(&page.artifact).execute(&mut *tx).await?;
    for crop in &page.crops {
        sqlx::query("INSERT INTO assets (id,tenant_id,course_id,created_by,sha256,media_type,bytes) VALUES ($1,$2,$3,$4,$5,'image/webp',$6)")
            .bind(crop.id).bind(tenant).bind(course).bind(creator).bind(&crop.sha256).bind(crop.bytes).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO ingestion_crops (id,tenant_id,course_id,run_id,page_number,asset_id,region_index,dpi,width,height,bbox)
            VALUES ($1,$2,$3,$4,$5,$1,$6,$7,$8,$9,$10)")
            .bind(crop.id).bind(tenant).bind(course).bind(run_id).bind(page.page_number).bind(crop.region_index)
            .bind(crop.dpi).bind(crop.width).bind(crop.height).bind(&crop.bbox).execute(&mut *tx).await?;
    }
    for chunk in &page.chunks {
        let language = chunk.language_code.as_deref().unwrap_or("en");
        let (vec_lit, model_id, revision, emb_dim, preprocess) = if let Some(emb) = &chunk.embedding
        {
            (
                Some(format_pgvector(&emb.values)?),
                Some(emb.model_id.as_str()),
                Some(emb.revision.as_str()),
                Some(EMBEDDING_DIM as i32),
                Some(emb.preprocess_version.as_str()),
            )
        } else {
            (None, None, None, None, None)
        };
        sqlx::query("INSERT INTO ingestion_chunks (id,tenant_id,course_id,run_id,page_number,chunk_order,source_text,normalized_candidate,
            normalization_version,text_sha256,structural_fingerprint,critical_tokens,language_code,is_quarantined,duplicate_of,
            embedding,embedding_model_id,embedding_revision,embedding_dim,preprocess_version)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'critical-v2-indent',$9,$10,$11,$12,$13,$14,($15::text)::vector,$16,$17,$18,$19)")
            .bind(chunk.id).bind(tenant).bind(course).bind(run_id).bind(page.page_number).bind(chunk.order).bind(&chunk.source_text)
            .bind(&chunk.normalized_candidate).bind(&chunk.text_sha256).bind(&chunk.structural_fingerprint).bind(&chunk.critical_tokens)
            .bind(language).bind(chunk.is_quarantined).bind(chunk.duplicate_of)
            .bind(vec_lit).bind(model_id).bind(revision).bind(emb_dim).bind(preprocess)
            .execute(&mut *tx).await?;
        for &crop in &chunk.crop_ids {
            sqlx::query("INSERT INTO ingestion_chunk_crops (tenant_id,course_id,run_id,chunk_id,crop_id) VALUES ($1,$2,$3,$4,$5)")
                .bind(tenant).bind(course).bind(run_id).bind(chunk.id).bind(crop).execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    Ok(true)
}

pub async fn finish_run(pool: &PgPool, job_id: Uuid, lease: Uuid, run_id: Uuid) -> Result<String> {
    let mut tx = pool.begin().await?;
    let (tenant, course, _) = fence(&mut tx, job_id, lease).await?;
    let expected:Option<i32>=sqlx::query_scalar("SELECT source_pages FROM ingestion_runs WHERE id=$1 AND job_id=$2 AND tenant_id=$3 AND course_id=$4")
        .bind(run_id).bind(job_id).bind(tenant).bind(course).fetch_optional(&mut *tx).await?;
    let expected = expected.ok_or(StorageError::Conflict)?;
    let (count,ready):(i64,i64)=sqlx::query_as("SELECT count(*),count(*) FILTER (WHERE status='ready') FROM ingestion_pages WHERE run_id=$1")
        .bind(run_id).fetch_one(&mut *tx).await?;
    if count != i64::from(expected) {
        return Err(StorageError::Conflict);
    }
    let status = if ready == count {
        "succeeded"
    } else {
        "needs_review"
    };
    sqlx::query("UPDATE ingestion_runs SET status=$2,updated_at=clock_timestamp() WHERE id=$1")
        .bind(run_id)
        .bind(status)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE jobs SET status=$2,stage=$2,lease_token=NULL,lease_until=NULL,progress_done=progress_total,
        updated_at=clock_timestamp() WHERE id=$1")
        .bind(job_id).bind(status).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO job_results (job_id,output) VALUES ($1,$2)")
        .bind(job_id)
        .bind(json!({"schema_version":1,"run_id":run_id,"status":status,"pages":count}))
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO job_events (job_id,stage) VALUES ($1,$2)")
        .bind(job_id)
        .bind(status)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(status.into())
}

pub async fn run_for_manager(
    pool: &PgPool,
    actor: Uuid,
    course: Uuid,
    run: Uuid,
) -> Result<RunView> {
    let mut tx = begin_as(pool, actor).await?;
    let view=sqlx::query_as("SELECT id,job_id,source_pages,status,pipeline_version FROM ingestion_runs WHERE id=$1 AND course_id=$2 AND can_manage_course(course_id)")
        .bind(run).bind(course).fetch_optional(&mut *tx).await?.ok_or(StorageError::NotFound)?;
    tx.commit().await?;
    Ok(view)
}

pub async fn crop_asset_for_read(
    pool: &PgPool,
    actor: Uuid,
    course: Uuid,
    crop: Uuid,
) -> Result<Uuid> {
    let mut tx = begin_as(pool, actor).await?;
    let asset=sqlx::query_scalar("SELECT asset_id FROM ingestion_crops WHERE id=$1 AND course_id=$2 AND can_read_ingestion(course_id)")
        .bind(crop).bind(course).fetch_optional(&mut *tx).await?.ok_or(StorageError::NotFound)?;
    tx.commit().await?;
    Ok(asset)
}

async fn query_similar_chunks_in_tx(
    tx: &mut PgConnection,
    tenant: Uuid,
    course: Uuid,
    query: &EmbeddingVector,
    scope: &EmbeddingScope,
) -> Result<Vec<SimilarChunk>> {
    validate_embedding_vector(query)?;
    if !valid_language_code(&scope.language_code)
        || !valid_label(&scope.model_id, 128)
        || !valid_label(&scope.revision, 128)
        || !valid_label(&scope.preprocess_version, 64)
        || !(1..=50).contains(&scope.limit)
    {
        return Err(StorageError::Invalid("invalid ANN search scope"));
    }
    if query.model_id != scope.model_id
        || query.revision != scope.revision
        || query.preprocess_version != scope.preprocess_version
    {
        return Err(StorageError::Invalid(
            "query embedding provenance mismatches target ANN partition",
        ));
    }
    let vector_literal = format_pgvector(&query.values)?;
    sqlx::query_as::<_, SimilarChunk>(
        "SELECT id, page_number, source_text, normalized_candidate, text_sha256,
                structural_fingerprint, critical_tokens,
                (1.0 - (embedding <=> ($8::text)::vector))::float8 AS cosine_similarity
         FROM ingestion_chunks
         WHERE tenant_id = $1
           AND course_id = $2
           AND language_code = $3
           AND NOT is_quarantined
           AND embedding IS NOT NULL
           AND embedding_dim = 1024
           AND embedding_model_id = $4
           AND embedding_revision = $5
           AND preprocess_version = $6
         ORDER BY embedding <=> ($8::text)::vector ASC
         LIMIT $7",
    )
    .bind(tenant)
    .bind(course)
    .bind(&scope.language_code)
    .bind(&scope.model_id)
    .bind(&scope.revision)
    .bind(&scope.preprocess_version)
    .bind(scope.limit)
    .bind(&vector_literal)
    .fetch_all(tx)
    .await
    .map_err(StorageError::from)
}

/// Scoped, quarantine-filtered HNSW cosine retrieval (`S-02` + `S-04`).
pub async fn search_similar_chunks(
    pool: &PgPool,
    actor: Uuid,
    course: Uuid,
    query: &EmbeddingVector,
    scope: &EmbeddingScope,
) -> Result<Vec<SimilarChunk>> {
    let mut tx = begin_as(pool, actor).await?;
    let tenant: Option<Uuid> =
        sqlx::query_scalar("SELECT tenant_id FROM courses WHERE id = $1 AND can_manage_course(id)")
            .bind(course)
            .fetch_optional(&mut *tx)
            .await?;
    let tenant = tenant.ok_or(StorageError::NotFound)?;
    let rows = query_similar_chunks_in_tx(&mut tx, tenant, course, query, scope).await?;
    tx.commit().await?;
    Ok(rows)
}

/// Fenced worker-side ANN lookup derived strictly from the locked job scope.
pub async fn search_similar_chunks_for_worker(
    pool: &PgPool,
    job_id: Uuid,
    lease: Uuid,
    query: &EmbeddingVector,
    scope: &EmbeddingScope,
) -> Result<Vec<SimilarChunk>> {
    let mut tx = pool.begin().await?;
    let (tenant, course, _) = fence(&mut tx, job_id, lease).await?;
    let rows = query_similar_chunks_in_tx(&mut tx, tenant, course, query, scope).await?;
    tx.commit().await?;
    Ok(rows)
}

/// Safe `S-03`/`P-04` merge filter: ANN cosine similarity alone never authorizes
/// a merge when numbers, units, formulas, negation, or leading indentation differ.
pub fn safe_dedup_candidate<'a>(
    structural_fingerprint: &str,
    candidates: &'a [SimilarChunk],
    min_cosine_similarity: f64,
) -> Option<&'a SimilarChunk> {
    if !digest_valid(structural_fingerprint)
        || !min_cosine_similarity.is_finite()
        || !(-1.0..=1.0001).contains(&min_cosine_similarity)
    {
        return None;
    }
    candidates.iter().find(|candidate| {
        candidate.structural_fingerprint == structural_fingerprint
            && candidate.cosine_similarity.is_finite()
            && candidate.cosine_similarity >= min_cosine_similarity
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_artifact_hash_ignores_object_key_order() {
        let a: Value = serde_json::from_str(r#"{"a":1,"b":{"x":2,"y":3}}"#).unwrap();
        let b: Value = serde_json::from_str(r#"{"b":{"y":3,"x":2},"a":1}"#).unwrap();
        assert_eq!(canonical_digest(&a).unwrap(), canonical_digest(&b).unwrap());
        assert_ne!(
            canonical_digest(&json!([1, 2])).unwrap(),
            canonical_digest(&json!([2, 1])).unwrap()
        );
    }
}
