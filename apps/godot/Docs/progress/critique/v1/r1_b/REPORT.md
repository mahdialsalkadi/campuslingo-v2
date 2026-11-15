# Critique Round 1 — Lens B: Game Feel, Isometric Camera, Audio & RTL UI Critic

**Score:** 8.0 / 10 (NEEDS FIXES BEFORE M4 SIGN-OFF)
**Artifacts Inspected:** `Docs/progress/m2/m2_study_rtl.png`, `Docs/progress/m3/m3_live_hud.png`, `Docs/progress/m4/m4_raid_action.png`, `scripts/main_village.gd`

## Verified Strengths
- Orthographic `-45°`/`+45°` pan inertia (`8.0/s`), `18m -> 54m` smooth zoom, `coc_edit_grid` edit-mode snapping toggle, AudioServer `Master/Music/SFX/UI` buses with `60ms` cooldowns, and Arabic RTL / English LTR HUD anchoring operate with zero overlap or clipping.

## Findings
- **[MAJOR] B-1 (Missing post-raid settlement summary toast in HUD):** Finishing a raid in `MainVillage._finish_raid()` updated `gold` and `trophies` silently without an in-game settlement confirmation banner in EN or AR RTL.
  - *Fix:* Display a localized 3.5-second settlement summary (`RAID +<G>G +<T>T` / `فوز +<G>ذ +<T>ك`) in `ResourceTopBar.shield_lbl` before restoring shield status cleanly.
- **[MAJOR] B-2 (Damaged-building feedback legibility during Raid combat):** Between single-frame strike flashes, attacked towers and halls lacked persistent health bars to signal target burn-down.
  - *Fix:* Add lightweight camera-facing billboard `RaidHpBar` indicators (`green -> amber -> red`) lazily on damaged non-wall buildings (`CombatParityMath.update_damage_bar`), hidden cleanly on raid reset.
