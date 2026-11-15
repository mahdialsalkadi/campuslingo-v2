# Project State — CampusLingo: Village Clash (Godot 4.3 Clean-Room PBR Architecture)

**Status:** `M5_PASSED` (All Gates `M0` -> `M5` Complete)
**Date:** `2026-11-15`
**Renderer:** `gl_compatibility` (AMD BC-250 / RADV GFX1013 + OpenGL headless/Xvfb safe)
**Micro-Sections:** `40 / 40 LIVE` (`OVER_150: []`, every `.gd` script in `scripts/` <= 150 lines)
**Asset Policy:** `$0.00` CC0 / OFL-1.1 / MIT (`162` rows verified in `data/asset_register.csv`, `0` non-permissive assets)

## Gate Verification Ledger
| Gate | Outcome | Key Verified Evidence |
|------|---------|-----------------------|
| **M0** | `M0_PASSED` | Sculpted island diorama, feathered ground-contact shadow (`coc_ground_shadow.gdshader`), water moat, 9 modular PBR buildings, StoryCam_A..D |
| **M1** | `M1_PASSED` | `tests/run_village_tests.py` (`7/7` M1_COMBAT_LAB + `10/10` movement parity within `<= 1.91%`), `tests/run_movement_tests.py` (`10/10`) |
| **M2** | `M2_PASSED` | `tools/godot.py sections` (`40/40 LIVE`), `scripts/verify_m2_headless.gd` (`5/5`), bilingual EN/AR RTL HUD, `MainGame.tscn` (`0` script/runtime errors) |
| **M3** | `M3_PASSED` | Live REST + WebSocket parity against `http://127.0.0.1:18080` (`scripts/verify_m3_live.gd` -> `7/7`: auth, fortress fetch, study attempt, PvP raid flow, boss strike, WS duel, economy restart persistence) |
| **M4** | `M4_PASSED` | 4-Lens Critique R1->R2 (`r2_a` `9.0`, `r2_b` `8.8`, `r2_c` `8.9`, `r2_d` `8.8`), `scripts/verify_e2e_headless.gd` & `godot:test` (`9/9`, `p50_step_ms=7.333`, `p95_step_ms=12.033`), OpenGL `gl_compatibility` 40-troop bench (`scripts/verify_gl_bench_m4.gd` -> `ok: true`, `draw_calls=1011`, `primitives=179764`, `p50_step_ms=8.94`, `p95_step_ms=14.756`, `hp_bars_visible=3`), `Docs/progress/m4/m4_playthrough.mp4` (`sha256: beaa61742e08258bf5580ae9c9208de240ebb20449c246cc3b35403000ed3262`) & `m4_raid_action.png` |
| **M5** | `M5_PASSED` | `export_presets.cfg` (`Linux/X11`, `x86_64`, embedded PCK), exported `builds/CampusLingoVillageClash.x86_64` (`112,626,048` bytes, `sha256: 4eb06c1a57130b24377f1f56b7fea01b9a0f66c23a08e940e7cbd1430a9fa9f4`), bundled `builds/LICENSES.md`, `builds/README.md`, `builds/RELEASE_MANIFEST.json`, `builds/SHA256SUMS.txt`, isolated `HOME`/`XDG_DATA_HOME` live smoke boot (`STANDALONE_EXIT=0`, `Docs/progress/m5/m5_standalone_smoke.png`) |

## Standalone Release Artifacts
- Linux Binary: `/home/mahdi/campuslingo-v2/apps/godot/builds/CampusLingoVillageClash.x86_64` (`108 MB`, self-contained release export)
- Preset: `/home/mahdi/campuslingo-v2/apps/godot/export_presets.cfg` (`Linux/X11`, `gl_compatibility`, `embed_pck=true`; and `Web`, `variant/thread_support=false`)
- WebGL2 / WASM Export: `/home/mahdi/campuslingo-v2/apps/godot/builds/web/index.html` (served live at `http://127.0.0.1:18080/` and `http://localhost:18080/` via `crates/server`)
