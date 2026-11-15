extends SceneTree
## Headless verification gate for M2 (LayoutPersistence, BuildDragPlacement, SEC-B01..06, SEC-T06, Modes).


func _initialize() -> void:
	call_deferred("_run_all")


func _run_all() -> void:
	var results := {}
	results["persistence_atomic"] = _test_persistence()
	results["drag_overlap_bounds"] = _test_drag_placement()
	results["upgrades_housing_walls"] = _test_buildings_and_walls()
	results["hero_rally_buff"] = _test_hero_rally()
	results["boss_duel_raid_parity"] = _test_modes_parity()
	FortressWallTile._mesh_cache.clear()
	var passed := 0
	for k in results.keys():
		if bool(results[k]):
			passed += 1
	print("VERIFY_M2_HEADLESS ", JSON.stringify({"section": "M2_VILLAGE_MODES", "passed": passed, "total": results.size(), "checks": results}))
	quit(0 if passed == results.size() else 1)


func _test_persistence() -> bool:
	var cid := "m2_test_course"
	var p := LayoutPersistence.layout_path(cid)
	var gp := ProjectSettings.globalize_path(p)
	for suf in ["", ".bak", ".tmp"]:
		if FileAccess.file_exists(gp + suf):
			DirAccess.remove_absolute(gp + suf)
	var starter := LayoutPersistence.build_starter_village([])
	var ok1 := LayoutPersistence.save_layout(cid, starter)
	var ok2 := LayoutPersistence.save_layout(cid, starter)
	var loaded := LayoutPersistence.load_layout(cid)
	var has_bak := FileAccess.file_exists(gp + ".bak")
	for suf in ["", ".bak", ".tmp"]:
		if FileAccess.file_exists(gp + suf):
			DirAccess.remove_absolute(gp + suf)
	return ok1 and ok2 and has_bak and loaded.size() == starter.size()


func _test_drag_placement() -> bool:
	var drag := BuildDragPlacement.new()
	var th := CoreArchiveTownHall.new()
	th.init_building({"gx": 18, "gz": 18, "tier": 2})
	var gm := GoldMineVault.new()
	gm.init_building({"gx": 10, "gz": 10, "tier": 1})
	var buildings := [th, gm]
	var valid_move := drag.try_move(gm, 12, 12, buildings)
	var overlap_reject := not drag.try_move(gm, 18, 18, buildings)
	var border_reject := not drag.try_move(gm, 1, 10, buildings)
	drag.free(); th.free(); gm.free()
	return valid_move and overlap_reject and border_reject


func _test_buildings_and_walls() -> bool:
	var th := CoreArchiveTownHall.new()
	root.add_child(th)
	th.init_building({"gx": 18, "gz": 18, "tier": 1})
	var th_up := th.upgrade_tier(3) and th.tier == 3 and th.max_hp > 2000.0
	var camp := StudyBarracksCamp.new()
	root.add_child(camp)
	camp.init_building({"gx": 14, "gz": 21, "tier": 0})
	var cap0 := camp.housing_capacity()
	camp.upgrade_tier(3)
	var cap3 := camp.housing_capacity()
	var host := Node3D.new()
	root.add_child(host)
	var w1 := FortressWallTile.new()
	w1.init_building({"gx": 15, "gz": 15, "tier": 1})
	var w2 := FortressWallTile.new()
	w2.init_building({"gx": 16, "gz": 15, "tier": 1})
	w1.upgrade_tier(3)
	w2.upgrade_tier(3)
	FortressWallTile.rebuild_wall_batches([w1, w2], host)
	var mm_count := 0
	for ch in host.get_children():
		if ch is MultiMeshInstance3D and (ch as MultiMeshInstance3D).multimesh != null:
			mm_count += (ch as MultiMeshInstance3D).multimesh.instance_count
	var w_hp := int(w1.max_hp)
	root.remove_child(th); root.remove_child(camp); root.remove_child(host)
	th.free(); camp.free(); w1.free(); w2.free(); host.free()
	return th_up and cap0 == 20 and cap3 == 80 and w_hp == 900 and mm_count >= 5


func _test_hero_rally() -> bool:
	var hero := FieldScholarHero.new()
	var foot := SquireFootman.new()
	root.add_child(hero)
	root.add_child(foot)
	hero.init_unit({"pos": GridMath.cell_to_world(20, 20, 1)})
	foot.init_unit({"pos": GridMath.cell_to_world(22, 20, 1)})
	var buffed := hero.trigger_rally([hero, foot], null)
	var rm := float(foot.get("_rally_mult"))
	var rt := float(foot.get("_rally_timer"))
	root.remove_child(hero); root.remove_child(foot)
	hero.free(); foot.free()
	return buffed == 2 and is_equal_approx(rm, 1.30) and rt >= 5.9


func _test_modes_parity() -> bool:
	var s_win := CombatParityMath.evaluate_raid_settlement_local(true, 2, 40)
	var b1 := BossRaidController.eval_boss_strike_damage(1, false, 60, 300)
	var b4 := BossRaidController.eval_boss_strike_damage(4, true, 60, 100)
	var duel := LiveDuelController.new()
	duel.arena = {"opponent": {"core_hp": 400, "tower_hp": [180, 180]}, "remaining_ms": 90000}
	duel.send_strike(0, 0, true)
	var opp_thp: Array = (duel.arena["opponent"] as Dictionary)["tower_hp"]
	duel.free()
	return bool(s_win["raid_succeeded"]) and int(b1["damage"]) == 0 and int(b4["damage"]) == 100 and int(b4["overkill_damage"]) == 20 and int(opp_thp[0]) == 85
