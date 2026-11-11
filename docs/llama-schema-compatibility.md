# Constrained JSON: property order is part of the runtime interface

During M0, the Qwen 4B GGUF produced a constant-valued target instead of the
requested multiply expression. It kept doing so even when a bounded repair
included the correct expression in ordinary `kind, op, args` JSON key order.
The execution/source verifier rejected those results; valid JSON was not enough.

The serialized Rust schema supplies the operation variant's properties in
`args, kind, op` order. llama.cpp's JSON-schema grammar construction emits
required property productions in the supplied property order. Thus examples
starting with `kind` bias the model toward a different valid variant whose
first property is `kind` (such as a constant) rather than the operation branch.

The successful synthetic prompt matches the enforced order:

```json
{
  "args": [
    { "kind": "constant", "value": 2 },
    { "kind": "variable", "name": "power" }
  ],
  "kind": "operation",
  "op": "multiply"
}
```

After changing the ordering instructions, two seeded trials produced the
requested `output = 2 * power` expression, retained all source bounds, and
passed exhaustive validation over all 11 slider states. Neither needed a
repair. The witness was `power = 6`. This is a tiny synthetic capability probe,
not a held-out generation benchmark or a 98.5% quality claim.

Evidence:

- Runtime: local llama-server 10199 / revision 290d243.
- Canonical schema: `packages/contracts/schema/SandboxSpec.json`.
- Worker probe: `crates/ingestion/src/main.rs`.
- Preserved attempts: `artifacts/private/benchmark-generator.initial.json` and
  timestamped `benchmark-generator.before-*.json` snapshots.
- Latest output: `artifacts/private/benchmark-generator.json`.
- The local llama.cpp source's `common/json-schema-to-grammar.cpp`,
  `_build_object_rule`, illustrates required-property ordering. Do not assume
  every locally checked-out source revision equals an installed binary.

For M3, derive prompt examples from the actual provider-compatible serialized
schema, or otherwise keep schema order and examples tested together. Keep
shape, execution/reachability, and source-faithfulness validation separate.
Never change a target's bounds to make an incorrect expression pass.
