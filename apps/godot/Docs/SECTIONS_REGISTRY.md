# 40 Micro-Section Registry

- **Total:** 40 | **LIVE:** 40 | **PLANNED:** 0
- **Max lines per SEC script:** `<= 150` lines (enforced cleanly without semicolon cramming).

| Section ID | Name | Track | Script | Lines | State | Role |
|---|---|---|---|---:|---|---|
| `SEC-C01` | RunGovernor | `core_net` | `scripts/core/run_governor.gd` | 21 | **LIVE** | Instance cap (MAX=1) & background mode |
| `SEC-C02` | DevTools | `core_net` | `scripts/core/dev_tools.gd` | 94 | **LIVE** | CLI --capture, --cam, --goto, --section |
| `SEC-C03` | SectionRegistry | `core_net` | `scripts/core/section_registry.gd` | 76 | **LIVE** | Live 40-section catalog & path health |
| `SEC-C04` | AuthCoursesAPI | `core_net` | `scripts/net/auth_courses_api.gd` | 96 | **LIVE** | Auth & Courses REST slice |
| `SEC-C05` | VillageStudyAPI | `core_net` | `scripts/net/village_study_api.gd` | 121 | **LIVE** | Fortress, Shields & FSRS-6 Attempts API |
| `SEC-C06` | RaidsDuelsAPI | `core_net` | `scripts/net/raids_duels_api.gd` | 103 | **LIVE** | Raids, Bosses, Clans, Leagues & Duels WS |
| `SEC-G01` | GridMath | `village_grid` | `scripts/village/grid_math.gd` | 149 | **LIVE** | 40x40 isometric cell math & bounds |
| `SEC-G02` | GridBoardRenderer | `village_grid` | `scripts/village/grid_board_renderer.gd` | 75 | **LIVE** | Sculpted PBR island, MultiMesh forest, water & edit-grid |
| `SEC-G03` | CameraPanZoom | `village_grid` | `scripts/village/camera_pan_zoom.gd` | 131 | **LIVE** | Isometric Clash Pan/Zoom camera & shadow sync |
| `SEC-G04` | BuildDragPlacement | `village_grid` | `scripts/village/build_drag_placement.gd` | 143 | **LIVE** | Edit-mode drag & green/red overlap check |
| `SEC-G05` | LayoutPersistence | `village_grid` | `scripts/village/layout_persistence.gd` | 93 | **LIVE** | Save/load village JSON & DAG seed layout |
| `SEC-B01` | CoreArchiveTownHall | `buildings_upgrades` | `scripts/buildings/core_archive_townhall.gd` | 21 | **LIVE** | 4x4 Town Hall Lv0->Lv3 |
| `SEC-B02` | GoldMineVault | `buildings_upgrades` | `scripts/buildings/gold_mine_vault.gd` | 29 | **LIVE** | 3x3 Gold Collector & Vault Lv0->Lv3 |
| `SEC-B03` | FocusExtractorTank | `buildings_upgrades` | `scripts/buildings/focus_extractor_tank.gd` | 29 | **LIVE** | 3x3 Focus Extractor & Tank Lv0->Lv3 |
| `SEC-B04` | FortressWallTile | `buildings_upgrades` | `scripts/buildings/fortress_wall_tile.gd` | 131 | **LIVE** | 1x1 Auto-tiling N/S/E/W Wall Lv0->Lv3 |
| `SEC-B05` | StudyBarracksCamp | `buildings_upgrades` | `scripts/buildings/study_barracks_camp.gd` | 16 | **LIVE** | 3x3 Study Barracks & Army Camp Lv0->Lv3 |
| `SEC-B06` | ClanHallShieldDome | `buildings_upgrades` | `scripts/buildings/clan_hall_shield_dome.gd` | 55 | **LIVE** | 3x3 Clan Hall, Shield Dome & Brownout |
| `SEC-D01` | QuillArcherSpire | `buildings_upgrades` | `scripts/buildings/defenses/quill_archer_spire.gd` | 14 | **LIVE** | 3x3 Archer Tower (Ground+Air) |
| `SEC-D02` | InkCannon | `buildings_upgrades` | `scripts/buildings/defenses/ink_cannon.gd` | 14 | **LIVE** | 3x3 Heavy Cannon (Ground only) |
| `SEC-D03` | RuneMortar | `buildings_upgrades` | `scripts/buildings/defenses/rune_mortar.gd` | 26 | **LIVE** | 3x3 Mortar (4-cell deadzone + splash) |
| `SEC-D04` | HiddenProctorTesla | `buildings_upgrades` | `scripts/buildings/defenses/hidden_proctor_tesla.gd` | 51 | **LIVE** | 2x2 Stealth Tower (4-cell trigger) |
| `SEC-UI01` | ResourceTopBar | `study_economy_ui` | `scripts/ui/resource_top_bar.gd` | 99 | **LIVE** | Top bar: Gold, Focus, Trophies, Shield |
| `SEC-UI02` | BuildingActionBar | `study_economy_ui` | `scripts/ui/building_action_bar.gd` | 72 | **LIVE** | Bottom bar: Info, Upgrade, Move, Repair |
| `SEC-UI03` | StudyFlashcardDrawer | `study_economy_ui` | `scripts/ui/study_flashcard_drawer.gd` | 141 | **LIVE** | FSRS-6 Study & Sandbox challenge UI |
| `SEC-UI04` | ArmyTrainingDrawer | `study_economy_ui` | `scripts/ui/army_training_drawer.gd` | 106 | **LIVE** | Army queue & troop housing drawer |
| `SEC-UI05` | SectionDebugInspector | `study_economy_ui` | `scripts/ui/section_debug_inspector.gd` | 135 | **LIVE** | F2 Section Inspector overlay (live debug) |
| `SEC-SIM01` | AStarWallPathfinder | `troops_raid_sim` | `scripts/combat/astar_wall_pathfinder.gd` | 107 | **LIVE** | 40x40 A* solver with 12.0 wall cost |
| `SEC-SIM02` | RaidTickStarSolver | `troops_raid_sim` | `scripts/combat/raid_tick_star_solver.gd` | 149 | **LIVE** | 20 Hz combat loop & 0-3 star settlement |
| `SEC-T01` | SquireFootman | `troops_raid_sim` | `scripts/troops/squire_footman.gd` | 8 | **LIVE** | Melee Footman (Space 1) |
| `SEC-T02` | LexiArcher | `troops_raid_sim` | `scripts/troops/lexi_archer.gd` | 8 | **LIVE** | Ranged Archer (fires over walls) |
| `SEC-T03` | ThesisBreaker | `troops_raid_sim` | `scripts/troops/thesis_breaker.gd` | 16 | **LIVE** | Wall Breaker (x4 wall damage) |
| `SEC-T04` | FormulaGiant | `troops_raid_sim` | `scripts/troops/formula_giant.gd` | 14 | **LIVE** | Defense-first Giant Tank (Space 5) |
| `SEC-T05` | GlyphDrake | `troops_raid_sim` | `scripts/troops/glyph_drake.gd` | 15 | **LIVE** | Flying Drake (ignores walls) |
| `SEC-T06` | FieldScholarHero | `troops_raid_sim` | `scripts/troops/field_scholar_hero.gd` | 38 | **LIVE** | Hero Unit with Rally Recital (+30% aura) |
| `SEC-M01` | PvpRaidController | `raid_modes` | `scripts/modes/pvp_raid_controller.gd` | 124 | **LIVE** | PvP Raid (/raids/scanner -> /settle) |
| `SEC-M02` | BossRaidController | `raid_modes` | `scripts/modes/boss_raid_controller.gd` | 73 | **LIVE** | Shared-HP Chapter Boss (/bosses) |
| `SEC-M03` | LiveDuelController | `raid_modes` | `scripts/modes/live_duel_controller.gd` | 139 | **LIVE** | Live 90s Duel WS & Clan War |
| `SEC-FX01` | JuiceEventBus | `vfx_audio` | `scripts/fx/juice.gd` | 99 | **LIVE** | Decoupled Juice VFX & pooled Audio bus |
| `SEC-FX02` | PbrMiniatureMaterial | `vfx_audio` | `scripts/fx/pbr_miniature_material.gd` | 148 | **LIVE** | Stylized Miniature PBR, Vertex AO, CLIP_UNDERGROUND & LOD binder |
| `SEC-FX03` | CombatBuildParticles | `vfx_audio` | `scripts/fx/combat_build_particles.gd` | 147 | **LIVE** | Additive halos, shockwaves, trails & rubble |
