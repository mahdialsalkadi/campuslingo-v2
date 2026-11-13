# M3 implementation evidence

Date: 2026-11-13. **Status: Software Phases A–D implemented and verified; human gates OPEN.**
Authority remains the frozen specification in `/home/mahdi/Downloads/plan/campuslingo_v2_master_plan.md` alongside `/home/mahdi/campuslingo-v2/planning/implementation_decisions.md`, `balance-v1.md`, and `g6b-window-evidence.md`.

## Delivered scope

### Phase A — Shared domain, Stage-3 windows, and PostgreSQL storage (`S-11`, `S-12`, `S-20`, `DEC-002`, `DEC-003`)

- Added `crates/storage/migrations/0004_m3_generation.sql` defining `content_versions`, `learning_unit_bundles` (enforcing all 18 valid `archetype` × `modifier` pairs via `learning_unit_bundles_pair_check`), `node_simulations` (`proof_domain = 'exhaustive_declared_discrete_grid'`, `1..=4096` states, `verified = TRUE`), and `axiom_packs` (`w_base_ms BETWEEN 8000 AND 15000`, `2..=4` prompts), with RLS and publication boundaries verified in PostgreSQL 16 (`m3_generation_tables_enforce_s11_s12_s20_dec002_dec003_and_audio_rls`).
- Extended `crates/domain/src/sandbox.rs` and `crates/domain/src/lib.rs` with `TargetEvaluation`, `StateEvaluation`, `evaluate_state`, and `EvaluationRequest::SandboxState` / `EvaluationResponse::SandboxState` so both native Rust and the browser WASM bundle evaluate concrete discrete slider states through the identical AST evaluator.

### Phase B — Single-lease two-pass `Qwen3.5-4B` generation, `P-03` calibration, and compute tuning

- Updated `.cargo/config.toml` build parallelism to `jobs = 8`.
- Added `default_compute_threads()` in `crates/ingestion/src/llama.rs` (clamped `available_parallelism` `4..=16`, scaling `--threads-batch` and `OMP_NUM_THREADS` up to `16`) and configured `crates/ingestion/src/generator.rs` to run `llama-server` with `--reasoning off`, `--batch-size 2048`, and `--ubatch-size 1024`.
- Switched Pass 1 GBNF `pair_code` from two-token numeric codes to single-token `"A".."R"` labels and anchored `extract_pair_logits` at the `"\"pair_code\": \""` token position so all 18 raw class logits are extracted accurately in Pass 1 while Pass 2 repairs pre-solved or unsafe `SandboxSpec` outputs under the same `ModelLease` (`model_restarts: 0`, `single_lease_reused: true`).
- Implemented per-language `P-03` temperature calibration (`crates/ingestion/src/calibration.rs`) with synthetic ECE `< 0.05` verification; real labeled English/Arabic course holdout calibration remains pending downstream corpus evaluation.

### Phase C — English `Kokoro-82M` `CLAUSE_CONCAT` (`P-07`) and license inventory

- Implemented `crates/ingestion/src/tts.rs` (gated under optional `onnx` feature for runtime ONNX inference) with pure-Rust `KokoroTokenizer`, `VoiceStyleBank`, `split_clauses_for_p07`, `trim_clause_silence`, `assemble_clause_concat` (`24 kHz`, `1440`-sample inter-clause gap, sample-derived `AudioWord` timings + synced `DirectorTrack`), and contained out-of-process `ffmpeg` (`libopus`) + `ffprobe` encoding via `encode_pcm_to_opus_contained`.
- Dynamic ONNX Runtime (`ort =2.0.0-rc.15` with `with_intra_threads(default_compute_threads())`) and `libespeak-ng.so.1.52.0` (`dlopen`/`dlsym` under `ESPEAK_MUTEX`) hold the exclusive `ModelLease` for `Kokoro-82M`.
- Recorded local package versions, SHA-256 digests, and license obligations in [m3-tts-license-inventory.md](m3-tts-license-inventory.md). Arabic TTS (`Language::Arabic`) is explicitly deferred post-M3.

### Phase D — Browser client, `G-2` WASM audit, and `P-06` mobile UI verification

- Added `packages/game-client/` (`src/controller.ts`, `tests/game_client.test.ts`, `public/{index.html,styles.css,app.mjs}`) bundled by `scripts/build_m0.mjs` into gitignored `packages/game-client/public/controller.js` (`4.5kb`).
- Extended `scripts/audit_m3_wasm_and_ui.mjs` to drive real headless `chrome-headless-shell` at `360 CSS px` mobile width (`prefers-reduced-motion: reduce`), verify `quantizeParameterStep` + `evaluateStateViaWasm` + `SourceRegion` hotspots + `CLAUSE_CONCAT` subtitles (`dir="auto"`) + negative `?compact-test=1` failure detection, and compare all `190` `tests/domain_vectors.json` requests inside Chromium against Node WASM parity.

Observed `artifacts/private/m3-g2-p06-audit.json` metrics:

