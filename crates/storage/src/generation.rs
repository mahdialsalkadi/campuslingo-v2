use std::collections::BTreeSet;

use campuslingo_domain::{
    contracts::{AudioManifest, Codec, DirectorTrack, LearningUnitBundle, SandboxSpec},
    sandbox,
    windows::{
        card_elixir_cost, entry as window_entry, is_stage3_drill_eligible,
        validate_axiom_pack_card_ids,
    },
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{Result, StorageError, begin_as, is_permission_denied, is_unique_violation};

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ContentVersionRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub course_id: Uuid,
    pub version_number: i32,
    pub source_run_id: Option<Uuid>,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct BundleRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub course_id: Uuid,
    pub node_id: Uuid,
    pub content_version_id: Uuid,
    pub archetype: String,
    pub modifier: String,
    pub card_elixir_cost: i16,
    pub world_theme: String,
    pub language: String,
    pub grid_x: i32,
    pub grid_y: i32,
    pub bundle_json: Value,
    pub source_sha256: String,
    pub generator_model_id: String,
    pub generator_revision: String,
    pub generator_version: String,
    pub schema_version: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct SimulationRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub course_id: Uuid,
    pub node_id: Uuid,
    pub content_version_id: Uuid,
    pub spec_json: Value,
    pub solution_witness: Value,
    pub states_checked: i32,
    pub proof_domain: String,
    pub verified: bool,
    pub verified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct AxiomPackItemRecord {
    pub pack_id: Uuid,
    pub tenant_id: Uuid,
    pub course_id: Uuid,
    pub position: i16,
    pub card_id: Uuid,
    pub w_base_ms: i32,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct AxiomPackRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub course_id: Uuid,
    pub node_id: Uuid,
    pub content_version_id: Uuid,
    pub archetype: String,
    pub modifier: String,
    pub w_base_ms: i32,
    pub card_elixir_cost: i16,
    pub prompt_count: i16,
    pub created_at: DateTime<Utc>,
    #[sqlx(skip)]
    pub items: Vec<AxiomPackItemRecord>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct AudioBriefingRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub course_id: Uuid,
    pub node_id: Uuid,
    pub content_version_id: Uuid,
    pub asset_id: Uuid,
    pub language: String,
    pub codec: String,
    pub alignment_method: String,
    pub duration_ms: i32,
    pub manifest_json: Value,
    pub director_tracks_json: Value,
    pub created_at: DateTime<Utc>,
}

pub struct NewLearningUnitBundle<'a> {
    pub course_id: Uuid,
    pub node_id: Uuid,
    pub content_version_id: Uuid,
    pub archetype: &'a str,
    pub modifier: &'a str,
    pub grid_x: i32,
    pub grid_y: i32,
    pub source_sha256: &'a str,
    pub generator_model_id: &'a str,
    pub generator_revision: &'a str,
    pub bundle: &'a LearningUnitBundle,
}

pub struct NewNodeSimulation<'a> {
    pub course_id: Uuid,
    pub node_id: Uuid,
    pub content_version_id: Uuid,
    pub spec: &'a SandboxSpec,
}

pub struct NewAxiomPack<'a> {
    pub course_id: Uuid,
    pub node_id: Uuid,
    pub content_version_id: Uuid,
    pub archetype: &'a str,
    pub modifier: &'a str,
    pub card_ids: &'a [Uuid],
}

pub struct NewAudioBriefing<'a> {
    pub course_id: Uuid,
    pub node_id: Uuid,
    pub content_version_id: Uuid,
    pub asset_id: Uuid,
    pub manifest: &'a AudioManifest,
    pub director_tracks: &'a [DirectorTrack],
}

fn map_db_err(error: sqlx::Error) -> StorageError {
    if is_permission_denied(&error) {
        StorageError::Forbidden
    } else if is_unique_violation(&error) {
        StorageError::Conflict
    } else {
        StorageError::Database(error)
    }
}

