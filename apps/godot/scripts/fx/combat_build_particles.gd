class_name CombatBuildParticles
extends Node3D
## SEC-FX03: Pooled combat projectiles, shockwave rings, additive halos, Tesla arcs & rubble puffs.

const SECTION_ID := "SEC-FX03"
const CONFIG_PATH := "res://data/fx_events.json"

var _events: Dictionary = {}
var _projectiles: Array[Dictionary] = []
var _fx_items: Array[Dictionary] = []
var _proj_meshes: Dictionary = {}
var _ring_shader: Shader = null
var _halo_shader: Shader = null
var _puff_mesh: SphereMesh = null

func _ready() -> void:
	_ring_shader = ResourceLoader.load("res://assets/shaders/coc_shockwave_ring.gdshader") as Shader
	_halo_shader = ResourceLoader.load("res://assets/shaders/coc_additive_halo.gdshader") as Shader
	_puff_mesh = SphereMesh.new()
	_puff_mesh.radius = 0.22
	_puff_mesh.height = 0.38
	_puff_mesh.radial_segments = 8
	_puff_mesh.rings = 4
	var parsed: Variant = JSON.parse_string(FileAccess.get_file_as_string(CONFIG_PATH)) if FileAccess.file_exists(CONFIG_PATH) else {}
	if parsed is Dictionary:
		_events = (parsed as Dictionary).get("events", {})
		for k in (parsed as Dictionary).get("projectiles", {}):
			var mp := String((parsed as Dictionary)["projectiles"][k])
			if ResourceLoader.exists(mp):
				_proj_meshes[k] = PbrMiniatureMaterial.load_skinned_glb_mesh(mp, {}, false)

func _process(delta: float) -> void:
	_step_projectiles(delta)
	_step_fx(delta)

func _step_projectiles(delta: float) -> void:
	for i in range(_projectiles.size() - 1, -1, -1):
		var p: Dictionary = _projectiles[i]
		p["t"] = float(p["t"]) + delta
		var u := clampf(float(p["t"]) / float(p["dur"]), 0.0, 1.0)
		var pos := (p["from"] as Vector3).lerp(p["to"] as Vector3, u)
		pos.y += 4.0 * float(p["apex"]) * u * (1.0 - u)
		var mi := p["node"] as MeshInstance3D
		if is_instance_valid(mi):
			var prev := mi.global_position
			mi.global_position = pos
			if pos.distance_to(prev) > 0.01 and String(p["kind"]) != "cannonball":
				mi.look_at(pos + (pos - prev), Vector3.UP)
			if u >= 1.0: mi.queue_free()
		if u >= 1.0: _projectiles.remove_at(i)

func _step_fx(delta: float) -> void:
	for i in range(_fx_items.size() - 1, -1, -1):
		var item: Dictionary = _fx_items[i]
		item["t"] = float(item["t"]) + delta
		var u := clampf(float(item["t"]) / float(item["dur"]), 0.0, 1.0)
		var mi := item["node"] as MeshInstance3D
		if is_instance_valid(mi):
			var kind := String(item["kind"])
			if kind == "ring":
				(item["mat"] as ShaderMaterial).set_shader_parameter("progress", u)
			elif kind == "halo":
				(item["mat"] as ShaderMaterial).set_shader_parameter("intensity", float(item["base_i"]) * (1.0 - u))
			elif kind == "puff":
				mi.global_position += (item["vel"] as Vector3) * delta
				mi.scale = Vector3.ONE * (0.65 + u * 0.7)
				(item["mat"] as StandardMaterial3D).albedo_color.a = 1.0 - u
			if u >= 1.0: mi.queue_free()
		if u >= 1.0: _fx_items.remove_at(i)

func spawn_event(event_id: String, world_pos: Vector3, _normal: Vector3 = Vector3.UP, scale_factor: float = 1.0) -> void:
	var spec: Dictionary = _events.get(event_id, {})
	if spec.has("ring"):
		var r: Array = spec["ring"]
		spawn_shockwave(world_pos + Vector3(0.0, 0.08, 0.0), float(r[0]) * scale_factor, Color(String(r[1]), 0.92), float(r[2]))
	if spec.has("halo"):
		var h: Array = spec["halo"]
		spawn_additive_halo(world_pos + Vector3(0.0, 0.9, 0.0), float(h[0]) * scale_factor, Color(String(h[1])), float(h[2]), float(h[3]))
	if spec.has("puffs"):
		var f: Array = spec["puffs"]
		_spawn_burst_puffs(world_pos + Vector3(0.0, 0.45, 0.0), Color(String(f[0])), int(f[1]), float(f[2]))

