# M0 status and open gates

This is a feasibility milestone, not a claim of a completed application.
Authoritative architecture: Rust/axum/tokio/sqlx/serde, shared official FSRS-6
and validation in native Rust/WASM, separate local model processes, optional
`ort` in a Rust worker. Python is offline evaluation only.

## Implemented foundations

- Cargo workspace, reproducible Rust/Node/Python evaluation lockfiles, small
  release builds, and bounded compilation parallelism.
- Explicit selection of FSRS-6 in the official `fsrs` crate. Upstream 7.0.0
  defaults to the FSRS-7 algorithm; using its default would be incorrect here.
- Canonical Rust content/protocol types, generated JSON Schema and TypeScript.
- Safe arithmetic AST and exhaustive verification over declared discrete slider
  grids, with bounded expression depth, nodes, and a 4096-state ceiling.
- Native/WASM entrypoint shared by memory, continuous shields, sandbox, and
  deployment validation. No unrestricted eval/code execution.
- Integer combat preconditions for energy, cost, lane, match clock, and sequence.
  This is not yet the full authoritative 90-second simulation.
- Loopback-only axum foundation with explicit health/readiness distinction,
  bounded validation sockets, and no premature production-ready claim.
- Rust cross-process model lease, existing-server detection, host-memory
  admission check, 3 GiB provisional host reserve, watchdog, HTTP inference,
  and owned-process cleanup. No user service is stopped.
- A dynamic `ort` feature that does not download ONNX Runtime automatically.
- Private automatic evaluation-corpus selection and read-only model audit.

## Measured results

Initial resource/model probes are recorded in `artifacts/benchmarks` and
`artifacts/private`. The first release-mode server probe measured:

| Measurement | Initial observation |
| --- | --- |
| Clients | 30 persistent validation WebSockets |
| Duration / successful requests | 10 seconds / 1,200 |
| Request errors | 0 |
| Idle / peak server RSS | 4.07 MiB / 8.43 MiB |
| Local response latency p50 / p95 / p99 | 0.67 ms / 1.76 ms / 4.35 ms |
| Database readiness | HTTP 503, honestly unconfigured |

The same sequential probe loaded both local GGUFs. The first generator run
produced valid JSON but invalid operation arity, and both invalid sandbox
outputs were rejected. Those failures are retained in initial evidence before
tightening the schema/prompt. The native Rust `ort` probe successfully loaded
`/usr/lib/libonnxruntime.so.1.29.0`; actual speech/model alignment is not tested.

The expanded rerun used 190 native/WASM inputs: 33 float fields exceeded the
absolute gate, with maximum observed drift approximately `8e-5` days. Discrete
combat/sandbox outcomes matched. The updated server probe measured 8.47 MiB peak
RSS in that run. The latest full resource probe measured 8.57 MiB peak RSS,
1,200 successful requests, and approximately 2.02 ms / 4.53 ms p95/p99.

The Qwen synthetic router matched 3/3 labels; Jev matched 2/3 labels. These tiny
samples are not a calibrated accuracy benchmark. Early generated targets were
invalid or constant-valued and unreachable, including after bounded repair;
all were rejected and retained. The cause was traced to grammar-required key
order: operation objects must start with `args`, not `kind`. With aligned
ordering instructions, both latest trials produced valid source-matching
multiply expressions, passed all 11 slider states, and needed no repairs.
See [schema compatibility](llama-schema-compatibility.md).

The synthetic generation smoke gate now passes. General pedagogical generation,
held-out quality, all archetypes, OCR, and speech are **not** established by two
identical seeded examples.

The latest Qwen process peaked around 3.52 GiB RSS with at least 6.75 GiB host memory
available in that probe; Jev peaked around 1.35 GiB RSS. Do not add these figures
to overlapping GPU allocations. Both owned server processes were reaped before
the next model loaded. These are foundation measurements, not full-game acceptance.
Do not interpret an available model repository or a successfully compiled crate
as a completed model-inference, TTS, OCR, or game-load benchmark.

## Numeric parity: approved f32-aware policy

The initial native/WASM run used 154 synthetic inputs. Sandbox/combat outcomes
and error behavior matched, but upstream `f32` transcendental math produced
small differences in mature-card stability/interval outputs, including about
`8e-5` days in that first set. Some values exceeded the approved absolute
`1e-6` gate. The expanded fixtures also cover continuous shield curves.

After reviewing those observations, the user explicitly approved:

```text
Floating memory estimates: abs_error <= 1e-6 + 1e-6 * abs(reference)
Integer combat, validation decisions, rounded days, other discrete outcomes: exact
```

