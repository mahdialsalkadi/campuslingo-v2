class_name CombatParityMath
extends RefCounted
## Client offline parity formulas matching crates/domain/src/raids_leagues.rs.

const RAID_TIME_LIMIT_S: float = 120.0
const BASE_RAID_TROPHY_STAKE: int = 15
const CLAN_RAID_WIN_POINTS: int = 25


static func evaluate_raid_settlement_local(sandbox_solved: bool, drills_passed: int, base_score_v2_points: int) -> Dictionary:
	var ok := sandbox_solved and drills_passed >= 2
	var pts := clampi(base_score_v2_points, 10, 5000)
	return {
		"status": "SETTLED_WIN" if ok else "SETTLED_LOSS",
		"raid_succeeded": ok,
		"attacker_trophy_gain": BASE_RAID_TROPHY_STAKE if ok else 0,
		"gold_awarded": pts * 2 if ok else 0,
		"clan_points_awarded": CLAN_RAID_WIN_POINTS if ok else 0,
		"practice_xp_awarded": 25 if drills_passed >= 1 else 10,
	}


static func evaluate_boss_strike_local(rating: int, overcharged: bool, base_damage: int, hp_before: int) -> Dictionary:
	var scale_num := 100
	if rating <= 1:
		scale_num = 0
	elif rating == 2:
		scale_num = 75
	elif rating >= 4:
		scale_num = 200 if overcharged else 125
	var raw_damage := int((base_damage * scale_num) / 100)
	var applied_damage := mini(raw_damage, maxi(0, hp_before))
	var hp_after := maxi(0, hp_before) - applied_damage
	return {
		"damage": applied_damage,
		"raw_damage": raw_damage,
		"overkill_damage": raw_damage - applied_damage,
		"boss_hp_after": hp_after,
		"defeated": hp_before > 0 and hp_after == 0,
		"boss_status": "DEFEATED" if hp_after == 0 else "ACTIVE",
	}


static func sync_solver_score(solver: RaidTickStarSolver, dt: float, buildings: Array) -> void:
	solver.elapsed_s += dt
	var total := 0
	var destroyed := 0
	var th_down := false
	for b in buildings:
		if b is FortressWallTile or not b.has_method("get_sim_state"):
			continue
		var s: Dictionary = b.get_sim_state()
		if String(s.get("kind", "")) == "WALL":
			continue
		total += 1
		var alive := bool(s.get("alive", true))
		update_damage_bar(b as Node3D, float(s.get("hp", 0.0)), float(s.get("max_hp", 1.0)), alive)
		if not alive:
			destroyed += 1
			if String(s.get("kind", "")) == "TOWNHALL":
				th_down = true
	solver.destruction_pct = (100.0 * float(destroyed) / float(maxi(total, 1)))
	var st := (1 if solver.destruction_pct >= 50.0 else 0) + (1 if th_down else 0) + (1 if destroyed == total and total > 0 else 0)
	solver.stars = mini(3, st)
	solver.ended = (solver.elapsed_s >= RAID_TIME_LIMIT_S) or (total > 0 and destroyed == total)


static func update_damage_bar(b: Node3D, hp: float, max_hp: float, alive: bool) -> void:
	if b == null:
		return
	var bar := b.get_node_or_null("RaidHpBar") as MeshInstance3D
	if not alive or max_hp <= 0.0 or hp >= max_hp - 0.5:
		if bar != null:
			bar.visible = false
		return
	if bar == null:
		bar = MeshInstance3D.new()
		bar.name = "RaidHpBar"
		var q := QuadMesh.new()
		q.size = Vector2(1.9, 0.20)
		bar.mesh = q
		bar.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		var mat := StandardMaterial3D.new()
		mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
		mat.billboard_mode = BaseMaterial3D.BILLBOARD_ENABLED
		bar.material_override = mat
		bar.position = Vector3(0.0, 3.6, 0.0)
		b.add_child(bar)
	var ratio := clampf(hp / max_hp, 0.02, 1.0)
	bar.visible = true
	bar.scale.x = ratio
	(bar.material_override as StandardMaterial3D).albedo_color = Color(0.25, 0.88, 0.35).lerp(Color(0.95, 0.22, 0.18), 1.0 - ratio)


static func clear_damage_bars(buildings: Array) -> void:
	for b in buildings:
		if b is Node:
			var bar := (b as Node).get_node_or_null("RaidHpBar") as Node3D
			if bar != null:
				bar.visible = false


static func cell_of(v: Variant) -> Vector2i:
	if v is Vector2i:
		return v
	if v is Dictionary:
		return Vector2i(int((v as Dictionary).get("gx", -1)), int((v as Dictionary).get("gz", -1)))
	if v is Node and (v as Node).get("grid_pos") is Vector2i:
		return (v as Node).get("grid_pos") as Vector2i
	return Vector2i(-1, -1)
