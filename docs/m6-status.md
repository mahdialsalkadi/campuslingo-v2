# Milestone M6 Status — Async Raids & Revenge, Append-Only Economy, 30-Player Weekly Leagues, Shared-HP Bosses & Clan Wars

Status: **COMPLETE AND VERIFIED**

## Implemented Scope

### Phase A — Domain Rules (`crates/domain`, `packages/engine`)
- **Interaction Sources & Contracts (`crates/domain/src/combat.rs`, `crates/domain/src/lib.rs`)**:
  - Extended `InteractionSource` to cover `PRACTICE`, `TUTORIAL_SANDBOX`, `SPECTATOR`, `LIVE_DUEL`, `GHOST_DUEL`, `BOT_DUEL`, `ASYNC_RAID`, `REVENGE_RAID`, `SOLO_BOSS`, `COOP_BOSS`, and `LEAGUE`.
- **Raids, Revenge, Leagues, Bosses & Clan Wars (`crates/domain/src/raids_leagues.rs`)**:
  - Implemented raid targetability & `raid_immune(u, t)` checks (`NEW_NODE_GRACE`, `VERSION_GRACE`, `NEW_PLAYER_BUBBLE`, `POST_RAID_COOLDOWN` 6h, `DORMANT` >= 7d, `RESUME_GRACE` 72h, Tier 0, `INACTIVE`, `INVALID`), private-course opt-in enforcement, allowlisted scanner projections without per-student recall/stability history leakage, and daily user-local 04:00 rollover caps (`MAX_OUTBOUND_RAIDS_PER_DAY = 10`, `MAX_INBOUND_RAIDS_PER_DAY = 3`, `MAX_UNORDERED_PAIR_RAIDS_DAY = 1`, `MAX_DAILY_TROPHY_LOSS_FROM_RAIDS = 45`, `REVENGE_WINDOW_HOURS = 48` with `mode_bps = 15000` in `compute_score_v2`).
  - Implemented `f64` clan war relative stability scoring (`delta_s = max(0.0, S_after - S_before)`, `relative = delta_s / max(S_before, 1.0)`, `raw_pts = min(50, round_ties_even(10.0 * ln(1.0 + relative)))`, `norm_pts = round_ties_even(raw_pts / sqrt(max(3, active_members_snapshot)))`) plus `25` clan points per successful raid (`38` on revenge), gated by `reward_eligible`.
  - Implemented 10-tier, 30-player weekly league rollover rules (top-5 promotion with >= 150-point floor, bottom-5 demotion for full brackets, deterministic tiebreaks, partial-bracket handling, and `DORMANT` promotion suspension until >= 3 active sessions are completed after return).
  - Implemented shared-HP boss strike resolution with overkill clamping.
  - Added 6 domain unit tests in `crates/domain/tests/m6_raids_leagues_bosses_clans.rs` and 2 WASM parity suites in `packages/engine/tests/m6_parity.test.mjs`.

### Phase B — PostgreSQL 16 Schema, Least-Privilege Grants & Storage (`crates/storage`, `scripts/`)
- **Migration (`crates/storage/migrations/0007_m6_raids_economy_leagues_clans.sql`)**:
  - Extended `interaction_source` and append-only economy/cosmetic ledger constraints (`gold`, `trophies`, `loot_chests`, `cosmetics`) with source-specific unique settlement keys and immutability triggers.
  - Added `raid_sessions` (immutable start-of-raid defender sector snapshot, scout + sandbox + 3 verification drills, atomic settlement, cooldown, and single-use revenge window), `league_brackets` and `league_standings` (`(league_scope, week_key, user_id)` uniqueness and idempotent rollover state), `chapter_bosses`, `boss_lobbies`, and `boss_contributions` (atomic shared-HP decrement and overkill/duplicate-claim prevention), plus `clans`, `clan_memberships`, `clan_wars`, and `clan_war_contributions` (with frozen week-start `active_members_snapshot`).
