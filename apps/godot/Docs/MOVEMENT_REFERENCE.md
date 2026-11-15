# Movement & Simulation Reference (`M1` REA Research — CampusLingo: Village Clash)

Generated from compiled backend binary inspection (`target/debug/campuslingo-server`) and discrete-to-continuous derivation (`private_research/scripts/derive.py` → `private_research/derived_reference.json`).

This file holds **factual behaviour only**: numbers, timing windows and rules, described in our own words. It contains no external binary disassembly or copied source code.

## How the numbers were obtained

1. **Compiled Backend Binary Inspection (`nm -C`)**: Inspected `/home/mahdi/campuslingo-v2/target/debug/campuslingo-server` (`28,882,264` bytes, `59` symbol hits, `6` core domain entrypoints verified: `evaluate_raid_settlement`, `evaluate_boss_strike`, `evaluate_raid_targetability`, `evaluate_league_rollover`, `compute_clan_stability_points`, `split_protection_predicates`).
2. **Domain Spec Cross-Check**: Cross-checked every combat constant against `crates/domain/src/raids_leagues.rs`, `crates/domain/src/shields.rs`, and `Docs/VILLAGE_AND_RAID_SPEC.md`. All checked constants agreed with zero conflicts.
3. **Discrete-to-Continuous Derivation (`private_research/scripts/derive.py`)**: Simulated continuous 60 Hz Godot kinematics (velocity-first integration `vy -= g*dt; y += vy*dt`, quadratic troop acceleration ramp) and 20 Hz deterministic RTS combat ticking against 10 Hz / 30 Hz reference steps, exporting ground-truth targets to `private_research/derived_reference.json`.

Confidence: **High** = verified directly in compiled domain binary + Rust crate source. **Med** = visual/feel timing rule tuned for RTS responsiveness. **Derived** = measured output of our 60 Hz / 20 Hz integrator in `private_research/scripts/derive.py`.

---

## Scale and time conversion

- Grid arena runs across `40 × 40` cells where **`1 cell = 1.50 m = 192 reference sub-units (u)`**, giving **`1 u = 1.50 / 192 = 7.8125 mm`**.
- Combat rules tick at **`20 Hz` (`dt = 0.05 s`)** while projectile arcs and camera smoothing integrate at **`60 Hz` (`dt = 0.01667 s`)** compared to a `10 Hz` / `30 Hz` reference loop.
- Never scale gravity or launch velocity independently when adjusting arena scale.

### Discrete-step correction (Mortar Shells & Shot Windows)

- **Parabolic Projectile Correction**: Under Godot's 60 Hz velocity-first update (`vy -= g * dt; y += vy * dt`), launching without discrete step compensation loses vertical apex compared to move-then-gravity integration. Adding **`+0.70 m/s`** (`+0.47 m/s` frame-step + `+0.23 m/s` Godot integration order) raises launch velocity to **`10.31 m/s`** under **`-28.12 m/s²`** gravity, reaching **`1.804 m` measured apex height** in `0.717 s` flight time (`uncorrected = 1.642 m`).
- **20 Hz Shot Phase Offset**: Initializing defense shot timers at **`0.5 × shot_interval`** aligns first-shot quantization with continuous DPS curves, yielding **`312.0` exact damage over `20.0 s`** under `-35%` Brownout (`24.0 × 0.65 × 20.0 = 312.0`, `0.0%` drift).

---

## Troop Kinematics (60 Hz Godot Scale)

| Troop Unit | Cells/s | Top Speed (`m/s`) | Accel Curve (`m/s²`) | Time to 90% (`s`) | Braking (`m/s²`) | Turn Rate (`°/s`) | Conf. |
| --- | ---: | ---: | --- | ---: | ---: | ---: | --- |
| `SquireFootman` (`SEC-T01`) | `1.8` | **`2.70`** | `7.73 → 2.50` | **`0.483`** | `28.10` | **`337.50`** | High / Derived |
| `LexiArcher` (`SEC-T02`) | `1.9` | **`2.85`** | `7.73 → 2.50` | **`0.517`** | `28.10` | `337.50` | High / Derived |
| `ThesisBreaker` (`SEC-T03`) | `2.4` | **`3.60`** | `7.73 → 2.50` | **`0.650`** | `28.10` | `337.50` | High / Derived |
| `FormulaGiant` (`SEC-T04`) | `1.1` | **`1.65`** | `7.73 → 2.50` | **`0.300`** | `28.10` | `337.50` | High / Derived |
| `GlyphDrake` (`SEC-T05`) | `1.4` | **`2.10`** | `7.73 → 2.50` | **`0.383`** | `28.10` | `337.50` | High / Derived |

---

## Defense Mechanics, A* Weights & Isometric Camera

| Rule / System | Reference Target | Godot Config (`village_profile.tres`) | Conf. |
| --- | --- | --- | --- |
| `A*` short wall detour vs breach | Walks around ≤12 extra cells; picks lowest-HP wall (`9` cells measured) | `wall_cost = 12.0 × (0.75 + 0.25 × hp_ratio)` | High / Derived |
| `ThesisBreaker` vs Wall | `25` base DPS, `×4.0` (`100.0` DPS vs Wall), bias `2.0` | Driven by `SEC-T03` + `SEC-SIM01` | High |
| `RuneMortar` (`SEC-D03`) | Inner deadzone `< 4.0` cells skipped; splash `r=2.0` cells; interval `3.5 s`; apex `1.804 m` | Driven by `SEC-D03` + `village_profile.tres` | High / Derived |
| `HiddenProctorTesla` (`SEC-D04`) | Hidden until troop `<= 4.0` cells | `stealth_reveal_radius = 4.0` | High |
| `InkCannon` (`SEC-D02`) Brownout | Ground-only (skips `GlyphDrake`); `-35%` fire rate (`0.65×`), cap `0.70 × TierHP` | `312.0` dmg over `20.0 s` | High |
| Isometric Camera (`SEC-G03`) | Pitch `-45.0°`, Yaw `+45.0°`, zoom `18.0..54.0 m` (default `34.0 m`), damping `8.0 /s` | Driven by `village_profile.tres` | Med |

---

## Automated Parity Enforcement

Implemented in `scripts/combat/village_profile.gd`, `scenes/combat/village_profile.tres`, `scripts/combat/raid_tick_star_solver.gd`, and `scripts/village/camera_pan_zoom.gd`. Run after any physics/combat change:

```bash
python3 tests/run_village_tests.py
python3 tests/run_movement_tests.py
```

Every metric in `tests/results/latest.json` must pass within **`±3.0%`** tolerance against `private_research/derived_reference.json`.
