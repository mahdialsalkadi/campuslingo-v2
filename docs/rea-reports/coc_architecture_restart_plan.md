# CampusLingo: Village Clash — Clean-Room Restart Plan (`M0` → `M5`)

**Target Workspace:** [apps/godot](../../apps/godot)  
**Backend Integration:** [routes.rs](../../crates/server/src/routes.rs) & [VILLAGE_AND_RAID_SPEC.md](../../apps/godot/Docs/VILLAGE_AND_RAID_SPEC.md)  
**Budget & Legal Boundary:** **$0.00 asset budget**, clean-room engineering only — **zero** Supercell binaries, `.glb`, `.sctx`, `.sc`, `.sfx`, or `.ttf` bytes may ever be copied into `campuslingo-v2`. Every choice below translates *architectural patterns* observed during static REA of `Clash of Clans 18.647.5` (`libsupercell_clashofclans.so`, `libg.so`, `libscid_sdk.so`, `libapp.so`, `assets/shaders/`, `assets/sc3d/`, `assets/sfx/`, `assets/titan/`) into original Godot 4.3 systems + verified `CC0`/`MIT`/`OFL` assets.

---

## 1. Diagnosis: Why the Previous Build Looked Cartoony & Broken

Inspecting the existing workspace and native captures ([coc_combat_lab_cam_a.png](screenshots/coc_combat_lab_cam_a.png), [coc_combat_lab_cam_b.png](screenshots/coc_combat_lab_cam_b.png), [coc_combat_lab_cam_d.png](screenshots/coc_combat_lab_cam_d.png)) alongside your feedback (*"ما بدي اسلوبها كرتونية يا اخي"*) revealed five exact root causes in the current tree:

1. **Wrong Visual Bible Left Over From a Platformer Kit**: `ART_RETRO_HIERARCHY.md` *(retired in M0)*, [DECISIONS.md](../../apps/godot/Docs/DECISIONS.md) (`D013`), and [CRITIC_PROMPTS.md](../../apps/godot/tools/workflows/CRITIC_PROMPTS.md) (`1.89 m` triple-jump leftovers!) mandated N64-style 2-band toon shading, banned metallic PBR, and forced uniform `2px` black comic ink hulls.
2. **Broken Shader Pipeline (`coc_stylized_toon.gdshader` *(retired in M0)*, `coc_outline_2px.gdshader` *(retired in M0)*, `toon_hierarchy_material.gd` *(retired in M0)*)**: Ignored normal, roughness, metallic, and AO maps completely, flattened surface shading to a 2-step cel ramp (`ROUGHNESS = 0.88`), and wrapped units in heavy black back-face comic outlines.
3. **Incompatible Mixed Asset Styles ([asset_register.csv](../../apps/godot/data/asset_register.csv), [combat_lab_scene.gd](../../apps/godot/scripts/combat/combat_lab_scene.gd))**: Mixed blocky toy KayKit characters (`barbarian_footman.glb`, `hooded_archer.glb`, `skeleton_breaker.glb`, `armored_giant.glb`, `scholar_mage.glb`), flat-shaded low-poly Quaternius RTS buildings, a Quaternius *Cyberpunk Pack* sci-fi turret (`cannon_turret.glb`), non-uniformly stretched walls (`scale.y *= 3.1; scale.z *= 1.6`), and oversized solid orange ground discs.
4. **Checkerboard Plastic Ground ([grid_board_renderer.gd](../../apps/godot/scripts/village/grid_board_renderer.gd))**: Rendered permanent alternating `#54A838` / `#4A9B31` flat checkerboard tiles instead of sculpted grass/dirt/cobblestone island terrain with an edit-only grid overlay.
5. **Dual Architecture Split Left Unfinished**: [project.godot](../../apps/godot/project.godot) still points `run/main_scene` at [MainGame.tscn](../../apps/godot/scenes/MainGame.tscn), whose `main_game.gd` *(retired in M0)* is a `1,573`-line monolith from the pre-Clash hologram UI instead of the modular `40 SEC-*` village client.

---

## 2. Clash of Clans `18.647.5` REA Findings → Godot 4.3 Clean-Room Blueprint

Every architectural pillar extracted from the APK directly dictates how our rebuilt Godot client works:

