/* Generated from canonical Rust/serde JSON Schema. Do not edit. */

/**
 * This interface was referenced by `SandboxSpec`'s JSON-Schema
 * via the `definition` "Archetype".
 */
export type Archetype = "PIPELINE" | "TRADEOFF" | "SPATIAL" | "CONTRAST" | "TRAP" | "AXIOM";
/**
 * This interface was referenced by `SandboxSpec`'s JSON-Schema
 * via the `definition` "Version".
 */
export type Version = "1.0.0";
/**
 * This interface was referenced by `SandboxSpec`'s JSON-Schema
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
 * This interface was referenced by `SandboxSpec`'s JSON-Schema
 * via the `definition` "MathOp".
 */
export type MathOp = "add" | "subtract" | "multiply" | "divide" | "min" | "max";

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
 * This interface was referenced by `SandboxSpec`'s JSON-Schema
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
 * This interface was referenced by `SandboxSpec`'s JSON-Schema
 * via the `definition` "NumericTarget".
 */
export interface NumericTarget {
  expression: Expression;
  lower: number;
  name: string;
  upper: number;
}
