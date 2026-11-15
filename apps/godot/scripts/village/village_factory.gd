class_name VillageFactory
extends RefCounted
## Spawns modular PBR building archetypes, troops, and handles layout/upgrade workflows.

static func spawn_building(spec: Dictionary, parent: Node) -> Node:
	var b := _new_building_node(String(spec.get("archetype", "gold_mine_vault")))
	parent.add_child(b)
	b.call("init_building", spec)
	return b


static func spawn_troop(unit_id: String, world_pos: Vector3, parent: Node) -> TroopAgentBase:
	var t := _new_troop_node(unit_id)
	parent.add_child(t)
	t.init_unit({"pos": world_pos})
	return t

static func _new_building_node(arch: String) -> Node:
	match arch:
		"core_archive_townhall": return CoreArchiveTownHall.new()
		"focus_extractor_tank": return FocusExtractorTank.new()
		"study_barracks_camp": return StudyBarracksCamp.new()
		"clan_hall_shield_dome": return ClanHallShieldDome.new()
		"fortress_wall_tile": return FortressWallTile.new()
		"quill_archer_spire": return QuillArcherSpire.new()
		"ink_cannon": return InkCannon.new()
		"rune_mortar": return RuneMortar.new()
		"hidden_proctor_tesla": return HiddenProctorTesla.new()
		_: return GoldMineVault.new()

static func _new_troop_node(unit_id: String) -> TroopAgentBase:
	match unit_id:
		"lexi_archer": return LexiArcher.new()
		"thesis_breaker": return ThesisBreaker.new()
		"formula_giant": return FormulaGiant.new()
		"glyph_drake": return GlyphDrake.new()
		"field_scholar_hero": return FieldScholarHero.new()
		_: return SquireFootman.new()

static func populate_village(course_id: String, root: Node, wall_batches: Node3D, army: ArmyTrainingDrawer, fortress_nodes: Array = []) -> Array:
	var saved := LayoutPersistence.load_layout(course_id)
	var specs := saved if not saved.is_empty() else LayoutPersistence.build_starter_village(fortress_nodes)
	for child in root.get_children():
		child.queue_free()
	var list: Array = []
	for raw in specs:
		if raw is Dictionary:
			list.append(spawn_building(raw as Dictionary, root))
	FortressWallTile.rebuild_wall_batches(list, wall_batches)
	army.set_housing_cap(total_housing_cap(list))
	return list

static func try_upgrade_building(b: Node, gold: int, buildings: Array, wall_batches: Node3D, course_id: String, army: ArmyTrainingDrawer, bar: BuildingActionBar) -> int:
	var cost := int(b.call("upgrade_cost")) if b.has_method("upgrade_cost") else 250
	if gold < cost or not b.has_method("upgrade_tier"):
		return -1
	if not b.call("upgrade_tier"):
		return -1
	FortressWallTile.rebuild_wall_batches(buildings, wall_batches)
	LayoutPersistence.save_layout(course_id, buildings)
	army.set_housing_cap(total_housing_cap(buildings))
	bar.bind_building(b)
	return gold - cost

static func try_move_building(b: Node, screen_pos: Vector2, cam: Camera3D, drag: BuildDragPlacement, buildings: Array, wall_batches: Node3D, course_id: String) -> bool:
	var sz := int(b.get_sim_state().get("size", 1))
	var c := drag.screen_to_cell(cam, screen_pos, sz)
	if not drag.try_move(b, c.x, c.y, buildings):
		return false
	if b is FortressWallTile:
		FortressWallTile.rebuild_wall_batches(buildings, wall_batches)
	LayoutPersistence.save_layout(course_id, buildings)
	return true

static func collect_building_yield(b: Node) -> Vector2i:
	if b.has_method("collect_gold"):
		return Vector2i(int(b.call("collect_gold")), 0)
	if b.has_method("collect_focus"):
		return Vector2i(0, int(b.call("collect_focus")))
	return Vector2i.ZERO

static func sync_backend_village(auth_api: AuthCoursesAPI, study_api: VillageStudyAPI, raid_api: RaidsDuelsAPI, buildings: Array) -> Dictionary:
	await auth_api.connect_demo_student()
	var cid := auth_api.get_course_id()
	var f := await study_api.fetch_fortress(cid)
	if f.get("data") is Dictionary:
		var fnodes: Array = (f["data"] as Dictionary).get("nodes", [])
		for idx in range(mini(fnodes.size(), buildings.size())):
			if fnodes[idx] is Dictionary and buildings[idx].has_method("apply_shield_snapshot"):
				var sh: Variant = (fnodes[idx] as Dictionary).get("shield")
				if sh is Dictionary:
					buildings[idx].call("apply_shield_snapshot", sh)
	var ec := await raid_api.get_economy(cid)
	var bal: Dictionary = (ec["data"] as Dictionary).get("balance", {}) if ec.get("data") is Dictionary else {}
	return bal

static func total_housing_cap(buildings: Array) -> int:
	var total := 0
	for b in buildings:
		if b is StudyBarracksCamp:
			total += (b as StudyBarracksCamp).housing_capacity()
	return maxi(total, 40)

static func find_building_at_cell(buildings: Array, cell: Vector2i) -> Node:
	for b in buildings:
		if not b.has_method("get_sim_state"):
			continue
		var s: Dictionary = b.get_sim_state()
		var sz := int(s.get("size", 1))
		var gx := int(s.get("gx", 0))
		var gz := int(s.get("gz", 0))
		if cell.x >= gx and cell.x < gx + sz and cell.y >= gz and cell.y < gz + sz:
			return b
	return null

static func apply_cli_flags(village: Node, top_bar: ResourceTopBar) -> void:
	for ch in village.get_node("HudCanvas/SectionDebugInspector").get_children():
		if ch is Button:
			(ch as Button).offset_top = -130.0
			(ch as Button).offset_bottom = -94.0
	var j := village.get_node_or_null("/root/Juice")
	if j != null:
		j.call("bind_scene_targets", village.get_node("CameraPanZoom"), village.get_node("CombatBuildParticles"))
	var args := PackedStringArray(OS.get_cmdline_user_args())
	if OS.has_feature("web"):
		var q := str(JavaScriptBridge.eval("window.location.search", true))
		if q.find("local=1") == -1 and not args.has("--live-connect"): args.append("--live-connect")
		if q.find("demo=1") != -1: args.append("--demo-raid")
		if q.find("rtl=1") != -1: args.append("--rtl")
		if q.find("study=1") != -1: args.append("--study")
		if q.find("edit=1") != -1: args.append("--show-edit-grid")
	for arg in args:
		if String(arg) == "--live-connect":
			await village.call("_sync_live_backend")
	for arg in args:
		var s := String(arg)
		if s == "--show-edit-grid":
			village.call("_on_mode_requested", "edit")
		elif s == "--study":
			village.call("_on_mode_requested", "study")
		elif s == "--rtl":
			top_bar.set_rtl(true)
		elif s.begins_with("--mode="):
			await village.call("_on_mode_requested", s.trim_prefix("--mode="))
		elif s == "--demo-raid":
			await village.call("_on_mode_requested", "raid")
			village.get_node("Services/PvpRaidController").call("spawn_demo_wave", village.get_node("Troops"), village.get("_troops"), village.get_node("CombatBuildParticles"))