fn serde_enum_str<T: Serialize>(value: &T) -> Result<String> {
    match serde_json::to_value(value)
        .map_err(|_| StorageError::Invalid("invalid contract value"))?
    {
        Value::String(s) => Ok(s),
        _ => Err(StorageError::Invalid(
            "expected string-serialized contract enum",
        )),
    }
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn verify_sandbox_spec(spec: &SandboxSpec) -> Result<(Value, Value, i32, String)> {
    let validation =
        sandbox::verify(spec).map_err(|_| StorageError::Invalid("invalid sandbox spec"))?;
    if !validation.valid {
        return Err(StorageError::Invalid(
            "sandbox failed deterministic verification or is pre-solved",
        ));
    }
    let witness = validation
        .witness
        .ok_or(StorageError::Invalid("missing sandbox solution witness"))?;
    let spec_json =
        serde_json::to_value(spec).map_err(|_| StorageError::Invalid("bad sandbox json"))?;
    let witness_json =
        serde_json::to_value(witness).map_err(|_| StorageError::Invalid("bad witness json"))?;
    let states_checked = i32::try_from(validation.states_checked)
        .map_err(|_| StorageError::Invalid("states overflow"))?;
    Ok((
        spec_json,
        witness_json,
        states_checked,
        validation.proof_domain.to_string(),
    ))
}

fn validate_audio_contract(
    manifest: &AudioManifest,
    tracks: &[DirectorTrack],
) -> Result<(String, String, String, i32)> {
    if !matches!(manifest.codec, Codec::Opus) {
        return Err(StorageError::Invalid("audio codec must be opus"));
    }
    if manifest.duration_ms == 0 || manifest.words.is_empty() {
        return Err(StorageError::Invalid("audio manifest must be non-empty"));
    }
    let mut prev_end = 0u32;
    for clause in &manifest.words {
        if clause.text.trim().is_empty()
            || clause.end_ms <= clause.start_ms
            || clause.end_ms > manifest.duration_ms
        {
            return Err(StorageError::Invalid("invalid audio clause interval"));
        }
        if clause.start_ms < prev_end {
            return Err(StorageError::Invalid(
                "audio clauses overlap or are unsorted",
            ));
        }
        prev_end = clause.end_ms;
    }
    for track in tracks {
        if track.end_ms <= track.start_ms
            || track.end_ms > manifest.duration_ms
            || !track.explode_factor.is_finite()
        {
            return Err(StorageError::Invalid("invalid director track bounds"));
        }
    }
    let lang = serde_enum_str(&manifest.language)?;
    let codec = serde_enum_str(&manifest.codec)?;
    let align = serde_enum_str(&manifest.alignment_method)?;
    let duration_ms = i32::try_from(manifest.duration_ms)
        .map_err(|_| StorageError::Invalid("duration overflow"))?;
    Ok((lang, codec, align, duration_ms))
}

async fn resolve_course_and_version_tx(
    transaction: &mut Transaction<'_, Postgres>,
    course_id: Uuid,
    node_id: Uuid,
    content_version_id: Uuid,
) -> Result<Uuid> {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT ln.tenant_id
         FROM learning_nodes ln
         JOIN content_versions cv
           ON cv.id = $3 AND cv.course_id = ln.course_id AND cv.tenant_id = ln.tenant_id
         WHERE ln.id = $2 AND ln.course_id = $1",
    )
    .bind(course_id)
    .bind(node_id)
    .bind(content_version_id)
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or(StorageError::NotFound)
}

