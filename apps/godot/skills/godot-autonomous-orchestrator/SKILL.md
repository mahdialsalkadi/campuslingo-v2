---
name: godot-autonomous-orchestrator
description: Master planning and multi-agent execution skill for building CampusLingo Village Clash in Godot 4.3 via tools/godot.py without GUI or MCP bottlenecks, enforcing 40 SEC-* micro-sections (<= 150L) and CC0 assets (D017).
---
# Godot Autonomous Orchestrator Skill (CampusLingo: Village Clash)

## 1. Core Architecture Rules
- **Never launch the Godot GUI Editor (`--editor`).** Author all scenes (`*.tscn`), resources (`*.tres`), and typed GDScript (`*.gd`) cleanly and verify via `python3 tools/godot.py`.
- **Enforce `D016` 40 Micro-Sections (`<= 150` lines/script) & `Docs/TECH_CONTRACT.md` 7-Track File Ownership**: `core_net`, `village_grid`, `buildings_upgrades`, `study_economy_ui`, `troops_raid_sim`, `raid_modes`, `vfx_audio`.
- **Enforce `D017` Free Open-Source Assets**: Source free `CC0`/`MIT` `.glb` & `.ogg`/`.wav` assets, log SHA-256 + URLs in `data/asset_register.csv` & `Docs/LICENSES.md`.
- **Hardware Safety**: `RunGovernor` FIRST autoload, strict `MAX_INSTANCES = 1` on Linux (`python3 tools/godot.py check`).

## 2. Gate Progression Protocol (M0 -> M5)
1. **M0 (`PASSED`)**: `tools/godot.py check/capture/movie`, `40 SEC-*` registry, `RunGovernor`, `DevTools`, `Juice`, `F2 SectionDebugInspector`, `MainGame.tscn` `StoryCam_A..D`, `godot:test` `[E2E PASS]`.
2. **M1 (`PASSED`)**: `CombatLab.tscn`, `GridMath`, `AStarWallPathfinder`, `RaidTickStarSolver`, `4` Defenses, `5` Troops, `Docs/MOVEMENT_REFERENCE.md`, `village_profile.tres`, `private_research/scripts/derive.py`, `run_village_tests.py` + `run_movement_tests.py` (`<= ±3.0%`).
3. **M2 (`NEXT`)**: Build remaining `21` `SEC-*` modules across the 7 tracks with isolated `tools/godot.py check` verification.
4. **M3**: Wire upgradable Clash-style 3D village + FSRS-6 study drawer + PvP Raids / Co-op Boss / 90s Duel to live `campuslingo-server` (`127.0.0.1:18080`).
5. **M4**: Execute `critique-fix-loop` (`4` Lenses across `StoryCam_A..D`) + automated bot upgrade & 3★ raid completion.
6. **M5**: Universal stylized Toon pass (`Docs/ART_RETRO_HIERARCHY.md`), texture optimization, Linux release export (`0` errors).
