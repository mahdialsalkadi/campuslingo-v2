class_name GridBoardRenderer
extends Node3D
## SEC-G02: Sculpted 40x40 grass plateau, animated water moat, perimeter forest & edit grid overlay.

const SECTION_ID := "SEC-G02"
const NATURE_LAYOUT_PATH := "res://data/m0_diorama_layout.json"

@onready var world_env: WorldEnvironment = $WorldEnvironment
@onready var sun_light: DirectionalLight3D = $SunLight
@onready var _island_plateau: MeshInstance3D = $IslandPlateau
@onready var _water_moat: MeshInstance3D = $WaterMoat
@onready var _edit_grid: MeshInstance3D = $EditGridOverlay
@onready var _pine_batch: MultiMeshInstance3D = $PineForestBatch
@onready var _pine_shadows: MultiMeshInstance3D = $PineShadowBatch
@onready var _boulder_batch: MultiMeshInstance3D = $BoulderBatch
@onready var _cliff_batch: MultiMeshInstance3D = $CliffRimBatch

var _grid_visible: bool = false


func _ready() -> void:
	PbrMiniatureMaterial.configure_diorama_lighting(world_env, sun_light, 34.0)
	_island_plateau.material_override = PbrMiniatureMaterial.create_island_terrain_material()
	_water_moat.material_override = PbrMiniatureMaterial.create_water_moat_material()
	_edit_grid.material_override = PbrMiniatureMaterial.create_edit_grid_material(0.0)
	set_edit_grid_visible("--show-edit-grid" in OS.get_cmdline_user_args())
	_populate_nature_perimeter()


func set_edit_grid_visible(show: bool) -> void:
	_grid_visible = show
	if _edit_grid == null:
		return
	_edit_grid.visible = show
	if _edit_grid.material_override is ShaderMaterial:
		(_edit_grid.material_override as ShaderMaterial).set_shader_parameter("overlay_alpha", 0.78 if show else 0.0)


func is_edit_grid_visible() -> bool:
	return _grid_visible


func sync_camera_lighting(cam: Camera3D) -> void:
	if cam == null or sun_light == null:
		return
	sun_light.directional_shadow_max_distance = clampf(cam.size * 1.9, 65.0, 105.0)


func _populate_nature_perimeter() -> void:
	if not FileAccess.file_exists(NATURE_LAYOUT_PATH):
		return
	var parsed: Variant = JSON.parse_string(FileAccess.get_file_as_string(NATURE_LAYOUT_PATH))
	if not (parsed is Dictionary):
		return
	var layout: Dictionary = parsed as Dictionary
	var slots: Dictionary = layout.get("material_slots", {})
	var pine_xfs: Array[Transform3D] = []
	var ps_xfs: Array[Transform3D] = []
	var rock_xfs: Array[Transform3D] = []
	var cliff_xfs: Array[Transform3D] = []
	for p in layout.get("pines", []):
		var pos := Vector3(float(p[0]), 0.0, float(p[1]))
		pine_xfs.append(Transform3D(Basis(Vector3.UP, float(p[2])).scaled(Vector3.ONE * float(p[3])), pos))
		ps_xfs.append(Transform3D(Basis().scaled(Vector3.ONE * (2.6 * float(p[3]))), pos + Vector3(0.0, 0.014, 0.0)))
	for b in layout.get("boulders", []):
		rock_xfs.append(Transform3D(Basis(Vector3.UP, float(b[3])).scaled(Vector3.ONE * float(b[4])), Vector3(float(b[0]), float(b[1]), float(b[2]))))
	for c in layout.get("cliffs", []):
		cliff_xfs.append(Transform3D(Basis(Vector3.UP, float(c[3])).scaled(Vector3(1.85, 1.35, 1.5)), Vector3(float(c[0]), float(c[1]), float(c[2]))))
	GridMath.populate_multimesh(_pine_batch, PbrMiniatureMaterial.load_skinned_glb_mesh("res://assets/models/m0_bench/pine_cluster_lod0.glb", slots), pine_xfs)
	var unit_q := PlaneMesh.new()
	unit_q.size = Vector2.ONE
	_pine_shadows.material_override = PbrMiniatureMaterial.create_contact_shadow_material(Vector2(0.32, 0.32), 0.40)
	GridMath.populate_multimesh(_pine_shadows, unit_q, ps_xfs)
	GridMath.populate_multimesh(_boulder_batch, PbrMiniatureMaterial.load_skinned_glb_mesh("res://assets/models/m0_bench/cliff_boulder_lod0.glb", slots), rock_xfs)
	GridMath.populate_multimesh(_cliff_batch, PbrMiniatureMaterial.load_skinned_glb_mesh("res://assets/models/m0_bench/cliff_segment_lod0.glb", slots), cliff_xfs)