pub async fn create_content_version(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    source_run_id: Option<Uuid>,
) -> Result<ContentVersionRecord> {
    let mut transaction = begin_as(pool, actor).await?;
    let row = sqlx::query_as::<_, ContentVersionRecord>(
        "INSERT INTO content_versions (tenant_id, course_id, version_number, source_run_id)
         SELECT c.tenant_id, c.id,
                COALESCE((SELECT MAX(version_number) + 1 FROM content_versions WHERE course_id = c.id), 1),
                $2
         FROM courses c
         WHERE c.id = $1 AND can_manage_course(c.id)
         RETURNING id, tenant_id, course_id, version_number, source_run_id, status, created_at",
    )
    .bind(course_id)
    .bind(source_run_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(map_db_err)?
    .ok_or(StorageError::NotFound)?;
    transaction.commit().await?;
    Ok(row)
}

pub async fn create_content_version_for_worker(
    pool: &PgPool,
    course_id: Uuid,
    source_run_id: Option<Uuid>,
) -> Result<ContentVersionRecord> {
    let mut transaction = pool.begin().await?;
    let row = sqlx::query_as::<_, ContentVersionRecord>(
        "INSERT INTO content_versions (tenant_id, course_id, version_number, source_run_id)
         SELECT c.tenant_id, c.id,
                COALESCE((SELECT MAX(version_number) + 1 FROM content_versions WHERE course_id = c.id), 1),
                $2
         FROM courses c
         WHERE c.id = $1
         RETURNING id, tenant_id, course_id, version_number, source_run_id, status, created_at",
    )
    .bind(course_id)
    .bind(source_run_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(map_db_err)?
    .ok_or(StorageError::NotFound)?;
    transaction.commit().await?;
    Ok(row)
}

pub async fn persist_learning_unit_bundle(
    pool: &PgPool,
    actor: Uuid,
    input: &NewLearningUnitBundle<'_>,
) -> Result<BundleRecord> {
    let mut transaction = begin_as(pool, actor).await?;
    let record = persist_learning_unit_bundle_tx(&mut transaction, input, true).await?;
    transaction.commit().await?;
    Ok(record)
}

pub async fn persist_learning_unit_bundle_for_worker(
    pool: &PgPool,
    input: &NewLearningUnitBundle<'_>,
) -> Result<BundleRecord> {
    let mut transaction = pool.begin().await?;
    let record = persist_learning_unit_bundle_tx(&mut transaction, input, false).await?;
    transaction.commit().await?;
    Ok(record)
}

async fn persist_learning_unit_bundle_tx(
    transaction: &mut Transaction<'_, Postgres>,
    input: &NewLearningUnitBundle<'_>,
    enforce_manager: bool,
) -> Result<BundleRecord> {
    let elixir_cost = card_elixir_cost(input.archetype, input.modifier)
        .map_err(|_| StorageError::Invalid("unsupported archetype/modifier pair"))?;
    if !is_sha256_hex(input.source_sha256) {
        return Err(StorageError::Invalid(
            "source_sha256 must be 64 lowercase hex chars",
        ));
    }
    if input.grid_x < 0 || input.grid_y < 0 {
        return Err(StorageError::Invalid(
            "grid coordinates must be non-negative",
        ));
    }
    if input.bundle.node_id != input.node_id.to_string() {
        return Err(StorageError::Invalid(
            "bundle node_id does not match node_id",
        ));
    }
    if input.bundle.tactical_summary.len() != 2
        || input.bundle.challenges.is_empty()
        || input.bundle.title.trim().is_empty()
    {
        return Err(StorageError::Invalid(
            "bundle missing summary, title, or challenges",
        ));
    }
    if input.generator_model_id.trim().is_empty()
        || input.generator_revision.trim().is_empty()
        || input.bundle.generator_version.trim().is_empty()
    {
        return Err(StorageError::Invalid("missing generator provenance"));
    }
    let schema_version = serde_enum_str(&input.bundle.schema_version)?;
    let world_theme = serde_enum_str(&input.bundle.world_theme)?;
    let language = serde_enum_str(&input.bundle.language)?;
    let bundle_json = serde_json::to_value(input.bundle)
        .map_err(|_| StorageError::Invalid("failed to encode bundle json"))?;

    let sandbox_verified = if let Some(spec) = input.bundle.sandbox.as_ref() {
        let spec_arch = serde_enum_str(&spec.archetype)?;
        if spec_arch != input.archetype || spec.modifier != input.modifier {
            return Err(StorageError::Invalid("sandbox archetype/modifier mismatch"));
        }
        Some(verify_sandbox_spec(spec)?)
    } else {
        None
    };

    let tenant_id = resolve_course_and_version_tx(
        transaction,
        input.course_id,
        input.node_id,
        input.content_version_id,
    )
    .await?;

    if enforce_manager {
        let allowed = sqlx::query_scalar::<_, bool>("SELECT can_manage_course($1)")
            .bind(input.course_id)
            .fetch_one(&mut **transaction)
            .await?;
        if !allowed {
            return Err(StorageError::Forbidden);
        }
    }

    let record = sqlx::query_as::<_, BundleRecord>(
        "INSERT INTO learning_unit_bundles (
             tenant_id, course_id, node_id, content_version_id,
             archetype, modifier, card_elixir_cost, world_theme, language,
             grid_x, grid_y, bundle_json, source_sha256,
             generator_model_id, generator_revision, generator_version, schema_version
         ) VALUES (
             $1, $2, $3, $4,
             $5, $6, $7, $8, $9,
             $10, $11, $12, $13,
             $14, $15, $16, $17
         )
         RETURNING id, tenant_id, course_id, node_id, content_version_id,
                   archetype, modifier, card_elixir_cost, world_theme, language,
                   grid_x, grid_y, bundle_json, source_sha256,
                   generator_model_id, generator_revision, generator_version, schema_version, created_at",
    )
    .bind(tenant_id)
    .bind(input.course_id)
    .bind(input.node_id)
    .bind(input.content_version_id)
    .bind(input.archetype)
    .bind(input.modifier)
    .bind(i16::from(elixir_cost))
    .bind(&world_theme)
    .bind(&language)
    .bind(input.grid_x)
    .bind(input.grid_y)
    .bind(&bundle_json)
    .bind(input.source_sha256)
    .bind(input.generator_model_id)
    .bind(input.generator_revision)
    .bind(&input.bundle.generator_version)
    .bind(&schema_version)
    .fetch_one(&mut **transaction)
    .await
    .map_err(map_db_err)?;

    if let Some((spec_json, witness_json, states_checked, proof_domain)) = sandbox_verified {
        sqlx::query(
            "INSERT INTO node_simulations (
                 tenant_id, course_id, node_id, content_version_id,
                 spec_json, solution_witness, states_checked, proof_domain, verified
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, TRUE)",
        )
        .bind(tenant_id)
        .bind(input.course_id)
        .bind(input.node_id)
        .bind(input.content_version_id)
        .bind(spec_json)
        .bind(witness_json)
        .bind(states_checked)
        .bind(proof_domain)
        .execute(&mut **transaction)
        .await
        .map_err(map_db_err)?;
    }

    Ok(record)
}

