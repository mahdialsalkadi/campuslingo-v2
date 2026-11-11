/* Generated from canonical Rust/serde JSON Schema. Do not edit. */

/**
 * This interface was referenced by `AudioManifest`'s JSON-Schema
 * via the `definition` "AlignmentMethod".
 */
export type AlignmentMethod = "model_durations" | "forced_alignment" | "asr_estimate";
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
  duration_ms: number;
  language: Language;
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
