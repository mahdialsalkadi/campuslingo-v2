extends SceneTree
## Gate M4 Real OpenGL (gl_compatibility) 40-Troop Raid Benchmark & Visual Capture.

func _initialize() -> void:
	call_deferred("_run_bench")

func _run_bench() -> void:
	var ud := DirAccess.open("user://")
	if ud != null:
		for fn in ud.get_files():
			if fn.begins_with("village_layout_") and fn.ends_with(".json"):
				ud.remove(fn)
	var main := (load("res://scenes/MainGame.tscn") as PackedScene).instantiate()
	root.add_child(main)
	await process_frame
	await process_frame
	await main.call("_on_mode_requested", "raid")
	var pvp := main.get_node("Services/PvpRaidController") as PvpRaidController
	var troops_root := main.get_node("Troops")
	var troops: Array = main.get("_troops")
	var buildings: Array = main.get("_buildings")
	var vfx := main.get_node("CombatBuildParticles")
	var wall_batches: Node3D = main.get_node("WallBatches")
	var wave := [["formula_giant", 4], ["thesis_breaker", 6], ["squire_footman", 16], ["lexi_archer", 10], ["glyph_drake", 3], ["field_scholar_hero", 1]]
	var idx := 0
	var hero: FieldScholarHero = null
	for entry in wave:
		for _c in range(int(entry[1])):
			var gx := 11 + (idx % 16) if idx < 28 else 8
			var gz := 27 + int(idx / 16) if idx < 28 else 13 + (idx - 28)
			var t := VillageFactory.spawn_troop(String(entry[0]), GridMath.cell_to_world(gx, gz, 1), troops_root)
			troops.append(t)
			if t is FieldScholarHero:
				hero = t as FieldScholarHero
			idx += 1
	if hero != null:
		hero.trigger_rally(troops, vfx)
	var step_ms: Array[float] = []
	for tick in range(110):
		var t0 := Time.get_ticks_usec()
		pvp.step_raid(0.05, buildings, troops, wall_batches, vfx)
		step_ms.append(float(Time.get_ticks_usec() - t0) / 1000.0)
		if tick % 3 == 0:
			await process_frame
	await process_frame
	root.get_texture().get_image().save_png("res://Docs/progress/m4/m4_bench.png")
	step_ms.sort()
	var dc := int(Performance.get_monitor(Performance.RENDER_TOTAL_DRAW_CALLS_IN_FRAME))
	var prims := int(Performance.get_monitor(Performance.RENDER_TOTAL_PRIMITIVES_IN_FRAME))
	var p50 := step_ms[int(step_ms.size() * 0.50)]
	var p95 := step_ms[int(step_ms.size() * 0.95)]
	var hp_bars := buildings.filter(func(b): var n := (b as Node).get_node_or_null("RaidHpBar"); return n != null and (n as Node3D).visible).size()
	var troop_cnt := troops.size()
	var pct := pvp._solver.destruction_pct
	await main.call("_finish_raid")
	var toast_txt := String((main.get_node("HudCanvas/CampusHUD/ResourceTopBar").get("shield_lbl") as Label).text)
	var ok := p95 < 16.67 and dc > 0 and hp_bars >= 1 and toast_txt.contains("RAID +")
	print("BENCH_GL_M4 ", JSON.stringify({"ok": ok, "troops": troop_cnt, "draw_calls": dc, "primitives": prims, "p50_step_ms": snappedf(p50, 0.001), "p95_step_ms": snappedf(p95, 0.001), "hp_bars_visible": hp_bars, "toast": toast_txt, "destruction_pct": snappedf(pct, 0.1)}))
	main.queue_free()
	FortressWallTile._mesh_cache.clear()
	ApiHttpClient._shared = null
	quit(0 if ok else 1)