pub async fn persist_verified_simulation(
    pool: &PgPool,
    actor: Uuid,
    input: &NewNodeSimulation<'_>,
) -> Result<SimulationRecord> {
    let (spec_json, witness_json, states_checked, proof_domain) = verify_sandbox_spec(input.spec)?;
    let mut transaction = begin_as(pool, actor).await?;
    let tenant_id = resolve_course_and_version_tx(
        &mut transaction,
        input.course_id,
        input.node_id,
        input.content_version_id,
    )
    .await?;
    let allowed = sqlx::query_scalar::<_, bool>("SELECT can_manage_course($1)")
        .bind(input.course_id)
        .fetch_one(&mut *transaction)
        .await?;
    if !allowed {
        return Err(StorageError::Forbidden);
    }
    let row = sqlx::query_as::<_, SimulationRecord>(
        "INSERT INTO node_simulations (
             tenant_id, course_id, node_id, content_version_id,
             spec_json, solution_witness, states_checked, proof_domain, verified
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, TRUE)
         RETURNING id, tenant_id, course_id, node_id, content_version_id,
                   spec_json, solution_witness, states_checked, proof_domain, verified, verified_at",
    )
    .bind(tenant_id)
    .bind(input.course_id)
    .bind(input.node_id)
    .bind(input.content_version_id)
    .bind(spec_json)
    .bind(witness_json)
    .bind(states_checked)
    .bind(proof_domain)
    .fetch_one(&mut *transaction)
    .await
    .map_err(map_db_err)?;
    transaction.commit().await?;
    Ok(row)
}

