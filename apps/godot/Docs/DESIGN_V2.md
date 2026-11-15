# Game Design Document (GDD v2) — CampusLingo: Village Clash

> **Game Title:** CampusLingo: Village Clash (قرية المعرفة القابلة للتطوير)  
> **Genre:** Isometric Upgradable Village Builder & Raid Strategy integrated with Spaced Repetition (`FSRS-6`)  
> **Perspective:** Clash-of-Clans-style Isometric Angled View (3D meshes on a 40×40 logical grid, stylized Toon lighting, crisp Pan & Zoom camera)  
> **Backend Authority:** `campuslingo-server` (`http://127.0.0.1:18080`)  
> **Status:** Approved (`M0_PASSED -> M1_READY`) with `D016` 40-section micro-modular breakdown (`Docs/SECTIONS_REGISTRY.md`)

---

## 1. High-Concept Pitch

Every concept in a CampusLingo course becomes a living, upgradable structure inside your personal isometric **Knowledge Village**. By answering FSRS-6 challenges and strengthening your memory (`stability` & `retrievability`), you harvest Gold and Focus Elixir to construct defenses, upgrade your Town Hall (`Core Archive`) and towers from wooden outposts to golden bastions, repair decaying (`Brownout`) structures, and train a tactical army to raid rival student villages, join Clan Wars, and topple shared-HP Chapter Bosses.

---

## 2. Core Gameplay Loop

1. **Manage & Upgrade Village (Home View):**
   - Inspect your `40×40` isometric grid (`Pitch 45°`, `Yaw 45°`), pan smoothly, and zoom in/out.
   - Enter **Village Edit Mode** to select buildings, drag them to new valid tiles (green footprint highlight = valid, red = blocked/overlapping), rotate walls, and save your defensive layout.
   - Buildings reflect real server state from `GET /v1/courses/{course_id}/fortress`:
     - **Tier (`-1/0..3`)** determines visual upgrade stage & base stats.
     - **Energy/Stability (`e_u_ticks`, `s_u_ticks`)** determines structure HP bar.
     - **Brownout (`is_brownout`)** visibly cracks and smokes the building, reducing tower fire rate until repaired by study.
     - **Active Protections (`ProtectionKind`)** render glowing sky-blue Shield Domes over immune structures (`NEW_NODE_GRACE`, `NEW_PLAYER_BUBBLE`, `POST_RAID_COOLDOWN`, etc.).
2. **Study to Fuel Economy & Repairs (`FSRS-6` Loop):**
   - Tap an upgradable or brownout building to open the **Study & Upgrade Drawer** (`/v1/courses/{course_id}/content` -> `/v1/courses/{course_id}/challenges/{id}/attempts` -> `/v1/attempts/{id}/answer`).
   - Correct answers raise FSRS-6 mastery (triggering server shield recompute `/shields/recompute`), repair brownout damage, unlock higher Town Hall / Defense tiers, and reward **Gold** & **Focus Elixir**.
3. **Train Army & Raid Rivals (PvP & PvE Modes):**
   - Train 5 distinct troop units in your Barracks using earned Focus Elixir.
   - **Scout & Raid Rival Villages (`PvP`):** Query `/v1/courses/{id}/raids/scanner`, choose an eligible non-immune target, start a raid (`/raids/start`), deploy troops along the outer deployment border + solve interactive challenge drills inside the raid timer (`120,000 ms` ceiling) to win `0★–3★`, Trophies, Gold chests, and Clan Points via `/raids/settle`.
   - **Shared Boss Encounters (`/bosses`):** Deploy siege squads alongside rated challenge strikes (`0%–200%` multiplier) to defeat Chapter Bosses (`Apex Warden`) and claim boss chests.
   - **Live 90s Duels (`/duels/ws`) & Clans (`/leagues`, `/clans`):** Compete in weekly 10-tier League Brackets (`<= 30` players) and Clan Stability Wars.

---

## 3. Upgradable Building Catalog (6 Building Families)

All buildings snap to integer grid coordinates on the `40×40` village floor and visually evolve across 4 tiers matching CampusLingo tiers (`Lv0` Foundation, `Lv1` Wood/Stone, `Lv2` Fortified Brick, `Lv3` Gilded Citadel).

