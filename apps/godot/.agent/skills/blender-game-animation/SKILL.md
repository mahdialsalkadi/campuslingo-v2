---
name: blender-game-animation
description: Headless Blender 5.x pipeline for cleaning AI-generated 3D meshes (Tripo P2), building procedural low-poly props, watertight-proxy rigging, vertex-color baking, and GLB export for Godot 4.x.
---
# Blender Game Animation & Mesh Pipeline Skill

## 1. Headless Execution Only
- Always run Blender in background mode with `--factory-startup` so GUI windows and the `BlenderMCP` add-on never block or collide across parallel subagents:
  ```bash
  blender -b --factory-startup --python tools/clean_mesh_blender.py -- --input raw.glb --output clean.glb --height 1.25 --yaw -90
  ```

## 2. Five Mandatory Fixes for Godot 4.x GLB Imports
1. **Axis Orientation Fix**: Tripo P2 exports models facing Blender `+X`. Rotate mesh vertices directly by `-90°` around Z (`obj.data.transform(rot_mat)`, NOT `rotation_euler`, because glTF exports quaternion rotations) so the exported model faces `+Z` and aligns with Godot's `-Z` forward wrapper.
2. **Origin & Metre Scale**: Shift vertices so `X=0, Y=0` is centered and `Z=0` sits exactly at the character's soles/base, scaled to real-world metre height (`1.25 m` hero, `0.6–1.6 m` enemy, `3.2 m` boss, `6–9 m` house).
3. **Material Metallic/Roughness Reset**: AI 3D generators export `metallic = 1.0` by default. Force `Metallic = 0.0` and `Roughness >= 0.85` on all Principled BSDF materials before exporting.
4. **Vertex Colors & WebP Import Fix**: Godot 4.7 ignores glTF vertex colors by default and fails on lossless WebP ORM textures. Bake vertex colors to UV textures and export images as PNG or lossy WebP (`q95`).
5. **Watertight Proxy Rigging (No Seam Tearing)**:
   - Never apply automatic bone-heat weights directly to a segmented/UV-split generated mesh (it tears at the seams).
   - Instead: create a remeshed watertight proxy hull around the character, parent the proxy to the armature with automatic weights, transfer vertex weights (`DataTransferModifier` -> `VGROUP_WEIGHTS`, nearest face interpolated) to the target mesh, and delete the proxy.
