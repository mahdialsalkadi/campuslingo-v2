---
name: higgsfield-generation
description: Generates consistent style-bible concept sheets, neutral front A-pose character references, retro textures, SFX, music loops, and voices while tracking credits in data/costs.jsonl.
---
# Higgsfield Concept, Texture & Audio Generation Skill

## 1. Style Bible & Character Reference Rules
- Before generating any 3D meshes, establish a **Style Bible** (`concepts/v2/style/`) defining hex color palettes and lighting for the Hub, each World, and the Boss Arena.
- For 3D character conversion, generate a **neutral front-facing A-pose sheet on a solid neutral background** with clear limb separation and no overlapping props (hats and backpacks must be generated as separate attachable meshes).
- Never include copyrighted franchise names, characters, or screenshots in prompts.

## 2. CLI & Cost Discipline
- Run `higgsfield generate cost` before `higgsfield generate create`.
- Convert provider credits conservatively (`$0.075` per credit unless billed USD is explicitly returned) and record every job ID and receipt path in `data/costs.jsonl`.