- **Grants & Harness (`scripts/m6_grants.sql`, `scripts/run_m1_db_tests.mjs`)**:
  - Added least-privilege runtime role grants in `scripts/m6_grants.sql` and wired `scripts/run_m1_db_tests.mjs` to apply M6 migrations and grants.
- **Storage API (`crates/storage/src/m6.rs`, `crates/storage/src/lib.rs`)**:
  - Exposed storage queries and transactional settlement methods for raid scanner/start/settle, idempotent `rollover_league_bracket` reconstruction, solo/async/live-lobby boss strikes, and clan war stability/raid contributions (`STABILITY` / `STABILITY_GAIN`).

### Phase C — Server Routes, Live Co-op Boss WebSocket & Game Client (`crates/server`, `packages/game-client`)
- **HTTP & WebSocket Server (`crates/server/src/m6.rs`, `crates/server/src/routes.rs`, `crates/server/src/lib.rs`)**:
  - Added authenticated routes for raid scan/start/settle/revenge, league brackets/rollover, solo/co-op bosses, clan war standings, and economy/cosmetic inventory.
  - Implemented `BossLobbyHub` and `/v1/bosses/ws` WebSocket handler enforcing `A-02` origin, query-spoof rejection, and lobby membership checks.
- **Client Operations UI (`packages/game-client/src/m6.ts`, `packages/game-client/src/shell.ts`, `packages/game-client/public/m4-shell.css`, `packages/game-client/tests/game_client.test.ts`)**:
  - Wired the M6 Operations HUD (`mountM6OperationsPanel`, `createInitialM6State`) covering Raid Scout/Execution, League Leaderboard, Shared-HP Boss Arena, Clan War HUD, and Cosmetics while preserving `>= 44x44` CSS px P-06 touch targets and WebGL triangle budgets.

## Verification Results

All checks below ran after the final edit with zero failures:

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check && git diff --check` | Passed (exit `0`) |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | Passed (exit `0`) |
| `cargo test --workspace --locked` | Passed — `campuslingo-domain`: `46 passed; 0 failed` (`3` lib + `9` adversarial + `9` domain + `2` g4_chains + `5` g6b_combat_and_scoring + `6` m6_raids_leagues_bosses_clans + `6` shields + `6` w_base_v1); `campuslingo-ingestion`: `64 passed; 0 failed; 14 ignored` (lib) + `2 passed; 0 failed` (`src/main.rs`); `campuslingo-storage`: `1 passed; 0 failed`; `crates/server/tests/m1.rs`: `23 ignored` (run via contained PG16 harness below) |
| `bash scripts/run_ingestion_contained.sh node scripts/run_m1_db_tests.mjs` | Passed — `23 passed; 0 failed; 0 ignored; finished in 105.84s` (includes `m6_raids_opt_in_caps_concurrent_cooldown_revenge_and_45_trophy_loss_cap`, `m6_boss_shared_hp_concurrency_overkill_and_chest_idempotency_plus_live_ws`, and `m6_league_30_player_150_floor_tiebreaks_dormancy_and_frozen_clan_war_roster`) |
| `npm run m0:build && npm run contracts:check && npm run typecheck` | Passed (exit `0`) |
| `npm run test:ts` | Passed — `10 passed; 0 failed` |
| `npm run test:wasm` | Passed — `9 passed; 0 failed` |
| `node --test tools/eval/*.test.mjs` | Passed — `20 passed; 0 failed` |
| `node scripts/audit_m3_wasm_and_ui.mjs` | Passed — `parityMatches: 190` (`total: 190`), P-06 mobile viewport `360px` (`doc_scroll_width_css_px: 360`, `58` controls checked, min size `44x44` px) |
| `node scripts/audit_m4_game_shell.mjs` | Passed — `rendererBackend: "webgl"`, `sceneTriangles: 1762`, `budgetOk: true`, `controlCount: 52`, `violations: []` across `mobile-360`, `tablet-768`, and `desktop-1280` |
