class_name PbrMiniatureMaterial
extends RefCounted
## SEC-FX02 — Clash-faithful stylized miniature PBR material, ACES lighting & contact shadow builder.

const SECTION_ID := "SEC-FX02"
const PBR_SHADER_PATH := "res://assets/shaders/coc_miniature_pbr.gdshader"
const SHADOW_SHADER_PATH := "res://assets/shaders/coc_ground_shadow.gdshader"
const TERRAIN_SHADER_PATH := "res://assets/shaders/coc_island_terrain.gdshader"
const WATER_SHADER_PATH := "res://assets/shaders/coc_water_moat.gdshader"
const EDIT_GRID_SHADER_PATH := "res://assets/shaders/coc_edit_grid.gdshader"
const CONFIG_PATH := "res://data/pbr_materials.json"

static var _config_cache: Dictionary = {}
static var _tex_cache: Dictionary = {}
static var _mat_cache: Dictionary = {}

static func _get_config() -> Dictionary:
	if _config_cache.is_empty() and FileAccess.file_exists(CONFIG_PATH):
		var parsed: Variant = JSON.parse_string(FileAccess.get_file_as_string(CONFIG_PATH))
		if parsed is Dictionary: _config_cache = parsed as Dictionary
	return _config_cache

static func _load_tex(path: String) -> Texture2D:
	if _tex_cache.has(path): return _tex_cache[path] as Texture2D
	if ResourceLoader.exists(path):
		var tex := ResourceLoader.load(path) as Texture2D
		_tex_cache[path] = tex
		return tex
	return null

static func configure_diorama_lighting(world_env: WorldEnvironment, sun: DirectionalLight3D, zoom_size_m: float = 34.0) -> void:
	var cfg: Dictionary = _get_config().get("environment", {})
	if world_env != null:
		var env := Environment.new()
		var sky_mat := ProceduralSkyMaterial.new()
		sky_mat.sky_top_color = Color(cfg.get("sky_top_color", "#3B78A8"))
		sky_mat.sky_horizon_color = Color(cfg.get("sky_horizon_color", "#D4DFC7"))
		sky_mat.ground_bottom_color = Color(cfg.get("ground_bottom_color", "#26341C"))
		sky_mat.ground_horizon_color = Color(cfg.get("ground_horizon_color", "#5A6B40"))
		var sky := Sky.new()
		sky.sky_material = sky_mat
		env.background_mode = Environment.BG_SKY
		env.sky = sky
		env.ambient_light_source = Environment.AMBIENT_SOURCE_SKY
		env.ambient_light_color = Color(cfg.get("ambient_color", "#DFE8F2"))
		env.ambient_light_energy = float(cfg.get("ambient_energy", 0.42))
		env.tonemap_mode = Environment.TONE_MAPPER_ACES
		env.glow_enabled = true
		env.glow_hdr_threshold = float(cfg.get("glow_hdr_threshold", 1.05))
		env.glow_bloom = float(cfg.get("glow_bloom", 0.0))
		env.glow_intensity = float(cfg.get("glow_intensity", 0.55))
		world_env.environment = env
	if sun != null:
		sun.light_color = Color(cfg.get("sun_color", "#FFF0D6"))
		sun.light_energy = float(cfg.get("sun_energy", 1.65))
		sun.shadow_enabled = true
		sun.shadow_bias = 0.03
		sun.shadow_normal_bias = 1.2
		sun.directional_shadow_mode = DirectionalLight3D.SHADOW_PARALLEL_2_SPLITS
		sun.directional_shadow_split_1 = 0.30
		sun.directional_shadow_max_distance = clampf(zoom_size_m * 1.9, 65.0, 105.0)

static func get_pbr_material(mat_key: String, use_instancing: bool = false, overrides: Dictionary = {}) -> ShaderMaterial:
	var cache_key := "%s_%s" % [mat_key, str(use_instancing)]
	if _mat_cache.has(cache_key) and overrides.is_empty(): return _mat_cache[cache_key] as ShaderMaterial
	var spec: Dictionary = (_get_config().get("materials", {}).get(mat_key, {}) as Dictionary).duplicate()
	for k in overrides: spec[k] = overrides[k]
	var mat := _make_shader_mat(PBR_SHADER_PATH)
	var tex_key := String(spec.get("tex", ""))
	if tex_key != "":
		for slot in [["albedo_tex", "has_albedo_tex", "albedo"], ["normal_tex", "has_normal_tex", "normal"], ["orm_tex", "has_orm_tex", "orm"]]:
			var tex := _load_tex("res://assets/textures/pbr/%s_%s.jpg" % [tex_key, slot[2]])
			if tex != null:
				mat.set_shader_parameter(slot[0], tex)
				mat.set_shader_parameter(slot[1], true)
	var uvs: Array = spec.get("uv_scale", [1.0, 1.0])
	mat.set_shader_parameter("albedo_tint", Color(spec.get("tint", "#FFFFFF")))
	mat.set_shader_parameter("uv_scale", Vector2(float(uvs[0]), float(uvs[1])))
	for p in ["normal_scale", "roughness_scale", "metallic_scale", "ao_strength", "vertex_ao_strength", "emission_energy"]:
		if spec.has(p): mat.set_shader_parameter(p, float(spec[p]))
	if spec.has("emission_color"): mat.set_shader_parameter("emission_color", Color(spec["emission_color"]))
	mat.set_shader_parameter("use_instance_data", use_instancing)
	if overrides.is_empty(): _mat_cache[cache_key] = mat
	return mat

