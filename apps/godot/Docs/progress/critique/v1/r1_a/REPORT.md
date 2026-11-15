# Critique Round 1 — Lens A: QA & Server Formula Parity Critic

**Score:** 7.9 / 10 (NEEDS FIXES BEFORE M4 SIGN-OFF)
**Artifacts Inspected:** `scripts/verify_e2e_headless.gd` (`7/9` initial run), `scripts/modes/pvp_raid_controller.gd`, `scripts/troops/troop_agent_base.gd`, `data/building_stats.json`, `crates/domain/src/raids_leagues.rs`

## Verified Strengths
- `M1` & `M2` parity suites (`7/7` M1_COMBAT_LAB, `10/10` movement parity within `<= 1.91%`, `5/5` M2 headless) and live backend (`7/7` `verify_m3_live.gd`) pass against `1.5m` cells, `[4..35]` buildable grid, `-35%` brownout cadence, `0/75/100/125/200%` Boss strike scaling, and atomic layout persistence.

## Findings
- **[BLOCKER] A-1 (`ThesisBreaker._next_steer_dir` runtime Dictionary property error):** In initial E2E run, accessing `wall_id.x` when `first_wall_on_path` returns a building `Dictionary` crashed pathing with stderr spam and inflated 40-troop `p95_step_ms` to `48.144ms`.
  - *Fix:* Normalize `Vector2i`/`Dictionary`/`Node` wall targets safely via `CombatParityMath.cell_of(wall_id)` and avoid per-building `get_sim_state()` calls inside non-matching loops.
- **[MAJOR] A-2 (Script modularity `> 150L` & persistent `user://` upgrade test contamination):** `pvp_raid_controller.gd` temporarily reached `156L` (`> 150L`), and `verify_e2e_headless.gd` failed `upgrade_persisted` when `user://village_layout_*.json` carried a tier-3 starter building from an earlier run.
  - *Fix:* Move `_sync_solver_score` into `CombatParityMath` (`<= 150L` everywhere) and clear `user://village_layout_*.json` before deterministic E2E runs.
- **[MAJOR] A-3 (Missing real OpenGL `gl_compatibility` 40-troop benchmark telemetry):** `--headless` uses Dummy rasterizer (`draw_calls: 0`), lacking real `gl_compatibility` draw-call and frame-step measurements.
  - *Fix:* Add `scripts/verify_gl_bench_m4.gd` under `xvfb-run` `gl_compatibility` capturing `m4_bench.png` and verifying `p95_step_ms < 16.67ms`.
