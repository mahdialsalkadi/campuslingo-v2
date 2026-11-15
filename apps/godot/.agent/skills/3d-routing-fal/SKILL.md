---
name: 3d-routing-fal
description: Routes 3D asset tasks between paid image-to-3D generation (fal.ai Tripo P2) and procedural Blender modeling, enforcing budget ceilings and pre-submission cost logging.
---
# 3D Asset Routing & fal.ai Tripo P2 Skill

## 1. Routing Decision Rule
- **Use Paid Image-to-3D (`fal.ai` `tripo3d/p2/image-to-3d`)**: ONLY for the Hero, NPCs, Enemy families, Boss, and high-value organic hero landmarks, starting from an accepted clean front A-pose reference image.
- **Use Free Procedural Blender Scripts (`blender -b --factory-startup --python`)**: For repeat architectural modules, crates, rails, ladders, platforms, walls, books, and simple geometric props.

## 2. Strict Cost & Retry Protocol
1. **Log Before Submitting**: Append a reservation row to `data/costs.jsonl` (`utc`, `phase`, `provider`, `model`, `job_id`, `max_reserved_usd`) BEFORE making any network call.
2. **Two-Attempt Cap**: At most 2 paid attempts per asset stage. If a timeout occurs, resume/poll the saved `job_id` — NEVER resubmit blindly. If 2 attempts fail, build a clean procedural Blender substitute.
3. **Post-Generation Cleanup**: Pass every downloaded `.glb` through `tools/clean_mesh_blender.py` and register its SHA-256 hash in `data/asset_register.csv` and `Docs/LICENSES.md`.
