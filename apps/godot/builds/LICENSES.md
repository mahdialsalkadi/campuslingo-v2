# Open-Source Assets, Textures, Fonts & Audio License Ledger (D017 / D020)

All PBR textures, fonts, audio cues, and 3D models created or downloaded for **CampusLingo: Village Clash** are free, open-source, commercial-safe **CC0 1.0 Public Domain**, **MIT**, or **SIL Open Font License 1.1 (`OFL-1.1`)** assets (`$0.00` budget). Exact byte sizes, URLs, and SHA-256 digests are audited in [data/asset_register.csv](../data/asset_register.csv). Zero proprietary APK bytes exist in this repository.

---

## 1. CC0 PBR Surface Texture Suite (`assets/textures/pbr/`)
Downloaded directly via PolyHaven API (`1024px` Diffuse + OpenGL Normal `nor_gl` + packed ORM `arm` = AO / Roughness / Metallic):

| Alias | Source Asset (`PolyHaven`) | License | Surface Role |
| :--- | :--- | :--- | :--- |
| `grass` | `grass_path_2` | CC0 1.0 | Sculpted village meadow turf |
| `sand` | `coast_sand_01` | CC0 1.0 | Island shoreline cliff border |
| `cobble` | `cobblestone_floor_04` | CC0 1.0 | Courtyard paving & plinth surrounds |
| `stone` | `medieval_blocks_05` | CC0 1.0 | Core Archive, towers, battlements & walls |
| `wood` | `wood_planks_grey` | CC0 1.0 | Timber framing, scaffolds & siege carriages |
| `slate` | `roof_slates_02` | CC0 1.0 | Pitched citadel & spire roofs |
| `terracotta` | `clay_roof_tiles_02` | CC0 1.0 | Barracks & collector warm tiled roofs |
| `rock` | `rock_boulder_dry` | CC0 1.0 | Coastal cliff skirt & perimeter boulders |
| `metal` | `metal_plate` | CC0 1.0 | Forged gunmetal barrels, brackets & iron rims |
| `bronze` | `rusty_metal_02` | CC0 1.0 | Patinated bronze & gilded royal trim accents |
| `water_normal` | Original Procedural PIL/NumPy | CC0 1.0 | Seamless 512px scrolling water moat wave normal |

---

## 2. Clean-Room Blender `5.3.0` Miniature Architecture (`assets/models/m0_bench/`+)
Generated reproducibly in headless Blender `5.3.0` (`tools/blender_build_m0_diorama.py` & `tools/clean_mesh_blender.py`) with beveled silhouettes, `Smart UV Project` UVs, baked vertex ambient occlusion (`COLOR_0`), and `LOD0 / LOD1 / LOD2` decimation:
- License: **CC0 1.0** (original project-generated geometry referencing Section 1 CC0 PBR textures).

---

## 3. Multilingual HUD Typography (`assets/fonts/`)
| Local File | Family | License | Role |
| :--- | :--- | :--- | :--- |
| `NotoNaskhArabic-Bold.ttf`, `NotoNaskhArabic-Regular.ttf` | Noto Naskh Arabic | OFL 1.1 | Arabic RTL HUD headers, prompts & damage callouts |
| `NotoSans-Bold.ttf`, `NotoSans-Regular.ttf` | Noto Sans | OFL 1.1 | English LTR numerals, buttons & section badges |

---

## 4. Audio SFX (`assets/audio/`)
| Local File | Source Pack (`KenneyNL`) | License | Usage in Game |
| :--- | :--- | :--- | :--- |
| `sfx_cannon.ogg`, `sfx_archer.ogg`, `sfx_destroy.ogg`, `sfx_hit.ogg` | `Starter-Kit-FPS` | CC0 1.0 / MIT | Cannon, Archer, Destruction & Hit SFX |
| `sfx_mortar_break.ogg`, `sfx_star.ogg`, `sfx_deploy.ogg` | `Starter-Kit-3D-Platformer` | CC0 1.0 / MIT | Mortar impact, Star reward & Deploy SFX |