| APK REA Evidence (`Clash 18.647.5`) | Real CoC Pattern Observed | Clean-Room Godot 4.3 Restart Design |
| :--- | :--- | :--- |
| **`assets/shaders/character.frag.shader` (`3.28 MB`)** (`albedoTexExternal`, `normalTexExternal`, `materialTexExternal`, `emissionTexExternal`, `irradianceTex`, `envTex`, `NORMAL`, `EMISSION`) | CoC is **Stylized Miniature PBR + IBL**, *not* flat cel-toon: micro-surface normal bumps, metal/stone/cloth roughness contrast, ambient AO crevices, subtle rim/irradiance lift. | Replace `SEC-FX02` with `PbrMiniatureMaterial` + `coc_miniature_pbr.gdshader` using Albedo + Normal + ORM (AO/Roughness/Metallic) + Emission + Sky Irradiance IBL + `color_mul`/`color_add` damage & brownout modulation. **Zero cartoon ink outlines.** |
| **`impostor_outline` / `stage_outline` / `OUTLINE`** | Outlines are **selective** (selection/targeting feedback or hero silhouette separation), never raw `2px` black comic marker around every unit. | Remove `coc_outline_2px.gdshader`. Use subtle ground-projected selection rings & soft view-angle rim highlight *only* on active hovered/selected units or buildings. |
| **`CLIP_UNDERGROUND` flag** | Subterranean props and ambush defenses clip smoothly against the ground plane without z-fighting. | Add `clip_y_plane` uniform to `coc_miniature_pbr.gdshader` for `HiddenProctorTesla` (`SEC-D04`) emergence & subterranean spawn effects. |
| **`background_sharper`, `custom_water`, `builtin_editor_grid`** | High-clarity painted/textured meadow terrain + animated water perimeter; grid lines appear **strictly during Village Edit/Build Mode**. | Rebuild `SEC-G02` (`GridBoardRenderer`) into a continuous sculpted island (`grass`/`dirt`/`cobble` blend + cliff skirt + animated scrolling water moat + perimeter forest). Render `coc_edit_grid.gdshader` **only** when placing/moving structures or previewing deploy bounds. |
| **`bloom_downsample`, `bloom_upsample`, `inverse_tonemap`** | Controlled HDR bloom keeps stone/timber grounded while making Elixir, Gold glints, Tesla arcs, and Shield Domes radiate cleanly. | Configure `WorldEnvironment` (`BG_SKY` `ProceduralSkyMaterial` + `ACES` tonemap + `glow_enabled = true`, `glow_hdr_threshold = 1.05`, `glow_bloom = 0.0`) inside `gl_compatibility`, paired with additive billboard/Fresnel halos (`blend_add`) in `SEC-FX03`. |
| **`assets/sc3d/` (`594` files, `_geo` + `.loda1.glb` + `.loda2.glb`, `_a/_e/_m/_n`)** | Strict **3-tier LOD mesh discipline** per building/troop plus separated PBR texture maps (`_a/_e/_m/_n`). | Standardize 3-tier LOD (`visibility_range_begin/end`) for buildings & hordes; split PBR texture atlases cleanly (`AmbientCG`/`PolyHaven` `CC0` maps packed with installed `PIL 12.3.0`). |
| **`custom_shockwave_sprite`, `particle_simple` (`v_colorMul`/`v_colorAdd`)** | Additive shockwave quads + cheap tinted GPU particle bursts for splashes, rubble, hit flashes, and upgrades. | Upgrade `SEC-FX03` (`CombatBuildParticles`) with `coc_shockwave_ring.gdshader`, mortar parabolic trail, arrow streaks, stone/wood splinters, and Tesla arc bursts. |
| **`libg.so` vs `libscid_sdk.so` (`5,811` exports) + `libapp.so` (`Flutter` Dart `3.9.2`)** | Strict **Simulation/Render Core (`libg.so`) vs Modular Social/Account/UI Overlay (`Flutter`/Protobuf)** separation. | Retire `main_game.gd` (`1,573L`). Keep `VillageWorld3D` pure (`<= 150L` modules); mount thin reactive UI drawers (`SEC-UI01..UI05`) talking only via typed API signals/snapshots. |
| **`libfmod.so` + `assets/sfx/` (`270`) + `assets/music/` (`31`)** | Dedicated audio bus architecture separating adaptive BGM states from spatial/pooled combat SFX. | Upgrade `SEC-FX01` (`JuiceEventBus`) with `Master / Music / SFX / UI` AudioServer buses, voice caps + per-cue cooldown gating (`60 ms` minimum) so 30-troop hordes never clip audio. |
| **`assets/titan/` (`8` fonts incl. `NotoNaskhArabic-Bold.ttf`) + `builtin_stage_text_sdf` / `_with_outline`** | Native Arabic RTL + Latin crisp HUD readability via SDF outlines *on HUD text only*. | Configure bilingual English (`LTR`) / Arabic (`RTL`) `Theme` using clean `OFL` fonts (`Noto Naskh Arabic` + crisp display font) with subtle HUD text outline/drop-shadow for readability over 3D terrain. |
| **`libsupercell_clashofclans.so` (C++ shield) + `libsentry` + `crates/server`** | Anti-tamper & state truth live behind hardened verification & telemetry. | Keep `campuslingo-server` 100% authoritative (`raids_leagues.rs`, `shields.rs`, FSRS-6); add structured client telemetry & atomic local layout cache (`user://`). |