`npm run test:wasm` enforces that policy. It preserves the absolute-only drift
and original vectors in diagnostics; the official crate has not been forked.
The policy change was explicitly approved, not applied silently.

Alternatives considered before approval (not selected):

1. Investigate a deterministic cross-target math backend or an explicitly
   reviewed upstream change while retaining the official FSRS-6 algorithm.
2. Evaluate running one identical compiled WASM artifact on both host/client,
   but benchmark the host runtime's RAM/CPU cost first; do not introduce it
   merely to conceal a difference or assume it fits the 50 MiB target.

Review scheduling accepts complete elapsed days explicitly because the
upstream FSRS-6 implementation rounds its nominally fractional API internally.
Shield decay uses the separate public continuous-time curve; it does not
silently inherit scheduler rounding.

## Hardware/model observations

- The host exposes approximately 11 GiB RAM and a 4 GiB GPU reservation.
  Vulkan/GTT reports overlap physical memory and must not be added together.
- Initial free disk was approximately 10 GiB; model downloads are not automatic.
- The local file named `qwen3.5-4b-instruct-q4_k_m.gguf` is actually **Q6_K**
  (`general.file_type=18`), approximately 3.64 GB, not the stated Q4 estimate.
- The local file named `qwen3.5-0.8b-instruct-q4_k_m.gguf` is **Q8_0**
  (`general.file_type=7`); metadata also points at a Base model lineage.
- The local Jev file reports **Q4_K_M** (`general.file_type=15`).
- SHA-256 and model metadata are in the private audit. Renamed local files
  have not yet been matched to specific upstream LFS objects.
- GGUF file-type meanings were checked against the existing llama.cpp header.
- `llama-server` 10199 / revision `290d243` sees the BC-250 Vulkan device;
  ROCm inspection reports `gfx1013`. Inference availability does not establish
  supported local LoRA/GRPO training.

## Corpus

The scanner found 55 PDFs and excludes administrative material and reference
books/standards under Materials from its representative-lecture selection.

Selected before OCR verification:

1. Digital text: `Topic 2 [Part 1] Types of Data-StudentsCopy.pdf` (64 pages).
2. Diagram/math: `1 LOGIC_v362761.pdf` (101 pages).
3. Low-text/image-based: `TOPIC 2_SET THEORY.pdf` (35 pages).

The third file's English classification is a candidate inferred from its
context/title; OCR must confirm it. Selection uses bounded page sampling,
language/layout heuristics, and source hashes, not an assertion that all pages
have been manually checked. Sources stay private and are not uploaded/copied.

## Remaining gates

Latest verification: 21 Rust tests, 5 TypeScript contract tests, 2 WASM tests
(190 shared inputs), 41 offline Python reference tests, TypeScript checking,
schema/type drift checking, and all-target/all-feature Clippy checks passed.
The approved numeric policy accepts all shared inputs; 33 absolute-only float
differences are preserved as diagnostics. The browser WASM module is about
363 KiB before transfer compression.

These passing foundation checks do not resolve the remaining role/runtime and
release requirements below. `artifacts/benchmarks/m0-status.json` records M0 as
not fully complete rather than treating every deferred requirement as passed.

- Verify native/WASM parity using the now-approved memory policy and exact
  discrete outcomes. Shared source alone does not guarantee bitwise float parity.
- The release server RSS, sequential GGUF loading, ONNX Runtime loading, and
  synthetic source-grounded sandbox probes ran. Production generation needs
  broader held-out evaluation and the complete pipeline without bypassing
  shape, execution, or source-faithfulness validation.
- Repository/license metadata has been fetched for candidate models, but full
  provenance, training-data terms, and per-voice license review are incomplete.
- OCR, embeddings, actual TTS synthesis/alignment, and optional GLB baking are
  not runtime-proven merely by API metadata or `ort` loading successfully.
- The Jev probe is synthetic classification only; probability calibration and
  English/Arabic educational benchmarks require labeled evaluation material.
- No large Python training stack was installed. Local M8 SFT/GRPO feasibility
  remains unproven; do not switch to cloud training without approval.
- Physical Android and iOS Safari/PWA acceptance remains a later gate.
- Full 15-duel load tests, database/cache contention, auth, and all game modes
  belong to M1/M5/M7, not the small M0 validation-socket measurement.

## Licensing notes

The official Rust `fsrs` crate declares BSD-3-Clause. axum/tokio declare MIT;
sqlx and ort declare MIT OR Apache-2.0. Candidate model cards mostly declare
Apache-2.0 or MIT, but those claims alone do not resolve all data/voice terms.
PyMuPDF in the offline evaluation tools is AGPL/commercial licensed. No project
distribution license, model redistribution grant, or public lecture-publication
permission has been inferred from the user's authorization to build locally.
