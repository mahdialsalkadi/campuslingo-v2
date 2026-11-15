class_name RaidTickStarSolver
extends RefCounted
## SEC-SIM02: 20Hz Combat Tick Loop, 0-3 Star Evaluator & raids_leagues.rs Parity.

const FIXED_DT: float = 0.05
const RAID_SESSION_CEILING_MS: int = 120000
const BASE_RAID_TROPHY_STAKE: int = 15
const REVENGE_RAID_TROPHY_STAKE: int = 22
const MAX_DAILY_TROPHY_LOSS_FROM_RAIDS: int = 45
const DAILY_PRACTICE_XP_CAP: int = 100
const CLAN_RAID_WIN_POINTS: int = 25
const CLAN_REVENGE_WIN_POINTS: int = 38

var buildings: Array[Dictionary] = []
var troops: Array[Dictionary] = []
var recent_events: Array[Dictionary] = []
var elapsed_s: float = 0.0
var stars: int = 0
var destruction_pct: int = 0
var townhall_destroyed: bool = false
var ended: bool = false
var profile: VillageProfile = VillageProfile.load_default()

func reset() -> void:
	buildings.clear(); troops.clear(); recent_events.clear()
	elapsed_s = 0.0; stars = 0; destruction_pct = 0; townhall_destroyed = false; ended = false

func add_building(spec: Dictionary) -> Dictionary:
	var b: Dictionary = spec.duplicate(true)
	b["alive"] = int(b.get("hp", 1)) > 0
	b["shot_timer"] = float(b.get("shot_interval", 0.0)) * profile.shot_phase_offset_ratio
	buildings.append(b); return b

func spawn_troop(spec: Dictionary) -> Dictionary:
	var t: Dictionary = spec.duplicate(true); troops.append(t); return t

func step(dt: float = FIXED_DT) -> void:
	recent_events.clear()
	if ended: return
	elapsed_s += dt
	var wall_map: Dictionary = AStarWallPathfinder.build_wall_map(buildings)
	for t in troops:
		if t.get("alive", false): _step_troop(t, wall_map, dt)
	for b in buildings:
		if b.get("alive", false) and String(b.get("kind", "")) == "DEFENSE": _step_defense(b, dt)
	_recompute_stars()
	if elapsed_s >= float(RAID_SESSION_CEILING_MS) / 1000.0 or stars >= 3: ended = true

func _step_troop(t: Dictionary, wall_map: Dictionary, dt: float) -> void:
	t["cooldown"] = maxf(0.0, float(t.get("cooldown", 0.0)) - dt)
	var tgt_v: Variant = _pick_target(t)
	if tgt_v == null: return
	var b := tgt_v as Dictionary; var r_cells: float = float(t["range_cells"])
	if GridMath.dist_to_footprint(t["pos"], b) <= r_cells and (bool(t["fires_over_walls"]) or String(b["kind"]) == "WALL" or wall_map.is_empty()):
		_strike(t, b); return
	var tc: Vector2i = GridMath.world_to_cell(t["pos"])
	var gc := Vector2i(int(b["gx"]) + int(b["size"]) / 2, int(b["gz"]) + int(b["size"]) / 2)
	var is_air: bool = String(t["layer"]) == "AIR"
	var path: Array[Vector2i] = AStarWallPathfinder.find_path(tc, gc, wall_map, float(t["wall_cost_bias"]), is_air)
	var blocking: Variant = null if is_air else AStarWallPathfinder.first_wall_on_path(path, wall_map)
	if blocking != null and GridMath.dist_to_footprint(t["pos"], blocking as Dictionary) <= r_cells:
		_strike(t, blocking as Dictionary); return
	if blocking == null and GridMath.dist_to_footprint(t["pos"], b) <= r_cells:
		_strike(t, b); return
	var wp: Vector2i = path[1] if path.size() > 1 else gc
	var next_w: Vector3 = GridMath.cell_to_world(wp.x, wp.y, 1); var pos: Vector3 = t["pos"]
	var dxz := Vector2(next_w.x - pos.x, next_w.z - pos.z)
	if dxz.length() > 0.01:
		var m: float = minf(dxz.length(), float(t["speed_cps"]) * GridMath.CELL_SIZE * dt)
		var dir: Vector2 = dxz.normalized()
		t["pos"] = Vector3(pos.x + dir.x * m, pos.y, pos.z + dir.y * m)

func _strike(t: Dictionary, b: Dictionary) -> void:
	if float(t["cooldown"]) > 0.0: return
	var iv: float = float(t["attack_interval"]); t["cooldown"] = iv
	var mult: float = float(t["wall_mult"]) if String(b["kind"]) == "WALL" else 1.0
	b["hp"] = maxi(0, int(b["hp"]) - int(round(float(t["dps"]) * iv * mult)))
	recent_events.append({"type": "hit", "pos": GridMath.cell_to_world(int(b["gx"]), int(b["gz"]), int(b["size"]))})
	if int(b["hp"]) <= 0:
		b["alive"] = false
		recent_events.append({"type": "destroy", "pos": GridMath.cell_to_world(int(b["gx"]), int(b["gz"]), int(b["size"]))})

