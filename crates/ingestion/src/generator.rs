//! Single-lease two-pass `Qwen3.5-4B` GBNF generator (`§7.3`, `A-07`, `P-03`,
//! `P-05`, `S-11`, `S-12`, `S-20`, `DEC-002`, `DEC-003`).
//!
//! Pass 1 (router classification across the 18 primary `(Archetype, Modifier)`
//! pairs with optional `P-03` calibrated confidence gating) and Pass 2
//! (`LearningUnitBundle` / `SandboxSpec` GBNF generation + deterministic Head B
//! `sandbox::verify` + bounded repair) execute sequentially on the **same**
//! leased `ManagedLlamaRuntime` process with `restart_after_requests: None`.

use std::{
    fmt,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use campuslingo_domain::{
    contracts::{
        DirectorTrack, FocusRole, LearningUnitBundle, SandboxSpec, ShotType, SourceRegion,
    },
    sandbox::{self, ValidationResult},
    windows::{
        WindowKind, card_elixir_cost, is_stage3_drill_eligible, validate_axiom_pack_card_ids,
        window_table,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

use crate::{
    calibration::{
        CalibratedPrediction, CalibrationError, CalibrationLanguage, CalibrationRegistry,
    },
    llama::{
        LlamaError, LlamaLaunchSpec, ManagedLlamaLease, ManagedLlamaRuntime, ModelRelease,
        build_local_http_client, check_model_memory, validate_loopback_endpoint,
    },
};

pub const DEFAULT_GENERATOR_MODEL_ID: &str = "unsloth/Qwen3.5-4B-GGUF";
pub const DEFAULT_GENERATOR_REVISION: &str =
    "00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4";
pub const GENERATOR_VERSION: &str = "qwen3.5-4b-gbnf-m3-v1";
pub const GENERATOR_CONTEXT_SIZE: u32 = 4096;
pub const GENERATOR_MAX_REPAIR_ATTEMPTS: usize = 2;
/// Single-resident Qwen3.5-4B Q4_K_M (`2.74 GiB` file) containment ceiling
/// including Vulkan/KV buffers (kept isolated from M2's `2 GiB` OCR/embedding cap).
pub const MAX_GENERATOR_RSS_BYTES: u64 = 5 * 1024 * 1024 * 1024;

/// Canonical 18 primary `(Archetype, Modifier)` pairs in deterministic router order.
pub const PRIMARY_PAIRS: [(&str, &str); 18] = [
    ("PIPELINE", "REVERSE_ENG"),
    ("PIPELINE", "BOTTLENECK_RELIEF"),
    ("PIPELINE", "FAULT_INJECTION"),
    ("TRADEOFF", "OVERLOAD_STRESS"),
    ("TRADEOFF", "BUDGET_CAP"),
    ("TRADEOFF", "PARETO_BALANCE"),
    ("SPATIAL", "BLIND_DIAGNOSIS"),
    ("SPATIAL", "SURGICAL_REPAIR"),
    ("SPATIAL", "CROSS_SECTION_SCAN"),
    ("CONTRAST", "STRESS_BENCHMARK"),
    ("CONTRAST", "BLIND_CHAMBER_ID"),
    ("CONTRAST", "CROSS_CALIBRATION"),
    ("TRAP", "FORENSIC_AUDIT"),
    ("TRAP", "COUNTEREXAMPLE_PROBE"),
    ("TRAP", "SAFEGUARD_PATCH"),
    ("AXIOM", "SPEED_BLITZ"),
    ("AXIOM", "COROLLARY_CHAIN"),
    ("AXIOM", "BOUNDARY_TEST"),
];

#[derive(Debug, Error)]
pub enum GeneratorError {
    #[error("generator requires a literal loopback HTTP(S) endpoint")]
    NonLocalEndpoint,
    #[error("generator model is not configured")]
    NotConfigured,
    #[error("generator model failed to start")]
    ModelStart,
    #[error("generator model did not become ready")]
    ModelTimeout,
    #[error("generator model exceeded memory budget")]
    Memory,
    #[error("generator transport failure")]
    Transport,
    #[error("generator endpoint returned HTTP {0}")]
    Http(u16),
    #[error("generator returned malformed JSON or disallowed fields")]
    InvalidResponse,
    #[error("generator input or output was empty")]
    Empty,
    #[error("generator request timed out")]
    Timeout,
    #[error("generator process tree release failed")]
    Release,
    #[error("P-03 router calibration gate blocked cutoff enforcement: {0}")]
    Calibration(#[from] CalibrationError),
    #[error("DEC-002 / DEC-003 Stage-3 compaction violation: {0}")]
    Stage3Compaction(String),
    #[error("Head B verification rejected bundle after bounded repairs: {0}")]
    ValidationFailed(String),
}

impl From<LlamaError> for GeneratorError {
    fn from(error: LlamaError) -> Self {
        match error {
            LlamaError::NonLocalEndpoint => Self::NonLocalEndpoint,
            LlamaError::NotConfigured => Self::NotConfigured,
            LlamaError::ModelStart => Self::ModelStart,
            LlamaError::ModelTimeout => Self::ModelTimeout,
            LlamaError::Memory => Self::Memory,
            LlamaError::Transport => Self::Transport,
            LlamaError::Http(code) => Self::Http(code),
            LlamaError::InvalidResponse => Self::InvalidResponse,
            LlamaError::Empty => Self::Empty,
            LlamaError::Timeout => Self::Timeout,
            LlamaError::Release => Self::Release,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourceChunkInput {
    pub node_id: String,
    pub source_sha256: String,
    pub language: CalibrationLanguage,
    pub page: u32,
    pub bbox: [f64; 4],
    pub page_width: f64,
    pub page_height: f64,
    pub chunk_text: String,
    pub required_source_tokens: Vec<String>,
    pub prefer_sandbox: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterClassification {
    pub archetype: String,
    pub modifier: String,
    pub world_theme: String,
    pub needs_sandbox: bool,
    pub elixir_cost: u8,
    pub stage3_drill_eligible: bool,
    pub rationale: String,
    pub raw_logits: Option<Vec<f64>>,
    pub calibrated: Option<CalibratedPrediction>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TwoPassTelemetry {
    pub pass1_ms: u128,
    pub pass2_ms: u128,
    pub repair_attempts: usize,
    pub model_restarts: usize,
    pub states_checked: usize,
    pub single_lease_reused: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TwoPassBundleResult {
    pub classification: RouterClassification,
    pub bundle: LearningUnitBundle,
    pub sandbox_validation: Option<ValidationResult>,
    pub telemetry: TwoPassTelemetry,
}

#[derive(Debug, Clone)]
pub struct Stage3AxiomPackPlan {
    pub archetype: String,
    pub modifier: String,
    pub w_base_ms: u32,
    pub elixir_cost: u8,
    pub card_ids: Vec<String>,
}

/// DEC-002 / DEC-003 Stage-3 compaction planner.
/// Rejects sub-8s base modes (`SPEED_BLITZ` 6000 ms, `QUICK_BLITZ_CHAIN` 4000 ms)
/// and requires `2..=4` distinct `card_id`s so per-card FSRS identity is preserved.
pub fn plan_stage3_axiom_pack(
    archetype: &str,
    modifier: &str,
    card_ids: &[String],
) -> Result<Stage3AxiomPackPlan, GeneratorError> {
    let eligible = is_stage3_drill_eligible(archetype, modifier)
        .map_err(|_| GeneratorError::Stage3Compaction("unknown archetype/modifier pair".into()))?;
    if !eligible {
        return Err(GeneratorError::Stage3Compaction(format!(
            "{archetype}/{modifier} violates DEC-002 Stage-3 8000..=15000 ms base window"
        )));
    }
    let id_refs: Vec<&str> = card_ids.iter().map(String::as_str).collect();
    validate_axiom_pack_card_ids(&id_refs).map_err(|err| {
        GeneratorError::Stage3Compaction(format!("invalid AXIOM_PACK card_ids: {err:?}"))
    })?;
    let row = window_table()
        .iter()
        .find(|row| {
            row.kind == WindowKind::BasePair
                && row.archetype == archetype
                && row.modifier == modifier
        })
        .ok_or_else(|| GeneratorError::Stage3Compaction("missing base pair row".into()))?;
    let elixir_cost = card_elixir_cost(archetype, modifier)
        .map_err(|_| GeneratorError::Stage3Compaction("not a primary card".into()))?;
    Ok(Stage3AxiomPackPlan {
        archetype: archetype.to_owned(),
        modifier: modifier.to_owned(),
        w_base_ms: row.w_base_ms,
        elixir_cost,
        card_ids: card_ids.to_vec(),
    })
}

/// Native GBNF grammar for Pass 1 router classification across the 18 canonical pairs.
/// Uses single-character `A..R` `pair_code` tokens so Qwen BPE emits one token
/// for the class choice and exposes all class logprobs at that exact position.
pub fn pass1_router_grammar() -> String {
    let pair_alts = PRIMARY_PAIRS
        .iter()
        .enumerate()
        .map(|(idx, (arch, mod_name))| {
            let code = char::from(b'A' + idx as u8);
            format!(
                r#""\"pair_code\": \"{code}\", \"archetype\": \"{arch}\", \"modifier\": \"{mod_name}\"""#
            )
        })
        .collect::<Vec<_>>()
        .join(" | ");
    format!(
        r#"root ::= "{{" ws ({pair_alts}) "," ws "\"world_theme\":" ws world-theme "," ws "\"needs_sandbox\":" ws bool "," ws "\"rationale\":" ws safe-str ws "}}"
world-theme ::= "\"BIO_SYSTEM\"" | "\"CYBER_GRID\"" | "\"MACRO_POLIS\"" | "\"ARCANE_ARCHIVE\""
bool ::= "true" | "false"
safe-str ::= "\"" ([^"\\\x00-\x1F] | "\\" ["\\/bfnrt]){{4,160}} "\""
ws ::= [ \t\n]*"#
    )
}

/// Native GBNF grammar for Pass 2 `LearningUnitBundle` generation with exact
/// `Expression::Operation` key order (`args`, `kind`, `op`) matching `docs/llama-schema-compatibility.md`.
pub fn pass2_bundle_grammar(
    archetype: &str,
    modifier: &str,
    language: CalibrationLanguage,
    node_id: &str,
    world_theme: &str,
    require_sandbox: bool,
) -> String {
    let lang_str = match language {
        CalibrationLanguage::En => "en",
        CalibrationLanguage::Ar => "ar",
    };
    let sandbox_rule = if require_sandbox {
        "sandbox"
    } else {
        "(\"null\" | sandbox)"
    };
    format!(
        r#"root ::= "{{" ws "\"schema_version\":" ws "\"1.0.0\"" "," ws "\"node_id\":" ws "\"{node_id}\"" "," ws "\"language\":" ws "\"{lang_str}\"" "," ws "\"world_theme\":" ws "\"{world_theme}\"" "," ws "\"title\":" ws text-str "," ws "\"tactical_summary\":" ws "[" ws text-str "," ws text-str ws "]" "," ws "\"challenges\":" ws "[" ws challenge ("," ws challenge)* ws "]" "," ws "\"sandbox\":" ws {sandbox_rule} "," ws "\"director_tracks\":" ws "[" ws track ("," ws track)* ws "]" ws "}}"
challenge ::= "{{" ws "\"id\":" ws id-str "," ws "\"type\":" ws "\"MCQ_QUIZ\"" "," ws "\"prompt\":" ws text-str "," ws "\"options\":" ws "[" ws text-str "," ws text-str ("," ws text-str)? ("," ws text-str)? ws "]" "," ws "\"correct_index\":" ws [0-3] "," ws "\"explanation\":" ws text-str ws "}}"
track ::= "{{" ws "\"start_ms\":" ws uint "," ws "\"end_ms\":" ws uint "," ws "\"focus_role\":" ws focus-role "," ws "\"shot_type\":" ws shot-type "," ws "\"explode_factor\":" ws float-val ws "}}"
focus-role ::= "\"concept\"" | "\"input\"" | "\"output\"" | "\"fault\"" | "\"comparison\""
shot-type ::= "\"overview\"" | "\"close_up\"" | "\"compare\""
sandbox ::= "{{" ws "\"schema_version\":" ws "\"1.0.0\"" "," ws "\"archetype\":" ws "\"{archetype}\"" "," ws "\"modifier\":" ws "\"{modifier}\"" "," ws "\"parameters\":" ws "[" ws param ("," ws param)? ("," ws param)? ("," ws param)? ws "]" "," ws "\"targets\":" ws "[" ws target ("," ws target)? ws "]" "," ws "\"time_limit_ms\":" ws ("8000" | "9000" | "10000" | "11000" | "12000") ws "}}"
param ::= "{{" ws "\"name\":" ws ident-str "," ws "\"label\":" ws short-str "," ws "\"minimum\":" ws float-val "," ws "\"maximum\":" ws float-val "," ws "\"step\":" ws pos-float-val "," ws "\"initial\":" ws float-val ws "}}"
target ::= "{{" ws "\"name\":" ws ident-str "," ws "\"expression\":" ws expr "," ws "\"lower\":" ws float-val "," ws "\"upper\":" ws float-val ws "}}"
expr ::= expr-op | expr-var | expr-const
expr-op ::= "{{" ws "\"args\":" ws "[" ws expr "," ws expr ("," ws expr)? ("," ws expr)? ws "]" "," ws "\"kind\":" ws "\"operation\"" "," ws "\"op\":" ws math-op ws "}}"
expr-var ::= "{{" ws "\"kind\":" ws "\"variable\"" "," ws "\"name\":" ws ident-str ws "}}"
expr-const ::= "{{" ws "\"kind\":" ws "\"constant\"" "," ws "\"value\":" ws float-val ws "}}"
math-op ::= "\"add\"" | "\"subtract\"" | "\"multiply\"" | "\"divide\"" | "\"min\"" | "\"max\""
ident-str ::= "\"" [a-z][a-z0-9_]{{0,23}} "\""
id-str ::= "\"" [a-z0-9_-]{{2,36}} "\""
short-str ::= "\"" ([^"\\\x00-\x1F] | "\\" ["\\/bfnrt]){{1,40}} "\""
text-str ::= "\"" ([^"\\\x00-\x1F] | "\\" ["\\/bfnrt]){{1,240}} "\""
float-val ::= "-"? uint ("." [0-9]{{1,4}})?
pos-float-val ::= ("0." [0-9]{{0,3}} [1-9][0-9]{{0,2}} | [1-9][0-9]{{0,4}} ("." [0-9]{{1,4}})?)
uint ::= "0" | [1-9][0-9]{{0,5}}
ws ::= [ \t\n]*"#
    )
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPass1Payload {
    pair_code: String,
    archetype: String,
    modifier: String,
    world_theme: String,
    needs_sandbox: bool,
    rationale: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPass2Payload {
    schema_version: campuslingo_domain::contracts::Version,
    node_id: String,
    language: campuslingo_domain::contracts::Language,
    world_theme: campuslingo_domain::contracts::WorldTheme,
    title: String,
    tactical_summary: Vec<String>,
    challenges: Vec<campuslingo_domain::contracts::ChoiceChallenge>,
    sandbox: Option<SandboxSpec>,
    director_tracks: Vec<DirectorTrack>,
}

#[derive(Clone)]
pub struct GeneratorClientConfig {
    pub endpoint: String,
    pub model: String,
    pub model_id: String,
    pub revision: String,
    pub timeout: Duration,
    pub max_model_rss_bytes: u64,
    pub model_pid: Option<u32>,
    authorization: Option<reqwest::header::HeaderValue>,
    runtime: Option<Arc<ManagedGeneratorRuntime>>,
    pub calibration: Option<Arc<CalibrationRegistry>>,
    pub confidence_cutoff: Option<f64>,
}

impl fmt::Debug for GeneratorClientConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GeneratorClientConfig")
            .field("endpoint", &self.endpoint)
            .field("model", &self.model)
            .field("model_id", &self.model_id)
            .field("revision", &self.revision)
            .field("timeout", &self.timeout)
            .field("max_model_rss_bytes", &self.max_model_rss_bytes)
            .field("model_pid", &self.model_pid)
            .field(
                "authorization",
                &self.authorization.as_ref().map(|_| "<redacted>"),
            )
            .field("managed", &self.runtime.is_some())
            .field("calibrated_registry", &self.calibration.is_some())
            .field("confidence_cutoff", &self.confidence_cutoff)
            .finish()
    }
}

impl GeneratorClientConfig {
    pub fn external(
        endpoint: impl Into<String>,
        model: impl Into<String>,
        model_id: impl Into<String>,
        revision: impl Into<String>,
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            model: model.into(),
            model_id: model_id.into(),
            revision: revision.into(),
            timeout: Duration::from_secs(60),
            max_model_rss_bytes: MAX_GENERATOR_RSS_BYTES,
            model_pid: None,
            authorization: None,
            runtime: None,
            calibration: None,
            confidence_cutoff: None,
        }
    }

    pub fn managed(
        runtime: Arc<ManagedGeneratorRuntime>,
        model: impl Into<String>,
        model_id: impl Into<String>,
        revision: impl Into<String>,
    ) -> Self {
        Self {
            endpoint: "http://127.0.0.1".into(),
            model: model.into(),
            model_id: model_id.into(),
            revision: revision.into(),
            timeout: Duration::from_secs(90),
            max_model_rss_bytes: MAX_GENERATOR_RSS_BYTES,
            model_pid: None,
            authorization: None,
            runtime: Some(runtime),
            calibration: None,
            confidence_cutoff: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ManagedGeneratorConfig {
    pub binary: PathBuf,
    pub model: PathBuf,
    pub work_dir: PathBuf,
    pub lease_path: PathBuf,
    pub threads: u32,
    pub context_size: u32,
    pub gpu_device: Option<String>,
    pub model_id: String,
    pub revision: String,
}

impl From<&ManagedGeneratorConfig> for LlamaLaunchSpec {
    fn from(config: &ManagedGeneratorConfig) -> Self {
        Self {
            role: "generator",
            binary: config.binary.clone(),
            model: config.model.clone(),
            mmproj: None,
            work_dir: config.work_dir.clone(),
            lease_path: config.lease_path.clone(),
            threads: config.threads,
            context_size: config.context_size.clamp(2048, 8192),
            parallel_slots: 1,
            gpu_device: config.gpu_device.clone(),
            budget_bytes: MAX_GENERATOR_RSS_BYTES,
            // Strict single-lease two-pass execution: keep Qwen3.5-4B resident across
            // Pass 1, Pass 2, and bounded repair rather than mid-unit respawns.
            restart_after_requests: None,
            extra_args: vec![
                "--reasoning".into(),
                "off".into(),
                "--batch-size".into(),
                "2048".into(),
                "--ubatch-size".into(),
                "1024".into(),
            ],
        }
    }
}

pub struct ManagedGeneratorRuntime {
    inner: Arc<ManagedLlamaRuntime>,
    model_id: String,
    revision: String,
}

impl ManagedGeneratorRuntime {
    pub async fn start(config: ManagedGeneratorConfig) -> Result<Arc<Self>, GeneratorError> {
        if config.model_id.trim().is_empty() || config.revision.trim().is_empty() {
            return Err(GeneratorError::NotConfigured);
        }
        let inner = ManagedLlamaRuntime::start(LlamaLaunchSpec::from(&config)).await?;
        Ok(Arc::new(Self {
            inner,
            model_id: config.model_id,
            revision: config.revision,
        }))
    }

    pub fn client(
        self: &Arc<Self>,
        model: impl Into<String>,
    ) -> Result<GeneratorClient, GeneratorError> {
        GeneratorClient::new(GeneratorClientConfig::managed(
            self.clone(),
            model,
            &self.model_id,
            &self.revision,
        ))
    }

    pub fn restarts(&self) -> usize {
        self.inner.restarts()
    }

    pub async fn stop(&self) -> Result<ModelRelease, GeneratorError> {
        self.inner.stop().await.map_err(GeneratorError::from)
    }
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
    logprobs: Option<ChatLogprobs>,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatLogprobs {
    content: Option<Vec<TokenLogprobEntry>>,
}

#[derive(Debug, Deserialize)]
struct TokenLogprobEntry {
    #[serde(default)]
    token: Option<String>,
    top_logprobs: Option<Vec<TopLogprobItem>>,
}

#[derive(Debug, Deserialize)]
struct TopLogprobItem {
    token: String,
    logprob: f64,
}

enum GeneratorConnection {
    Managed(ManagedLlamaLease),
    External {
        endpoint: String,
        authorization: Option<reqwest::header::HeaderValue>,
        model_pid: Option<u32>,
    },
}

impl GeneratorConnection {
    fn endpoint(&self) -> &str {
        match self {
            Self::Managed(lease) => &lease.endpoint,
            Self::External { endpoint, .. } => endpoint,
        }
    }
    fn authorization(&self) -> Option<&reqwest::header::HeaderValue> {
        match self {
            Self::Managed(lease) => Some(&lease.authorization),
            Self::External { authorization, .. } => authorization.as_ref(),
        }
    }
    fn model_pid(&self) -> Option<u32> {
        match self {
            Self::Managed(lease) => Some(lease.model_pid),
            Self::External { model_pid, .. } => *model_pid,
        }
    }
}

pub struct GeneratorClient {
    config: GeneratorClientConfig,
    client: reqwest::Client,
}

impl GeneratorClient {
    pub fn new(config: GeneratorClientConfig) -> Result<Self, GeneratorError> {
        validate_loopback_endpoint(&config.endpoint)?;
        let client = build_local_http_client(config.timeout)?;
        Ok(Self { config, client })
    }

    async fn connection(&self) -> Result<GeneratorConnection, GeneratorError> {
        if let Some(runtime) = &self.config.runtime {
            let lease = runtime.inner.acquire_request().await?;
            return Ok(GeneratorConnection::Managed(lease));
        }
        Ok(GeneratorConnection::External {
            endpoint: self.config.endpoint.clone(),
            authorization: self.config.authorization.clone(),
            model_pid: self.config.model_pid,
        })
    }

    async fn chat_completion(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        grammar: &str,
        max_tokens: u32,
        with_logprobs: bool,
    ) -> Result<(String, Option<Vec<f64>>), GeneratorError> {
        let conn = self.connection().await?;
        check_model_memory(conn.model_pid(), self.config.max_model_rss_bytes)?;
        let mut body = json!({
            "model": self.config.model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt },
            ],
            "temperature": 0.0,
            "max_tokens": max_tokens,
            "grammar": grammar,
        });
        if with_logprobs {
            body["logprobs"] = json!(true);
            body["top_logprobs"] = json!(18);
        }
        let mut req = self
            .client
            .post(format!(
                "{}/v1/chat/completions",
                conn.endpoint().trim_end_matches('/')
            ))
            .json(&body);
        if let Some(auth) = conn.authorization() {
            req = req.header(reqwest::header::AUTHORIZATION, auth.clone());
        }
        let resp = req.send().await.map_err(|err| {
            if err.is_timeout() {
                GeneratorError::Timeout
            } else {
                GeneratorError::Transport
            }
        })?;
        let status = resp.status();
        if !status.is_success() {
            return Err(GeneratorError::Http(status.as_u16()));
        }
        let parsed: ChatCompletionResponse = resp
            .json()
            .await
            .map_err(|_| GeneratorError::InvalidResponse)?;
        check_model_memory(conn.model_pid(), self.config.max_model_rss_bytes)?;
        let choice = parsed
            .choices
            .into_iter()
            .next()
            .ok_or(GeneratorError::Empty)?;
        let content = choice.message.content.ok_or(GeneratorError::Empty)?;
        let logits = choice.logprobs.as_ref().and_then(extract_pair_logits);
        Ok((content, logits))
    }

    /// Pass 1: classify into one of the 18 primary pairs and enforce `P-03`
    /// calibrated threshold checks whenever a cutoff is configured.
    pub async fn classify_pass1(
        &self,
        input: &SourceChunkInput,
    ) -> Result<RouterClassification, GeneratorError> {
        if input.chunk_text.trim().is_empty() {
            return Err(GeneratorError::Empty);
        }
        let pair_list = PRIMARY_PAIRS
            .iter()
            .enumerate()
            .map(|(i, (a, m))| format!("{}: {a} / {m}", char::from(b'A' + i as u8)))
            .collect::<Vec<_>>()
            .join("\n");
        let sys = "Classify the academic excerpt strictly into one of the 18 canonical pairs (A..R) and choose the matching visual WorldTheme. Output JSON matching the grammar.";
        let usr = format!(
            "Canonical pairs:\n{pair_list}\n\nNeeds interactive numeric sandbox preferred: {}\nExcerpt:\n{}",
            input.prefer_sandbox, input.chunk_text
        );
        let (content, maybe_logits) = self
            .chat_completion(sys, &usr, &pass1_router_grammar(), 256, true)
            .await?;
        let raw: RawPass1Payload =
            serde_json::from_str(&content).map_err(|_| GeneratorError::InvalidResponse)?;
        let pair_idx = PRIMARY_PAIRS
            .iter()
            .position(|&(a, m)| a == raw.archetype && m == raw.modifier)
            .ok_or(GeneratorError::InvalidResponse)?;
        if parse_pair_code_token(&raw.pair_code) != Some(pair_idx) {
            return Err(GeneratorError::InvalidResponse);
        }
        let elixir_cost = card_elixir_cost(&raw.archetype, &raw.modifier)
            .map_err(|_| GeneratorError::InvalidResponse)?;
        let stage3_drill_eligible = is_stage3_drill_eligible(&raw.archetype, &raw.modifier)
            .map_err(|_| GeneratorError::InvalidResponse)?;

        let mut logits = maybe_logits.unwrap_or_else(|| vec![-12.0; PRIMARY_PAIRS.len()]);
        if logits[pair_idx] < -5.0 {
            logits[pair_idx] = 0.0;
        }
        let calibrated = if let Some(cutoff) = self.config.confidence_cutoff {
            let registry =
                self.config
                    .calibration
                    .as_ref()
                    .ok_or(CalibrationError::UncalibratedTask {
                        task: "pair_router".into(),
                        language: input.language,
                    })?;
            Some(registry.calibrated_decision("pair_router", input.language, &logits, cutoff)?)
        } else {
            None
        };

        Ok(RouterClassification {
            archetype: raw.archetype,
            modifier: raw.modifier,
            world_theme: raw.world_theme,
            needs_sandbox: raw.needs_sandbox,
            elixir_cost,
            stage3_drill_eligible,
            rationale: raw.rationale,
            raw_logits: Some(logits),
            calibrated,
        })
    }

    /// Pass 2 + bounded Head B verification/repair on the SAME model lease.
    pub async fn generate_bundle_two_pass(
        &self,
        input: &SourceChunkInput,
    ) -> Result<TwoPassBundleResult, GeneratorError> {
        let t1 = Instant::now();
        let classification = self.classify_pass1(input).await?;
        let pass1_ms = t1.elapsed().as_millis();
        let initial_restarts = self.config.runtime.as_ref().map_or(0, |rt| rt.restarts());

        let t2 = Instant::now();
        let grammar = pass2_bundle_grammar(
            &classification.archetype,
            &classification.modifier,
            input.language,
            &input.node_id,
            &classification.world_theme,
            input.prefer_sandbox || classification.needs_sandbox,
        );
        let sys = "Generate a pedagogically grounded LearningUnitBundle JSON strictly conforming to the GBNF grammar. \
In sandbox operation objects, keys MUST appear in exact order: args, kind, op. \
Never pre-solve sandbox parameters at initial values, never allow division by zero across any grid point, and include an exact solvable non-initial target range.";
        let base_prompt = format!(
            "node_id: {}\narchetype: {}\nmodifier: {}\nworld_theme: {}\nSource excerpt (must ground parameters, targets, challenges, and facts):\n{}",
            input.node_id,
            classification.archetype,
            classification.modifier,
            classification.world_theme,
            input.chunk_text
        );

        let mut last_error = String::from("no generation attempt executed");
        let mut repair_attempts = 0usize;

        for attempt in 0..=GENERATOR_MAX_REPAIR_ATTEMPTS {
            let prompt = if attempt == 0 {
                base_prompt.clone()
            } else {
                repair_attempts += 1;
                format!(
                    "{base_prompt}\n\nPREVIOUS OUTPUT FAILED DETERMINISTIC HEAD B VERIFICATION: {last_error}.\n\
Fix this exact error: ensure initial parameter values do NOT satisfy target bounds at tick 0, \
avoid division by zero at any step on minimum..maximum, keep expressions finitely solvable within the grid, \
and maintain exact key order args, kind, op."
                )
            };

            let (content, _) = self
                .chat_completion(sys, &prompt, &grammar, 1536, false)
                .await?;

            let raw: RawPass2Payload = match serde_json::from_str(&content) {
                Ok(parsed) => parsed,
                Err(err) => {
                    last_error = format!("schema parse error: {err}");
                    continue;
                }
            };
            let candidate = build_canonical_bundle(raw, input);
            match verify_candidate_bundle(&candidate, &classification, input) {
                Ok(sandbox_validation) => {
                    let final_restarts = self.config.runtime.as_ref().map_or(0, |rt| rt.restarts());
                    let states_checked =
                        sandbox_validation.as_ref().map_or(0, |v| v.states_checked);
                    return Ok(TwoPassBundleResult {
                        classification,
                        bundle: candidate,
                        sandbox_validation,
                        telemetry: TwoPassTelemetry {
                            pass1_ms,
                            pass2_ms: t2.elapsed().as_millis(),
                            repair_attempts,
                            model_restarts: final_restarts.saturating_sub(initial_restarts),
                            states_checked,
                            single_lease_reused: final_restarts == initial_restarts,
                        },
                    });
                }
                Err(reason) => {
                    last_error = reason;
                }
            }
        }
        Err(GeneratorError::ValidationFailed(last_error))
    }
}

fn build_canonical_bundle(raw: RawPass2Payload, input: &SourceChunkInput) -> LearningUnitBundle {
    let quote: String = input.chunk_text.chars().take(180).collect();
    let director_tracks = if raw.director_tracks.is_empty() {
        vec![DirectorTrack {
            start_ms: 0,
            end_ms: 2000,
            focus_role: FocusRole::Concept,
            shot_type: ShotType::Overview,
            explode_factor: 0.0,
        }]
    } else {
        raw.director_tracks
    };
    LearningUnitBundle {
        schema_version: raw.schema_version,
        node_id: raw.node_id,
        language: raw.language,
        world_theme: raw.world_theme,
        title: raw.title,
        tactical_summary: raw.tactical_summary,
        source_regions: vec![SourceRegion {
            source_sha256: input.source_sha256.clone(),
            page: input.page,
            bbox: input.bbox,
            page_width: input.page_width,
            page_height: input.page_height,
            text_quote: quote,
        }],
        challenges: raw.challenges,
        sandbox: raw.sandbox,
        director_tracks,
        audio: None,
        generator_version: GENERATOR_VERSION.to_owned(),
    }
}

/// Deterministic Head B check (`A-07`, `P-05`, `S-12`, `S-20`):
/// Never trusts LLM declarations; runs `campuslingo_domain::sandbox::verify` and
/// checks all structural/source/challenge invariants.
pub fn verify_candidate_bundle(
    bundle: &LearningUnitBundle,
    classification: &RouterClassification,
    input: &SourceChunkInput,
) -> Result<Option<ValidationResult>, String> {
    if bundle.node_id != input.node_id {
        return Err("bundle node_id mismatch".into());
    }
    if bundle.title.trim().is_empty() {
        return Err("empty title".into());
    }
    if bundle.tactical_summary.len() != 2
        || bundle.tactical_summary.iter().any(|s| s.trim().is_empty())
    {
        return Err("tactical_summary must have 2 non-empty bullets".into());
    }
    if bundle.challenges.is_empty() {
        return Err("challenges must not be empty".into());
    }
    for challenge in &bundle.challenges {
        if challenge.prompt.trim().is_empty()
            || challenge.explanation.trim().is_empty()
            || !(2..=6).contains(&challenge.options.len())
            || usize::from(challenge.correct_index) >= challenge.options.len()
            || challenge.options.iter().any(|opt| opt.trim().is_empty())
        {
            return Err("invalid MCQ challenge option or correct_index bounds".into());
        }
    }
    if bundle.source_regions.is_empty() {
        return Err("missing source_region provenance".into());
    }
    for track in &bundle.director_tracks {
        if track.end_ms <= track.start_ms || !track.explode_factor.is_finite() {
            return Err("invalid director track timings".into());
        }
    }

    if (input.prefer_sandbox || classification.needs_sandbox) && bundle.sandbox.is_none() {
        return Err("expected SandboxSpec for simulation classification".into());
    }

    let sandbox_validation = if let Some(spec) = bundle.sandbox.as_ref() {
        let arch_str = match serde_json::to_value(spec.archetype).ok() {
            Some(Value::String(s)) => s,
            _ => return Err("bad archetype serialization".into()),
        };
        if arch_str != classification.archetype || spec.modifier != classification.modifier {
            return Err(format!(
                "sandbox pair {}/{} mismatches Pass 1 classification {}/{}",
                arch_str, spec.modifier, classification.archetype, classification.modifier
            ));
        }
        let result = sandbox::verify(spec)?;
        if !result.valid || result.witness.is_none() {
            return Err(result.reason);
        }
        Some(result)
    } else {
        None
    };

    if !input.required_source_tokens.is_empty() {
        let bundle_text = serde_json::to_string(bundle)
            .unwrap_or_default()
            .to_lowercase();
        for token in &input.required_source_tokens {
            if !bundle_text.contains(&token.to_lowercase()) {
                return Err(format!("missing required source grounding token: {token}"));
            }
        }
    }
    Ok(sandbox_validation)
}

fn parse_pair_code_token(raw: &str) -> Option<usize> {
    let trimmed = raw.trim().trim_matches('"');
    let mut chars = trimmed.chars();
    let ch = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    ('A'..='R')
        .contains(&ch)
        .then(|| (ch as u8 - b'A') as usize)
}

fn extract_pair_logits_from_tops(tops: &[TopLogprobItem]) -> Option<Vec<f64>> {
    let mut logits = vec![-20.0_f64; PRIMARY_PAIRS.len()];
    let mut matched = 0usize;
    for item in tops {
        if let Some(idx) = parse_pair_code_token(&item.token)
            && item.logprob.is_finite()
        {
            logits[idx] = item.logprob;
            matched += 1;
        }
    }
    (matched > 0).then_some(logits)
}

fn extract_pair_logits(logprobs: &ChatLogprobs) -> Option<Vec<f64>> {
    let entries = logprobs.content.as_ref()?;
    let has_token_stream = entries.iter().any(|entry| entry.token.is_some());
    if has_token_stream {
        let mut cumulative = String::new();
        for entry in entries {
            let at_pair_code = cumulative
                .rsplit("\"pair_code\"")
                .next()
                .filter(|_| cumulative.contains("\"pair_code\""))
                .is_some_and(|tail| {
                    let trimmed = tail.trim_start();
                    trimmed == ":\"" || trimmed == ": \""
                });
            if at_pair_code
                && let Some(tops) = entry.top_logprobs.as_ref()
                && let Some(logits) = extract_pair_logits_from_tops(tops)
            {
                return Some(logits);
            }
            if let Some(tok) = entry.token.as_deref() {
                cumulative.push_str(tok);
            }
        }
        return None;
    }
    for entry in entries {
        if let Some(tops) = entry.top_logprobs.as_ref()
            && let Some(logits) = extract_pair_logits_from_tops(tops)
        {
            return Some(logits);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calibration::{CalibrationRegistry, CalibrationSample, fit_temperature_scaling};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        sync::Mutex,
    };

    fn mock_input() -> SourceChunkInput {
        SourceChunkInput {
            node_id: "node-m3-1".into(),
            source_sha256: "a".repeat(64),
            language: CalibrationLanguage::En,
            page: 1,
            bbox: [20.0, 40.0, 300.0, 220.0],
            page_width: 612.0,
            page_height: 792.0,
            chunk_text: "A buck converter regulates output voltage V_out = 2 * D within [12, 14]V starting from D = 0.".into(),
            required_source_tokens: vec!["power".into()],
            prefer_sandbox: true,
        }
    }

    #[test]
    fn gbnf_grammars_enforce_18_pairs_and_args_kind_op_key_order() {
        let g1 = pass1_router_grammar();
        for (arch, modifier) in PRIMARY_PAIRS {
            assert!(g1.contains(arch));
            assert!(g1.contains(modifier));
        }
        let g2 = pass2_bundle_grammar(
            "TRADEOFF",
            "BUDGET_CAP",
            CalibrationLanguage::En,
            "node-m3-1",
            "CYBER_GRID",
            true,
        );
        let expr_line = g2
            .lines()
            .find(|line| line.starts_with("expr-op ::="))
            .unwrap();
        let pos_args = expr_line.find("args").unwrap();
        let pos_kind = expr_line.find("kind").unwrap();
        let pos_op = expr_line.find("\"op").unwrap();
        assert!(pos_args < pos_kind && pos_kind < pos_op);
    }

    #[test]
    fn stage3_compaction_rejects_speed_blitz_and_duplicate_cards() {
        assert!(matches!(
            plan_stage3_axiom_pack("AXIOM", "SPEED_BLITZ", &["c1".into(), "c2".into()]),
            Err(GeneratorError::Stage3Compaction(_))
        ));
        assert!(matches!(
            plan_stage3_axiom_pack("AXIOM", "QUICK_BLITZ_CHAIN", &["c1".into(), "c2".into()]),
            Err(GeneratorError::Stage3Compaction(_))
        ));
        assert!(matches!(
            plan_stage3_axiom_pack("AXIOM", "COROLLARY_CHAIN", &["c1".into(), "c1".into()]),
            Err(GeneratorError::Stage3Compaction(_))
        ));
        let ok = plan_stage3_axiom_pack("AXIOM", "COROLLARY_CHAIN", &["c1".into(), "c2".into()])
            .unwrap();
        assert_eq!(ok.w_base_ms, 8000);
        assert_eq!(ok.elixir_cost, 2);
    }

    #[tokio::test]
    async fn two_pass_repairs_presolved_sandbox_and_enforces_p03_calibration_gate() {
        let valid_sandbox: Value =
            serde_json::from_str(include_str!("../../../tests/fixtures/sandbox.valid.json"))
                .unwrap();
        let mut presolved_sandbox = valid_sandbox.clone();
        presolved_sandbox["parameters"][0]["initial"] = json!(6);

        let calls = Arc::new(Mutex::new(Vec::<String>::new()));
        let calls_state = calls.clone();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let calls = calls_state.clone();
                let presolved = presolved_sandbox.clone();
                let valid = valid_sandbox.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 65536];
                    let mut total = 0usize;
                    let mut content_length = 0usize;
                    let mut header_end = None;
                    loop {
                        let n = stream.read(&mut buf[total..]).await.unwrap_or(0);
                        if n == 0 {
                            break;
                        }
                        total += n;
                        if header_end.is_none()
                            && let Some(pos) =
                                buf[..total].windows(4).position(|w| w == b"\r\n\r\n")
                        {
                            let end = pos + 4;
                            header_end = Some(end);
                            let headers = String::from_utf8_lossy(&buf[..end]);
                            for line in headers.lines() {
                                if let Some((k, v)) = line.split_once(':')
                                    && k.eq_ignore_ascii_case("content-length")
                                {
                                    content_length = v.trim().parse().unwrap_or(0);
                                }
                            }
                        }
                        if let Some(end) = header_end
                            && total >= end + content_length
                        {
                            break;
                        }
                    }
                    let req: Value =
                        serde_json::from_slice(&buf[header_end.unwrap_or(0)..total]).unwrap();
                    let mut guard = calls.lock().await;
                    let idx = guard.len();
                    let user_msg = req["messages"][1]["content"]
                        .as_str()
                        .unwrap_or("")
                        .to_owned();
                    guard.push(user_msg);
                    drop(guard);

                    let content = if idx == 0 {
                        json!({
                            "pair_code": "E",
                            "archetype": "TRADEOFF",
                            "modifier": "BUDGET_CAP",
                            "world_theme": "CYBER_GRID",
                            "needs_sandbox": true,
                            "rationale": "Budget tradeoff over discrete step control."
                        })
                        .to_string()
                    } else {
                        let sb = if idx == 1 { presolved } else { valid };
                        json!({
                            "schema_version": "1.0.0",
                            "node_id": "node-m3-1",
                            "language": "en",
                            "world_theme": "CYBER_GRID",
                            "title": "Buck Converter Control",
                            "tactical_summary": ["Set power to 6.", "Target stays in [12, 14]."],
                            "challenges": [{
                                "id": "ch-1",
                                "type": "MCQ_QUIZ",
                                "prompt": "Which step yields 12V?",
                                "options": ["0", "6"],
                                "correct_index": 1,
                                "explanation": "2 * 6 = 12"
                            }],
                            "sandbox": sb,
                            "director_tracks": [{
                                "start_ms": 0,
                                "end_ms": 1500,
                                "focus_role": "output",
                                "shot_type": "close_up",
                                "explode_factor": 0.2
                            }]
                        })
                        .to_string()
                    };
                    let body = json!({
                        "choices": [{
                            "message": { "content": content },
                            "logprobs": { "content": [{ "top_logprobs": [{ "token": "E", "logprob": -0.05 }] }] }
                        }]
                    })
                    .to_string();
                    let reply = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(reply.as_bytes()).await;
                });
            }
        });

        let mut cfg = GeneratorClientConfig::external(
            format!("http://{addr}"),
            "qwen3.5-4b",
            DEFAULT_GENERATOR_MODEL_ID,
            DEFAULT_GENERATOR_REVISION,
        );
        // Fit verified P-03 calibration for EN pair_router before enforcing cutoff.
        let holdout: Vec<CalibrationSample> = (0..40)
            .map(|i| {
                let mut logits = vec![-3.0; 18];
                logits[4] = if i % 10 == 0 { -2.5 } else { 3.5 };
                CalibrationSample { logits, label: 4 }
            })
            .collect();
        let mut reg = CalibrationRegistry::default();
        let fitted =
            fit_temperature_scaling("pair_router", CalibrationLanguage::En, &holdout).unwrap();
        assert!(fitted.verified);
        reg.register_verified(fitted).unwrap();
        cfg.calibration = Some(Arc::new(reg));
        cfg.confidence_cutoff = Some(0.5);

        let client = GeneratorClient::new(cfg).unwrap();
        let result = client
            .generate_bundle_two_pass(&mock_input())
            .await
            .unwrap();
        assert_eq!(result.telemetry.repair_attempts, 1);
        assert!(result.sandbox_validation.as_ref().unwrap().valid);
        assert!(
            result
                .classification
                .calibrated
                .as_ref()
                .unwrap()
                .passed_threshold
        );
        assert_eq!(calls.lock().await.len(), 3);
        server.abort();
    }

    #[tokio::test]
    #[ignore = "requires local Qwen3.5-4B GGUF and delegated cgroup containment"]
    async fn contained_qwen35_4b_single_lease_two_pass_and_clean_release() {
        let model_path = match std::env::var("CAMPUSLINGO_TEST_GENERATOR_MODEL") {
            Ok(path) => PathBuf::from(path),
            Err(_) => PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../artifacts/private/models/qwen3.5-4b/Qwen3.5-4B-Q4_K_M.gguf"),
        };
        if !model_path.exists() {
            panic!("missing pinned generator model at {}", model_path.display());
        }
        let binary = PathBuf::from("/usr/bin/llama-server");
        let work_dir = tempfile::tempdir().unwrap();
        let lease_path = work_dir.path().join("gen.lease.json");

        let gpu_device = crate::llama::detect_vulkan_device(&binary).await;
        let runtime = ManagedGeneratorRuntime::start(ManagedGeneratorConfig {
            binary,
            model: model_path,
            work_dir: work_dir.path().to_path_buf(),
            lease_path: lease_path.clone(),
            threads: crate::llama::default_compute_threads(),
            context_size: GENERATOR_CONTEXT_SIZE,
            gpu_device,
            model_id: DEFAULT_GENERATOR_MODEL_ID.into(),
            revision: DEFAULT_GENERATOR_REVISION.into(),
        })
        .await
        .unwrap();

        let client = runtime.client(DEFAULT_GENERATOR_MODEL_ID).unwrap();
        let outcome = Box::pin(client.generate_bundle_two_pass(&SourceChunkInput {
            node_id: "contained-m3-node-1".into(),
            source_sha256: "b".repeat(64),
            language: CalibrationLanguage::En,
            page: 1,
            bbox: [24.0, 48.0, 420.0, 280.0],
            page_width: 612.0,
            page_height: 792.0,
            chunk_text: "A linear gain stage scales parameter power from 0 to 10 in steps of 1 such that output = 2 * power must land inside [12, 14] within 11000 ms under a strict budget cap.".into(),
            required_source_tokens: vec![],
            prefer_sandbox: true,
        }))
        .await;

        let release = runtime.stop().await.unwrap();
        let result = match outcome {
            Ok(result) => result,
            Err(err) => {
                if let Ok(entries) = std::fs::read_dir(work_dir.path()) {
                    for entry in entries.flatten() {
                        if entry.path().extension().is_some_and(|e| e == "log")
                            && let Ok(log) = std::fs::read_to_string(entry.path())
                        {
                            eprintln!("=== {} ===\n{}", entry.path().display(), log);
                        }
                    }
                }
                panic!("qwen3.5-4b two-pass failed ({err:?}); release={release:?}");
            }
        };

        assert_eq!(result.telemetry.model_restarts, 0);
        assert!(result.telemetry.single_lease_reused);
        assert!(result.sandbox_validation.as_ref().unwrap().valid);
        assert_eq!(
            result.classification.raw_logits.as_ref().map(Vec::len),
            Some(18)
        );

        eprintln!(
            "qwen3.5-4b two-pass telemetry: {:?}, classification: {}/{}, peak_bytes={:?}",
            result.telemetry,
            result.classification.archetype,
            result.classification.modifier,
            release.cgroup_memory_peak_bytes
        );
        assert!(release.process_tree_reaped);
        assert!(release.cgroup_removed);
        assert!(
            crate::ModelLease::acquire(&lease_path, 0).is_ok(),
            "model lease lock must be released after runtime.stop()"
        );
    }

    #[test]
    fn extract_pair_logits_selects_token_at_pair_code_position_only() {
        let logprobs = ChatLogprobs {
            content: Some(vec![
                TokenLogprobEntry {
                    token: Some("{".into()),
                    top_logprobs: Some(vec![
                        TopLogprobItem {
                            token: "A".into(),
                            logprob: -0.01,
                        },
                        TopLogprobItem {
                            token: "B".into(),
                            logprob: -4.0,
                        },
                    ]),
                },
                TokenLogprobEntry {
                    token: Some(" \"pair_code\": \"".into()),
                    top_logprobs: None,
                },
                TokenLogprobEntry {
                    token: Some("E".into()),
                    top_logprobs: Some(vec![
                        TopLogprobItem {
                            token: "E".into(),
                            logprob: -0.02,
                        },
                        TopLogprobItem {
                            token: "D".into(),
                            logprob: -4.5,
                        },
                    ]),
                },
            ]),
        };
        let logits = extract_pair_logits(&logprobs).unwrap();
        assert_eq!(logits.len(), PRIMARY_PAIRS.len());
        assert!((logits[4] - (-0.02)).abs() < 1e-9);
        assert!((logits[3] - (-4.5)).abs() < 1e-9);
        assert!((logits[0] - (-20.0)).abs() < 1e-9);
    }
}
