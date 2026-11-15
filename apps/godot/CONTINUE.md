# CONTINUE HERE — CampusLingo: Village Clash Handoff

Everything needed for any fresh agent session or subagent to resume work on **CampusLingo: Village Clash** (the isometric Clash-of-Clans-style upgradable village & raid game) lives in this folder and `Docs/`.

## 1. Read These First (In Order)
1. `Docs/STATE.md` — Exact current gate status (`M1_PASSED -> M2_READY`) and active constraints.
2. `Docs/SECTIONS_REGISTRY.md` — Index of all `40` Micro-Sections (`SEC-C01`..`SEC-FX03`) per Decision `D016`.
3. `Docs/DECISIONS.md` — Locked architectural decisions (`D001`–`D017`, including `D017` open-source web asset sourcing).
4. `Docs/LICENSES.md` & `data/asset_register.csv` — Free CC0/MIT `.glb` 3D models & `.ogg` SFX inventory.
5. `Docs/MOVEMENT_REFERENCE.md` & `scenes/combat/village_profile.tres` — REA (`rea-physics-reverse`) binary-verified + derived kinematic/combat parity config (`tests/run_movement_tests.py`).
6. `.agent/skills/` — All 6 Agent Kit skills integrated inside `apps/godot/`.
4. `Docs/DESIGN_V2.md` — Approved game design (6 upgradable building families, 5 troop units + Hero, FSRS-6 study loop, Hex palettes).
5. `Docs/TECH_CONTRACT.md` — Strict 7-track file ownership matrix, physics layers, duck-typed interfaces, and machine caps.
6. `Docs/VILLAGE_AND_RAID_SPEC.md` — Numerical verification target for `M1` (`40×40` grid, `A*` wall weights, DPS/HP curves, and exact `campuslingo-server` raid/boss parity).

## 2. Environment & Verified Linux Commands
- **Godot Binary:** `/home/mahdi/.local/bin/godot` (`4.3.stable.official.77dcf97d8`)
- **Central CLI Wrapper (`tools/godot.py`):**
  - `python3 tools/godot.py check` — Headless syntax & script check (`0` script/runtime errors)
  - `python3 tools/godot.py sections` — Lists all 40 `SEC-*` micro-sections (`python3 tools/godot.py sections --id SEC-B01`)
  - `python3 tools/godot.py capture res://scenes/MainGame.tscn Docs/progress/m0/out.png --cam StoryCam_A`
  - `python3 tools/godot.py movie res://scenes/MainGame.tscn Docs/progress/m0/out.avi --frames 120`
- **Existing Verified E2E Smoke Check (Phase 1 Baseline):**
  - `npm --prefix /home/mahdi/campuslingo-v2 run godot:test`
- **Live Backend Stack (`campuslingo-server` on `http://127.0.0.1:18080`, PostgreSQL 16):**
  - `node /home/mahdi/campuslingo-v2/scripts/run_live_stack.mjs`
- **Hardware Ceiling (`D006`):**
  - Never run more than **1** Godot instance at a time (`MAX_INSTANCES = 1` on this 2-core / 11 GB RAM Linux machine).
  - Never launch `--editor` during agent execution; keep all agent verification headless or CLI-captured via `StoryCam_A..D`.

## 3. Customized Critique-Fix Loop (Planned for Gate `M4`)
1. **Track Builders** work strictly within their owned folders (`core_net`, `village_grid`, `buildings_upgrades`, `study_economy_ui`, `troops_raid_sim`, `raid_modes`, `vfx_audio`).
2. **4 Critic Lenses** run cleanly without prior session bias (`QA & Backend Sync`, `Isometric Camera & Build UX`, `Village Progression & FSRS-6 Economy Balance`, `Isometric Visual Hierarchy`).
3. **Fixer Pass** resolves all `BLOCKER` and `MAJOR` findings and re-captures `StoryCam_A..D`.
4. **Automated Bot Verification** constructs/upgrades buildings, answers FSRS-6 review cards, trains troops, and completes a 3★ PvP raid cleanly on the live stack.

> **Current Gate Status:** `M1_PASSED -> M2_READY`. Gate `M1` (`14` isolated scripts + `CombatLab.tscn` + `7/7` `run_village_tests.py` + `28` CC0/MIT assets) passed cleanly. Ready to implement Gate `M2` remaining micro-sections (`SEC-C04..C06`, `SEC-G04..G05`, `SEC-B01..B06`, `SEC-UI01..UI04`, `SEC-T06`, `SEC-M01..M03`, `SEC-FX02..FX03`).