pub async fn create_axiom_pack(
    pool: &PgPool,
    actor: Uuid,
    input: &NewAxiomPack<'_>,
) -> Result<AxiomPackRecord> {
    let mut transaction = begin_as(pool, actor).await?;
    let record = create_axiom_pack_tx(&mut transaction, input, true).await?;
    transaction.commit().await?;
    Ok(record)
}

pub async fn create_axiom_pack_for_worker(
    pool: &PgPool,
    input: &NewAxiomPack<'_>,
) -> Result<AxiomPackRecord> {
    let mut transaction = pool.begin().await?;
    let record = create_axiom_pack_tx(&mut transaction, input, false).await?;
    transaction.commit().await?;
    Ok(record)
}

async fn create_axiom_pack_tx(
    transaction: &mut Transaction<'_, Postgres>,
    input: &NewAxiomPack<'_>,
    enforce_manager: bool,
) -> Result<AxiomPackRecord> {
    let eligible = is_stage3_drill_eligible(input.archetype, input.modifier)
        .map_err(|_| StorageError::Invalid("unknown archetype/modifier pair"))?;
    if !eligible {
        return Err(StorageError::Invalid(
            "DEC-002 requires Stage-3 base window in 8000..=15000 ms",
        ));
    }
    let window = window_entry(input.archetype, input.modifier)
        .ok_or(StorageError::Invalid("unknown window entry"))?;
    let elixir_cost = card_elixir_cost(input.archetype, input.modifier)
        .map_err(|_| StorageError::Invalid("not a primary card"))?;

    let card_id_strings: Vec<String> = input.card_ids.iter().map(Uuid::to_string).collect();
    let card_id_refs: Vec<&str> = card_id_strings.iter().map(String::as_str).collect();
    validate_axiom_pack_card_ids(&card_id_refs)
        .map_err(|_| StorageError::Invalid("AXIOM_PACK requires 2..=4 distinct card_ids"))?;

    let tenant_id = resolve_course_and_version_tx(
        transaction,
        input.course_id,
        input.node_id,
        input.content_version_id,
    )
    .await?;

    if enforce_manager {
        let allowed = sqlx::query_scalar::<_, bool>("SELECT can_manage_course($1)")
            .bind(input.course_id)
            .fetch_one(&mut **transaction)
            .await?;
        if !allowed {
            return Err(StorageError::Forbidden);
        }
    }

    let unique_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT id) FROM challenges
         WHERE id = ANY($1) AND course_id = $2 AND tenant_id = $3",
    )
    .bind(input.card_ids)
    .bind(input.course_id)
    .bind(tenant_id)
    .fetch_one(&mut **transaction)
    .await?;
    if unique_count != i64::try_from(input.card_ids.len()).unwrap_or(0) {
        return Err(StorageError::Invalid(
            "all card_ids must exist in the target course",
        ));
    }

    let w_base_ms =
        i32::try_from(window.w_base_ms).map_err(|_| StorageError::Invalid("window overflow"))?;
    let prompt_count =
        i16::try_from(input.card_ids.len()).map_err(|_| StorageError::Invalid("count overflow"))?;

    let mut pack = sqlx::query_as::<_, AxiomPackRecord>(
        "INSERT INTO axiom_packs (
             tenant_id, course_id, node_id, content_version_id,
             archetype, modifier, w_base_ms, card_elixir_cost, prompt_count
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING id, tenant_id, course_id, node_id, content_version_id,
                   archetype, modifier, w_base_ms, card_elixir_cost, prompt_count, created_at",
    )
    .bind(tenant_id)
    .bind(input.course_id)
    .bind(input.node_id)
    .bind(input.content_version_id)
    .bind(input.archetype)
    .bind(input.modifier)
    .bind(w_base_ms)
    .bind(i16::from(elixir_cost))
    .bind(prompt_count)
    .fetch_one(&mut **transaction)
    .await
    .map_err(map_db_err)?;

    let mut items = Vec::with_capacity(input.card_ids.len());
    for (idx, card_id) in input.card_ids.iter().enumerate() {
        let pos = i16::try_from(idx).map_err(|_| StorageError::Invalid("position overflow"))?;
        let item = sqlx::query_as::<_, AxiomPackItemRecord>(
            "INSERT INTO axiom_pack_items (pack_id, tenant_id, course_id, position, card_id, w_base_ms)
             VALUES ($1, $2, $3, $4, $5, $6)
             RETURNING pack_id, tenant_id, course_id, position, card_id, w_base_ms",
        )
        .bind(pack.id)
        .bind(tenant_id)
        .bind(input.course_id)
        .bind(pos)
        .bind(card_id)
        .bind(w_base_ms)
        .fetch_one(&mut **transaction)
        .await
        .map_err(map_db_err)?;
        items.push(item);
    }
    pack.items = items;
    Ok(pack)
}

