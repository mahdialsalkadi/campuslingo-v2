class_name DefenseTurretBase
extends VillageBuildingBase
## Shared tactical defense base (extends VillageBuildingBase): turret aiming, stealth & brownout DPS.

var dps: float = 18.0
var shot_interval: float = 0.8
var range_min: float = 0.0
var range_max: float = 8.0
var targets_ground: bool = true
var targets_air: bool = false
var splash_radius: float = 0.0
var stealth_reveal_radius: float = 0.0
var revealed: bool = true
var shot_timer: float = 0.4
var dmg_dealt_total: float = 0.0

var _profile: VillageProfile = null
var _base_mount: Node3D = null
var _turret_pivot: Node3D = null
var _muzzle_y: float = 1.5
var _soot_timer: float = 0.25
var _initial_revealed: bool = true

func _on_cfg_loaded(spec: Dictionary) -> void:
	_profile = VillageProfile.load_default()
	var dps_arr: Array = _cfg.get("dps", [18])
	dps = float(dps_arr[mini(tier, dps_arr.size() - 1)])
	shot_interval = float(_cfg.get("shot_interval", 0.8))
	shot_timer = shot_interval * _profile.shot_phase_offset_ratio
	range_min = float(_cfg.get("range_min", 0.0))
	range_max = float(_cfg.get("range_max", 8.0))
	targets_ground = bool(_cfg.get("targets_ground", true))
	targets_air = bool(_cfg.get("targets_air", false))
	splash_radius = float(_cfg.get("splash_radius", 0.0))
	stealth_reveal_radius = float(_cfg.get("stealth_reveal_radius", 0.0))
	if not spec.is_empty():
		_initial_revealed = stealth_reveal_radius <= 0.0 or bool(spec.get("revealed", false))
		revealed = _initial_revealed
	_muzzle_y = float(_cfg.get("muzzle_y", 1.5))

func _mount_visuals() -> void:
	_mount_meshes(_cfg)

func _mount_meshes(cfg: Dictionary) -> void:
	if _base_mount == null:
		_base_mount = Node3D.new()
		_base_mount.name = "BaseMount"
		add_child(_base_mount)
		var base_p := String(cfg.get("base_model", ""))
		if base_p != "":
			var n := GridMath.mount_glb(_base_mount, "%s_lod0.glb" % base_p, float(cfg.get("target_h", 1.2)), Vector3.ONE * 2.0, Color.WHITE, float(cfg.get("max_xz", 3.2)))
			PbrMiniatureMaterial.apply_material_map(n, {}, false)
			GridMath.apply_lod_visibility(n)
		if cfg.has("turret_model"):
			_turret_pivot = Node3D.new()
			_turret_pivot.name = "TurretPivot"
			_turret_pivot.position = Vector3(0.0, float(cfg.get("turret_y", 0.4)), 0.0)
			add_child(_turret_pivot)
			var tn := GridMath.mount_glb(_turret_pivot, "%s_lod0.glb" % String(cfg["turret_model"]), float(cfg.get("turret_h", 1.1)), Vector3.ONE, Color.WHITE)
			PbrMiniatureMaterial.apply_material_map(tn, {}, false)
		PbrMiniatureMaterial.attach_contact_shadow(self, Vector2.ONE * (footprint_cells * GridMath.CELL_SIZE * 0.85), 0.42, 0.44)
	scale = Vector3.ONE * (1.0 + 0.04 * float(tier))
	_apply_brownout_visual()

func step_combat(dt: float, units: Array, vfx: Node) -> void:
	if not alive:
		return
	if browned_out and vfx != null:
		_soot_timer -= dt
		if _soot_timer <= 0.0:
			_soot_timer = 1.6
			_emit_juice("brownout", global_position)
	if not _check_stealth(units):
		return
	shot_timer = maxf(0.0, shot_timer - dt)
	var tgt := _find_target(units)
	if tgt == null:
		return
	if _turret_pivot != null:
		var aim := Vector3(tgt.global_position.x, _turret_pivot.global_position.y, tgt.global_position.z)
		if aim.distance_squared_to(_turret_pivot.global_position) > 0.01:
			_turret_pivot.look_at(aim, Vector3.UP)
	if shot_timer > 0.0:
		return
	shot_timer = shot_interval
	_fire_at(tgt, units, vfx)

func _check_stealth(units: Array) -> bool:
	if revealed:
		return true
	for u in units:
		if u != null and u.is_alive() and GridMath.cell_distance(global_position, u.global_position) <= stealth_reveal_radius:
			revealed = true
			_emit_juice("troop_deploy", global_position, 1.3)
			break
	return revealed

func _find_target(units: Array) -> Node3D:
	var best: Node3D = null
	var best_d := 1e9
	for u in units:
		if u == null or not u.is_alive():
			continue
		var is_air := bool(u.is_air_unit())
		if (is_air and not targets_air) or (not is_air and not targets_ground):
			continue
		var d := GridMath.cell_distance(global_position, u.global_position)
		if d >= range_min and d <= range_max and d < best_d:
			best_d = d
			best = u
	return best

func _fire_at(tgt: Node3D, units: Array, vfx: Node) -> void:
	var dmg := dps * shot_interval * (0.65 if browned_out else 1.0)
	dmg_dealt_total += dmg
	if splash_radius > 0.0:
		for u in units:
			if u != null and u.is_alive() and not u.is_air_unit() and GridMath.cell_distance(tgt.global_position, u.global_position) <= splash_radius:
				u.take_damage(dmg)
	else:
		tgt.take_damage(dmg)
	_on_fired_fx(tgt, vfx)

func _on_fired_fx(_tgt: Node3D, _vfx: Node) -> void:
	pass

func set_brownout(b: bool) -> void:
	browned_out = b
	_apply_brownout_visual()

func _apply_brownout_visual() -> void:
	var tint := Color(0.55, 0.47, 0.42, 1.0) if browned_out else Color.WHITE
	if _base_mount != null:
		GridMath.apply_lod_visibility(_base_mount)
		GridMath.set_color_mul(_base_mount, tint)
	if _turret_pivot != null:
		GridMath.set_color_mul(_turret_pivot, tint)

func reset_for_raid() -> void:
	super.reset_for_raid()
	dmg_dealt_total = 0.0
	revealed = _initial_revealed

func get_sim_state() -> Dictionary:
	var d := super.get_sim_state()
	d.merge({"dps": dps, "shot_interval": shot_interval, "range_min": range_min, "range_max": range_max, "targets_ground": targets_ground, "targets_air": targets_air, "splash_radius": splash_radius, "stealth_reveal_radius": stealth_reveal_radius, "revealed": revealed, "fire_rate_mult": 0.65 if browned_out else 1.0}, true)
	return d
