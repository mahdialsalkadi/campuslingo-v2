# Agent Governance & Operating Rules — CampusLingo: Village Clash

Read `TASK.md` and `Docs/` before writing or modifying any Godot scripts or scenes.

1. **Workspace Discipline**: Keep all Godot game assets, scripts, and scenes inside `/home/mahdi/campuslingo-v2/apps/godot/` and governance records inside `Docs/`. Do not touch Rust crates (`crates/`) or unrelated web packages unless explicitly authorized by a new decision in `Docs/DECISIONS.md`.
2. **Single Integration Owner, 7 Isolated Tracks**: The main session acts as Integration Owner (`Docs/TECH_CONTRACT.md`, `Docs/STATE.md`, `Docs/DECISIONS.md`, gate approvals). Parallel subagents work inside strictly disjoint track directories:
   - `core_net`
   - `village_grid`
   - `buildings_upgrades`
   - `study_economy_ui`
   - `troops_raid_sim`
   - `raid_modes`
   - `vfx_audio`
3. **CLI-First Execution**: Run Godot strictly through CLI/headless verification (`~/.local/bin/godot --headless` / `tools/godot.py`). Run any procedural mesh scripts in Blender background mode (`blender -b --factory-startup --python ...`). Never use GUI editors during autonomous builds.
4. **Hardware & Memory Protection (`D006`)**:
   - Host hardware: 2 CPU cores, ~11 GB RAM.
   - Enforce **`MAX_INSTANCES = 1`** concurrent Godot process at all times. Limit concurrent text-writing subagents to `<= 2`.
5. **Continuous Visual & Automated Proof**:
   - `M1`: All simulation/grid/backend-parity assertions in `tests/run_village_tests.py` must pass within `±3%` of `Docs/VILLAGE_AND_RAID_SPEC.md`.
   - `M2–M4`: Capture isometric camera angles `StoryCam_A..D` and verify zero headless errors (`SCRIPT ERROR`, `Parse Error`) after every change.
6. **Zero Fake Implementation**: A written `.gd` or `.tscn` file does not count as done until tested headlessly against live/seeded contracts and verified visually where applicable.
