# Autonomous Production Brief & Milestone Gates — CampusLingo: Village Clash

## Outcome
Build an isometric **Clash-of-Clans-style upgradable Knowledge Village & Raid Strategy game** inside Godot `4.3.stable` (evolving `/home/mahdi/campuslingo-v2/apps/godot/`), seamlessly powered by the live `campuslingo-server` backend (`http://127.0.0.1:18080`).

## Read Before Starting Any Gate
1. `CLAUDE.md` and `AGENTS.md`
2. `Docs/DECISIONS.md` (`D001`–`D015`)
3. `Docs/DESIGN_V2.md` (Game loops, 6 building families Lv 0→3, 5 troops + Field Hero, Hex palettes)
4. `Docs/TECH_CONTRACT.md` (7-track file ownership, collision layers, duck-typed interfaces)
5. `Docs/VILLAGE_AND_RAID_SPEC.md` (Numerical constants for `M1` automated tests & backend parity)

## Execution Milestones (Gates `M0` to `M5`)

| Gate | Deliverable | Required Proof |
| :--- | :--- | :--- |
| **M0: Linux Tooling & Reproducible Start** | Approve `Docs/DESIGN_V2.md`, adapt `tools/godot.py` to Linux (`~/.local/bin/godot`), add `RunGovernor` (`MAX_INSTANCES=1`) + `DevTools` + `StoryCam_A..D` rig while preserving existing Phase 1 baseline | Headless `check` passes with zero errors; baseline isometric PNG capture + 10s Movie Maker clip verified |
| **M1: Village Grid & Raid Math Lab** | `CombatLab.tscn` (`40×40` grid), isometric Pan/Zoom camera, building placement/upgrade stats, `A*` wall-weighted pathfinding, unit DPS/priority loop, `tests/run_village_tests.py` | 100% of `M1` checks pass within `±3%` of `Docs/VILLAGE_AND_RAID_SPEC.md` and exact integer parity with `raids_leagues.rs` |
| **M2: 7 Parallel Track Foundations** | Isolated implementation across `core_net`, `village_grid`, `buildings_upgrades`, `study_economy_ui`, `troops_raid_sim`, `raid_modes`, `vfx_audio` (`<= 2` concurrent subagents) | Each track passes headless `check` and produces look-dev captures |
| **M3: Upgradable Home Village + Live Raid/Boss Loop** | Full editable isometric village synced with `/fortress`, Study-to-Upgrade drawer (`/content` -> `/attempts`), PvP Raids (`/raids/*`), Bosses (`/bosses`), Clans & Leagues | All modes launch cleanly; `StoryCam_A..D` screenshots captured across Home Village, Raid Arena, and Boss Arena |
| **M4: 4-Lens Critique-Fix Loop & Bot Playthrough** | Independent critique across 4 lenses (`QA/Net`, `Camera/Build UX`, `Progression/Economy Balance`, `Isometric Visual Hierarchy`) + Fixer pass + automated bot | Critic scores improve across rounds; automated bot builds/upgrades village, passes FSRS-6 cards, and completes a `3★` raid on live backend |
| **M5: Release Candidate** | Texture/mesh budget audit, Linux standalone export, full `--goto` smoke suite | Exported Linux binary boots and cycles through all modes with `0` script/runtime errors |

Stay locked at **`M0_READY`** until the Owner approves `Docs/DESIGN_V2.md`. Update `Docs/STATE.md` and `CONTINUE.md` at every gate transition.