pub async fn persist_audio_briefing(
    pool: &PgPool,
    actor: Uuid,
    input: &NewAudioBriefing<'_>,
) -> Result<AudioBriefingRecord> {
    let mut transaction = begin_as(pool, actor).await?;
    let record = persist_audio_briefing_tx(&mut transaction, input, true).await?;
    transaction.commit().await?;
    Ok(record)
}

pub async fn persist_audio_briefing_for_worker(
    pool: &PgPool,
    input: &NewAudioBriefing<'_>,
) -> Result<AudioBriefingRecord> {
    let mut transaction = pool.begin().await?;
    let record = persist_audio_briefing_tx(&mut transaction, input, false).await?;
    transaction.commit().await?;
    Ok(record)
}

async fn persist_audio_briefing_tx(
    transaction: &mut Transaction<'_, Postgres>,
    input: &NewAudioBriefing<'_>,
    enforce_manager: bool,
) -> Result<AudioBriefingRecord> {
    let (lang, codec, align, duration_ms) =
        validate_audio_contract(input.manifest, input.director_tracks)?;
    let manifest_json = serde_json::to_value(input.manifest)
        .map_err(|_| StorageError::Invalid("failed to encode audio manifest"))?;
    let tracks_json = serde_json::to_value(input.director_tracks)
        .map_err(|_| StorageError::Invalid("failed to encode director tracks"))?;

    let tenant_id = resolve_course_and_version_tx(
        transaction,
        input.course_id,
        input.node_id,
        input.content_version_id,
    )
    .await?;

    if enforce_manager {
        let allowed = sqlx::query_scalar::<_, bool>("SELECT can_manage_course($1)")
            .bind(input.course_id)
            .fetch_one(&mut **transaction)
            .await?;
        if !allowed {
            return Err(StorageError::Forbidden);
        }
    }

    let valid_asset: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM assets
             WHERE id = $1 AND course_id = $2 AND tenant_id = $3
               AND media_type IN ('audio/ogg', 'audio/opus')
         )",
    )
    .bind(input.asset_id)
    .bind(input.course_id)
    .bind(tenant_id)
    .fetch_one(&mut **transaction)
    .await?;
    if !valid_asset {
        return Err(StorageError::Invalid(
            "audio asset missing or wrong course/media_type",
        ));
    }

    let record = sqlx::query_as::<_, AudioBriefingRecord>(
        "INSERT INTO node_audio_briefings (
             tenant_id, course_id, node_id, content_version_id, asset_id,
             language, codec, alignment_method, duration_ms, manifest_json, director_tracks_json
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
         RETURNING id, tenant_id, course_id, node_id, content_version_id, asset_id,
                   language, codec, alignment_method, duration_ms, manifest_json, director_tracks_json, created_at",
    )
    .bind(tenant_id)
    .bind(input.course_id)
    .bind(input.node_id)
    .bind(input.content_version_id)
    .bind(input.asset_id)
    .bind(&lang)
    .bind(&codec)
    .bind(&align)
    .bind(duration_ms)
    .bind(manifest_json)
    .bind(tracks_json)
    .fetch_one(&mut **transaction)
    .await
    .map_err(map_db_err)?;
    Ok(record)
}

