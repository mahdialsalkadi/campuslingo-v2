---
name: rea-physics-reverse
description: Formulates exact numerical kinematics, discrete-step corrections, binary domain symbol verification, and simulation parity equations into Docs/MOVEMENT_REFERENCE.md, scenes/combat/village_profile.tres, and automated parity tests.
---
# Movement & Simulation Physics Specification & Verification Skill (REA Workflow)

## 1. Why Numerical Specs Beat Vague Prompts
Prompting an agent to "make troops move and shoot like Clash of Clans" produces uncalibrated speeds and inconsistent tick drift. Always drive combat/camera code from `Docs/MOVEMENT_REFERENCE.md`, `private_research/scripts/derive.py`, and `scenes/combat/village_profile.tres`.

## 2. Scale & Discrete-to-Continuous Integration (Godot `20 Hz` / `60 Hz`)
- **Unit Scale**: `1 cell = 1.50 m = 192 reference sub-units (u)` (`1 u = 7.8125 mm`).
- **Binary Verification**: Inspect compiled domain symbols (`nm -C ../../target/debug/campuslingo-server`) to confirm `evaluate_raid_settlement`, `evaluate_boss_strike`, `evaluate_raid_targetability`, `evaluate_league_rollover`, `compute_clan_stability_points`, and `split_protection_predicates`.
- **Discrete-Step Correction**: Add `+0.70 m/s` (`+0.47 m/s` frame-step + `+0.23 m/s` Godot velocity-first order) to `RuneMortar` shell launch speed (`10.31 m/s` under `-28.12 m/s²` gravity -> `1.804 m` apex), and initialize `20 Hz` tower timers at `0.50 × shot_interval` (`village_profile.tres`).
- **Troop Ramp**: Start accel `7.73 m/s²` tapering to `2.50 m/s²`, turn rate `337.50 °/s`, braking `28.10 m/s²`.

## 3. Automated Parity Enforcement (`<= ±3.0%`)
- Regenerate derivations after changing any domain constant: `python3 private_research/scripts/derive.py`
- Run full Gate M1 checks + parity table: `python3 tests/run_village_tests.py` and `python3 tests/run_movement_tests.py`
