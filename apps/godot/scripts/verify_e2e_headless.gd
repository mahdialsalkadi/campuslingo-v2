extends SceneTree
## Gate M4 E2E Playthrough & 40-Troop Raid Stress Benchmark (FPS, draw calls, p95 step ms).

var _main: Node = null
var _elapsed_total_s: float = 0.0


func _initialize() -> void:
	call_deferred("_run_e2e")


func _run_e2e() -> void:
	var ud := DirAccess.open("user://")
	if ud != null:
		for fn in ud.get_files():
			if fn.begins_with("village_layout_") and fn.ends_with(".json"):
				ud.remove(fn)
	var checks := {}
	var packed := load("res://scenes/MainGame.tscn") as PackedScene
	_main = packed.instantiate()
	root.add_child(_main)
	await process_frame
	await process_frame

	var buildings: Array = _main.get("_buildings")
	checks["village_seeded"] = buildings.size() >= 10

	var sel_b: Node = null
	var bo_b: Node = null
	for b in buildings:
		if b.has_method("get_sim_state"):
			var st: Dictionary = b.get_sim_state()
			if String(st.get("archetype", "")) == "gold_mine_vault":
				sel_b = b
			elif bool(st.get("browned_out", false)):
				bo_b = b

	if sel_b != null:
		var tier_before := int(sel_b.get("tier"))
		_main.call("_on_upgrade_building", sel_b)
		checks["upgrade_persisted"] = int(sel_b.get("tier")) == tier_before + 1
	else:
		checks["upgrade_persisted"] = false

	if bo_b != null:
		_main.call("_on_repair_building", bo_b)
		_main.call("_on_study_submitted", {"correct": true, "fsrs_rating": 4, "overcharged": true})
		checks["brownout_repaired"] = not bool(bo_b.get("browned_out"))
	else:
		checks["brownout_repaired"] = false

	_main.call("_on_mode_requested", "edit")
	await process_frame
	var drag := _main.get_node("BuildDragPlacement") as BuildDragPlacement
	var moved := drag != null and sel_b != null and drag.try_move(sel_b, 14, 14, buildings)
	checks["edit_mode_move"] = moved
	_main.call("_on_mode_requested", "village")
	await process_frame

	await _main.call("_on_mode_requested", "raid")
	var troops_root: Node = _main.get_node("Troops")
	var troops: Array = _main.get("_troops")
	var vfx: Node = _main.get_node("CombatBuildParticles")
	var army := _main.get_node("HudCanvas/CampusHUD/ArmyTrainingDrawer") as ArmyTrainingDrawer
	var wave := [
		["squire_footman", 16], ["lexi_archer", 10], ["thesis_breaker", 6],
		["formula_giant", 4], ["glyph_drake", 3], ["field_scholar_hero", 1],
	]
	var spawn_idx := 0
	var hero: FieldScholarHero = null
	for entry in wave:
		for _c in range(int(entry[1])):
			var gx := 6 + (spawn_idx % 28)
			var gz := 6 if spawn_idx < 28 else 32
			var wp := GridMath.cell_to_world(gx, gz, 1)
			var t := VillageFactory.spawn_troop(String(entry[0]), wp, troops_root)
			troops.append(t)
			if t is FieldScholarHero:
				hero = t as FieldScholarHero
			spawn_idx += 1
	checks["spawned_40_troops"] = troops.size() == 40
	var buffed := hero.trigger_rally(troops, vfx) if hero != null else 0
	checks["hero_rally_triggered"] = buffed >= 1

	var pvp := _main.get_node("Services/PvpRaidController") as PvpRaidController
	var wall_batches: Node3D = _main.get_node("WallBatches")
	var step_ms: Array[float] = []
	for _tick in range(180):
		var t0 := Time.get_ticks_usec()
		pvp.step_raid(0.05, buildings, troops, wall_batches, vfx)
		var elapsed := float(Time.get_ticks_usec() - t0) / 1000.0
		step_ms.append(elapsed)
	await process_frame
	step_ms.sort()
	var p50 := step_ms[int(step_ms.size() * 0.50)]
	var p95 := step_ms[int(step_ms.size() * 0.95)]
	var mx := step_ms[step_ms.size() - 1]
	var solver := pvp._solver
	var pct: float = solver.destruction_pct if solver != null else 0.0
	checks["combat_progressed"] = pct > 0.0
	checks["p95_under_16ms"] = p95 < 16.67

	var dc := int(Performance.get_monitor(Performance.RENDER_TOTAL_DRAW_CALLS_IN_FRAME))
	var prims := int(Performance.get_monitor(Performance.RENDER_TOTAL_PRIMITIVES_IN_FRAME))
	var fps := Engine.get_frames_per_second()
	await _main.call("_finish_raid")
	checks["raid_reset_clean"] = troops.is_empty() and String(_main.get("mode")) == "village"

	var passed := 0
	for k in checks.keys():
		if bool(checks[k]):
			passed += 1
	var report := {
		"passed": passed, "total": checks.size(), "checks": checks,
		"troops": 40, "ticks": step_ms.size(),
		"p50_step_ms": snappedf(p50, 0.001), "p95_step_ms": snappedf(p95, 0.001), "max_step_ms": snappedf(mx, 0.001),
		"destruction_pct": snappedf(pct, 0.1), "draw_calls": dc, "primitives": prims, "engine_fps": fps,
	}
	print("VERIFY_E2E_M4 ", JSON.stringify(report))
	_main.queue_free()
	FortressWallTile._mesh_cache.clear()
	ApiHttpClient._shared = null
	quit(0 if passed == checks.size() else 1)