> [!IMPORTANT]
> **Empirical Host Renderer Lock (`AMD BC-250 / RADV GFX1013`)**: Testing `godot --rendering-method mobile` under `xvfb-run` crashed with `signal 4` (`ERROR: None of the devices supports both graphics and present queues` / `No DRI3 support detected - required for presentation`). Because `OpenGL 4.6 Core` (`gl_compatibility`) renders hardware-accelerated on AMD BC-250 (`2566 MB` free VRAM) **and** runs deterministically inside `xvfb-run`, **`M0` must lock `gl_compatibility`** so live desktop rendering and automated CI visual critiques match 100% with zero shader drift.

---

## 3. Clean-Room Asset & Tooling Strategy (`$0` Budget, Blender `5.3.0` Installed)

Tooling verified on this host: `godot` `4.3.stable.official` (`/home/mahdi/.local/bin/godot`), **`Blender 5.3.0`** (`/home/mahdi/.local/bin/blender`, official release installed cleanly into `~/.local/opt/blender-5.3.0-linux-x64`), `PIL 12.3.0`, and `ffmpeg`.

To eliminate both the KayKit cartoon look and boxy raw procedural primitives under our `$0` budget:
- **Buildings, Defenses, Walls & Environment (`Blender 5.3.0 bpy` + `CC0` Kitbash + `AmbientCG` PBR)**:
  1. Download **`CC0` PBR surface textures (`1024px`)** from **AmbientCG / PolyHaven** (dressed ashlar masonry, mossy meadow soil, cobblestone courtyard, dark aged oak timber, slate/terracotta roof tiles, forged gunmetal/bronze, brushed gold trim, normal-mapped water) into `assets/textures/pbr/` (packed/normalized with `PIL 12.3.0`).
  2. Update [clean_mesh_blender.py](../../apps/godot/tools/clean_mesh_blender.py) (remove the old `metallic=0.0, roughness>=0.85` cartoon clamp) and author headless **Blender 5.3.0 `bpy`** builders (`Bevel`, `Array`, `Subdivision`, `Smart UV Project`, baked vertex **`AO` in `COLOR_0`**, and `Decimate` **`_lod0 / _lod1 / _lod2.glb`**) alongside curated `CC0` non-cartoony medieval architectural/character `.glb` assets.
  3. **Remove All Cartoon Prototype Artifacts Directly** (no leftover clutter):
     - Delete `assets/shaders/coc_outline_2px.gdshader` and `assets/shaders/coc_stylized_toon.gdshader`.
     - Delete `Docs/ART_RETRO_HIERARCHY.md` and `Docs/progress/m1/` cartoon renders/clips; replace with `Docs/COC_VISUAL_PILLAR_SPEC.md`.
     - Delete KayKit blocky cartoon troop `.glb`s/textures, Cyberpunk `cannon_turret.glb`, and low-detail placeholder kits (`data/asset_register.csv` & `Docs/LICENSES.md` rebuilt cleanly).
     - Remove retired hologram monolith files (`scripts/main_game.gd`, `scripts/fortress_3d_view.gd`, `scripts/sandbox_2d_view.gd`, `scripts/backend_client.gd`) once `MainGame.tscn` is reset to the clean `M0` village entry.
     - Rewrite [CRITIC_PROMPTS.md](../../apps/godot/tools/workflows/CRITIC_PROMPTS.md) so all 4 lenses enforce CoC tactical strategy + stylized miniature PBR instead of SM64 platformer rules.
