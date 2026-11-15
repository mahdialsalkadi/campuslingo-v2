# Ready-to-Use Subagent Prompts for the 4-Lens Critique & Fix Loop

Every critique round writes `Docs/progress/critique/<AREA>/r<N>_<lens>/REPORT.md` with a numeric score `/10` and explicit `[BLOCKER]` / `[MAJOR]` / `[MINOR]` items. Gate `M4` passes only when all 4 lenses score `>= 8.5/10` with zero `[BLOCKER]` or `[MAJOR]`.

## 1. Lens A — QA & Server Formula Parity Critic
> Read [TECH_CONTRACT.md](../../Docs/TECH_CONTRACT.md) and [VILLAGE_AND_RAID_SPEC.md](../../Docs/VILLAGE_AND_RAID_SPEC.md). Inspect `<SCENE_PATH>` and its `SEC-*` scripts. Verify every simulation rule against `raids_leagues.rs`, `shields.rs`, and `derived_reference.json` (`1.5 m` cells, `[4..35]` buildable core, `0.5 × shot_interval` phase offset, `+0.70 m/s` mortar gravity compensation, `-35%` Brownout cadence, `0%/75%/100%/125%/200%` Boss strike scaling, `10/3/1` raid caps). Check atomic JSON layout persistence, script line counts (`<= 150L`), and zero leaked resources or runtime errors.

## 2. Lens B — Game Feel, Isometric Camera, Audio & RTL UI Critic
> Inspect `StoryCam_A`..`StoryCam_D` and a 120-frame gameplay clip in `<SCENE_PATH>`. Evaluate orthogonal pan inertia damping (`8.0 /s`), smooth zoom (`18 m -> 54 m`), crisp drag-placement snapping with `coc_edit_grid` toggle, AudioServer `Music/SFX/UI` voice pooling (`>= 60 ms` horde cooldown gating), and bilingual Arabic RTL / English LTR HUD legibility with zero clipped controls.

## 3. Lens C — Strategy Loop, Pathing & Progression Balance Critic
> Evaluate `<SCENE_PATH>` against [DESIGN_V2.md](../../Docs/DESIGN_V2.md). Confirm FSRS-6 study attempts directly repair `Brownout` structures (`e_u_ticks <= 6950`) and fund `Lv0->Lv3` upgrades, `A*` wall detour (`<= 12` cells) vs breach decisions create meaningful base-design choices, `RuneMortar` 4-cell deadzone invites close-range dives, and `HiddenProctorTesla` `clip_y_plane` reveals create real tactical surprise.

## 4. Lens D — Clash of Clans Stylized Miniature PBR & LOD Cohesion Critic
> Audit native viewport captures of `<SCENE_PATH>` against [COC_VISUAL_PILLAR_SPEC.md](../../Docs/COC_VISUAL_PILLAR_SPEC.md). Explicitly FAIL (`[BLOCKER]`) any instance of: (1) black comic ink outlines on units or structures, (2) flat N64/cartoon cel shading ignoring normal/ORM maps, (3) blocky toy character proportions or un-beveled primitive box buildings, (4) checkerboard grass floors or oversized solid orange ground discs, (5) floating structures lacking `coc_ground_shadow` contact skirts and baked vertex AO, (6) far-corner directional shadow cutoff under `54 m` zoom, or (7) unbatched individual wall/forest draw calls bypassing `MultiMeshInstance3D` or missing `LOD0/1/2` visibility ranges.