- `g2_wasm_audit`:
  - `wasm_bytes`: `416123` (~`406.4 KiB`), `js_glue_bytes`: `6961` (~`6.8 KiB`), `wasm_opt_present`: `false` (`wasm-opt` binary absent on `PATH`)
  - `chromium_parity_vectors`: `total: 190`, `valid: 183`, `expectedErrors: 7`, `parityMatches: 190`, `elapsedMs: 5.4`
  - `chromium_js_heap_used_bytes`: `1403736`, `chromium_js_heap_total_bytes`: `2883584`
  - `chromium_process_tree_rss_bytes`: `557142016` across `7` processes (`chromium_process_tree_hwm_bytes`: `557142016`)
- `p06_mobile_ui`:
  - `viewport_css_px`: `360`, `doc_scroll_width_css_px`: `360`
  - `interactive_controls_checked`: `9`, `min_observed_width_px`: `48`, `min_observed_height_px`: `48`
  - `negative_compact_test_violations`: `9`, `reduced_motion_transition_duration`: `"0s"`
  - `codec_status`: `".opus supported (clock fallback)"`

## Post-final-edit verification matrix

All checks below ran sequentially after the final code/formatting edit (`cargo fmt --all`) with zero failures:

| Check | Outcome | Observed details |
| --- | --- | --- |
| `cargo fmt --all -- --check && git diff --check` | Pass | Zero formatting or whitespace diffs |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Pass | Finished in `6.86s`, 0 warnings |
| `cargo clippy -p campuslingo-ingestion --all-features --all-targets --locked -- -D warnings` | Pass | Finished in `6.13s`, 0 warnings |
| `cargo test --workspace --all-features --locked` | Pass | `campuslingo-domain`: `3 + 9 + 9 + 6 = 27` passed; `campuslingo-ingestion`: `64` passed / `15` ignored (`--lib`) + `2` passed (`--bin`); `campuslingo-storage`: `1` passed; `campuslingo-server/tests/m1.rs`: `13` ignored; `0` failed |
| `npm run m0:build && npm run contracts:check && npm run typecheck && npm run test:ts && npm run test:wasm && node --test tools/eval/*.test.mjs` | Pass | Release + WASM (`2.92s`), `controller.js` (`4.5kb`), `190` vectors, `contracts:check`, `typecheck`, `7/7` TS tests, `2/2` WASM parity tests, `20/20` `tools/eval` tests |
| `bash scripts/verify_m2_containment.sh` | Pass | `14 passed; 0 failed` (`41.44s`, cgroup `Memory peak: 3.7G (swap: 0B)`), includes live `contained_qwen3_embedding_and_exclusive_model_lease`, `contained_qwen35_4b...`, and `contained_ffmpeg_encodes_and_probes_valid_opus` |
| `bash scripts/run_ingestion_contained.sh node scripts/run_m1_db_tests.mjs` | Pass | `13 passed; 0 failed` PG16 integration tests (`45.81s`, cgroup `Memory peak: 700M (swap: 0B)`) |
| `bash scripts/run_ingestion_contained.sh cargo test -p campuslingo-ingestion --locked --lib contained_qwen35_4b_single_lease_two_pass_and_clean_release -- --ignored --nocapture` | Pass | `1 passed; 0 failed` (`26.50s`; `pass1_ms: 2282`, `pass2_ms: 22327`, `repair_attempts: 1`, `model_restarts: 0`, `states_checked: 11`, `single_lease_reused: true`, `classification: TRADEOFF/BUDGET_CAP`, `peak_bytes=422637568`, cgroup `Memory peak: 685.5M`) |
| `bash scripts/run_ingestion_contained.sh cargo test -p campuslingo-ingestion --features onnx --locked --lib contained_kokoro -- --ignored --nocapture` | Pass | `1 passed; 0 failed` (`5.19s`; `clauses=2`, `duration_ms=7770`, `opus_ms=7777`, `opus_bytes=31474`, `peak_rss_mb=544`, `CPU time consumed: 56.881s`, cgroup `Memory peak: 1003.2M`) |
| `node scripts/audit_m3_wasm_and_ui.mjs` | Pass | Wrote `artifacts/private/m3-g2-p06-audit.json` (`190/190` Chromium parity matches, `360px` mobile gate `48x48 >= 44px`) |

## Open gates & deferred items (require human / later-stage action)

1. **M2 `G-1` Visual Annotation Gate (OPEN):** Human visual review of `artifacts/private/m2-full-deck-1794541547719/review-1794542002239/` remains uncompleted.
2. **M2 License Sign-Off (OPEN):** Human review of [m2-license-signoff-checklist.md](m2-license-signoff-checklist.md) remains open.
3. **M3 TTS Distribution Sign-Off (OPEN):** Human legal review of GPLv3 `libespeak-ng`, `GPL-3.0-only` `ffmpeg`, and `af_heart.bin` voice provenance in [m3-tts-license-inventory.md](m3-tts-license-inventory.md) before distribution.
4. **Arabic TTS (DEFERRED):** `Language::Arabic` audio synthesis (`piper-voices`) is not implemented in M3 and stays deferred post-M3.
5. **Real-Corpus `P-03` Holdout Calibration (OPEN):** Temperature fitting and fail-closed ECE gating are verified on synthetic fixtures; real labeled EN/AR course holdout tuning runs against labeled curriculum data.
