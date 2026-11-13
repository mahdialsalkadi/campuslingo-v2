/* Generated from canonical Rust/serde JSON Schema. Do not edit. */

/**
 * This interface was referenced by `AudioManifest`'s JSON-Schema
 * via the `definition` "AlignmentMethod".
 */
export type AlignmentMethod = "model_durations" | "forced_alignment" | "asr_estimate" | "CLAUSE_CONCAT";
/**
 * This interface was referenced by `AudioManifest`'s JSON-Schema
 * via the `definition` "Codec".
 */
export type Codec = "opus";
/**
 * This interface was referenced by `AudioManifest`'s JSON-Schema
 * via the `definition` "Language".
 */
export type Language = "en" | "ar";

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
 * This interface was referenced by `AudioManifest`'s JSON-Schema
 * via the `definition` "AudioWord".
 */
export interface AudioWord {
  end_ms: number;
  start_ms: number;
  text: string;
}
