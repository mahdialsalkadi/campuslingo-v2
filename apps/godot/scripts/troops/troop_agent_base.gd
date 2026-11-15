class_name TroopAgentBase
extends Node3D
## Shared miniature troop base implementing RaidUnit, 60Hz accel/turn & 20Hz A* wall avoidance.

const STATS_PATH := "res://data/unit_stats.json"
static var _stats_cache: Dictionary = {}

var unit_id: String = "squire_footman"
var hp: float = 110.0
var max_hp: float = 110.0
var move_speed_mps: float = 2.70
var dps: float = 14.0
var attack_interval: float = 0.80
var range_cells: float = 0.6
var priority: String = "ANY"
var wall_mult: float = 1.0
var is_air: bool = false
var hover_y: float = 0.0
var alive: bool = true

var _profile: VillageProfile = null
var _vel_xz: Vector2 = Vector2.ZERO
var _yaw: float = 0.0
var _attack_timer: float = 0.0
var _repath_timer: float = 0.0
var _rally_timer: float = 0.0
var _rally_mult: float = 1.0
var _path: Array[Vector2i] = []
var _mesh_pivot: Node3D = null
var _anim_phase: float = 0.0

static func load_spec(uid: String) -> Dictionary:
	if _stats_cache.is_empty() and FileAccess.file_exists(STATS_PATH):
		var p: Variant = JSON.parse_string(FileAccess.get_file_as_string(STATS_PATH))
		if p is Dictionary: _stats_cache = (p as Dictionary).get("units", {})
	return (_stats_cache.get(uid, {}) as Dictionary).duplicate(true)

func init_unit(spec: Dictionary) -> void:
	unit_id = String(spec.get("unit_id", unit_id))
	var cfg := load_spec(unit_id)
	max_hp = float(cfg.get("hp", 110))
	hp = float(spec.get("hp", max_hp))
	move_speed_mps = float(cfg.get("speed_cells", 1.8)) * GridMath.CELL_SIZE
	dps = float(cfg.get("dps", 14.0))
	attack_interval = float(cfg.get("attack_interval", 0.80))
	range_cells = float(cfg.get("range_cells", 0.6))
	priority = String(cfg.get("priority", "ANY"))
	wall_mult = float(cfg.get("wall_mult", 1.0))
	is_air = String(cfg.get("layer", "GROUND")) == "AIR"
	hover_y = float(cfg.get("hover_y", 0.0))
	_profile = VillageProfile.load_default()
	if spec.has("pos"): position = spec["pos"] as Vector3
	position.y = 0.0
	alive = hp > 0.0
	_mount_mesh(cfg)

func _mount_mesh(cfg: Dictionary) -> void:
	if _mesh_pivot != null: return
	_mesh_pivot = Node3D.new(); _mesh_pivot.name = "MeshPivot"; _mesh_pivot.position.y = hover_y; add_child(_mesh_pivot)
	var mpath := String(cfg.get("model", ""))
	if mpath != "":
		var n := GridMath.mount_glb(_mesh_pivot, "%s_lod0.glb" % mpath, float(cfg.get("target_h", 1.45)), Vector3.ONE, Color.WHITE, float(cfg.get("max_xz", 0.0)))
		PbrMiniatureMaterial.apply_material_map(n, {}, false)
	PbrMiniatureMaterial.attach_contact_shadow(self, Vector2(0.85, 0.85), 0.35, 0.24 if is_air else 0.42)

func step_unit(dt: float, buildings: Array, wall_map: Dictionary, vfx: Node) -> void:
	if not alive: return
	if _rally_timer > 0.0:
		_rally_timer = maxf(0.0, _rally_timer - dt)
		if _rally_timer <= 0.0: _rally_mult = 1.0
	_attack_timer = maxf(0.0, _attack_timer - dt); _repath_timer = maxf(0.0, _repath_timer - dt)
	var tgt := _select_target(buildings)
	if tgt == null: _steer_toward(Vector2.ZERO, dt, false); return
	var bdict: Dictionary = tgt.get_sim_state()
	if GridMath.dist_to_footprint(global_position, bdict) <= range_cells:
		var bc := GridMath.cell_to_world(int(bdict["gx"]), int(bdict["gz"]), int(bdict["size"]))
		_steer_toward(Vector2(bc.x - global_position.x, bc.z - global_position.z), dt, false)
		if _attack_timer <= 0.0: _attack_timer = attack_interval; _strike_target(tgt, bdict, vfx)
	else:
		_steer_toward(_next_steer_dir(bdict, wall_map, buildings, vfx), dt, true)

