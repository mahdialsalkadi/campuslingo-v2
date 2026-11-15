# Chronological Development Log (Append-Only)

Every agent workstream appends a dated entry below with what was built, what captures were recorded, and what tests passed.

### 2026-11-14 • Planning Phase & Kit Adaptation (`M0 READY`)
- Adapted the architecture, 7 isolated tracks, 40 `SEC-*` micro-sections (`D016`), and Gate `M0..M5` roadmap from `Autonomous-Godot-Agent-Kit.zip` into **CampusLingo: Village Clash** without altering existing server contracts (`crates/domain/src/raids_leagues.rs`, `shields.rs`).
- Authored governance & contracts: `CLAUDE.md`, `AGENTS.md`, `TASK.md`, `CONTINUE.md`, `Docs/DECISIONS.md`, `Docs/DESIGN_V2.md`, `Docs/VILLAGE_AND_RAID_SPEC.md`, `Docs/TECH_CONTRACT.md`, `Docs/SECTIONS_REGISTRY.md`, `Docs/STATE.md`.

### 2026-11-14 • Gate M0 Execution (`M0_PASSED -> M1_READY`)
- Implemented core autoloads & tooling: `tools/godot.py` (Linux `4.3.stable`, `MAX_SLOTS=1`, `check`, `sections`, `capture`, `movie`), `RunGovernor` (`SEC-C01`), `DevTools` (`SEC-C02`), `SectionRegistry` (`SEC-C03`), `Juice` (`SEC-FX01`), and `SectionDebugInspector` (`SEC-UI05`).
- Verified `StoryCam_A..D` capture (`Docs/progress/m0/m0_storycam_a.png`, `m0_inspector_sec_b01.png`), `m0_smoke.mp4`, and `npm --prefix /home/mahdi/campuslingo-v2 run godot:test` (`[E2E PASS]`).

### 2026-11-15 • Gate M1 + Open-Source Assets (`D017`) + Full REA Parity (`M1_PASSED -> M2_READY`)
- Added `D017` and downloaded **28 free open-source CC0/MIT assets** (16 Quaternius/Kenney `.glb` models, 5 `.png` texture colormaps, 7 Kenney `.ogg` audio cues) verified via `godot --import` and logged in `Docs/LICENSES.md` + `data/asset_register.csv`.
- Implemented all **14 M1 Micro-Sections** (`<= 150` lines each): `SEC-G01..G03`, `SEC-D01..D04`, `SEC-SIM01..SIM02`, `SEC-T01..T05` + `CombatLab.tscn`.
- Installed `.agent/skills/` & `skills/`, executed `private_research/scripts/derive.py` (inspected `6` compiled symbols via `nm -C` in `target/debug/campuslingo-server` + derived 60 Hz / 20 Hz continuous kinematic/ballistic integration corrections), wrote `Docs/MOVEMENT_REFERENCE.md`, created `scenes/combat/village_profile.tres`, and verified `tests/run_village_tests.py` + `tests/run_movement_tests.py` (`100%` pass within `±3.0%`).

### 2026-11-15 • Clean-Room CoC Visual & PBR Restart — Gate M0 (`M0_PASSED -> M1_READY`)
- **Clean-Room Removal & Visual Reset (`D018..D023`):** Purged rejected cartoon outline/toon shaders, KayKit toy models, checkerboard terrain, and over-limit monolith scripts; authored `Docs/COC_VISUAL_PILLAR_SPEC.md`, updated `Docs/DECISIONS.md` (`D018..D023`), `Docs/DESIGN_V2.md`, `Docs/TECH_CONTRACT.md`, `Docs/SECTIONS_REGISTRY.md`, and `tools/workflows/CRITIC_PROMPTS.md`.
- **$0.00 PBR Textures & Blender `5.3.0` Pipeline:** Installed user-space Blender `5.3.0` (`sha256` verified), fetched `30` PolyHaven `1K` `CC0` maps + procedural `water_normal.png` + `OFL-1.1` Noto Arabic/Sans fonts, and authored `tools/blender_pbr_kit.py` + `tools/blender_build_m0_diorama.py` generating `9` beveled, Smart-UV'd, BVH vertex-AO-baked (`COLOR_0`), triangulated (`TANGENT`) miniature architectural prop families across `LOD0/LOD1/LOD2` (`27` `.glb` files; `69` total audited rows in `data/asset_register.csv`).
- **M0 Shader Suite, `SEC-FX02` & Island Diorama (`<= 150L`):** Authored `coc_miniature_pbr.gdshader`, `coc_ground_shadow.gdshader`, `coc_island_terrain.gdshader`, `coc_water_moat.gdshader`, `coc_edit_grid.gdshader`, `coc_additive_halo.gdshader`, `PbrMiniatureMaterial` (`147L`), `M0` Showcase (`148L`), cleaned `GridMath` (`143L`), and zoom-synced `CameraPanZoom` (`131L`).
- **Visual & Technical Gate (`M0_PASSED`):** `tools/godot.py check` passed (`0` errors), `tools/godot.py sections` reported `40` total (`10` live, `30` planned), `verify_e2e_headless.gd` printed `M0_E2E_VERIFICATION_PASSED`, and visual inspection of `StoryCam_A..D`, `SEC-FX02` inspector, `m0_edit_grid_on.png`, and `m0_showcase.mp4` (`60` frames) passed after iterative terrain saturation, contact-shadow tightening, crystal halo de-clipping, and 4-side coastal cliff alignment fixes.

