# Technical Contract — CampusLingo: Village Clash (7-Track Multi-Agent Architecture)

Every agent working inside `/home/mahdi/campuslingo-v2/apps/godot/` follows this contract strictly.  
The **Integration Owner** (main session) owns this file, `Docs/STATE.md`, `Docs/DECISIONS.md`, and final gate sign-off. Subagents write **exclusively** inside their assigned track directories and log milestones in `Docs/DEVLOG.md`.

---

## 1. Engine, Machine Constraints & CLI Execution
- **Engine:** Godot `4.3.stable` (verified installed at `/home/mahdi/.local/bin/godot`). Existing Phase 1 files (`project.godot`, `scenes/MainGame.tscn`, `scripts/*.gd`) remain untouched and operational during `M0` planning.
- **Hardware Ceiling (`D006`):** Machine has `2` CPU cores and `~11 GB` RAM. Enforce **`MAX_INSTANCES = 1`** concurrent Godot process via `RunGovernor` and at most **2 concurrent text-writing subagents**.
- **CLI-First Execution (Never `--editor`):**
  - Headless script check / verification: `/home/mahdi/.local/bin/godot --headless --path /home/mahdi/campuslingo-v2/apps/godot --quit`
  - Once `tools/godot.py` is configured in `M0`: `python3 tools/godot.py check`, `capture`, `movie`, `run`.

---

## 2. Strict File Ownership Matrix (Zero Merge Conflicts)

| Subagent Track | Exclusive Owned Paths (Write ONLY here) | Primary Deliverable |
| :--- | :--- | :--- |
| **`core_net`** | `project.godot`, `scripts/core/`, `scripts/net/`, `scenes/boot/` | `RunGovernor`, `DevTools` (`--cam`, `--goto`, `--capture`), `WorldRoot` router, HTTP/WS bridge (`CampusBackendClient`). |
| **`village_grid`** | `scripts/village/`, `scenes/village/` | `40×40` isometric grid (`1.5 m/cell`), Clash-style Pan/Zoom camera (`Pitch -45°`, `Yaw 45°`), Edit/Move mode, overlap validator, `StoryCam_A..D` markers. |
| **`buildings_upgrades`** | `scripts/buildings/`, `scenes/buildings/` | 6 building families (`Core Archive`, `Collectors/Vaults`, 4 `Defensive Towers`, auto-tiling `Walls`, `Barracks`, `Clan Hall`), Lv0→3 procedural meshes, `Brownout` & Shield domes. |
| **`study_economy_ui`** | `scripts/ui/`, `scenes/ui/` | Top resource bar (Gold, Focus, Trophies, Shield), Build/Upgrade drawer, Study Challenge overlay (`/content` -> `/attempts`), League & Clan panels. |
| **`troops_raid_sim`** | `scripts/troops/`, `scripts/combat/`, `scenes/troops/`, `tests/` | `20 Hz` raid loop, perimeter deployment validator, `A*` wall-weighted pathfinding, 5 troop AI behaviors + `Field Scholar Hero`, `0★..3★` evaluator & `tests/run_village_tests.py`. |
| **`raid_modes`** | `scripts/modes/`, `scenes/modes/` | Raid Scanner & Async PvP Village loader (`/raids/scanner`..`/settle`), Shared-HP Chapter Boss arena (`/bosses`), Live 90s Duel connector (`/duels/ws`). |
| **`vfx_audio`** | `assets/shaders/`, `scripts/fx/`, `scenes/fx/`, `assets/audio/` | Stylized Miniature PBR shader suite (`coc_miniature_pbr`, `coc_ground_shadow`, `coc_island_terrain`, `coc_water_moat`, `coc_edit_grid`), `Juice` pooled VFX/Audio bus (`SEC-FX01`), `PbrMiniatureMaterial` (`SEC-FX02`), additive halos & particles (`SEC-FX03`). |

