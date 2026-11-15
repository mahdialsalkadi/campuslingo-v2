# Critique Round 1 — Lens C: Strategy Loop, Pathing & Progression Balance Critic

**Score:** 8.1 / 10 (NEEDS FIXES BEFORE M4 SIGN-OFF)
**Artifacts Inspected:** `Docs/progress/m4/m4_raid_action.png`, `Docs/progress/m4/m4_playthrough.mp4`, `scripts/modes/pvp_raid_controller.gd`, `scripts/troops/troop_agent_base.gd`

## Verified Strengths
- FSRS-6 ratings `1..4` and Overcharge directly repair brownout towers, `GoldMineVault`/`FocusExtractorTank` upgrades (`Lv0->Lv3`) fund army training, `RuneMortar` 4-cell deadzone invites inside-ring dives, and `HiddenProctorTesla` surprises advancing squads cleanly.

## Findings
- **[MAJOR] C-1 (Synchronized A\* repath herd spikes during 40-troop waves):** All 37 ground units spawned at tick 0 with identical `0.25s` repath timers caused periodic CPU spikes (`p95 = 24.766ms > 16.67ms`) when running A\* on the same tick.
  - *Fix:* Stagger repath cooldowns across units (`0.45 + float(get_index() % 12) * 0.05`) and skip `FortressWallTile` before dictionary allocation in score sync.
- **[MAJOR] C-2 (`--demo-raid` showcase south assault framing):** Initial south spawn row (`gz = 31`) positioned southern Thesis Breakers and Squires near the bottom screen margin instead of actively engaging southern walls in camera view alongside the SW front.
  - *Fix:* Shift south showcase wave to `gx = 12..25, gz = 28` and west wave to `gx = 9, gz = 14..24` so Hero Rally, Giants, Drakes, Breakers, Squires, and Archers are visibly engaged across both fronts.