## 2026-11-15 — Gate `M1` Passed (`M1_PASSED -> M2_READY`)
- Generated `93` stylized miniature PBR `.glb` models in headless **Blender `5.3.0`** across Defenses (`8` families × 3 LODs), Buildings (`9` families × 3 LODs), Troops (`6` families × 3 LODs), and Projectiles (`3` single-LOD meshes); verified `COLOR_0` + `TANGENT` + `NORMAL` + `TEXCOORD_0` and SHA-256 registration (`162` total entries in `data/asset_register.csv`).
- Implemented & verified `11` new M1 sections (`21/40 LIVE`): `SEC-G02` (`GridBoardRenderer`), `SEC-FX03` (`CombatBuildParticles`), `SEC-D01..D04` (`QuillArcherSpire`, `InkCannon`, `RuneMortar`, `HiddenProctorTesla`), `SEC-T01..T05` (`SquireFootman`, `LexiArcher`, `ThesisBreaker`, `FormulaGiant`, `GlyphDrake`) plus `Music/SFX/UI` audio pools in `SEC-FX01`.
- Executed `tests/run_village_tests.py` (`7/7 PASS`), `tests/run_movement_tests.py` (`10/10 PASS`), `tools/godot.py check` (`0` errors), and visually inspected `Docs/progress/m1/m1_storycam_{a,b,c,d}.png`, `m1_inspector.png`, and `m1_combat.mp4`.

## Milestone M2 — Village Gameplay Loop, Modes, Bilingual RTL & PBR Polish (M2_PASSED)
- Wired `scenes/MainGame.tscn` (`scripts/main_village.gd` + `scripts/village/village_factory.gd` + `scripts/modes/mode_modal_helper.gd` + `scripts/modes/combat_parity_math.gd`) connecting all `40 / 40` `SEC-*` scripts cleanly under `150` lines each.
- Audited & fixed contact shadow UV edge clipping (`coc_ground_shadow.gdshader` `edge_fade`), RTL resource counters/drawers localization (`ResourceTopBar`, `ArmyTrainingDrawer`, `StudyFlashcardDrawer`), HUD drawer spacing above `Sections (F2)`, and Tier-1 brownout visibility on starter southwest `InkCannon`.
- Verified `tools/godot.py sections` (`40/40 LIVE`), `tools/godot.py check` (`0` errors), `verify_m1_headless` (`7/7`), `run_movement_tests` (`10/10`), `verify_m2_headless` (`5/5`), and inspected `Docs/progress/m2/m2_{storycam_a,storycam_b,storycam_c,storycam_d,edit_grid,study_rtl,raid}.png`.

## Milestone M3 — Live Server REST/WS End-to-End Integration (M3_PASSED)
- Shared `ApiHttpClient._shared` across `AuthCoursesAPI`, `VillageStudyAPI`, and `RaidsDuelsAPI` (`<= 150L`) with deferred-entry await guard, cookie jar forwarding, and safe empty/non-JSON status handling.
- Locked seeded fortress course selection (`00000000-0000-4000-8000-000000000201`), omitted null `revenge_source_raid_id` on `/raids/start`, sequenced async `--live-connect` prior to CLI `--mode=*`, and tightened `ModeModalDrawer` vertical height.
- Verified `verify_m3_live.gd` (`7/7` against running `campuslingo-server` + ephemeral PostgreSQL) including session logout/re-login economy persistence, and visually inspected `Docs/progress/m3/m3_live_{hud,boss,duel}.png`.

## 2026-11-15 — Gate M4 (E2E Benchmark, Playthrough Video & 4-Lens Critiques)
- **Combat & Pathing Fixes:**
  - Moved `_sync_solver_score` into `CombatParityMath` (`scripts/modes/combat_parity_math.gd`, `110L`) and added `CombatParityMath.cell_of(v)` so `TroopAgentBase._next_steer_dir` (`scripts/troops/troop_agent_base.gd`, `150L`) safely handles `Dictionary`, `Vector2i`, and `Node` wall targets, fixing `ThesisBreaker` pathing errors.
  - Staggered A* repath timers (`0.45 + float(get_index() % 12) * 0.05`) and skipped `FortressWallTile` prior to `get_sim_state()` allocation inside `CombatParityMath.sync_solver_score`, bringing 40-troop `p95_step_ms` down from `48.144ms` -> `12.095ms` (headless) and `14.756ms` (OpenGL `gl_compatibility` via `xvfb-run`).
  - Reset ephemeral saved layouts at start of `verify_e2e_headless.gd` and `verify_gl_bench_m4.gd` so deterministic starter-to-tier upgrades execute every run (`9/9 passed`).