**Shared Append-Only Docs:** `Docs/DEVLOG.md`, `data/evidence.jsonl`, `data/asset_register.csv` (locked at `0$` in `data/costs.jsonl`).
**Micro-Modular Rule (`D016`):** Each of the 7 tracks is further subdivided into isolated **`SEC-*` Micro-Sections** (`40` total across `SEC-C01`..`SEC-FX03`, `<= 150` lines target per script) cataloged in [`Docs/SECTIONS_REGISTRY.md`](SECTIONS_REGISTRY.md) and registered in `SectionRegistry` (`SEC-C03`). Debugging and targeted customization operate on single `SEC-*` scripts without touching sibling modules.

---

## 3. Coordinate Frame, Scale & Isometric Conventions
- `1 unit = 1 metre`, `+Y` up. Logical village floor sits on `Y = 0.0`.
- Grid cell `(gx, gz)` with `gx, gz ∈ [0..39]` maps to world center `((gx - 19.5) * 1.5, 0.0, (gz - 19.5) * 1.5)`.
- Structure heights scale cleanly for isometric readability: Walls `1.2 m`, Towers `2.4–3.8 m`, Core Archive `4.2–5.8 m`, Troops `0.9–1.6 m`.

---

## 4. Physics & Collision Layers (`project.godot` extension planned in `M0`)
1. `ground_floor` (raycast plane for isometric pan & tile selection)
2. `building_footprint` (StaticBody3D selectable buildings & raid targets)
3. `wall_segment` (`1×1` wall colliders)
4. `ground_unit` (troops & hero on ground plane)
5. `air_unit` (`Glyph Drake`)
6. `projectile` (tower bolts, cannonballs, mortar shells)
7. `trigger_sensor` (`Hidden Proctor` stealth radius & rally aura)
8. `camera_ignore` (non-blocking visual props & shield domes)

---

## 5. Duck-Typed Interfaces Between Tracks (Check with `has_method`)

### Building (`class_name VillageBuilding`, Node3D, group `"buildings"`)
- `get_building_id() -> String`, `get_node_id() -> String` (CampusLingo DAG ID)
- `get_footprint_size() -> Vector2i`, `get_grid_origin() -> Vector2i`
- `get_level() -> int`, `get_hp_ratio() -> float`, `is_destroyed() -> bool`
- `apply_server_state(node_state: Dictionary) -> void`
- `take_raid_damage(amount: int, from_unit_kind: String) -> void`
- `has_defense_capability() -> bool`, `get_defense_stats() -> Dictionary`

### Raid Unit (`class_name RaidUnit`, CharacterBody3D/Node3D, group `"raid_units"`)
- `init_unit(archetype: String, grid_ref: Node) -> void`
- `take_damage(amount: int, from_pos: Vector3) -> void`
- `apply_rally_buff(speed_mult: float, atk_mult: float, duration_sec: float) -> void`
- `is_air_unit() -> bool`, `is_alive() -> bool`

### Juice Event Bus (`Juice` Autoload: `scripts/fx/juice.gd`)
- `Juice.emit_event(event_id: String, world_pos: Vector3, normal := Vector3.UP, scale := 1.0)`
- Standard IDs: `"build_place"`, `"upgrade_burst"`, `"brownout_smoke"`, `"shield_pulse"`, `"wall_crack"`, `"tower_shot"`, `"explosion_small"`, `"hero_rally"`.

### Mode / Arena Scene Contract (`WorldRoot` / `VillageRoot`)
Every top-level playable mode (`HOME_VILLAGE`, `RAID_ARENA`, `BOSS_ARENA`, `DUEL_ARENA`, `COMBAT_LAB`) exposes:
- `mode_id: String`
- `StoryCams/` containing `StoryCam_A` (Full Isometric Overview), `StoryCam_B` (Core Archive Close-Up), `StoryCam_C` (Defense Perimeter), `StoryCam_D` (Battle Action Angle)
- Clean `--goto=<MODE>` & `--capture=<OUT_PNG>` CLI entry hooks via `DevTools`.
