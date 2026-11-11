/* Generated from canonical Rust/serde JSON Schema. Do not edit. */

export type ClientCommand =
  | {
      card_id: string;
      lane: number;
      match_id: string;
      protocol_version: Version;
      sequence: number;
      type: "deploy";
    }
  | {
      challenge_id: string;
      match_id: string;
      protocol_version: Version;
      selected_index: number;
      sequence: number;
      type: "answer";
    };
export type Version = "1.0.0";
