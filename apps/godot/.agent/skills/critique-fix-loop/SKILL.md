---
name: critique-fix-loop
description: Executes the 4-lens Critique-to-Perfection loop (QA, Game Feel, Fun/Design, Visual Hierarchy) using fixed StoryCam captures, Movie Maker clips, and automated completion bots.
---
# 4-Lens Critique-Fix Loop Skill ("Loop to Perfection")

## 1. Loop Workflow
Never mark a world, character, UI, or boss track complete after a single implementation pass. Execute the 4-step loop:
1. **Capture Evidence**: Run `python tools/godot.py capture` across `StoryCam_A`, `StoryCam_B`, `StoryCam_C`, `StoryCam_D` and `python tools/godot.py movie` for a 10–20s gameplay trace.
2. **Run 4 Independent Critic Lenses in Parallel**:
   - **Lens 1 (`qa_technical`)**: Checks parse errors, collider mismatches (e.g. decorative props blocking paths), softlocks, frame times, and jump-arc reachability.
   - **Lens 2 (`game_feel_camera`)**: Checks spawn safety (no damage within first 5s), camera floor/wall clipping, landing visibility, and bounce/wind forgiveness.
   - **Lens 3 (`fun_level_design`)**: Checks mission variety, 3-beat pacing, landmark guidance, and enemy attack telegraphs.
   - **Lens 4 (`visual_hierarchy_retro`)**: Enforces `Docs/ART_RETRO_HIERARCHY.md` — calm matte environments (`roughness >= 0.85, metallic = 0.0`), high-pop collectibles/enemies, universal toon shader, and `2px` outlines on gameplay actors only (using `abs(PROJECTION_MATRIX[1][1])` for Vulkan).
3. **Merge & Fix**: Each critic writes `Docs/progress/critique/<track>/r<round>_<lens>/REPORT.md` with a score `/10` and `[BLOCKER]`, `[MAJOR]`, `[MINOR]` tags. The Fixer agent resolves all Blockers and Majors in order.
4. **Bot Completion Proof**: A scripted bot driving the real player controller must collect 100% of the area's objectives on a fresh profile (`--profile=bot_verify`).