func _next_steer_dir(bdict: Dictionary, wall_map: Dictionary, buildings: Array, vfx: Node) -> Vector2:
	var bc := GridMath.cell_to_world(int(bdict["gx"]), int(bdict["gz"]), int(bdict["size"]))
	if is_air: return Vector2(bc.x - global_position.x, bc.z - global_position.z)
	if _repath_timer <= 0.0 or _path.is_empty():
		_repath_timer = 0.45 + float(get_index() % 12) * 0.05
		_path = AStarWallPathfinder.find_path(GridMath.world_to_cell(global_position), Vector2i(int(bdict["gx"]), int(bdict["gz"])), wall_map)
	var wall_id: Variant = AStarWallPathfinder.first_wall_on_path(_path, wall_map)
	if wall_id != null:
		var wc := CombatParityMath.cell_of(wall_id)
		for wb in buildings:
			var wd: Dictionary = wb.get_sim_state() if (wb is FortressWallTile and (wb as FortressWallTile).grid_pos == wc) else {}
			if not wd.is_empty() and GridMath.dist_to_footprint(global_position, wd) <= range_cells:
				if _attack_timer <= 0.0: _attack_timer = attack_interval; _strike_target(wb, wd, vfx)
				return Vector2.ZERO
	if _path.size() > 1:
		var nw := GridMath.cell_to_world(_path[1].x, _path[1].y, 1)
		if GridMath.world_to_cell(global_position) == _path[1] and _path.size() > 2: _path.remove_at(0); nw = GridMath.cell_to_world(_path[1].x, _path[1].y, 1)
		return Vector2(nw.x - global_position.x, nw.z - global_position.z)
	return Vector2(bc.x - global_position.x, bc.z - global_position.z)

func _steer_toward(dir_xz: Vector2, dt: float, move_active: bool) -> void:
	var spd := move_speed_mps * _rally_mult
	var desired := dir_xz.normalized() * spd if (move_active and dir_xz.length_squared() > 1e-4) else Vector2.ZERO
	var accel: float = lerpf(_profile.accel_start_mps2, _profile.accel_top_taper_mps2, clampf(_vel_xz.length() / maxf(spd, 0.1), 0.0, 1.0)) if move_active else _profile.braking_decel_mps2
	_vel_xz = _vel_xz.move_toward(desired, accel * dt)
	global_position += Vector3(_vel_xz.x, 0.0, _vel_xz.y) * dt
	var aim := desired if dir_xz.length_squared() > 1e-4 else _vel_xz
	if aim.length_squared() > 1e-4:
		var target_yaw := atan2(-aim.x, -aim.y)
		var max_turn := deg_to_rad(_profile.turn_rate_deg_s) * dt
		_yaw += clampf(angle_difference(_yaw, target_yaw), -max_turn, max_turn)
		rotation.y = _yaw
	if _mesh_pivot != null:
		_anim_phase += dt * (11.0 if move_active else 3.0)
		_mesh_pivot.position.y = hover_y + absf(sin(_anim_phase)) * (0.14 if is_air else (0.09 if move_active else 0.0))

func _select_target(buildings: Array) -> Node:
	var best: Node = null; var best_d := 1e9
	for prio_pass in [priority, "ANY"]:
		for b in buildings:
			if b == null or not b.has_method("get_sim_state"): continue
			var s: Dictionary = b.get_sim_state()
			if not bool(s.get("alive", true)): continue
			var k := String(s.get("kind", ""))
			if (prio_pass == "WALL" and k != "WALL") or (prio_pass == "DEFENSE" and k != "DEFENSE") or (prio_pass == "ANY" and k == "WALL"): continue
			var d := GridMath.dist_to_footprint(global_position, s)
			if d < best_d: best_d = d; best = b
		if best != null: return best
	return null

func _strike_target(tgt: Node, s: Dictionary, vfx: Node) -> void:
	var dmg := dps * attack_interval * _rally_mult * (wall_mult if String(s.get("kind", "")) == "WALL" else 1.0)
	tgt.call("take_damage", dmg)
	var bc := GridMath.cell_to_world(int(s["gx"]), int(s["gz"]), int(s["size"]))
	var pkind := String(load_spec(unit_id).get("projectile", ""))
	if vfx != null and pkind != "":
		vfx.call("spawn_projectile", pkind, global_position + Vector3(0.0, 1.0 + hover_y, 0.0), bc + Vector3(0.0, 0.8, 0.0), 0.18, 0.35)
	_emit_juice("hit", bc)

func _emit_juice(ev: String, pos: Vector3, scale_f: float = 1.0) -> void:
	var j := get_node_or_null("/root/Juice") if is_inside_tree() else null
	if j != null: j.call("emit_event", ev, pos, Vector3.UP, scale_f)
func take_damage(dmg: float) -> void:
	if not alive: return
	hp = maxf(0.0, hp - dmg)
	if hp <= 0.0: alive = false; visible = false; _emit_juice("hit", global_position)
func apply_rally_buff(mult: float, dur: float) -> void: _rally_mult = maxf(_rally_mult, mult); _rally_timer = maxf(_rally_timer, dur)
func is_air_unit() -> bool: return is_air
func is_alive() -> bool: return alive
