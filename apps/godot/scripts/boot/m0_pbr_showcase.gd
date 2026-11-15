extends Node3D
## SEC-M0 — Clean-Room CoC Miniature PBR Island & Benchmark Diorama Controller.

const LAYOUT_PATH := "res://data/m0_diorama_layout.json"
const HALO_SHADER_PATH := "res://assets/shaders/coc_additive_halo.gdshader"

@onready var _world_env: WorldEnvironment = $WorldEnvironment
@onready var _sun_light: DirectionalLight3D = $SunLight
@onready var _island_plateau: MeshInstance3D = $Environment/IslandPlateau
@onready var _water_moat: MeshInstance3D = $Environment/WaterMoat
@onready var _edit_grid: MeshInstance3D = $Environment/EditGridOverlay
@onready var _heroes_root: Node3D = $Props/Heroes
@onready var _wall_pillars: MultiMeshInstance3D = $Props/WallPillarBatch
@onready var _wall_conns: MultiMeshInstance3D = $Props/WallConnectorBatch
@onready var _wall_shadows: MultiMeshInstance3D = $Props/WallShadowBatch
@onready var _pine_batch: MultiMeshInstance3D = $Props/PineForestBatch
@onready var _pine_shadows: MultiMeshInstance3D = $Props/PineShadowBatch
@onready var _boulder_batch: MultiMeshInstance3D = $Props/BoulderBatch
@onready var _cliff_batch: MultiMeshInstance3D = $Props/CliffRimBatch
@onready var _camera: CameraPanZoom = $CameraPanZoom

var _layout: Dictionary = {}
var _grid_visible: bool = false

func _ready() -> void:
	_layout = _load_json(LAYOUT_PATH)
	PbrMiniatureMaterial.configure_diorama_lighting(_world_env, _sun_light, _camera.zoom_size_m)
	_island_plateau.material_override = PbrMiniatureMaterial.create_island_terrain_material()
	_water_moat.material_override = PbrMiniatureMaterial.create_water_moat_material()
	_edit_grid.material_override = PbrMiniatureMaterial.create_edit_grid_material(0.0)
	set_edit_grid_visible("--show-edit-grid" in OS.get_cmdline_user_args())
	var slots: Dictionary = _layout.get("material_slots", {})
	_build_heroes(slots)
	_build_walls(slots)
	_build_nature(slots)
	_sync_sun_with_active_camera()

func _process(_delta: float) -> void:
	_sync_sun_with_active_camera()

func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and not event.echo:
		if (event as InputEventKey).keycode == KEY_G:
			set_edit_grid_visible(not _grid_visible)

func set_edit_grid_visible(show: bool) -> void:
	_grid_visible = show
	_edit_grid.visible = show
	if _edit_grid.material_override is ShaderMaterial:
		(_edit_grid.material_override as ShaderMaterial).set_shader_parameter("overlay_alpha", 0.78 if show else 0.0)

func _sync_sun_with_active_camera() -> void:
	var vp := get_viewport()
	if vp == null or _sun_light == null:
		return
	var cam := vp.get_camera_3d()
	if cam != null:
		_sun_light.directional_shadow_max_distance = clampf(cam.size * 1.9, 65.0, 105.0)

func _build_heroes(slot_map: Dictionary) -> void:
	for entry in _layout.get("heroes", []):
		var spec: Dictionary = entry
		var holder := Node3D.new()
		holder.name = String(spec.get("name", "HeroProp"))
		var p: Array = spec.get("pos", [0.0, 0.0, 0.0])
		holder.position = Vector3(float(p[0]), float(p[1]), float(p[2]))
		holder.rotation_degrees = Vector3(0.0, float(spec.get("rot_y_deg", 0.0)), 0.0)
		_heroes_root.add_child(holder)
		var base_path := String(spec.get("model", ""))
		var h_m := float(spec.get("target_h", 2.0))
		var xz_m := float(spec.get("max_xz", 0.0))
		var lod0 := GridMath.mount_glb(holder, "%s_lod0.glb" % base_path, h_m, Vector3.ONE, Color.WHITE, xz_m)
		var lod1 := GridMath.mount_glb(holder, "%s_lod1.glb" % base_path, h_m, Vector3.ONE, Color.WHITE, xz_m)
		var lod2 := GridMath.mount_glb(holder, "%s_lod2.glb" % base_path, h_m, Vector3.ONE, Color.WHITE, xz_m)
		for lod_node in [lod0, lod1, lod2]:
			PbrMiniatureMaterial.apply_material_map(lod_node, slot_map, false)
		GridMath.apply_lod_visibility(lod0, lod1, lod2, 26.0, 42.0)
		var fp: Array = spec.get("footprint", [2.5, 2.5])
		PbrMiniatureMaterial.attach_contact_shadow(holder, Vector2(float(fp[0]), float(fp[1])), 0.42, 0.44)
		if spec.has("halo"):
			_attach_halo(holder, spec["halo"] as Dictionary)

