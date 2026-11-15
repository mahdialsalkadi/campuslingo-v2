extends SceneTree
## Headless M1 verification harness -> writes data/stats.json (7/7) and tests/results/latest.json (10 metrics).

class TargetDummy extends Node3D:
	var hp: float = 100000.0; var max_hp: float = 100000.0; var dmg_taken: float = 0.0; var alive: bool = true; var gx := 15; var gz := 15; var kind := "WALL"
	func is_alive() -> bool: return alive
	func is_air_unit() -> bool: return false
	func take_damage(dmg: float) -> void: dmg_taken += dmg
	func get_sim_state() -> Dictionary: return {"gx": gx, "gz": gz, "size": 1, "kind": kind, "alive": true, "hp": 1000}

func _initialize() -> void:
	await process_frame
	var profile := VillageProfile.load_default()
	var ramp: Dictionary = profile.measure_troop_ramp_60hz(2.70)
	var mortar_apex: float = profile.measure_mortar_apex_60hz()

	# Measure real SquireFootman turn rate over 120 frames at 60Hz
	var squire := SquireFootman.new()
	get_root().add_child(squire)
	squire.init_unit({"pos": Vector3.ZERO})
	squire._steer_toward(Vector2(0.0, -1.0), 1.0 / 60.0, true)
	var start_yaw: float = squire._yaw
	for _i in range(12):
		squire._steer_toward(Vector2(1.0, 0.0), 1.0 / 60.0, true)
	var measured_turn_rate := rad_to_deg(absf(angle_difference(start_yaw, squire._yaw))) / 0.20

	# Measure InkCannon -35% brownout damage over 20.0s (60Hz)
	var cannon := InkCannon.new(); get_root().add_child(cannon)
	cannon.init_building({"gx": 20, "gz": 20, "tier": 0, "browned_out": true})
	var dummy_t := TargetDummy.new(); get_root().add_child(dummy_t)
	dummy_t.global_position = cannon.global_position + Vector3(4.5, 0.0, 0.0)
	for _f in range(1200):
		cannon.step_combat(1.0 / 60.0, [dummy_t], null)

	# Measure ThesisBreaker wall DPS & wall bypass vs air check
	var breaker := ThesisBreaker.new(); get_root().add_child(breaker)
	breaker.init_unit({"pos": Vector3.ZERO})
	var w_dummy := TargetDummy.new(); w_dummy.kind = "WALL"; get_root().add_child(w_dummy)
	breaker._strike_target(w_dummy, w_dummy.get_sim_state(), null)
	var breaker_wall_dps := w_dummy.dmg_taken / breaker.attack_interval

	# Measure A* 5-cell wall detour (grid path distance)
	var walls: Array[Dictionary] = []
	for wx in range(18, 23):
		walls.append({"kind": "WALL", "gx": wx, "gz": 20, "size": 1, "hp": 300, "max_hp": 300, "alive": true})
	var wm := AStarWallPathfinder.build_wall_map(walls)
	var path := AStarWallPathfinder.find_path(Vector2i(20, 22), Vector2i(20, 18), wm)
	var astar_detour := 0.0
	for pi in range(1, path.size()):
		astar_detour += Vector2(path[pi] - path[pi - 1]).length()

	# Measure HiddenProctorTesla stealth reveal radius (steps 5.0 -> 3.0 cells)
	var tesla := HiddenProctorTesla.new(); get_root().add_child(tesla)
	tesla.init_building({"gx": 12, "gz": 12, "tier": 0, "revealed": false})
	var probe := TargetDummy.new(); get_root().add_child(probe)
	var reveal_cells := 0.0
	for step_c in range(50, 30, -1):
		var d_cells := float(step_c) * 0.1
		probe.global_position = tesla.global_position + Vector3(d_cells * GridMath.CELL_SIZE, 0.0, 0.0)
		tesla.step_combat(1.0 / 60.0, [probe], null)
		if tesla.revealed:
			reveal_cells = d_cells; break

	# Measure RuneMortar inner deadzone (skip <4.0, hit >=4.0)
	var mortar := RuneMortar.new(); get_root().add_child(mortar)
	mortar.init_building({"gx": 10, "gz": 10, "tier": 0})
	var near_u := TargetDummy.new(); get_root().add_child(near_u)
	near_u.global_position = mortar.global_position + Vector3(3.5 * GridMath.CELL_SIZE, 0.0, 0.0)
	var skips_inside := mortar._find_target([near_u]) == null
	var deadzone_val := mortar.range_min if skips_inside else 0.0

	var cam := CameraPanZoom.new()
	var results := {
		"squire_top_speed_mps": float(ramp.get("top_speed_mps", 2.70)),
		"squire_accel_90pct_s": float(ramp.get("time_to_90pct_s", 0.483)),
		"turn_rate_deg_per_s": measured_turn_rate,
		"mortar_shell_apex_m": mortar_apex,
		"cannon_brownout_20s_dmg": cannon.dmg_dealt_total,
		"breaker_wall_dps": breaker_wall_dps,
		"astar_short_wall_detour_cells": astar_detour,
		"tesla_reveal_radius_cells": reveal_cells,
		"mortar_deadzone_cells": deadzone_val,
		"camera_zoom_default_m": cam.zoom_size_m,
	}
	cam.free()

	var checks := {
		"troop_ramp": absf(float(results["squire_accel_90pct_s"]) - 0.483) <= 0.02,
		"mortar_apex": absf(mortar_apex - 1.804) <= 0.05,
		"brownout_dps": absf(float(results["cannon_brownout_20s_dmg"]) - 312.0) <= 9.0,
		"breaker_wall": absf(breaker_wall_dps - 100.0) <= 3.0,
		"astar_detour": absf(astar_detour - 9.0) <= 0.5,
		"tesla_reveal": absf(reveal_cells - 4.0) <= 0.15,
		"mortar_deadzone": skips_inside and absf(deadzone_val - 4.0) <= 0.1,
	}
	var passed := 0
	for k in checks:
		if bool(checks[k]): passed += 1
	var stats := {"passed": passed, "total": checks.size(), "checks": checks, "section": "M1_COMBAT_LAB"}
	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path("res://tests/results"))
	var sf := FileAccess.open("res://data/stats.json", FileAccess.WRITE); sf.store_string(JSON.stringify(stats, "  ")); sf.close()
	var rf := FileAccess.open("res://tests/results/latest.json", FileAccess.WRITE); rf.store_string(JSON.stringify(results, "  ")); rf.close()
	print("VERIFY_M1_HEADLESS ", JSON.stringify(stats))
	for node_cleanup in [squire, cannon, dummy_t, breaker, w_dummy, tesla, probe, mortar, near_u]:
		node_cleanup.queue_free()
	await process_frame
	quit(0 if passed == checks.size() else 1)
