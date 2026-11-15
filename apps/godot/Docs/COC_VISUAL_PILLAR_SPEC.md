# Clash of Clans Architectural & Visual Pillar Spec (`D018` — Replaces `ART_RETRO_HIERARCHY.md`)

> Clean-room engineering spec translating architectural patterns discovered via static REA of `Clash of Clans 18.647.5` into original Godot `4.3.stable` (`gl_compatibility`) systems using **$0.00 `CC0`/`MIT`/`OFL` assets** and **Blender `5.3.0`**. Zero proprietary Supercell assets or code may ever enter `campuslingo-v2`.

---

## 1. Visual Style Mandate: Stylized Miniature PBR (NOT Cartoon Toon)

- **Hard Ban List (Immediate Critique `[BLOCKER]`):**
  - **ZERO uniform black comic ink outlines** (`coc_outline_2px`-style back-face hulls are permanently banned).
  - **ZERO 2-band N64/cartoon cel-ramps** discarding normals, roughness, and metallic reflectance.
  - **ZERO blocky toy proportions** (e.g., KayKit capsule hands/heads or stretched sci-fi placeholders).
  - **ZERO permanent grass checkerboards** (`#54A838/#4A9B31` repeating tiles).
  - **ZERO oversized solid orange pad discs** floating under units/buildings.

- **Required Visual Fidelity (`character.frag.shader` & `bloom` Translation):**
  1. **Tactile Miniature PBR**: Every surface uses calibrated **Albedo + OpenGL Normal (`nor_gl`) + packed ORM (`AO/Roughness/Metallic`) + Emissive** maps plus baked **Vertex Ambient Occlusion (`COLOR.r`)** exported from Blender `5.3.0`.
  2. **Natural Material Separation**: Dressed masonry (`roughness ~0.78–0.90`), aged oak timber (`0.68–0.80`), forged bronze/iron (`metallic 0.85, roughness 0.30–0.45`), brushed royal gold (`metallic 0.92, roughness 0.24`), and velvety meadow moss (`roughness ~0.92`).
  3. **Selective Selection Feedback**: Hover/selection emphasis uses ground-projected tactical rings and subtle view-angle Fresnel rim — never black outlines.
  4. **`CLIP_UNDERGROUND` Support**: Subterranean defenses (`SEC-D04` Hidden Proctor Tesla) and spawn rises clip cleanly along `clip_y_plane` in `coc_miniature_pbr.gdshader`.

---

## 2. Empirical `gl_compatibility` Lighting, Shadow & Bloom Rules (`AMD BC-250`)

- **Renderer Lock:** `gl_compatibility` (OpenGL 4.6 Core / `xvfb-run` safe; `mobile`/Vulkan crashes without DRI3 queues in headless capture).
- **WorldEnvironment Baseline (Always Active From Frame 1):**
  - `background_mode = BG_SKY` (`ProceduralSkyMaterial` — avoids `BG_COLOR` post-process `4x` luminance jump).
  - `tonemap_mode = TONE_MAPPER_ACES`, `glow_enabled = true`, `glow_hdr_threshold = 1.05`, `glow_bloom = 0.0`.
  - Pair emissive gameplay accents (Elixir crystals, Tesla arcs, Shield Dome, Hero Rally) with additive billboard/Fresnel halos (`blend_add`) in `SEC-FX03`.
- **Contact Shadows (No SSAO in `gl_compatibility`) & Sun Splits:**
  - Every building, wall cluster, and hero/unit carries a soft dark ground-contact shadow skirt (`coc_ground_shadow.gdshader`, `~0.35 m` bleed + worn dirt edge) plus baked vertex AO (`COLOR.r`).
  - `DirectionalLight3D`: `-52°` pitch, `SHADOW_PARALLEL_2_SPLITS`, `directional_shadow_split_1 = 0.30`, zoom-scaled `directional_shadow_max_distance = clampf(zoom_size_m * 1.9, 65.0, 105.0)`.

---

## 3. Terrain, Water, Edit-Grid, LOD & Batching Discipline

- **Island Environment (`background_sharper` + `custom_water` + `builtin_editor_grid`):**
  - Continuous sculpted grassy meadow (`coc_island_terrain.gdshader`) blending lush moss/grass, central stone courtyard cobbles, and sandy cliff shore edge surrounded by animated coastal water (`coc_water_moat.gdshader`).
  - Tactical build/deploy grid (`coc_edit_grid.gdshader`) stays **hidden** during idle village & combat showcase, appearing smoothly **only** in Edit/Move or Troop Deploy preview mode.
- **LOD0 / LOD1 / LOD2 (`assets/sc3d/` `_geo/.loda1/.loda2` Pattern):**
  - Buildings and dense props export `_lod0` (full beveled/tiled silhouette), `_lod1` (~45% tris), and `_lod2` (~20% tris) bound via `visibility_range_begin/end`.
- **`MultiMeshInstance3D` Batching:**
  - Static perimeter forest trees/boulders (`SEC-G02`), `FortressWallTile` Pillar + E/S connectors per tier (`SEC-B04`), and repeated projectiles share `MultiMeshInstance3D` batches (`< 80` static draw calls total), reading per-instance HP/brownout/selection state through `COLOR` & `INSTANCE_CUSTOM`.

---

## 4. UI / Simulation Split, Audio Pooling & Multilingual SDF Typography

- **Engine vs UI Separation (`libg.so` vs `libscid_sdk`/`Flutter` Pattern):**
  - 3D simulation scenes (`SEC-G*`, `SEC-B*`, `SEC-D*`, `SEC-SIM*`, `SEC-T*`) never build HUD menus directly; declarative `.tscn` UI drawers (`SEC-UI01..05`) communicate purely via typed state snapshots and signals.
  - Declarative layouts live in `.tscn` scenes and static stats tables live in `res://data/*.json`/`.tres` so every `SEC-*.gd` script remains readable and `<= 150` lines.
- **Audio Bus & Pooling (`libfmod.so` Pattern):**
  - Dedicated `Master`, `Music`, `SFX`, and `UI` buses with voice caps and `>= 60 ms` per-cue cooldown gating so 30-troop hordes never distort audio.
- **Arabic RTL & English LTR HUD Typography:**
  - Floating HUD headers and combat damage callouts use subtle SDF outline/drop-shadow styling on readable `OFL` fonts (`Noto Naskh Arabic` + `Noto Sans`), supporting clean RTL mirroring and LTR parity.
