# REA Reverse-Engineering Reports, Clean-Room Architecture Plans & Captures

Complete archive of the static **REA (`reverse-engineer-anything`)** investigation, clean-room translation specifications, planning documents, visual critique renders, and extracted JSON evidence bundles generated during the **CampusLingo: Village Clash** ([`apps/godot`](../../apps/godot)) build.

## Planning & Analysis Reports
- [`coc_architecture_restart_plan.md`](./coc_architecture_restart_plan.md) — Clean-room `M0 → M5` restart architecture plan translating static REA findings (`libsupercell_clashofclans.so`, `libg.so`, `libscid_sdk.so`, `libapp.so`, `assets/shaders/`, `assets/sc3d/`) into `gl_compatibility` PBR shaders, 3-tier LODs, `AudioServer` buses, and modular `<=150L` GDScript systems.
- [`citadel_clash_planning_phase.md`](./citadel_clash_planning_phase.md) — Micro-section architecture (`40` isolated `SEC-*` modules), agent ownership contracts, and `M0 → M5` verification plan.
- [`m1_visual_showcase.md`](./m1_visual_showcase.md) — M1 visual showcase & 4-lens critique summary.
- [`verify_web_cdp.mjs`](./verify_web_cdp.mjs) — Headless Chromium CDP runtime verification script for the served Godot WebGL build.

## Godot Workspace Design & Milestone Specs (`apps/godot/Docs/`)
- [`COC_VISUAL_PILLAR_SPEC.md`](../../apps/godot/Docs/COC_VISUAL_PILLAR_SPEC.md) — `D018` stylized miniature PBR specification & clean-room asset policy (`$0` `CC0`/`MIT`/`OFL`)
- [`VILLAGE_AND_RAID_SPEC.md`](../../apps/godot/Docs/VILLAGE_AND_RAID_SPEC.md) — Grid scale, defense/troop formulas, and server parity spec
- [`MOVEMENT_REFERENCE.md`](../../apps/godot/Docs/MOVEMENT_REFERENCE.md) & [`private_research/derived_reference.json`](../../apps/godot/private_research/derived_reference.json) — Quantitative troop kinematics and mortar arc calibration
- [`DECISIONS.md`](../../apps/godot/Docs/DECISIONS.md), [`DESIGN_V2.md`](../../apps/godot/Docs/DESIGN_V2.md), [`TECH_CONTRACT.md`](../../apps/godot/Docs/TECH_CONTRACT.md), [`SECTIONS_REGISTRY.md`](../../apps/godot/Docs/SECTIONS_REGISTRY.md), [`STATE.md`](../../apps/godot/Docs/STATE.md), [`DEVLOG.md`](../../apps/godot/Docs/DEVLOG.md), and [`Docs/progress/`](../../apps/godot/Docs/progress/) (`m0`–`m5` captures & `critique/v1` audit reports)

## Screenshots & REA JSON Evidence
- [`screenshots/`](./screenshots) — 9 inspection & StoryCam PNG renders (`coc_combat_lab_cam_*`, `m0_*`, `m1_*`)
- [`evidence/`](./evidence/README.md) — Clean-room JSON REA evidence exports (`coc_apk_evidence.json`, `coc_jadx_evidence.json`, `coc_evidence_bundle.json`, plus 74 deduplicated per-operation REA step outputs under `evidence/steps/`). Raw `.apk` and `.so` binaries (`coc_arm64/`) are strictly excluded.
