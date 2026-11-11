use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum Version {
    #[serde(rename = "1.0.0")]
    V1,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Archetype {
    Pipeline,
    Tradeoff,
    Spatial,
    Contrast,
    Trap,
    Axiom,
}

impl Archetype {
    pub fn modifiers(self) -> &'static [&'static str] {
        match self {
            Self::Pipeline => &["REVERSE_ENG", "BOTTLENECK_RELIEF", "FAULT_INJECTION"],
            Self::Tradeoff => &["OVERLOAD_STRESS", "BUDGET_CAP", "PARETO_BALANCE"],
            Self::Spatial => &["BLIND_DIAGNOSIS", "SURGICAL_REPAIR", "CROSS_SECTION_SCAN"],
            Self::Contrast => &["STRESS_BENCHMARK", "BLIND_CHAMBER_ID", "CROSS_CALIBRATION"],
            Self::Trap => &["FORENSIC_AUDIT", "COUNTEREXAMPLE_PROBE", "SAFEGUARD_PATCH"],
            Self::Axiom => &["SPEED_BLITZ", "COROLLARY_CHAIN", "BOUNDARY_TEST"],
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorldTheme {
    BioSystem,
    CyberGrid,
    MacroPolis,
    ArcaneArchive,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MathOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Min,
    Max,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Expression {
    Constant {
        value: f64,
    },
    Variable {
        name: String,
    },
    Operation {
        op: MathOp,
        #[schemars(length(min = 2, max = 4))]
        args: Vec<Expression>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ParameterSpec {
    pub name: String,
    pub label: String,
    pub minimum: f64,
    pub maximum: f64,
    pub step: f64,
    pub initial: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NumericTarget {
    pub name: String,
    pub expression: Expression,
    pub lower: f64,
    pub upper: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SandboxSpec {
    pub schema_version: Version,
    pub archetype: Archetype,
    pub modifier: String,
    #[schemars(length(min = 1, max = 4))]
    pub parameters: Vec<ParameterSpec>,
    #[schemars(length(min = 1, max = 8))]
    pub targets: Vec<NumericTarget>,
    #[schemars(range(min = 1000, max = 30000))]
    pub time_limit_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceRegion {
    pub source_sha256: String,
    pub page: u32,
    pub bbox: [f64; 4],
    pub page_width: f64,
    pub page_height: f64,
    pub text_quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum ChoiceType {
    #[serde(rename = "MCQ_QUIZ")]
    Mcq,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChoiceChallenge {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: ChoiceType,
    pub prompt: String,
    #[schemars(length(min = 2, max = 6))]
    pub options: Vec<String>,
    pub correct_index: u8,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FocusRole {
    Concept,
    Input,
    Output,
    Fault,
    Comparison,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ShotType {
    Overview,
    CloseUp,
    Compare,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DirectorTrack {
    pub start_ms: u32,
    pub end_ms: u32,
    pub focus_role: FocusRole,
    pub shot_type: ShotType,
    pub explode_factor: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum Language {
    #[serde(rename = "en")]
    English,
    #[serde(rename = "ar")]
    Arabic,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum Codec {
    #[serde(rename = "opus")]
    Opus,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AlignmentMethod {
    ModelDurations,
    ForcedAlignment,
    AsrEstimate,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioWord {
    pub text: String,
    pub start_ms: u32,
    pub end_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioManifest {
    pub asset_id: String,
    pub language: Language,
    pub codec: Codec,
    pub text: String,
    pub duration_ms: u32,
    pub alignment_method: AlignmentMethod,
    pub words: Vec<AudioWord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LearningUnitBundle {
    pub schema_version: Version,
    pub node_id: String,
    pub language: Language,
    pub world_theme: WorldTheme,
    pub title: String,
    #[schemars(length(min = 2, max = 2))]
    pub tactical_summary: Vec<String>,
    pub source_regions: Vec<SourceRegion>,
    pub challenges: Vec<ChoiceChallenge>,
    pub sandbox: Option<SandboxSpec>,
    pub director_tracks: Vec<DirectorTrack>,
    pub audio: Option<AudioManifest>,
    pub generator_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientCommand {
    Deploy {
        protocol_version: Version,
        match_id: String,
        sequence: u32,
        card_id: String,
        lane: u8,
    },
    Answer {
        protocol_version: Version,
        match_id: String,
        sequence: u32,
        challenge_id: String,
        selected_index: u8,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicQuestion {
    pub id: String,
    pub prompt: String,
    #[schemars(length(min = 2, max = 6))]
    pub options: Vec<String>,
}

pub fn schemas() -> Vec<(&'static str, schemars::Schema)> {
    vec![
        ("SandboxSpec", schemars::schema_for!(SandboxSpec)),
        (
            "LearningUnitBundle",
            schemars::schema_for!(LearningUnitBundle),
        ),
        ("DirectorTrack", schemars::schema_for!(DirectorTrack)),
        ("AudioManifest", schemars::schema_for!(AudioManifest)),
        ("ClientCommand", schemars::schema_for!(ClientCommand)),
        ("PublicQuestion", schemars::schema_for!(PublicQuestion)),
    ]
}