func spawn_projectile(kind: String, from_pos: Vector3, to_pos: Vector3, flight_s: float, apex_height: float = 0.0) -> void:
	var mi := MeshInstance3D.new()
	mi.mesh = _proj_meshes.get(kind, _puff_mesh) as Mesh
	mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	mi.scale = Vector3.ONE * (1.25 if kind == "mortar" else 1.15)
	add_child(mi)
	mi.global_position = from_pos
	_projectiles.append({"node": mi, "kind": kind, "from": from_pos, "to": to_pos, "t": 0.0, "dur": maxf(flight_s, 0.08), "apex": apex_height})

func spawn_tesla_arc(from_pos: Vector3, to_pos: Vector3) -> void:
	var im := ImmediateMesh.new()
	var mat := StandardMaterial3D.new()
	mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	mat.blend_mode = BaseMaterial3D.BLEND_MODE_ADD
	mat.albedo_color = Color(0.35, 0.92, 1.0, 1.0)
	mat.cull_mode = BaseMaterial3D.CULL_DISABLED
	im.surface_begin(Mesh.PRIMITIVE_TRIANGLE_STRIP, mat)
	for i in range(6):
		var pt := from_pos.lerp(to_pos, float(i) / 5.0)
		if i > 0 and i < 5:
			pt += Vector3(sin(float(i * 19 + Time.get_ticks_msec()) * 0.1) * 0.28, cos(float(i * 13)) * 0.22, cos(float(i * 7)) * 0.28)
		im.surface_add_vertex(pt - Vector3(0.08, 0.08, 0.0))
		im.surface_add_vertex(pt + Vector3(0.08, 0.08, 0.0))
	im.surface_end()
	_add_transient("arc", im, mat, Vector3.ZERO, 0.16)
	spawn_additive_halo(from_pos, 1.65, Color(0.28, 0.90, 1.0), 1.45, 0.16)
	spawn_additive_halo(to_pos + Vector3(0.0, 0.6, 0.0), 1.35, Color(0.32, 0.94, 1.0), 1.35, 0.16)

func spawn_shockwave(pos: Vector3, radius_m: float, col: Color, dur: float) -> void:
	if _ring_shader == null: return
	var quad := PlaneMesh.new()
	quad.size = Vector2(radius_m * 2.0, radius_m * 2.0)
	_add_transient("ring", quad, _shader_mat(_ring_shader, {"ring_color": col, "progress": 0.05}), pos, dur)

func spawn_additive_halo(pos: Vector3, size_m: float, col: Color, intensity: float, dur: float) -> void:
	if _halo_shader == null: return
	var q := QuadMesh.new()
	q.size = Vector2(size_m, size_m)
	_add_transient("halo", q, _shader_mat(_halo_shader, {"halo_color": col, "intensity": intensity}), pos, dur, {"base_i": intensity})

func _spawn_burst_puffs(origin: Vector3, col: Color, count: int, speed: float) -> void:
	for i in range(count):
		var mat := StandardMaterial3D.new()
		mat.albedo_color = col
		mat.roughness = 0.95
		mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
		var ang := float(i) * TAU / maxf(float(count), 1.0)
		_add_transient("puff", _puff_mesh, mat, origin, 0.42, {"vel": Vector3(cos(ang) * speed * 0.65, speed * 0.85, sin(ang) * speed * 0.65)})

func _shader_mat(s: Shader, params: Dictionary) -> ShaderMaterial:
	var m := ShaderMaterial.new()
	m.shader = s
	for k in params: m.set_shader_parameter(k, params[k])
	return m

func _add_transient(kind: String, mesh: Mesh, mat: Material, pos: Vector3, dur: float, extra: Dictionary = {}) -> void:
	var mi := MeshInstance3D.new()
	mi.mesh = mesh
	if kind != "arc": mi.material_override = mat
	mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	add_child(mi)
	mi.global_position = pos
	var item := {"kind": kind, "node": mi, "mat": mat, "t": 0.0, "dur": dur}
	for k in extra: item[k] = extra[k]
	_fx_items.append(item)