- **Preserve & Reuse Cleanly**: [tools/godot.py](../../apps/godot/tools/godot.py) (`MAX_SLOTS=1`), [run_governor.gd](../../apps/godot/scripts/core/run_governor.gd) (`SEC-C01`), [dev_tools.gd](../../apps/godot/scripts/core/dev_tools.gd) (`SEC-C02`), [section_registry.gd](../../apps/godot/scripts/core/section_registry.gd) (`SEC-C03`), [VILLAGE_AND_RAID_SPEC.md](../../apps/godot/Docs/VILLAGE_AND_RAID_SPEC.md), [MOVEMENT_REFERENCE.md](../../apps/godot/Docs/MOVEMENT_REFERENCE.md), `private_research/scripts/derive.py`, [village_profile.tres](../../apps/godot/scenes/combat/village_profile.tres), [run_village_tests.py](../../apps/godot/tests/run_village_tests.py), `run_movement_tests.py`, [run_godot_game.sh](../../scripts/run_godot_game.sh), and all `crates/` backend code.

---

## 4. Gate-by-Gate Restart Plan (`M0` → `M5`)

Respecting **`D006` Hardware Guard** (`MAX_INSTANCES = 1` Godot run, `<= 2` concurrent editing subagents, strict track ownership) and **Clean `<= 150L` Modularity**: declarative UI/scene trees live in `.tscn` prefabs and static data tables live in `res://data/*.json`/`.tres` so every `SEC-*.gd` script stays clean, readable, and strictly `<= 150` lines without single-line cramming.

### Gate `M0`: Clean Reset, Renderer/Shadow/Bloom Lock & Blender PBR Preflight
- **Goal**: Strip every cartoon/platformer leftover, lock the `gl_compatibility` PBR + `BG_SKY` ACES/Bloom + Contact Shadow baseline, update Blender `5.3.0` asset tooling, and prove visual quality with calibrated Blender-built PBR diorama props + island terrain.
- **Work Breakdown**:
  1. **Governance & Spec Reset**: Delete cartoon docs/shaders/models directly; update [DECISIONS.md](../../apps/godot/Docs/DECISIONS.md) (`D018+`), author `Docs/COC_VISUAL_PILLAR_SPEC.md`, rewrite `CRITIC_PROMPTS.md` & `DESIGN_V2.md`, update `SEC-FX02` in `SectionRegistry` (`PbrMiniatureMaterial` at `scripts/fx/pbr_miniature_material.gd`).
  2. **Renderer, Shadow & Shader Baseline**: Configure [project.godot](../../apps/godot/project.godot) (`gl_compatibility`, MSAA 3D = 2, viewport `1360x840`, clean `M0` entry scene). Lock lighting with `BG_SKY` (`ProceduralSkyMaterial`), `TONE_MAPPER_ACES`, `glow_enabled = true`, `glow_hdr_threshold = 1.05`, `glow_bloom = 0.0` (verified in Godot 4.3 `gl_compatibility` without `BG_COLOR` lift), plus `DirectionalLight3D` (`-52°` pitch, `SHADOW_PARALLEL_2_SPLITS`, `directional_shadow_split_1 = 0.30`, zoom-scaled `directional_shadow_max_distance = clampf(zoom_size_m * 1.9, 65.0, 105.0)`). Author core shaders:
     - `assets/shaders/coc_miniature_pbr.gdshader` (`albedo`, `normal`, `orm`, `emission`, vertex `AO` in `COLOR.r`, `MultiMesh` `INSTANCE_CUSTOM`, `clip_y_plane`, `color_mul`/`color_add`, selective rim).
     - `assets/shaders/coc_ground_shadow.gdshader` (`~0.35 m` soft dark contact shadow + warm worn dirt skirt beneath every footprint so structures never float without SSAO).
     - `assets/shaders/coc_island_terrain.gdshader`, `assets/shaders/coc_water_moat.gdshader`, `assets/shaders/coc_edit_grid.gdshader`.
  3. **Blender 5.3.0 + PBR Texture Pipeline**: Update `tools/clean_mesh_blender.py` (preserve PBR roughness/metallic + bake `COLOR_0` vertex AO + `LOD0/1/2` decimation), fetch AmbientCG/PolyHaven `CC0` PBR textures (`assets/textures/pbr/`), and generate/verify the initial calibrated M0 stone plinth, battlements, and PBR nature test props via `blender -b`.
  4. **Verification Gate (`M0_PASSED`)**:
     - `python3 tools/godot.py check` → `0` errors
     - `python3 tools/godot.py sections` → `40` registered sections (0 stale refs)
     - Capture M0 clean sculpted island + Blender-baked PBR reference diorama (`StoryCam_A..D`) → visually verify crisp normal/roughness lighting, soft contact shadows, `ACES` glow halo + `blend_add` fallback test, animated water moat, zero checkerboards, zero black cartoon outlines.

