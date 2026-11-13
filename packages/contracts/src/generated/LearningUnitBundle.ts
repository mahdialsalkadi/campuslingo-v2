/* Generated from canonical Rust/serde JSON Schema. Do not edit. */

/**
 * This interface was referenced by `LearningUnitBundle`'s JSON-Schema
 * via the `definition` "AlignmentMethod".
 */
export type AlignmentMethod = "model_durations" | "forced_alignment" | "asr_estimate" | "CLAUSE_CONCAT";
/**
 * This interface was referenced by `LearningUnitBundle`'s JSON-Schema
 * via the `definition` "Codec".
 */
export type Codec = "opus";
/**
 * This interface was referenced by `LearningUnitBundle`'s JSON-Schema
 * via the `definition` "Language".
 */
export type Language = "en" | "ar";
/**
 * This interface was referenced by `LearningUnitBundle`'s JSON-Schema
 * via the `definition` "ChoiceType".
 */
export type ChoiceType = "MCQ_QUIZ";
/**
 * This interface was referenced by `LearningUnitBundle`'s JSON-Schema
 * via the `definition` "FocusRole".
 */
export type FocusRole = "concept" | "input" | "output" | "fault" | "comparison";
/**
 * This interface was referenced by `LearningUnitBundle`'s JSON-Schema
 * via the `definition` "ShotType".
 */
export type ShotType = "overview" | "close_up" | "compare";
/**
 * This interface was referenced by `LearningUnitBundle`'s JSON-Schema
 * via the `definition` "Archetype".
 */
export type Archetype = "PIPELINE" | "TRADEOFF" | "SPATIAL" | "CONTRAST" | "TRAP" | "AXIOM";
/**
 * This interface was referenced by `LearningUnitBundle`'s JSON-Schema
 * via the `definition` "Version".
 */
export type Version = "1.0.0";
/**
 * This interface was referenced by `LearningUnitBundle`'s JSON-Schema
 * via the `definition` "Expression".
 */
export type Expression =
  | {
      kind: "constant";
      value: number;
    }
  | {
      kind: "variable";
      name: string;
    }
  | {
      /**
       * @minItems 2
       * @maxItems 4
       */
      args:
        | [Expression, Expression]
        | [Expression, Expression, Expression]
        | [Expression, Expression, Expression, Expression];
      kind: "operation";
      op: MathOp;
    };
/**
 * This interface was referenced by `LearningUnitBundle`'s JSON-Schema
 * via the `definition` "MathOp".
 */
export type MathOp = "add" | "subtract" | "multiply" | "divide" | "min" | "max";
/**
 * This interface was referenced by `LearningUnitBundle`'s JSON-Schema
 * via the `definition` "WorldTheme".
 */
export type WorldTheme = "BIO_SYSTEM" | "CYBER_GRID" | "MACRO_POLIS" | "ARCANE_ARCHIVE";

export interface LearningUnitBundle {
  audio?: AudioManifest | null;
  challenges: ChoiceChallenge[];
  director_tracks: DirectorTrack[];
  generator_version: string;
  language: Language;
  node_id: string;
  sandbox?: SandboxSpec | null;
  schema_version: Version;
  source_regions: SourceRegion[];
  /**
   * @minItems 2
   * @maxItems 2
   */
  tactical_summary: [string, string];
  title: string;
  world_theme: WorldTheme;
}
/**
 * This interface was referenced by `LearningUnitBundle`'s JSON-Schema
 * via the `definition` "AudioManifest".
 */
export interface AudioManifest {
  alignment_method: AlignmentMethod;
  asset_id: string;
  codec: Codec;
  degraded?: boolean | null;
  duration_ms: number;
  language: Language;
  normalized_wer?: number | null;
  quality_gate_passed?: boolean | null;
  quality_note?: string | null;
  text: string;
  words: AudioWord[];
}
/**
 * This interface was referenced by `LearningUnitBundle`'s JSON-Schema
 * via the `definition` "AudioWord".
 */
export interface AudioWord {
  end_ms: number;
  start_ms: number;
  text: string;
}
/**
 * This interface was referenced by `LearningUnitBundle`'s JSON-Schema
 * via the `definition` "ChoiceChallenge".
 */
export interface ChoiceChallenge {
  correct_index: number;
  explanation: string;
  id: string;
  /**
   * @minItems 2
   * @maxItems 6
   */
  options:
    | [string, string]
    | [string, string, string]
    | [string, string, string, string]
    | [string, string, string, string, string]
    | [string, string, string, string, string, string];
  prompt: string;
  type: ChoiceType;
}
/**
 * This interface was referenced by `LearningUnitBundle`'s JSON-Schema
 * via the `definition` "DirectorTrack".
 */
export interface DirectorTrack {
  end_ms: number;
  explode_factor: number;
  focus_role: FocusRole;
  shot_type: ShotType;
  start_ms: number;
}
/**
 * This interface was referenced by `LearningUnitBundle`'s JSON-Schema
 * via the `definition` "SandboxSpec".
 */
export interface SandboxSpec {
  archetype: Archetype;
  modifier: string;
  /**
   * @minItems 1
   * @maxItems 4
   */
  parameters:
    | [ParameterSpec]
    | [ParameterSpec, ParameterSpec]
    | [ParameterSpec, ParameterSpec, ParameterSpec]
    | [ParameterSpec, ParameterSpec, ParameterSpec, ParameterSpec];
  schema_version: Version;
  /**
   * @minItems 1
   * @maxItems 8
   */
  targets:
    | [NumericTarget]
    | [NumericTarget, NumericTarget]
    | [NumericTarget, NumericTarget, NumericTarget]
    | [NumericTarget, NumericTarget, NumericTarget, NumericTarget]
    | [NumericTarget, NumericTarget, NumericTarget, NumericTarget, NumericTarget]
    | [NumericTarget, NumericTarget, NumericTarget, NumericTarget, NumericTarget, NumericTarget]
    | [NumericTarget, NumericTarget, NumericTarget, NumericTarget, NumericTarget, NumericTarget, NumericTarget]
    | [
        NumericTarget,
        NumericTarget,
        NumericTarget,
        NumericTarget,
        NumericTarget,
        NumericTarget,
        NumericTarget,
        NumericTarget
      ];
  time_limit_ms: number;
}
/**
 * This interface was referenced by `LearningUnitBundle`'s JSON-Schema
 * via the `definition` "ParameterSpec".
 */
export interface ParameterSpec {
  initial: number;
  label: string;
  maximum: number;
  minimum: number;
  name: string;
  step: number;
}
/**
 * This interface was referenced by `LearningUnitBundle`'s JSON-Schema
 * via the `definition` "NumericTarget".
 */
export interface NumericTarget {
  expression: Expression;
  lower: number;
  name: string;
  upper: number;
}
/**
 * This interface was referenced by `LearningUnitBundle`'s JSON-Schema
 * via the `definition` "SourceRegion".
 */
export interface SourceRegion {
  /**
   * @minItems 4
   * @maxItems 4
   */
  bbox: [number, number, number, number];
  page: number;
  page_height: number;
  page_width: number;
  source_sha256: string;
  text_quote: string;
}
