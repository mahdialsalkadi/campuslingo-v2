# Critique Round 1 — Lens D: Clash of Clans Stylized Miniature PBR & LOD Cohesion Critic

**Score:** 7.8 / 10 (NEEDS FIXES BEFORE M4 SIGN-OFF)
**Artifacts Inspected:** `Docs/progress/m4/m4_raid_action.png`, `Docs/progress/m2/m2_storycam_{a,b,c,d}.png`, `scripts/buildings/focus_extractor_tank.gd`, `assets/models/buildings/focus_extractor_tank_lod0.glb`

## Verified Strengths
- Zero black comic ink outlines, zero checkerboard grass terrain, clean feathered elliptical `coc_ground_shadow.gdshader` contact skirts + baked vertex AO (`COLOR_0`), directional 2-split shadows across zoom range, `MultiMeshInstance3D` batched pine forest and perimeter walls (`Lv0->Lv3`), and 3-tier `_lod0/_lod1/_lod2.glb` meshes.

## Findings
- **[MAJOR] D-1 (`FocusExtractorTank` neon magenta Elixir vat appearance):** In `m4_raid_action.png`, the cylindrical vat in `focus_extractor_tank_lod0.glb` rendered in bright neon magenta (`~#e600ff`), dangerously close to an unshaded missing-texture pink artifact rather than rich crystal Elixir glass.
  - *Fix:* Override the `"elixir_violet"` surface slot in `FocusExtractorTank._mount_visuals()` with translucent royal amethyst glass (`Color(0.36, 0.12, 0.62, 0.90)`, restrained deep-violet emission `0.24`).
- **[MINOR] D-2 (Progress screenshot `.import` clutter in `Docs/progress/`):** Godot editor/headless import passes created `.import` metadata files alongside PNG captures in `Docs/progress/m4/`.
  - *Fix:* Add `Docs/.gdignore` and remove stray `.import` sidecars from `Docs/progress/`.