pub async fn list_course_bundles(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    content_version_id: Uuid,
) -> Result<Vec<BundleRecord>> {
    let mut transaction = begin_as(pool, actor).await?;
    let rows = sqlx::query_as::<_, BundleRecord>(
        "SELECT id, tenant_id, course_id, node_id, content_version_id,
                archetype, modifier, card_elixir_cost, world_theme, language,
                grid_x, grid_y, bundle_json, source_sha256,
                generator_model_id, generator_revision, generator_version, schema_version, created_at
         FROM learning_unit_bundles
         WHERE course_id = $1 AND content_version_id = $2
         ORDER BY grid_y, grid_x, id",
    )
    .bind(course_id)
    .bind(content_version_id)
    .fetch_all(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(rows)
}

pub async fn get_node_simulation(
    pool: &PgPool,
    actor: Uuid,
    node_id: Uuid,
    content_version_id: Uuid,
) -> Result<Option<SimulationRecord>> {
    let mut transaction = begin_as(pool, actor).await?;
    let row = sqlx::query_as::<_, SimulationRecord>(
        "SELECT id, tenant_id, course_id, node_id, content_version_id,
                spec_json, solution_witness, states_checked, proof_domain, verified, verified_at
         FROM node_simulations
         WHERE node_id = $1 AND content_version_id = $2",
    )
    .bind(node_id)
    .bind(content_version_id)
    .fetch_optional(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(row)
}

pub async fn get_axiom_pack(
    pool: &PgPool,
    actor: Uuid,
    node_id: Uuid,
    content_version_id: Uuid,
) -> Result<Option<AxiomPackRecord>> {
    let mut transaction = begin_as(pool, actor).await?;
    let pack = sqlx::query_as::<_, AxiomPackRecord>(
        "SELECT id, tenant_id, course_id, node_id, content_version_id,
                archetype, modifier, w_base_ms, card_elixir_cost, prompt_count, created_at
         FROM axiom_packs
         WHERE node_id = $1 AND content_version_id = $2",
    )
    .bind(node_id)
    .bind(content_version_id)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(mut pack) = pack else {
        transaction.commit().await?;
        return Ok(None);
    };
    let items = sqlx::query_as::<_, AxiomPackItemRecord>(
        "SELECT pack_id, tenant_id, course_id, position, card_id, w_base_ms
         FROM axiom_pack_items
         WHERE pack_id = $1
         ORDER BY position",
    )
    .bind(pack.id)
    .fetch_all(&mut *transaction)
    .await?;
    let unique_cards: BTreeSet<Uuid> = items.iter().map(|i| i.card_id).collect();
    if unique_cards.len() != items.len() {
        return Err(StorageError::Invalid(
            "duplicate card_id in stored axiom pack",
        ));
    }
    pack.items = items;
    transaction.commit().await?;
    Ok(Some(pack))
}

pub async fn get_node_audio_briefing(
    pool: &PgPool,
    actor: Uuid,
    node_id: Uuid,
    content_version_id: Uuid,
    language: &str,
) -> Result<Option<AudioBriefingRecord>> {
    let mut transaction = begin_as(pool, actor).await?;
    let row = sqlx::query_as::<_, AudioBriefingRecord>(
        "SELECT id, tenant_id, course_id, node_id, content_version_id, asset_id,
                language, codec, alignment_method, duration_ms, manifest_json, director_tracks_json, created_at
         FROM node_audio_briefings
         WHERE node_id = $1 AND content_version_id = $2 AND language = $3",
    )
    .bind(node_id)
    .bind(content_version_id)
    .bind(language)
    .fetch_optional(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(row)
}