static func _make_shader_mat(path: String) -> ShaderMaterial:
	var mat := ShaderMaterial.new()
	mat.shader = ResourceLoader.load(path) as Shader
	return mat

static func _resolve_slot_material(base_mat: Material, slot_map: Dictionary, use_instancing: bool) -> ShaderMaterial:
	var sn := String(base_mat.resource_name).to_lower() if base_mat != null else ""
	for p in slot_map:
		if p != "default" and sn.find(String(p).to_lower()) >= 0: return get_pbr_material(String(slot_map[p]), use_instancing)
	var base_key := sn.get_slice(".", 0)
	if _get_config().get("materials", {}).has(base_key): return get_pbr_material(base_key, use_instancing)
	return get_pbr_material(String(slot_map.get("default", "stone")), use_instancing)

static func apply_material_map(root: Node, slot_map: Dictionary, use_instancing: bool = false) -> void:
	if root is MeshInstance3D and (root as MeshInstance3D).mesh != null:
		var mi := root as MeshInstance3D
		for i in range(mi.mesh.get_surface_count()): mi.set_surface_override_material(i, _resolve_slot_material(mi.mesh.surface_get_material(i), slot_map, use_instancing))
	for c in root.get_children(): apply_material_map(c, slot_map, use_instancing)

static func load_skinned_glb_mesh(path: String, slot_map: Dictionary, use_instancing: bool = true) -> Mesh:
	var inst := (ResourceLoader.load(path) as PackedScene).instantiate()
	var mi := _first_mesh_inst(inst)
	var mesh := mi.mesh.duplicate() as Mesh
	for i in range(mesh.get_surface_count()): mesh.surface_set_material(i, _resolve_slot_material(mesh.surface_get_material(i), slot_map, use_instancing))
	inst.queue_free()
	return mesh

static func _first_mesh_inst(n: Node) -> MeshInstance3D:
	if n is MeshInstance3D and (n as MeshInstance3D).mesh != null: return n as MeshInstance3D
	for c in n.get_children():
		var found := _first_mesh_inst(c)
		if found != null: return found
	return null

static func create_island_terrain_material() -> ShaderMaterial:
	var mat := _make_shader_mat(TERRAIN_SHADER_PATH)
	for layer in ["grass", "cobble", "sand"]:
		for mt in ["albedo", "normal", "orm"]: mat.set_shader_parameter("%s_%s" % [layer, mt], _load_tex("res://assets/textures/pbr/%s_%s.jpg" % [layer, mt]))
	return mat

static func create_water_moat_material() -> ShaderMaterial:
	return _shader_with_params(WATER_SHADER_PATH, {"water_normal_tex": _load_tex("res://assets/textures/pbr/water_normal.png")})

static func create_edit_grid_material(alpha: float = 0.0) -> ShaderMaterial:
	return _shader_with_params(EDIT_GRID_SHADER_PATH, {"overlay_alpha": alpha})

static func create_contact_shadow_material(half_extents: Vector2, opacity: float = 0.68) -> ShaderMaterial:
	return _shader_with_params(SHADOW_SHADER_PATH, {"half_extents": half_extents, "opacity": opacity})

static func _shader_with_params(path: String, params: Dictionary) -> ShaderMaterial:
	var mat := _make_shader_mat(path)
	for k in params: mat.set_shader_parameter(k, params[k])
	return mat

static func attach_contact_shadow(host: Node3D, footprint_m: Vector2, pad_m: float = 0.85, opacity: float = 0.68) -> MeshInstance3D:
	var quad_size := footprint_m + Vector2(pad_m, pad_m)
	var mesh := PlaneMesh.new(); mesh.size = quad_size
	var mi := MeshInstance3D.new(); mi.name = "ContactShadowSkirt"; mi.mesh = mesh
	mi.material_override = create_contact_shadow_material(Vector2(footprint_m.x / (2.0 * quad_size.x), footprint_m.y / (2.0 * quad_size.y)), opacity)
	mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	mi.position = Vector3(0.0, 0.015, 0.0)
	host.add_child(mi)
	return mi