---

### Gate `M1`: PBR Village Arena, MultiMesh Forest, Defenses, 5 Troops & Combat Parity (`SEC-*`: 19 Live)
- **Goal**: Build the playable, realistic-miniature-PBR CombatLab (`scenes/tests/CombatLab.tscn`) hitting 100% math/physics parity with `campuslingo-server` **before** building progression/UI tracks.
- **Tracks & Sections Built**:
  - **Track 7 (`vfx_audio`)**: `SEC-FX01` `JuiceEventBus` (AudioServer `Master/Music/SFX/UI` buses + 60ms pooled SFX limiter), `SEC-FX02` `PbrMiniatureMaterial` (PBR/vertex-AO uniform binder, tier `0..3` material shift, brownout soot/flicker, `LOD0/1/2` visibility binder), `SEC-FX03` `CombatBuildParticles` (`coc_shockwave_ring.gdshader`, additive billboard/Fresnel halos `blend_add` for Tesla/Focus/Shield glow emphasis, parabolic trail, wood/stone splinters).
  - **Track 2 (`village_grid`)**: `SEC-G01` `GridMath` (`40×40`, `1.5m` cells, `[4..35]` interior — cleaned of cartoon overrides), `SEC-G02` `GridBoardRenderer` (sculpted `coc_island_terrain` meadow + surrounding rock/pine forest batched via **`MultiMeshInstance3D`** + animated `coc_water_moat` + hidden-by-default `coc_edit_grid`), `SEC-G03` `CameraPanZoom` (locked `-45°`/`+45°` isometric pan/zoom, zoom-scaled directional shadow split distance, trauma shake).
  - **Track 3 (`buildings_upgrades` Defenses)**: Blender-sculpted beveled/tiled PBR `LOD0/1/2` defenses (`SEC-D01` `QuillArcherSpire`, `SEC-D02` `InkCannon`, `SEC-D03` `RuneMortar`, `SEC-D04` `HiddenProctorTesla` submerged via `clip_y_plane`), each paired with `.tscn` prefabs & `coc_ground_shadow.gdshader` contact skirts.
  - **Track 5 (`troops_raid_sim`)**: `SEC-SIM01` `AStarWallPathfinder`, `SEC-SIM02` `RaidTickStarSolver`, `SEC-T01` `SquireFootman`, `SEC-T02` `LexiArcher`, `SEC-T03` `ThesisBreaker`, `SEC-T04` `FormulaGiant`, `SEC-T05` `GlyphDrake` (curated non-cartoony proportioned rigged `.glb` troops + PBR leather/steel/cloth maps), plus `MultiMeshInstance3D` batched projectiles/walls preview.
- **Verification Gate (`M1_PASSED`)**:
  - `python3 tests/run_village_tests.py` → `7/7 PASS`
  - `python3 tests/run_movement_tests.py` → `10/10 PASS` within `<= ±3.0%` of `private_research/derived_reference.json`
  - Every live `SEC-*` script `<= 150` lines (zero single-line cramming)
  - `tools/godot.py capture scenes/tests/CombatLab.tscn ... --cam=StoryCam_A..D` + `tools/godot.py movie ... --frames 120` → **Owner Visual Approval Gate BEFORE M2**.

---

