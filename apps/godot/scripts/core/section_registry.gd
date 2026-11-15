extends Node
## SEC-C03: SectionRegistry (2nd Autoload).
## Live index of all 40 Micro-Sections (`SEC-*`) across the 7 tracks (D016).

const SECTION_ID := "SEC-C03"

const DEFINITIONS: Array[Dictionary] = [
	{"id": "SEC-C01", "name": "RunGovernor", "track": "core_net", "path": "scripts/core/run_governor.gd", "role": "Instance cap (MAX=1) & background mode"},
	{"id": "SEC-C02", "name": "DevTools", "track": "core_net", "path": "scripts/core/dev_tools.gd", "role": "CLI --capture, --cam, --goto, --section"},
	{"id": "SEC-C03", "name": "SectionRegistry", "track": "core_net", "path": "scripts/core/section_registry.gd", "role": "Live 40-section catalog & path health"},
	{"id": "SEC-C04", "name": "AuthCoursesAPI", "track": "core_net", "path": "scripts/net/auth_courses_api.gd", "role": "Auth & Courses REST slice"},
	{"id": "SEC-C05", "name": "VillageStudyAPI", "track": "core_net", "path": "scripts/net/village_study_api.gd", "role": "Fortress, Shields & FSRS-6 Attempts API"},
	{"id": "SEC-C06", "name": "RaidsDuelsAPI", "track": "core_net", "path": "scripts/net/raids_duels_api.gd", "role": "Raids, Bosses, Clans, Leagues & Duels WS"},
	{"id": "SEC-G01", "name": "GridMath", "track": "village_grid", "path": "scripts/village/grid_math.gd", "role": "40x40 isometric cell math & bounds"},
	{"id": "SEC-G02", "name": "GridBoardRenderer", "track": "village_grid", "path": "scripts/village/grid_board_renderer.gd", "role": "Sculpted PBR island, MultiMesh forest, water & edit-grid"},
	{"id": "SEC-G03", "name": "CameraPanZoom", "track": "village_grid", "path": "scripts/village/camera_pan_zoom.gd", "role": "Isometric Clash Pan/Zoom camera & shadow sync"},
	{"id": "SEC-G04", "name": "BuildDragPlacement", "track": "village_grid", "path": "scripts/village/build_drag_placement.gd", "role": "Edit-mode drag & green/red overlap check"},
	{"id": "SEC-G05", "name": "LayoutPersistence", "track": "village_grid", "path": "scripts/village/layout_persistence.gd", "role": "Save/load village JSON & DAG seed layout"},
	{"id": "SEC-B01", "name": "CoreArchiveTownHall", "track": "buildings_upgrades", "path": "scripts/buildings/core_archive_townhall.gd", "role": "4x4 Town Hall Lv0->Lv3"},
	{"id": "SEC-B02", "name": "GoldMineVault", "track": "buildings_upgrades", "path": "scripts/buildings/gold_mine_vault.gd", "role": "3x3 Gold Collector & Vault Lv0->Lv3"},
	{"id": "SEC-B03", "name": "FocusExtractorTank", "track": "buildings_upgrades", "path": "scripts/buildings/focus_extractor_tank.gd", "role": "3x3 Focus Extractor & Tank Lv0->Lv3"},
	{"id": "SEC-B04", "name": "FortressWallTile", "track": "buildings_upgrades", "path": "scripts/buildings/fortress_wall_tile.gd", "role": "1x1 Auto-tiling N/S/E/W Wall Lv0->Lv3"},
	{"id": "SEC-B05", "name": "StudyBarracksCamp", "track": "buildings_upgrades", "path": "scripts/buildings/study_barracks_camp.gd", "role": "3x3 Study Barracks & Army Camp Lv0->Lv3"},
	{"id": "SEC-B06", "name": "ClanHallShieldDome", "track": "buildings_upgrades", "path": "scripts/buildings/clan_hall_shield_dome.gd", "role": "3x3 Clan Hall, Shield Dome & Brownout"},
	{"id": "SEC-D01", "name": "QuillArcherSpire", "track": "buildings_upgrades", "path": "scripts/buildings/defenses/quill_archer_spire.gd", "role": "3x3 Archer Tower (Ground+Air)"},
	{"id": "SEC-D02", "name": "InkCannon", "track": "buildings_upgrades", "path": "scripts/buildings/defenses/ink_cannon.gd", "role": "3x3 Heavy Cannon (Ground only)"},
	{"id": "SEC-D03", "name": "RuneMortar", "track": "buildings_upgrades", "path": "scripts/buildings/defenses/rune_mortar.gd", "role": "3x3 Mortar (4-cell deadzone + splash)"},
	{"id": "SEC-D04", "name": "HiddenProctorTesla", "track": "buildings_upgrades", "path": "scripts/buildings/defenses/hidden_proctor_tesla.gd", "role": "2x2 Stealth Tower (4-cell trigger)"},
	{"id": "SEC-UI01", "name": "ResourceTopBar", "track": "study_economy_ui", "path": "scripts/ui/resource_top_bar.gd", "role": "Top bar: Gold, Focus, Trophies, Shield"},
	{"id": "SEC-UI02", "name": "BuildingActionBar", "track": "study_economy_ui", "path": "scripts/ui/building_action_bar.gd", "role": "Bottom bar: Info, Upgrade, Move, Repair"},
	{"id": "SEC-UI03", "name": "StudyFlashcardDrawer", "track": "study_economy_ui", "path": "scripts/ui/study_flashcard_drawer.gd", "role": "FSRS-6 Study & Sandbox challenge UI"},
	{"id": "SEC-UI04", "name": "ArmyTrainingDrawer", "track": "study_economy_ui", "path": "scripts/ui/army_training_drawer.gd", "role": "Army queue & troop housing drawer"},
	{"id": "SEC-UI05", "name": "SectionDebugInspector", "track": "study_economy_ui", "path": "scripts/ui/section_debug_inspector.gd", "role": "F2 Section Inspector overlay (live debug)"},
	{"id": "SEC-SIM01", "name": "AStarWallPathfinder", "track": "troops_raid_sim", "path": "scripts/combat/astar_wall_pathfinder.gd", "role": "40x40 A* solver with 12.0 wall cost"},
	{"id": "SEC-SIM02", "name": "RaidTickStarSolver", "track": "troops_raid_sim", "path": "scripts/combat/raid_tick_star_solver.gd", "role": "20 Hz combat loop & 0-3 star settlement"},
	{"id": "SEC-T01", "name": "SquireFootman", "track": "troops_raid_sim", "path": "scripts/troops/squire_footman.gd", "role": "Melee Footman (Space 1)"},
	{"id": "SEC-T02", "name": "LexiArcher", "track": "troops_raid_sim", "path": "scripts/troops/lexi_archer.gd", "role": "Ranged Archer (fires over walls)"},
	{"id": "SEC-T03", "name": "ThesisBreaker", "track": "troops_raid_sim", "path": "scripts/troops/thesis_breaker.gd", "role": "Wall Breaker (x4 wall damage)"},
	{"id": "SEC-T04", "name": "FormulaGiant", "track": "troops_raid_sim", "path": "scripts/troops/formula_giant.gd", "role": "Defense-first Giant Tank (Space 5)"},
	{"id": "SEC-T05", "name": "GlyphDrake", "track": "troops_raid_sim", "path": "scripts/troops/glyph_drake.gd", "role": "Flying Drake (ignores walls)"},
	{"id": "SEC-T06", "name": "FieldScholarHero", "track": "troops_raid_sim", "path": "scripts/troops/field_scholar_hero.gd", "role": "Hero Unit with Rally Recital (+30% aura)"},
	{"id": "SEC-M01", "name": "PvpRaidController", "track": "raid_modes", "path": "scripts/modes/pvp_raid_controller.gd", "role": "PvP Raid (/raids/scanner -> /settle)"},
	{"id": "SEC-M02", "name": "BossRaidController", "track": "raid_modes", "path": "scripts/modes/boss_raid_controller.gd", "role": "Shared-HP Chapter Boss (/bosses)"},
	{"id": "SEC-M03", "name": "LiveDuelController", "track": "raid_modes", "path": "scripts/modes/live_duel_controller.gd", "role": "Live 90s Duel WS & Clan War"},
	{"id": "SEC-FX01", "name": "JuiceEventBus", "track": "vfx_audio", "path": "scripts/fx/juice.gd", "role": "Decoupled Juice VFX & pooled Audio bus"},
	{"id": "SEC-FX02", "name": "PbrMiniatureMaterial", "track": "vfx_audio", "path": "scripts/fx/pbr_miniature_material.gd", "role": "Stylized Miniature PBR, Vertex AO, CLIP_UNDERGROUND & LOD binder"},
	{"id": "SEC-FX03", "name": "CombatBuildParticles", "track": "vfx_audio", "path": "scripts/fx/combat_build_particles.gd", "role": "Additive halos, shockwaves, trails & rubble"}
]


func get_all_sections() -> Array[Dictionary]:
	var out: Array[Dictionary] = []
	for item in DEFINITIONS:
		var copy: Dictionary = item.duplicate()
		var res_path := "res://" + String(copy["path"])
		copy["exists"] = ResourceLoader.exists(res_path) or FileAccess.file_exists(res_path)
		copy["state"] = "LIVE" if bool(copy["exists"]) else "PLANNED"
		out.append(copy)
	return out


func get_section(sec_id: String) -> Dictionary:
	var target := sec_id.to_upper()
	for item in get_all_sections():
		if String(item["id"]) == target:
			return item
	return {}


func get_summary_counts() -> Dictionary:
	var live_count := 0
	var all := get_all_sections()
	for item in all:
		if bool(item["exists"]):
			live_count += 1
	return {"total": all.size(), "live": live_count, "planned": all.size() - live_count}