func _step_defense(b: Dictionary, dt: float) -> void:
	var bc: Vector3 = GridMath.cell_to_world(int(b["gx"]), int(b["gz"]), int(b["size"]))
	if not bool(b.get("revealed", true)):
		for t in troops:
			if t.get("alive", false) and GridMath.cell_distance(bc, t["pos"]) <= float(b["stealth_reveal_radius"]):
				b["revealed"] = true; recent_events.append({"type": "tesla_reveal", "pos": bc}); break
		if not bool(b.get("revealed", false)): return
	b["shot_timer"] = maxf(0.0, float(b.get("shot_timer", 0.0)) - dt)
	if float(b["shot_timer"]) > 0.0: return
	var best: Variant = null; var best_d: float = 1e9
	for t in troops:
		if not t.get("alive", false): continue
		var lay: String = String(t["layer"])
		if not ((lay == "GROUND" and bool(b["targets_ground"])) or (lay == "AIR" and bool(b["targets_air"]))): continue
		var d: float = GridMath.cell_distance(bc, t["pos"])
		if d >= float(b["range_min"]) and d <= float(b["range_max"]) and d < best_d: best_d = d; best = t
	if best == null: return
	var tgt := best as Dictionary; var iv: float = float(b["shot_interval"]); b["shot_timer"] = iv
	var dmg: int = int(round(float(b["dps"]) * iv * float(b.get("fire_rate_mult", 1.0))))
	var splash: float = float(b.get("splash_radius", 0.0))
	for o in (troops if splash > 0.0 else [tgt]):
		if o.get("alive", false) and (splash <= 0.0 or (String(o["layer"]) == "GROUND" and GridMath.cell_distance(tgt["pos"], o["pos"]) <= splash)):
			o["hp"] = maxi(0, int(o["hp"]) - dmg)
			if int(o["hp"]) <= 0: o["alive"] = false
	recent_events.append({"type": "mortar" if splash > 0.0 else "shot", "archetype": b["archetype"], "pos": tgt["pos"], "from": bc})

func _pick_target(t: Dictionary) -> Variant:
	var prio: String = String(t.get("priority", "ANY"))
	var pref: Array[Dictionary] = []; var fall: Array[Dictionary] = []
	for b in buildings:
		if not b.get("alive", true): continue
		var k: String = String(b.get("kind", ""))
		if (prio == "DEFENSE" and k == "DEFENSE") or (prio == "WALL" and k == "WALL") or (prio == "ANY" and k != "WALL"): pref.append(b)
		elif k != "WALL" or prio == "WALL": fall.append(b)
	var best: Variant = null; var best_d: float = 1e9
	for b in (pref if not pref.is_empty() else fall):
		var d: float = GridMath.dist_to_footprint(t["pos"], b)
		if d < best_d: best_d = d; best = b
	return best

func _recompute_stars() -> void:
	var tot: int = 0; var dead: int = 0; townhall_destroyed = false
	for b in buildings:
		if String(b.get("kind", "")) == "WALL": continue
		tot += 1
		if not b.get("alive", true):
			dead += 1
			if String(b.get("kind", "")) == "TOWNHALL": townhall_destroyed = true
	destruction_pct = floori(100.0 * float(dead) / float(maxi(1, tot)))
	stars = 3 if (destruction_pct >= 100 and tot > 0) else ((1 if townhall_destroyed else 0) + (1 if destruction_pct >= 50 else 0))

static func predict_raid_settlement(sandbox_ok: bool, drills: int, elapsed_ms: int, revenge: bool, def_loss: int, xp_today: int, immune: bool, eligible: bool, pts: int) -> Dictionary:
	if elapsed_ms > RAID_SESSION_CEILING_MS:
		return {"status": "EXPIRED", "raid_succeeded": false, "attacker_trophy_gain": 0, "defender_trophy_loss": 0, "practice_xp_awarded": 0, "gold_awarded": 0, "clan_points_awarded": 0, "loot_chest_tier": ""}
	var rem_xp: int = maxi(0, DAILY_PRACTICE_XP_CAP - maxi(0, xp_today))
	if immune:
		return {"status": "SETTLED_COOLDOWN_BLOCKED", "raid_succeeded": false, "attacker_trophy_gain": 0, "defender_trophy_loss": 0, "practice_xp_awarded": mini(15, rem_xp), "gold_awarded": 0, "clan_points_awarded": 0, "loot_chest_tier": ""}
	if sandbox_ok and drills >= 2:
		var stake: int = REVENGE_RAID_TROPHY_STAKE if revenge else BASE_RAID_TROPHY_STAKE
		return {"status": "SETTLED_WIN", "raid_succeeded": true, "attacker_trophy_gain": stake if eligible else 0, "defender_trophy_loss": mini(stake, maxi(0, MAX_DAILY_TROPHY_LOSS_FROM_RAIDS - maxi(0, def_loss))), "practice_xp_awarded": 0, "gold_awarded": clampi(pts, 10, 5000) * 2 if eligible else 0, "clan_points_awarded": (CLAN_REVENGE_WIN_POINTS if revenge else CLAN_RAID_WIN_POINTS) if eligible else 0, "loot_chest_tier": ("APEX_RAID_CHEST" if drills == 3 else "STANDARD_RAID_CHEST") if eligible else ""}
	return {"status": "SETTLED_LOSS", "raid_succeeded": false, "attacker_trophy_gain": 0, "defender_trophy_loss": 0, "practice_xp_awarded": mini(5, rem_xp), "gold_awarded": 0, "clan_points_awarded": 0, "loot_chest_tier": ""}

static func predict_boss_strike(rating: int, overcharged: bool, base_dmg: int, hp_before: int) -> Dictionary:
	var scale_num: int = 0 if rating == 1 else (75 if rating == 2 else (100 if rating == 3 else (200 if overcharged else 125)))
	var raw: int = (base_dmg * scale_num) / 100
	var applied: int = mini(raw, hp_before); var after: int = hp_before - applied
	return {"raw_damage": raw, "applied_damage": applied, "overkill_damage": raw - applied, "boss_hp_after": after, "defeated": hp_before > 0 and after == 0}
