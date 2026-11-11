/* Generated from canonical Rust/serde JSON Schema. Do not edit. */

/**
 * This interface was referenced by `DirectorTrack`'s JSON-Schema
 * via the `definition` "FocusRole".
 */
export type FocusRole = "concept" | "input" | "output" | "fault" | "comparison";
/**
 * This interface was referenced by `DirectorTrack`'s JSON-Schema
 * via the `definition` "ShotType".
 */
export type ShotType = "overview" | "close_up" | "compare";

export interface DirectorTrack {
  end_ms: number;
  explode_factor: number;
  focus_role: FocusRole;
  shot_type: ShotType;
  start_ms: number;
}
