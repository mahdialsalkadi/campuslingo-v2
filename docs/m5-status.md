# M5 Implementation Evidence

Date: 2026-11-13. **Status: M5 Authoritative 90s Duels, `rating_map_version = 2`, `score_v2`, Ghosts/Bots & Event-Sourced Crash Recovery Implemented and Verified.**

Authority remains `planning/campuslingo_v2_master_plan.md` (`CL2-MASTER-UNIFIED-v1.8.1`) alongside `planning/implementation_decisions.md`.

## Delivered Scope

### 1. Phase A — `rating_map_version = 2`, `score_v2`, Windows (`DEC-007`) & 2-Lane Combat (`crates/domain`, `packages/engine`)

- **Windows & Quick-Blitz (`crates/domain/src/windows.rs`):**
  - Added `w_base_for_live_duel_ms`, `w_personal_eff_ms`, `w_personal_cut_from_base_ms`, `w_quick_blitz_chain_eff_ms` (`("AXIOM", "QUICK_BLITZ_CHAIN")` un-truncated `4.0s`), `w_quick_blitz_chain_cut_ms`, `w_sudden_death_cut_ms`, and `w_round_cut_and_m1_ms` (`DEC-007`).
- **Authoritative Rating, Scoring & 90s Arena (`crates/domain/src/combat.rs`, `crates/domain/src/lib.rs`):**
  - Implemented `rating_map_version = 2` (`D-02-A` ordered precedence: `VOID_DELIVERY | VOID_TRANSPORT_LOSS | TOO_FAST (<150ms)` capped at 2 voids before escalating to `Again = 1`; `Again = 1` on `TIMEOUT | ABANDONED_AFTER_REVEAL | !is_correct`; `Hard = 2` on `hint_used | server_sandbox_submits > 1 | partial_credit | L_net > W_personal_eff`; `Easy = 4` on first-submit, no-hint, full credit with `L_net <= 0.50 * W_personal_eff`; `Good = 3`), `evaluate_eligibility`, `round_ties_even_points`, `compute_score_v2` (exact `i128` basis-point scoring clamped at `3.0x` prior to ties-even rounding), `round_comp_bps`, and `compute_duel_contribution`.
  - Implemented `DuelArenaState` 90-second 2-lane combat state machine (continuous energy regeneration, `2x` energy in final 30s, tournament-normalized ranked combat levels, `2.0x` Overcharge/shield-pierce, Counter-Strike window, `VOID_TRANSPORT_LOSS` combat fizzle capped at 2 voids per match), `adapt_ghost_action` (`F-03` state-aware ghost replay adaptation), and `decide_bot_action`.
  - Exposed `RatingMapV2`, `ScoreV2`, and `DuelStrike` via `evaluate`, `evaluate_json`, and `wasm_evaluate_json` with native tests in [crates/domain/tests/g6b_combat_and_scoring.rs](file:///home/mahdi/campuslingo-v2/crates/domain/tests/g6b_combat_and_scoring.rs) and WASM parity tests in [packages/engine/tests/m5_parity.test.mjs](file:///home/mahdi/campuslingo-v2/packages/engine/tests/m5_parity.test.mjs).

### 2. Phase B — Persistence, Exposure Uniqueness, Daily Caps & `R-07` Crash Recovery (`crates/storage`)

- **Schema & Least-Privilege Grants ([crates/storage/migrations/0006_m5_duels_and_replays.sql](file:///home/mahdi/campuslingo-v2/crates/storage/migrations/0006_m5_duels_and_replays.sql), [scripts/m5_grants.sql](file:///home/mahdi/campuslingo-v2/scripts/m5_grants.sql)):**
  - Added `matches`, `match_snapshots`, `match_replay_events`, `match_settlements`, and `user_combat_telemetry` with composite tenant/course foreign keys, ghost retrieval indexing, and explicit winner/draw/void constraints.
  - Extended `review_logs` with `rating BETWEEN 1 AND 4`, `(user_id, challenge_id, scope_kind, scope_id)` uniqueness across `MATCH`/`STUDY_SESSION`, `user_local_quota_date` (`04:00` local rollover), and `enforce_review_exposure_and_caps()` enforcing the 30-minute `SAME_DAY_MIN_GAP` short-term step rule and `3/card` & `60/course` daily reward caps.
- **Duel Storage & `R-07` Recovery ([crates/storage/src/duels.rs](file:///home/mahdi/campuslingo-v2/crates/storage/src/duels.rs), [crates/storage/src/lib.rs](file:///home/mahdi/campuslingo-v2/crates/storage/src/lib.rs)):**
  - Implemented match start, 10-second snapshot persistence, bounded replay event append, atomic settlement, duel review recording, permissioned ghost replay lookup, and `recover_or_void_interrupted_matches` (replaying committed events from the latest 10-second snapshot inside the 15-second grace window while freezing the outage clock, or idempotently settling `VOID_SERVER_INTERRUPTION` with entry refunds past grace while preserving `review_logs`).

### 3. Phase C — WebSocket Server, Matchmaking & Client UI (`crates/server`, `packages/game-client`)

- **WebSocket & HTTP Handlers ([crates/server/src/duels.rs](file:///home/mahdi/campuslingo-v2/crates/server/src/duels.rs), [crates/server/src/routes.rs](file:///home/mahdi/campuslingo-v2/crates/server/src/routes.rs), [crates/server/src/lib.rs](file:///home/mahdi/campuslingo-v2/crates/server/src/lib.rs)):**
  - Implemented `DuelHub`, authenticated `/v1/duels/ws` upgrade (`A-02` session-cookie + Origin + query-spoof rejection, `#[serde(deny_unknown_fields)]` anti-spoofing, `F-04` actor projection filtering), `O-03` WebSocket budgets (`5s` heartbeat, `10s` client timeout, `15s` reconnect grace, `SNAPSHOT_RESYNC`, `32`-frame outbound queue cap, `4096`-byte payload limit, `16` cmds/`2s` rate limit), and `/v1/courses/{course_id}/duels/matchmake` (`4s` wait budget falling back to `GHOST` or `BOT`).
- **Live 90s Duel Client UI ([packages/game-client/src/duel.ts](file:///home/mahdi/campuslingo-v2/packages/game-client/src/duel.ts), [packages/game-client/src/shell.ts](file:///home/mahdi/campuslingo-v2/packages/game-client/src/shell.ts), [packages/game-client/public/m4-shell.css](file:///home/mahdi/campuslingo-v2/packages/game-client/public/m4-shell.css), [packages/game-client/tests/game_client.test.ts](file:///home/mahdi/campuslingo-v2/packages/game-client/tests/game_client.test.ts)):**
  - Wired the live 90-second 2-lane duel view and `<= 2.5s` non-blocking `HudToastController` into the game shell with `>= 44x44 CSS px` controls across mobile, tablet, and desktop viewports.
- **Two-Client Adversarial & DB Integration Suite ([crates/server/tests/m1.rs](file:///home/mahdi/campuslingo-v2/crates/server/tests/m1.rs)):**
  - Added self-contained RFC 6455 `RawWsClient` and 4 M5 integration tests covering `S-08` exposure uniqueness/caps, two-client WebSocket spoofing/fizzle/resync/`A-02`/`O-03`/`F-04` checks, `R-07` crash recovery + idempotent voiding, and 4s `GHOST`/`BOT` fallback + 15-duel load foundation.

## Verification Matrix

| Check | Outcome | Observed Details |
| --- | --- | --- |
| `cargo fmt --all -- --check && git diff --check && cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | Pass | Exited `0` after final edit |
| `cargo test --workspace --locked` | Pass (non-PG suite; ran before final test-only edit to `crates/server/tests/m1.rs`) | `campuslingo-domain`: `40/40` (`3` unit + `9` adversarial + `9` domain + `2` g4_chains + `5` g6b_combat_and_scoring + `6` shields + `6` w_base_v1); `campuslingo-ingestion`: `64` passed / `14` ignored (`--lib`) + `2` passed (`--bin`); `campuslingo-storage`: `1` passed; `m1`: `20` ignored; `0` failed |
| `npm run m0:build && npm run contracts:check && npm run typecheck && npm run test:ts && npm run test:wasm && node --test tools/eval/*.test.mjs` | Pass (ran before final test-only edit to `crates/server/tests/m1.rs`) | Schemas & types verified (`190` synthetic vectors); `9/9` TS tests passed; `7/7` WASM parity tests passed; `20/20` `tools/eval` tests passed |
| `node scripts/audit_m3_wasm_and_ui.mjs && node scripts/audit_m4_game_shell.mjs` | Pass (ran before final test-only edit to `crates/server/tests/m1.rs`) | `190/190` Chromium parity (`4.4ms`); `51` `P-06` controls checked on M3 audit (`min_observed_width_px: 44`, `min_observed_height_px: 44`, `360px` width); M4 shell audit: `rendererBackend: "webgl"`, `sceneTriangles: 1762`, `budgetOk: true`, `45` controls with `0` violations across `360px`/`768px`/`1280px` |
| `bash scripts/run_ingestion_contained.sh node scripts/run_m1_db_tests.mjs` | Pass (ran after final edit) | `20 passed; 0 failed` (`68.89s`, cgroup `Memory peak: 627.1M`), including all 4 `m5_*` PG16 & live RFC 6455 WebSocket integration tests |
