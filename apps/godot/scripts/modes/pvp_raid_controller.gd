class_name PvpRaidController
extends Node
## SEC-M01: Async PvP Raid orchestration (/raids/scanner -> /start -> combat ticks -> /settle).

const SECTION_ID := "SEC-M01"

signal raid_updated(stars: int, pct: int, remain_s: float)
signal raid_settled(settlement: Dictionary)

var _api: RaidsDuelsAPI = null
var _solver: RaidTickStarSolver = RaidTickStarSolver.new()
var active: bool = false
var raid_id: String = ""
var candidates: Array = []
var active_candidate: Dictionary = {}
var last_settlement: Dictionary = {}
var _cached_walls: int = -1
var _wall_map: Dictionary = {}


func bind_services(api: RaidsDuelsAPI, solver: RaidTickStarSolver = null) -> void:
	_api = api
	_solver = solver if solver != null else RaidTickStarSolver.new()


func scan_targets() -> Array:
	if _api == null:
		return []
	var res := await _api.scan_raids()
	var data: Variant = res.get("data")
	candidates = (data as Dictionary).get("candidates", []) if data is Dictionary else []
	return candidates


func start_next_raid(buildings: Array) -> Dictionary:
	if candidates.is_empty():
		await scan_targets()
	var chosen: Dictionary = {}
	for c in candidates:
		if c is Dictionary and not bool((c as Dictionary).get("raid_immune", false)):
			chosen = c as Dictionary
			break
	if chosen.is_empty() and not candidates.is_empty():
		chosen = candidates[0] as Dictionary
	active_candidate = chosen
	raid_id = "offline-raid-1"
	if _api != null and not chosen.is_empty():
		var res := await _api.start_raid(String(chosen.get("defender_user_id", "")), String(chosen.get("node_id", "")), bool(chosen.get("has_revenge_window", false)), chosen.get("revenge_source_raid_id", null))
		var rd: Variant = (res.get("data") as Dictionary).get("raid") if res.get("data") is Dictionary else null
		if rd is Dictionary:
			raid_id = String((rd as Dictionary).get("raid_id", (rd as Dictionary).get("id", raid_id)))
	if _solver != null:
		_solver.reset()
	_cached_walls = -1
	_wall_map.clear()
	active = true
	return {"ok": true, "raid_id": raid_id, "target": chosen}


func step_raid(dt: float, buildings: Array, troops: Array, walls_host: Node3D, vfx: Node) -> bool:
	if not active or _solver == null:
		return false
	var prev_walls := _count_alive_walls(buildings)
	if prev_walls != _cached_walls:
		_cached_walls = prev_walls
		_wall_map = AStarWallPathfinder.build_wall_map(buildings.map(func(b): return b.get_sim_state()))
	for b in buildings:
		if b.has_method("step_combat"):
			b.step_combat(dt, troops, vfx)
	for t in troops:
		if t.has_method("step_unit"):
			t.step_unit(dt, buildings, _wall_map, vfx)
	var after_walls := _count_alive_walls(buildings)
	if after_walls != prev_walls:
		_cached_walls = after_walls
		_wall_map = AStarWallPathfinder.build_wall_map(buildings.map(func(b): return b.get_sim_state()))
		if walls_host != null:
			FortressWallTile.rebuild_wall_batches(buildings, walls_host)
	CombatParityMath.sync_solver_score(_solver, dt, buildings)
	var rem: float = CombatParityMath.RAID_TIME_LIMIT_S - _solver.elapsed_s
	raid_updated.emit(_solver.stars, int(roundf(_solver.destruction_pct)), rem)
	return _solver.ended


func deploy_troop(cell: Vector2i, buildings: Array, drag: BuildDragPlacement, army: ArmyTrainingDrawer, troops_root: Node, troops: Array, vfx: Node) -> bool:
	var wp := GridMath.cell_to_world(cell.x, cell.y, 1)
	if not GridMath.can_deploy_troop(wp, drag.build_occupancy(buildings)):
		return false
	var uid := army.active_unit
	if not army.consume_deploy_unit(uid):
		return false
	troops.append(VillageFactory.spawn_troop(uid, wp, troops_root))
	vfx.spawn_event("troop_deploy", wp)
	return true


func settle_current_raid(sandbox_solved: bool = true, drills_passed: int = -1) -> Dictionary:
	active = false
	var pct: float = _solver.destruction_pct if _solver != null else 55.0
	var stars: int = _solver.stars if _solver != null else 2
	var drills := clampi(stars, 1, 3) if drills_passed < 0 else clampi(drills_passed, 0, 3)
	var pts := clampi(int(roundf(pct * 0.4)) + stars * 8, 10, 60)
	if _api != null and raid_id != "":
		var res: Dictionary = await _api.settle_raid(raid_id, sandbox_solved, drills, pts)
		var data: Variant = res.get("data")
		if data is Dictionary and (data as Dictionary).get("settlement") is Dictionary:
			last_settlement = (data as Dictionary)["settlement"] as Dictionary
			raid_settled.emit(last_settlement)
			return last_settlement
	last_settlement = CombatParityMath.evaluate_raid_settlement_local(sandbox_solved, drills, pts)
	raid_settled.emit(last_settlement)
	return last_settlement


func finish_and_reset(troops: Array, buildings: Array, wall_batches: Node3D, army: ArmyTrainingDrawer) -> Dictionary:
	var res := await settle_current_raid(true)
	for t in troops:
		if is_instance_valid(t):
			t.queue_free()
	troops.clear()
	for b in buildings:
		if b.has_method("reset_for_raid"):
			b.call("reset_for_raid")
	CombatParityMath.clear_damage_bars(buildings)
	FortressWallTile.rebuild_wall_batches(buildings, wall_batches)
	army.set_raid_mode(false)
	return res


func _count_alive_walls(buildings: Array) -> int:
	var n := 0
	for b in buildings:
		if b is FortressWallTile and (b as FortressWallTile).alive:
			n += 1
	return n


func spawn_demo_wave(troops_root: Node, troops: Array, vfx: Node) -> void:
	var wave := [["formula_giant", 2], ["thesis_breaker", 4], ["squire_footman", 8], ["lexi_archer", 6], ["glyph_drake", 2], ["field_scholar_hero", 1]]
	var idx := 0
	for entry in wave:
		for _i in range(int(entry[1])):
			var gx := 12 + (idx % 14) if idx < 14 else 9
			var gz := 28 if idx < 14 else 14 + (idx - 14) * 2
			var t := VillageFactory.spawn_troop(String(entry[0]), GridMath.cell_to_world(gx, gz, 1), troops_root)
			troops.append(t)
			if t is FieldScholarHero:
				(t as FieldScholarHero).trigger_rally(troops, vfx)
			idx += 1