| Family | Footprint | Server Mapping | Upgrade Visual Progression (Lv 0 → Lv 3) | Gameplay Role |
| :--- | :--- | :--- | :--- | :--- |
| **1. Core Archive (Town Hall)** | `4×4` | Course root / highest milestone node | Wooden Tent → Timber Keep → Stone Hall → Golden Astrolabe Citadel | Central high-HP headquarters (`1★` when destroyed); caps max level of other buildings. |
| **2. Knowledge Collectors & Vaults** | `3×3` | Concept nodes (`PIPELINE` / `TRADEOFF`) | Bare Dig Site → Quarry Cart → Steam Pump → Glowing Crystal Extractor | Produces & stores Gold and Focus Elixir; primary raid loot targets. |
| **3. Defensive Towers (4 Subtypes)** | `2×2` / `3×3` | Mastered course nodes (`AXIOM`, `SPATIAL`, `CONTRAST`, `TRAP`) | wooden scaffold → single barrel/bow → reinforced twin turret → rune-imbued spire | Defends village: **Quill Archer Spire** (ground+air single target), **Ink Cannon** (heavy ground), **Rune Mortar** (splash with 4-cell deadzone), **Hidden Proctor** (pops up only when troops enter 4-cell trigger radius). |
| **4. Fortress Walls** | `1×1` | Defensive perimeter (auto-tiling `N/S/E/W`) | Stake Fence → Cobblestone Wall → Iron-Braced Masonry → Arcane Rampart | Channels ground attackers, protects Core Archive & Vaults; destroyed fastest by `Thesis Breakers`. |
| **5. Study Barracks & Army Camp** | `3×3` | Sandbox / Practice progress | Campfire Rug → Canvas Tent → Timber Hall → Bannered Academy | Unlocks higher-tier troops & increases raid housing capacity (`20` → `40` → `60` → `80`). |
| **6. Clan Hall & Shield Beacon** | `3×3` | `/clans` & `ProtectionKind` shields | Signal Post → Watchtower → Obsidian Spire → Beacon Dome | Displays League tier badge, active Clan War banners, and projects visible dome overlays when `raid_immune` holds. |

---

## 4. Raid Army: 5 Troop Units + 1 Field Hero

| Unit Name | Housing | Damage Type | Preferred Target | Special Mechanic |
| :--- | :--- | :--- | :--- | :--- |
| **Squire Footman** | `1` | Melee Single | Nearest Structure | Fast, low-cost swarm attacker. |
| **Lexi Archer** | `1` | Ranged Single (Ground+Air) | Nearest Structure | Shoots ink bolts over `1×1` Fortress Walls. |
| **Thesis Breaker** | `2` | Area Wall Siege | **Walls (`×4` dmg)** | Runs directly to enclosed wall segments (`A*` target weight biased to enclosures) and detonates. |
| **Formula Giant** | `5` | Heavy Melee | **Defenses First** | High-HP tank that absorbs Cannon/Mortar fire while protecting Archers. |
| **Glyph Drake** | `8` | Flying Ranged Breath | Nearest Structure | Flies directly over Fortress Walls; targeted only by Archer Spires & Hidden Proctors. |
| **Field Scholar Hero** | `Hero (Free)` | Melee + Aura | Nearest / Manual Rally | Activates **"Rally Recital"** once per raid: boosts nearby troop attack & move speed by `+30%` for `6.0 s`. |

---

## 5. Village Visual Style & Color Palettes (Hex)

Follows a **Clash-of-Clans Stylized Miniature PBR Hierarchy** ([COC_VISUAL_PILLAR_SPEC.md](COC_VISUAL_PILLAR_SPEC.md)) with calibrated `Albedo + NormalGL + ORM (AO/Roughness/Metallic) + Vertex AO`, soft ground contact shadow decals, `BG_SKY` ACES glow + additive FX halos, `LOD0/1/2` visibility ranges, sculpted terrain with animated coastal water, edit-only tactical grid lines, and **zero** cartoon black outlines. Each course theme tints your village accents:

| Theme / Context | Terrain Primary | Terrain Alt | Grid Line | Building Base | Accent / Trim | Healthy HP | Brownout / Hazard | Shield Dome |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Daylight Home Village (Default / MACRO_POLIS)** | `#5FAE6E` | `#4E9A5D` | `#CBE896` | `#E8DCC2` | `#FBBF24` | `#34D399` | `#F59E0B` | `#38BDF8` |
| **Bio-System Meadows (`BIO_SYSTEM`)** | `#4FA87B` | `#3B8662` | `#A7F3D0` | `#D9F2E6` | `#2DD4BF` | `#5EEAD4` | `#F97316` | `#38BDF8` |
| **Cyber-Grid Silicon Courtyard (`CYBER_GRID`)** | `#2B3A67` | `#1E294D` | `#38BDF8` | `#C7D2FE` | `#22D3EE` | `#34D399` | `#F43F5E` | `#60A5FA` |
| **Arcane-Archive Courtyard (`ARCANE_ARCHIVE`)** | `#5B4A8A` | `#44366B` | `#C4B5FD` | `#EDE9FE` | `#A78BFA` | `#34D399` | `#F472B6` | `#818CF8` |
| **Raid Twilight Alert (`PvP / Boss`)** | `#3A4E3F` | `#2B3B2F` | `#FCA5A5` | `#F3E8D2` | `#FF4D6D` | `#FFD166` | `#FF006E` | `#B5179E` |

---

## 6. Star Victory & Raid Progression Rules

Every PvP or Practice Raid awards up to **3 Stars** synced cleanly with backend drill verification:
- **1★:** Destroy `>= 50%` of enemy village HP OR destroy the **Core Archive (Town Hall)**.
- **2★:** Destroy `>= 50%` AND destroy the **Core Archive** (mirrors backend `drills_passed >= 2` threshold for raid victory).
- **3★:** `100%` village destruction within `RAID_SESSION_CEILING_MS` (`120.0 s`) + `drills_passed == 3` (`APEX_RAID_CHEST`).
