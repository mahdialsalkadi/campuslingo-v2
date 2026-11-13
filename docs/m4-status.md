# M4 Implementation Evidence

Date: 2026-11-13. **Status: M4 Software & Browser Audits Implemented and Verified.**

Authority remains `planning/campuslingo_v2_master_plan.md` (`CL2-MASTER-UNIFIED-v1.8.1`) alongside `planning/implementation_decisions.md`.

## Delivered Scope

### 1. Gate `G-4` Closed — Sequential `fsrs-rs` Native vs. WASM Parity & Custom `S-19` Weights

- Extended `crates/domain/src/memory.rs` (`ReviewInput.parameters: Option<[f32; 21]>`, `validate_fsrs6_parameters`, `fsrs6_with_parameters`) to validate and evaluate per-user custom `S-19` 21-weight FSRS-6 parameter vectors alongside the pinned `FSRS6_DEFAULT_PARAMETERS`.
- Added native integration test [crates/domain/tests/g4_chains.rs](file:///home/mahdi/campuslingo-v2/crates/domain/tests/g4_chains.rs) and Node WASM parity harness [packages/engine/tests/g4_chains.test.mjs](file:///home/mahdi/campuslingo-v2/packages/engine/tests/g4_chains.test.mjs), emitting reproducible evidence to [artifacts/benchmarks/g4-sequential-chains.json](file:///home/mahdi/campuslingo-v2/artifacts/benchmarks/g4-sequential-chains.json).
- Observed metrics from [artifacts/benchmarks/g4-sequential-chains.json](file:///home/mahdi/campuslingo-v2/artifacts/benchmarks/g4-sequential-chains.json):
  - `fsrs_crate_version`: `7.0.0` (`FSRS-6`), `wasm_sha256`: `a5348e2477f88ec4ce99637406ec36712606d0745dcff7f3d596479f68a87e63`.
  - `chains`: `4` (`steady_progression_default`, `lapses_and_recovery_default`, `high_retention_mixed_intervals`, `custom_s19_21_weights`), `steps_per_chain`: `60`, `reviews_per_mode`: `240` (self-evolving and server-canonical).
  - `max_abs_error`: `0.0003000000001520675`, `max_continuous_shield_abs_error`: `1.0000000005838672e-7`, `self_evolving_scheduled_days_deltas`: `0`, `custom_s19_21_weights_verified`: `true`.

### 2. Shield & Protection Engine (`crates/domain`, `crates/storage`, `crates/server`)

- **Shared domain (`crates/domain/src/shields.rs`):**
  - Implemented `ticks_1e4` (round-ties-to-even `10^-4` integer quantization) and `ticks_1e4_checked`.
  - Implemented `next_brownout_state` hysteresis (`E_INITIAL = 7000`, `E_ENTER_BROWNOUT = 6950`, `E_EXIT_BROWNOUT = 7050`) and `compute_tier` (`INACTIVE`/`None` and Tiers `0..=3` using `S_PROTO_TICKS = 50_000`, `S_APEX_TICKS = 210_000`, `E_APEX = 8500`).
  - Implemented `validate_prerequisite_dag` and `evaluate_course_shields` with `E_u(t)` DAG propagation (`MAX_DAG_DEPTH = 16`, `MAX_DAG_CLOSURE = 256`, self-loop/cycle rejection, and brownout/exemption handling).
  - Implemented `split_protection_predicates` separating `raid_immune` from `propagation_exempt`, strict `AND` `is_new_player_bubble` (`NEW_PLAYER_BUBBLE_HOURS = 48`, `NEW_PLAYER_BUBBLE_TRANSITION_LIMIT = 20`), and non-negative `compute_version_grace` (`VERSION_NOTICE_DAYS = 7`, `VERSION_ABSOLUTE_CAP_DAYS = 14`, `VERSION_GRACE_BASE_HOURS = 72`, `VERSION_GRACE_PER_CARD_HOURS = 12`, `VERSION_GRACE_MAX_HOURS = 168`, `VERSION_GRACE_ROLLING_WINDOW_DAYS = 30`, `VERSION_GRACE_ROLLING_CAP_HOURS = 336`).
  - Added WASM parity test [packages/engine/tests/shields_parity.test.mjs](file:///home/mahdi/campuslingo-v2/packages/engine/tests/shields_parity.test.mjs) and domain unit tests [crates/domain/tests/shields.rs](file:///home/mahdi/campuslingo-v2/crates/domain/tests/shields.rs).
- **PostgreSQL 16 schema, role isolation & worker persistence:**
  - Added [crates/storage/migrations/0005_m4_shields_and_quarantine.sql](file:///home/mahdi/campuslingo-v2/crates/storage/migrations/0005_m4_shields_and_quarantine.sql) and [scripts/m4_grants.sql](file:///home/mahdi/campuslingo-v2/scripts/m4_grants.sql), defining `content_version_cards`, `node_prerequisites`, `user_fsrs_parameters` (`REAL[]` `S-19` weights), `user_protection_state` (`DEC-005`), `user_dormancy_periods`, `user_node_protection`, `user_card_version_grace`, `user_node_grace_ledger`, `user_course_version_seen`, `user_node_version_engagement`, `user_node_shield_state` (`DEC-004`), `shield_recompute_queue`, and `node_reports`.
  - Preserved M1 role isolation via `SECURITY DEFINER` helpers (`user_account_created_at(UUID)`, `actor_course_fsrs_card_states(UUID, UUID)`, `is_report_cluster_eligible(UUID, UUID, TEXT)`) using `session_user` without granting `campuslingo_app` direct `SELECT` on `users` or `fsrs_cards`.
  - Implemented [crates/storage/src/shields.rs](file:///home/mahdi/campuslingo-v2/crates/storage/src/shields.rs) (`VersionGraceTiming`, `evaluate_and_grant_version_grace`, `compute_and_persist_shields_tx`, stale 15-minute `STALE_RECOMPUTE_SECS` async recompute queue, `submit_node_report` 3-report version-scoped `S-13` quarantine preserving `fsrs_cards` and `review_logs`).
  - Wired routes in [crates/server/src/routes.rs](file:///home/mahdi/campuslingo-v2/crates/server/src/routes.rs): `GET /v1/courses/{course_id}/shields`, `POST /v1/courses/{course_id}/shields/recompute`, `GET /v1/courses/{course_id}/fortress`, and `POST /v1/courses/{course_id}/nodes/{node_id}/report`.

### 3. 3D Strategy Shell & 6 Archetypes × 3 Modifiers (`packages/game-client`)

- [packages/game-client/src/catalog.ts](file:///home/mahdi/campuslingo-v2/packages/game-client/src/catalog.ts): Defined all 4 `WORLD_THEMES` (`BIO_SYSTEM`, `CYBER_GRID`, `MACRO_POLIS`, `ARCANE_ARCHIVE`) in `THEME_PALETTES`, all 6 archetypes × 3 modifiers (`HOLOGRAM_CATALOG` = 18 presets verified through WASM), `QUICK_BLITZ_CHAIN_SEQUENCE`, `DEFAULT_FORTRESS_STATE`, and bilingual `en`/`ar` `AudioManifest` + `DirectorTrack` cues.
- [packages/game-client/src/scene3d.ts](file:///home/mahdi/campuslingo-v2/packages/game-client/src/scene3d.ts): Implemented Three.js `Fortress3DScene` with WebGL2 rendering, tiered bastion towers (`0..=3` + `INACTIVE`), DAG bridges, protection domes, raycast selection, 6 procedural 3D hologram assemblies, `DirectorTrack` camera framing, and `prefers-reduced-motion` support.
- [packages/game-client/src/validator.ts](file:///home/mahdi/campuslingo-v2/packages/game-client/src/validator.ts) & [packages/game-client/src/sandbox2d.ts](file:///home/mahdi/campuslingo-v2/packages/game-client/src/sandbox2d.ts): Implemented `validateAssetBudget` (`A-04` limits: `maxSceneTriangles: 15_000`, `maxAssetTriangles: 8_000`, `maxInstancedUnitTriangles: 2_500`, `maxTextureDimension: 2048`, `maxBufferBytes: 16 * 1024 * 1024`, local/inline URI & script guards), `FpsHysteresisMonitor` (`O-05` `< 28` FPS for `3000ms` auto-fallback), and `ConceptSandbox2D` SVG/DOM fallback.
- [packages/game-client/src/shell.ts](file:///home/mahdi/campuslingo-v2/packages/game-client/src/shell.ts), [packages/game-client/public/m4-shell.css](file:///home/mahdi/campuslingo-v2/packages/game-client/public/m4-shell.css), [packages/game-client/public/index.html](file:///home/mahdi/campuslingo-v2/packages/game-client/public/index.html), and [scripts/build_m0.mjs](file:///home/mahdi/campuslingo-v2/scripts/build_m0.mjs): Mounted `#m4-game-shell`, built `packages/game-client/public/shell.js` (`599.8kb` minified, gitignored), wired `.opus` playback/Mute (Zero Advantage) and bilingual `dir="auto"` captions, `S-13` report counter, manual 2D/3D recovery toggle, and `<28 FPS` stress toggle.

## Post-Final-Edit Verification Matrix

All checks below ran after all code edits (`shell.ts`, `audit_m4_game_shell.mjs`; only `.gitignore` was updated afterward prior to the Cargo/Node/DB verification runs):

| Check | Outcome | Observed Details |
| --- | --- | --- |
| `cargo fmt --all -- --check && git diff --check` | Pass | Exited `0` |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | Pass | Exited `0` |
| `cargo test --workspace --locked` | Pass | `campuslingo-domain`: `3 + 9 + 9 + 2 + 6 + 6 = 35` passed; `campuslingo-ingestion`: `64` passed / `14` ignored (`--lib`) + `2` passed (`--bin`); `campuslingo-storage`: `1` passed; `m1`: `16` ignored; `0` failed |
| `npm run m0:build` | Pass | Built release + WASM, `controller.js` (`4.5kb`), `shell.js` (`599.8kb`), exported schemas/types and `190` vectors |
| `npm run contracts:check && npm run typecheck && npm run test:ts && npm run test:wasm && node --test tools/eval/*.test.mjs` | Pass | Contracts & types verified; `8/8` TS tests passed; `4/4` WASM tests passed; `20/20` `tools/eval` tests passed |
| `CAMPUSLINGO_TEST_FILTER=m4_ npm run m1:verify` | Pass | `3 passed; 0 failed` (`m4_shields_dag_brownout_tier_and_stale_recompute_enforced_in_postgres`, `m4_dormancy_bubble_non_negative_version_grace_and_dec005_rls`, `m4_s13_content_quarantine_and_anti_brigading`) |
| `bash scripts/run_ingestion_contained.sh node scripts/run_m1_db_tests.mjs` | Pass | `16 passed; 0 failed` across M1 (`6`), M2 (`6`), M3 (`1`), and M4 (`3`) PG16 integration tests (`59.18s`, cgroup `Memory peak: 378M`) |
| `npm run m1:verify` (uncontained host env) | Partial (`14/16`) | `14 passed; 2 failed` (only the 2 contained M2 Poppler/worker tests failed uncontained: `m2_worker_persists_actual_pdf_pages_crops_and_chunks`, `m2_worker_persists_sequential_chunk_embeddings_over_loopback`; both pass under `run_ingestion_contained.sh`) |
| `node scripts/audit_m3_wasm_and_ui.mjs` | Pass | `190/190` Chromium parity (`4.2ms`), `42` `P-06` controls `>= 44x44 CSS px` (`360px` width, zero overflow) |
| `node scripts/audit_m4_game_shell.mjs` | Pass | Wrote [artifacts/private/m4-ui-audit.json](file:///home/mahdi/campuslingo-v2/artifacts/private/m4-ui-audit.json): `rendererBackend: "webgl"`, `sceneTriangles: 1762`, `unitTriangleMax: 384`, `budgetOk: true`, `fps: 100.4`, `themesVisitedCount: 4`, `templatesVerifiedCount: 18`, `blitzActive: true`, `ar` `dir="auto"` + `close_up (input)` verified, `AUTO_FALLBACK_2D` + `3D_ACTIVE` recovery verified, `36` controls with `0` `P-06` violations and zero horizontal overflow across `360px`/`768px`/`1280px` |