func _attach_halo(host: Node3D, cfg: Dictionary) -> void:
	var sz := float(cfg.get("size", 2.6))
	var quad := QuadMesh.new()
	quad.size = Vector2(sz, sz)
	var mat := ShaderMaterial.new()
	mat.shader = ResourceLoader.load(HALO_SHADER_PATH) as Shader
	mat.set_shader_parameter("halo_color", Color(cfg.get("color", "#2ED6FF")))
	mat.set_shader_parameter("intensity", float(cfg.get("intensity", 0.85)))
	var mi := MeshInstance3D.new()
	mi.mesh = quad
	mi.material_override = mat
	mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var off: Array = cfg.get("offset", [0.0, 2.0, 0.0])
	mi.position = Vector3(float(off[0]), float(off[1]), float(off[2]))
	host.add_child(mi)

func _build_walls(slot_map: Dictionary) -> void:
	var cells: Dictionary = {}
	for box in _layout.get("wall_boxes", []):
		for x in range(mini(int(box[0]), int(box[2])), maxi(int(box[0]), int(box[2])) + 1):
			for z in range(mini(int(box[1]), int(box[3])), maxi(int(box[1]), int(box[3])) + 1):
				cells[Vector2i(x, z)] = true
	var p_xfs: Array[Transform3D] = []
	var c_xfs: Array[Transform3D] = []
	var s_xfs: Array[Transform3D] = []
	for cell: Vector2i in cells.keys():
		var wp := Vector3(cell.x * GridMath.CELL_SIZE, 0.0, cell.y * GridMath.CELL_SIZE)
		p_xfs.append(Transform3D(Basis.IDENTITY, wp))
		s_xfs.append(Transform3D(Basis().scaled(Vector3(1.62, 1.0, 1.62)), wp + Vector3(0.0, 0.015, 0.0)))
		if cells.has(Vector2i(cell.x + 1, cell.y)):
			c_xfs.append(Transform3D(Basis.IDENTITY, wp + Vector3(0.75, 0.0, 0.0)))
			s_xfs.append(Transform3D(Basis().scaled(Vector3(1.65, 1.0, 1.15)), wp + Vector3(0.75, 0.015, 0.0)))
		if cells.has(Vector2i(cell.x, cell.y + 1)):
			c_xfs.append(Transform3D(Basis(Vector3.UP, PI * 0.5), wp + Vector3(0.0, 0.0, 0.75)))
			s_xfs.append(Transform3D(Basis().scaled(Vector3(1.15, 1.0, 1.65)), wp + Vector3(0.0, 0.015, 0.75)))
	GridMath.populate_multimesh(_wall_pillars, PbrMiniatureMaterial.load_skinned_glb_mesh("res://assets/models/m0_bench/wall_pillar_lod0.glb", slot_map), p_xfs)
	GridMath.populate_multimesh(_wall_conns, PbrMiniatureMaterial.load_skinned_glb_mesh("res://assets/models/m0_bench/wall_connector_lod0.glb", slot_map), c_xfs)
	var unit_q := PlaneMesh.new()
	unit_q.size = Vector2.ONE
	_wall_shadows.material_override = PbrMiniatureMaterial.create_contact_shadow_material(Vector2(0.34, 0.34), 0.42)
	GridMath.populate_multimesh(_wall_shadows, unit_q, s_xfs)

func _build_nature(slot_map: Dictionary) -> void:
	var pine_xfs: Array[Transform3D] = []
	var ps_xfs: Array[Transform3D] = []
	var rock_xfs: Array[Transform3D] = []
	var cliff_xfs: Array[Transform3D] = []
	for p in _layout.get("pines", []):
		var pos := Vector3(float(p[0]), 0.0, float(p[1]))
		pine_xfs.append(Transform3D(Basis(Vector3.UP, float(p[2])).scaled(Vector3.ONE * float(p[3])), pos))
		ps_xfs.append(Transform3D(Basis().scaled(Vector3.ONE * (2.6 * float(p[3]))), pos + Vector3(0.0, 0.014, 0.0)))
	for b in _layout.get("boulders", []):
		rock_xfs.append(Transform3D(Basis(Vector3.UP, float(b[3])).scaled(Vector3.ONE * float(b[4])), Vector3(float(b[0]), float(b[1]), float(b[2]))))
	for c in _layout.get("cliffs", []):
		cliff_xfs.append(Transform3D(Basis(Vector3.UP, float(c[3])).scaled(Vector3(1.85, 1.35, 1.5)), Vector3(float(c[0]), float(c[1]), float(c[2]))))
	GridMath.populate_multimesh(_pine_batch, PbrMiniatureMaterial.load_skinned_glb_mesh("res://assets/models/m0_bench/pine_cluster_lod0.glb", slot_map), pine_xfs)
	var unit_q := PlaneMesh.new()
	unit_q.size = Vector2.ONE
	_pine_shadows.material_override = PbrMiniatureMaterial.create_contact_shadow_material(Vector2(0.32, 0.32), 0.40)
	GridMath.populate_multimesh(_pine_shadows, unit_q, ps_xfs)
	GridMath.populate_multimesh(_boulder_batch, PbrMiniatureMaterial.load_skinned_glb_mesh("res://assets/models/m0_bench/cliff_boulder_lod0.glb", slot_map), rock_xfs)
	GridMath.populate_multimesh(_cliff_batch, PbrMiniatureMaterial.load_skinned_glb_mesh("res://assets/models/m0_bench/cliff_segment_lod0.glb", slot_map), cliff_xfs)

func _load_json(path: String) -> Dictionary:
	var parsed: Variant = JSON.parse_string(FileAccess.get_file_as_string(path))
	return parsed as Dictionary if parsed is Dictionary else {}