- **4-Lens Critique-Fix Cycle (`Docs/progress/critique/v1/`):**
  - Round 1 identified `[MAJOR]` defects in FocusExtractor neon-magenta glass (`A-1`), `--demo-raid` south wave framing (`A-2`), missing in-battle damaged building HP bars (`B-1`), missing post-raid settlement reward toast (`C-1`), and missing real OpenGL `gl_compatibility` 40-troop telemetry (`D-1`/`D-2`).
  - Implemented fixes across `scripts/buildings/focus_extractor_tank.gd` (`49L`, deep amethyst glass override), `scripts/modes/pvp_raid_controller.gd` (`149L`, `spawn_demo_wave` framing & `clear_damage_bars`), `scripts/modes/combat_parity_math.gd` (`RaidHpBar` unshaded billboard quad), `scripts/main_village.gd` (`145L`, localized post-raid `RAID +<G>G +<T>T` settlement toast), `scripts/village/village_factory.gd` (`150L`, `--demo-raid` & `Juice` binding), and `Docs/.gdignore`.
  - Round 2 verified all fixes via `m4_bench.png`, `m4_raid_action.png`, and `m4_playthrough.mp4` (`sha256: beaa61742e08258bf5580ae9c9208de240ebb20449c246cc3b35403000ed3262`) (`r2_a: 9.0`, `r2_b: 8.8`, `r2_c: 8.9`, `r2_d: 8.8`).

## 2026-11-15 — Gate M5 (Linux x86_64 Release Export & Isolated Live Smoke Boot)
- Installed Godot `4.3.stable` Linux export templates (`~/.local/share/godot/export_templates/4.3.stable/`).
- Audited `data/asset_register.csv` (`162` rows, `0` non-permissive assets) and bundled `builds/LICENSES.md`, `builds/README.md`, `builds/SHA256SUMS.txt`, and `builds/RELEASE_MANIFEST.json`.
- Created `export_presets.cfg` (`Linux/X11`, `embed_pck=true`) and exported release binary `builds/CampusLingoVillageClash.x86_64` (`112,626,048` bytes, `sha256: 4eb06c1a57130b24377f1f56b7fea01b9a0f66c23a08e940e7cbd1430a9fa9f4`).
- Smoke-tested standalone binary under fresh isolated `HOME`/`XDG_DATA_HOME` against live backend (`http://127.0.0.1:18080`) with `xvfb-run` `gl_compatibility`: exited `0` (`STANDALONE_EXIT=0`) and captured `Docs/progress/m5/m5_standalone_smoke.png` showing `LIVE` backend state and Tier-3 village geometry.

### 2026-11-15 • WebGL2 / WASM Browser Export & Live Server Wiring
- Installed Godot `4.3.stable` Web export templates (`~/.local/share/godot/export_templates/4.3.stable/web_*.zip`) and added `[preset.1]` (`Web`, `variant/thread_support=false`, `html/canvas_resize_policy=2`, Service Worker & CacheStorage cleanup header) to `export_presets.cfg`.
- Updated `scripts/net/api_http_client.gd` (`window.location.origin` resolution + browser-managed same-origin cookies on Web), `scripts/village/village_factory.gd` (auto `--live-connect` + `?demo=1` / `?study=1` / `?rtl=1` / `?edit=1` / `?local=1` URL query parsing, `149L`), and `scripts/ui/study_flashcard_drawer.gd` (fetch live `/v1/courses/{id}/content` UUID challenges before offline fallback, `149L`); exported release bundle to `builds/web/` (`index.html`, `index.js`, `index.wasm`, `index.pck`, `index.audio.worklet.js`, icons).
- Updated `crates/server/src/lib.rs` to serve `apps/godot/builds/web/` on `/` and `/index.html`, serve root Web assets (`/{file}`), return an unregistering/cache-clearing `/sw.js`, keep `/legacy` escape hatch & `CAMPUSLINGO_LEGACY_WEB=1`, and rebuilt release binary `target/release/campuslingo-server` running live on `http://127.0.0.1:18080`.
- Verified in real headless Chromium WebGL 2 CDP (`Docs/progress/m5/m5_browser_live.png`, `m5_browser_demo.png`, `m5_browser_study.png` -> `0` exceptions, green `LIVE` indicator, live PostgreSQL `/v1/courses/.../fortress`, `/economy`, and `/content` UUID challenges rendered), plus live Firefox play (`GET /v1/duels/ws -> 101`, Boss lobby/claim `201/200`), `OVER_150: []`, `tools/godot.py check`, `VERIFY_M1_HEADLESS` (`7/7`), `MOVEMENT_PARITY` (`10/10`), `VERIFY_M2_HEADLESS` (`5/5`), and `VERIFY_E2E_M4` (`9/9`).
