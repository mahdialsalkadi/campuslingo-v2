# Village & Raid Numerical Specification (`M1` Verification Standard)

> Replaces `MOVEMENT_REFERENCE.md` (which belonged to the kit's 3D platformer example).  
> Every constant below is the authoritative target for **Gate `M1` (`run_village_tests.py`)** and must match within `±3%` simulation tolerance and exact integer parity with `/home/mahdi/campuslingo-v2/crates/domain/src/raids_leagues.rs` and `shields.rs`.

---

## 1. Isometric Grid & Camera Constants

| Parameter | Value | Notes |
| :--- | :--- | :--- |
| Grid dimensions | `40 × 40` cells | Logical integer cells `(0..39, 0..39)` |
| Cell physical size | `1.5 m × 1.5 m` | Total arena = `60.0 m × 60.0 m`, centered at `(0, 0, 0)` |
| Buildable village core | `[4..35, 4..35]` (`32 × 32` interior) | Outer `4`-cell border ring reserved exclusively as **Perimeter Troop Deploy Zone** |
| Isometric camera angles | `Pitch = -45.0°`, `Yaw = 45.0°` | Fixed Clash-of-Clans isometric orientation (no free vertical tumble) |
| Camera zoom range (FOV/Ortho size) | `18.0 m` (closest) to `54.0 m` (full village), default `34.0 m` | Smooth zoom wheel speed `2.5 m / notch` |
| Camera pan speed & bounds | Max speed `28.0 m/s`, inertia damping `8.0 /s` | Clamped to `[-34.0, +34.0]` on X and Z |
| Combat simulation tick | `20 Hz` (`dt = 0.05 s` fixed step) | Visual rendering interpolated smoothly at native 60 FPS |

---

## 2. Upgradable Building Stats & Mapping to CampusLingo Tiers

Building HP scales with `tier` (`0..3`) and CampusLingo FSRS-6 `e_u_ticks` / `is_brownout`:
- **Base HP Formula per Tier (`t ∈ {0, 1, 2, 3}`):** `TierHP = BaseHP × (1.0 + 0.45 × t)`
- **Brownout Penalty (`is_brownout == true`, `e_u_ticks <= 6950`):** Effective defensive fire rate reduced by `-35%` and max visual integrity capped at `0.70 × TierHP`.
- **Shield Immunity (`raid_immune == true`):** From `split_protection_predicates` in `shields.rs` (`tier == 0`, `state != ACTIVE`, or any `ProtectionKind` active).

| Building Archetype | Size (Cells) | Lv0 Base HP | Lv3 Max HP (`×2.35`) | Attack DPS (Lv1/Lv3) | Range (Cells) | Special Constraints |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| Core Archive (Town Hall) | `4×4` | `1200` | `2820` | `0` | `—` | Awards `1★` immediately when destroyed |
| Knowledge Collector / Vault | `3×3` | `450` | `1058` | `0` | `—` | Loot weighted `40%` of raid Gold |
| Study Barracks / Clan Hall | `3×3` | `500` | `1175` | `0` | `—` | Housing + Clan banner & shield dome source |
| Quill Archer Spire | `3×3` | `400` | `940` | `18 / 34` | `9.0` | Hits Ground + Air (`Glyph Drake`) |
| Ink Cannon | `3×3` | `460` | `1081` | `24 / 45` | `8.0` | Ground-only single target |
| Rune Mortar | `3×3` | `420` | `987` | `20 / 38` (Splash `r=2.0`) | `4.0 – 11.0` | **`4.0`-cell inner deadzone**; fires every `3.5 s` |
| Hidden Proctor Tesla | `2×2` | `320` | `752` | `32 / 58` | `6.5` | Hidden underground until troop enters `4.0` cells; hits Ground + Air |
| Fortress Wall Segment | `1×1` | `300` | `900` | `0` | `—` | Auto-connects orthogonally; blocks ground pathing unless broken |

---

## 3. Troop & Hero Combat Stats + `A*` Pathfinding

- **Pathfinding (`A*` on `40×40` Grid):**
  - Open grass/road cell cost: **`1.0`** (diagonal **`1.414`**).
  - Intact `Fortress Wall` cell cost for standard ground units: **`12.0`** (units walk around walls if detour <= 12 cells, otherwise stop and breach the lowest-cost wall on path).
  - `Thesis Breaker` wall cost bias: **`2.0`** and **Target Priority = Enclosed Walls** (`×4.0` damage multiplier vs `Wall`).
  - `Glyph Drake` air pathing: ignores all walls (`cost = 1.0` everywhere).

| Unit | HP | DPS | Attack Range (Cells) | Move Speed (Cells/s) | Target Layer | Priority Rule |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| Squire Footman | `110` | `14` | `0.6` | `1.8` | Ground | Nearest building (or blocking wall) |
| Lexi Archer | `48` | `12` | `3.5` | `1.9` | Ground | Nearest building (fires over walls) |
| Thesis Breaker | `75` | `25` (`100` vs Wall) | `0.6` | `2.4` | Ground | Enclosed Walls (`×4` bonus) |
| Formula Giant | `540` | `26` | `0.7` | `1.1` | Ground | Nearest active **Defense Tower** |
| Glyph Drake | `620` | `42` | `2.2` | `1.4` | Air | Nearest building (ignores walls) |
| Field Scholar Hero | `950` | `55` | `0.8` | `2.0` | Ground | Nearest target; Rally adds `+30%` atk/spd in `r=5.0` cells for `6.0 s` |

---

## 4. Authoritative Backend Parity (`campuslingo-server`)

Exact constants mirrored from `/home/mahdi/campuslingo-v2/crates/domain/src/raids_leagues.rs` & `shields.rs`:

| Constant / Rule | Exact Server Value | Enforcement |
| :--- | :--- | :--- |
| Raid Session Time Ceiling (`RAID_SESSION_CEILING_MS`) | `120,000 ms` (`120.0 s`) | Timer HUD + raid auto-settle |
| Daily Outbound Raid Cap (`MAX_OUTBOUND_RAIDS_PER_DAY`) | `10` | Shown on Raid Scanner UI |
| Daily Inbound Raid Cap (`MAX_INBOUND_RAIDS_PER_DAY`) | `3` | Checked by `/raids/scanner` |
| Daily Pair Raid Cap (`MAX_UNORDERED_PAIR_RAIDS_DAY`) | `1` | Checked before raid start |
| Daily Trophy Loss Cap (`MAX_DAILY_TROPHY_LOSS_FROM_RAIDS`) | `45` | Server settlement cap |
| Trophy Stakes (`BASE` / `REVENGE`) | `15` / `22` (`48 h` revenge window) | Raid preview banner |
| Raid Victory & Chest Tiers (`evaluate_raid_settlement`) | `sandbox_solved && drills_passed >= 2` -> Win; `3` drills -> `"APEX_RAID_CHEST"`, `2` -> `"STANDARD_RAID_CHEST"` | Raid end summary |
| Boss Strike Scaling (`evaluate_boss_strike`) | Rating `1=0%`, `2=75%`, `3=100%`, `4=125%`, `4+Overcharged=200%` of `base_damage`, clamped to `boss_hp_before` | Shared Boss Raid HUD |
| Clan Points per Win (`CLAN_RAID` / `CLAN_REVENGE`) | `25` / `38` | Clan War contribution UI |
| League Bracket Rules | Tiers `1..=10`, max `30` players, min `150` trophies to promote | League screen |
| FSRS-6 Shield Thresholds (`shields.rs`) | `E_ENTER_BROWNOUT=6950`, `E_INITIAL=7000`, `E_EXIT_BROWNOUT=7050`, `E_APEX=8500`, `S_PROTO=50000`, `S_APEX=210000` | Building Tier & Brownout visuals |

---

## 5. Gate `M1` Automated Verification Targets (`game/tests/run_village_tests.py`)

`M1` passes only when 100% of these deterministic checks succeed cleanly in headless mode (`±3%` on continuous combat timing/HP curves, exact equality on backend formulas):
1. `grid_snap_and_overlap`: Building placement (`4×4`, `3×3`, `2×2`, `1×1`) rejects out-of-bounds `[4..35]` or overlapping footprints and saves/reloads layout identically.
2. `upgrade_tier_and_brownout`: Building HP, mesh variant, and fire-rate match `TierHP` and `-35%` brownout penalty.
3. `deploy_zone_enforcement`: Troop deployment rejected inside occupied interior, accepted cleanly on perimeter tiles.
4. `astar_wall_detour_vs_breach`: Unit walks around short wall (`<= 12` cells extra path) and attacks lowest-HP wall segment when core is fully enclosed.
5. `defender_archetypes`: `Rune Mortar` skips targets `< 4.0` cells; `Hidden Proctor` stays hidden until troop distance `<= 4.0` cells; `Glyph Drake` ignores walls and is untargeted by `Ink Cannon`.
6. `raid_victory_and_stars`: Destruction `%` and Town Hall `1★` logic match `0★..3★` specification across seeded battles.
7. `backend_settlement_parity`: Local predictor matches `evaluate_raid_settlement` and `evaluate_boss_strike` ratings (`0%/75%/100%/125%/200%`) across all boundary cases.