### Gate `M2`: Complete All Remaining 21 `SEC-*` Micro-Sections (`40 / 40` Live)
- **Goal**: Build all remaining modules in parallel pairs (`<= 2` editing subagents at once per `D006`, strict track isolation), keeping `.tscn` layouts and `res://data/*.json` separated so `40/40` `SEC-*.gd` scripts stay readable and `<= 150` lines.
- **Execution Batches**:
  - **Batch A — Network & Persistence**:
    - *Track 1*: `SEC-C04` `AuthCoursesAPI`, `SEC-C05` `VillageStudyAPI`, `SEC-C06` `RaidsDuelsAPI`.
    - *Track 2*: `SEC-G04` `BuildDragPlacement` (toggles `coc_edit_grid` on grab, snaps footprint, renders subtle valid/invalid edge pulse), `SEC-G05` `LayoutPersistence` (`user://village_layout_<course>.json` atomic temp+rename backup + DAG seed merge).
  - **Batch B — All Upgradable Buildings & Hero (`MultiMeshInstance3D` Walls)**:
    - *Track 3*: `SEC-B01` `CoreArchiveTownHall` (`4×4`, `Lv0..Lv3`, `1★`), `SEC-B02` `GoldMineVault` (`3×3`), `SEC-B03` `FocusExtractorTank` (`3×3`), `SEC-B04` `FortressWallTile` (`1×1` auto-tiling **Pillar + E/S Connector `MultiMeshInstance3D` batching per tier `Lv0..Lv3`** with `COLOR`/`INSTANCE_CUSTOM` per-tile HP damage & highlight + light cell colliders), `SEC-B05` `StudyBarracksCamp` (`3×3`), `SEC-B06` `ClanHallShieldDome` (`3×3`, hexagonal translucent shield dome + additive rim halo + brownout soot visual).
    - *Track 5*: `SEC-T06` `FieldScholarHero` (`950 HP`, `55 DPS`, Rally Recital `+30%` atk/spd `r=5.0` cells for `6.0 s`).
  - **Batch C — Decoupled Clash HUD (`Flutter` Layer Equivalent) & Raid Modes**:
    - *Track 4*: `SEC-UI01` `ResourceTopBar`, `SEC-UI02` `BuildingActionBar`, `SEC-UI03` `StudyFlashcardDrawer` (declarative `.tscn` layout + `res://data/` UI styles, FSRS-6 `1..4`, Overcharge, Sandbox, RTL/LTR bilingual SDF styling), `SEC-UI04` `ArmyTrainingDrawer`.
    - *Track 6*: `SEC-M01` `PvpRaidController`, `SEC-M02` `BossRaidController`, `SEC-M03` `LiveDuelController`.
  - **Core Wiring**: Slim `<= 150L` `scripts/main_village.gd` mounted on [MainGame.tscn](../../apps/godot/scenes/MainGame.tscn).
- **Verification Gate (`M2_PASSED`)**:
  - `python3 tools/godot.py sections` → `SUMMARY={"live":40,"planned":0,"total":40}`
  - All `40/40` `SEC-*` scripts `<= 150` lines; `tools/godot.py check` → `0` errors
  - Total static village draw calls `< 80` (`MultiMeshInstance3D` verified).

---

### Gate `M3`: Live `campuslingo-server` End-to-End Integration
- **Goal**: Connect every UI drawer and raid controller to real PostgreSQL 16 + `campuslingo-server` (`http://127.0.0.1:18080`) with zero mock stubs.
- **End-to-End Scenarios Verified**:
  1. **Auth & Village Hydration**: Register/login (`POST /auth/login`), fetch `/courses` + `/courses/{id}/fortress` + `/shields` → map DAG nodes 1:1 onto `[4..35]` grid (`Tier -1/0` scaffold, `Lv1..3` stone/brick/gold, `is_brownout` `<= 6950` soot + `-35%` fire rate, `raid_immune` Shield Dome).
  2. **Study-to-Upgrade Loop**: Open `StudyFlashcardDrawer` (`SEC-UI03`) → `/practice` → `/attempts` → `/attempts/{id}/answer` + `/shields/recompute` → verify resource delta, automatic brownout repair visuals, tier mesh swap (`Lv1 → Lv2`), and upgrade celebration SFX/particles.
  3. **Async PvP Raid (`/raids/*`)**: Opt in (`POST /raids/opt-in`) → `/scanner` (show daily `10/3/1` caps & `15/22` trophy stakes) → `/start` → live 3D `120 s` raid against opponent's hydrated layout → `/settle` (`STANDARD_RAID_CHEST` / `APEX_RAID_CHEST`).
  4. **Shared Chapter Boss & Live Duel**: `/courses/{id}/bosses` + `/lobbies` + `/strike` (`0%/75%/100%/125%/200%` damage scale) + `/bosses/ws`; `/duels/matchmake` + `/duels/ws` + `/clans/*` + `/leagues/*`.
  5. **RTL/LTR & BGM/SFX Switch**: Toggle Arabic RTL (`Noto Naskh Arabic`) and confirm zero layout clipping or broken anchoring.
