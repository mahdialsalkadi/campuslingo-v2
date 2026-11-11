/* Generated from canonical Rust/serde JSON Schema. Do not edit. */

export interface PublicQuestion {
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
}