- **Verification Gate (`M3_PASSED`)**:
  - `npm --prefix /home/mahdi/campuslingo-v2 run godot:test` → `[E2E PASS]` against live `run_live_stack.mjs`
  - Visual captures (`StoryCam_A..D`) for Home Village, Build Edit Mode (`coc_edit_grid` active), Active PvP Raid, and Boss/Study HUD overlays.

---

### Gate `M4`: Re-Calibrated 4-Lens Critique-Fix Loop, Automated Bot Play & Profiling
- **Goal**: Catch every visual, tactical, audio, or server-parity flaw via adversarial critique + headless playthrough automation.
- **Execution**:
  1. **Re-Calibrated 4-Lens Critique-Fix Rounds** ([CRITIC_PROMPTS.md](../../apps/godot/tools/workflows/CRITIC_PROMPTS.md)):
     - *Lens A (QA & Parity)*: Zero server formula drift against `raids_leagues.rs` / `shields.rs`, atomic save recovery, clean resource cleanup.
     - *Lens B (Game Feel & Audio/Camera)*: Smooth inertial pan/zoom, crisp build placement feel, clean audio bus ducking & uncapped horde legibility, RTL HUD legibility.
     - *Lens C (Tactical Base & Raid Balance)*: Fun study-to-upgrade pacing, A* wall breaches feel earned, Mortar deadzone & Tesla ambush create real counterplay.
     - *Lens D (CoC Miniature PBR Cohesion)*: Rejects *any* lingering cartoon outline, stretched mesh, neon clipping, ungrounded orange disc, or checker grass; audits `LOD0/1/2` switches across zoom `18 m → 54 m`.
     - Repeat critique → fix → re-capture until **all 4 lenses ≥ `8.5/10`** with zero `[BLOCKER]` or `[MAJOR]`.
  2. **Automated End-to-End Bot Playthrough (`scripts/verify_e2e_headless.gd` + Movie Maker)**: Bots login, answer flashcards, build/upgrade/move walls, achieve `3★` raid victory, and strike Chapter Boss while logging deterministic telemetry + `.mp4` showcase clip.
  3. **Hardware Performance Profile (`AMD BC-250`)**: Profile worst-case 40-troop + multi-defense battle under `gl_compatibility`, tuning `LOD1/LOD2` ranges (`visibility_range_begin/end`) & particle budget to sustain `<= 16.6 ms` frame time (`60 FPS` target).
- **Verification Gate (`M4_PASSED`)**:
  - 4 critique reports saved in `Docs/progress/critique/` all scoring `>= 8.5/10`
  - `10/10` movement + `7/7` village + `godot:test` all green
  - Automated `.mp4` playthrough recorded & verified (`sha256` in evidence log).

---

### Gate `M5`: Linux Release Export & Clean-Room Packaging Audit
- **Goal**: Produce a standalone runnable Linux binary completely clean of debug hooks, test bloat, and proprietary content.
- **Work Breakdown**:
  1. Author `export_presets.cfg` (`Linux`, `x86_64`, `gl_compatibility`), excluding `private_research/`, `tests/`, `.agent/`, and temporary dev logs from export PCK filters (per [BUILD.md](../../apps/godot/Docs/BUILD.md)).
  2. Run `godot --headless --path . --export-release "Linux" ../builds/CampusLingoVillageClash.x86_64`.
  3. Clean-room & License Audit: Verify every packaged resource against `data/asset_register.csv` & `Docs/LICENSES.md`; bundle `LICENSES.md` + `README.md` alongside binary.
  4. **Packaged Smoke Boot**: Launch exported binary against a fresh temporary `HOME`/`user://` directory + `xvfb-run` + live `campuslingo-server` (`Ready` + viewport capture + clean exit `0`), avoiding any false pass from editor-only testing.
- **Verification Gate (`M5_PASSED` / Ship Ready)**:
  - Exported binary launches cleanly, authenticates against `127.0.0.1:18080`, renders live village, passes smoke capture with `0` runtime/resource errors, SHA256 logged in release manifest.
