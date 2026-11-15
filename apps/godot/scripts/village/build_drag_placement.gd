class_name BuildDragPlacement
extends Node3D
## SEC-G04: Village edit-mode drag placement & green/red footprint overlap validator.

const SECTION_ID := "SEC-G04"

signal building_selected(building: Node)
signal building_moved(building: Node, gx: int, gz: int)
signal move_rejected(building: Node, gx: int, gz: int)

var edit_mode: bool = false
var selected_building: Node = null
var _board: GridBoardRenderer = null
var _cursor_mesh: MeshInstance3D = null
var _cursor_mat: StandardMaterial3D = null


func _ready() -> void:
	_ensure_cursor()


func bind_board(board: GridBoardRenderer) -> void:
	_board = board


func set_edit_mode(enabled: bool) -> void:
	edit_mode = enabled
	if _board != null:
		_board.set_edit_grid_visible(enabled)
	if not enabled:
		selected_building = null
		if _cursor_mesh != null:
			_cursor_mesh.visible = false


func select_building(b: Node) -> void:
	selected_building = b
	building_selected.emit(b)
	_refresh_cursor(b, true)


func build_occupancy(buildings: Array, exclude_node: Node = null) -> PackedByteArray:
	var occ := GridMath.create_occupancy_grid()
	for b in buildings:
		if b == null or b == exclude_node or not b.has_method("get_sim_state"):
			continue
		var s: Dictionary = b.get_sim_state()
		if not bool(s.get("alive", true)):
			continue
		GridMath.set_footprint(occ, int(s.get("gx", 0)), int(s.get("gz", 0)), int(s.get("size", 1)), 1)
	return occ


func can_place(gx: int, gz: int, size: int, buildings: Array, moving_node: Node = null) -> bool:
	var occ := build_occupancy(buildings, moving_node)
	return GridMath.can_place_footprint(gx, gz, size, occ)


func preview_at(gx: int, gz: int, buildings: Array, moving_node: Node = null) -> bool:
	var size := _size_of(moving_node)
	var valid := can_place(gx, gz, size, buildings, moving_node)
	_show_cursor(gx, gz, size, valid)
	return valid


func try_move(node: Node, gx: int, gz: int, buildings: Array) -> bool:
	if node == null or not node.has_method("get_sim_state"):
		return false
	var s: Dictionary = node.get_sim_state()
	var size := int(s.get("size", 1))
	if not can_place(gx, gz, size, buildings, node):
		move_rejected.emit(node, gx, gz)
		_refresh_cursor(node, false)
		return false
	if node.has_method("set_grid_pos"):
		node.call("set_grid_pos", gx, gz)
	else:
		node.set("grid_pos", Vector2i(gx, gz))
		node.position = GridMath.cell_to_world(gx, gz, size)
	building_moved.emit(node, gx, gz)
	_show_cursor(gx, gz, size, true)
	var j := get_node_or_null("/root/Juice") if is_inside_tree() else null
	if j != null:
		j.call("emit_event", "place_ok", GridMath.cell_to_world(gx, gz, size), Vector3.UP, 0.8)
	return true


func screen_to_cell(cam: Camera3D, screen_pos: Vector2, size: int = 1) -> Vector2i:
	if cam == null:
		return Vector2i(20, 20)
	var ro := cam.project_ray_origin(screen_pos)
	var rd := cam.project_ray_normal(screen_pos)
	var hit: Variant = Plane(Vector3.UP, 0.0).intersects_ray(ro, rd)
	if hit == null:
		return Vector2i(20, 20)
	var wp: Vector3 = hit as Vector3
	var half := float(size) * 0.5 * GridMath.CELL_SIZE
	var anchor := wp - Vector3(half - GridMath.CELL_SIZE * 0.5, 0.0, half - GridMath.CELL_SIZE * 0.5)
	return GridMath.world_to_cell(anchor)


func _size_of(node: Node) -> int:
	if node != null and node.has_method("get_sim_state"):
		return int((node.call("get_sim_state") as Dictionary).get("size", 3))
	return 3


func _refresh_cursor(node: Node, valid: bool) -> void:
	if node == null or not node.has_method("get_sim_state"):
		if _cursor_mesh != null:
			_cursor_mesh.visible = false
		return
	var s: Dictionary = node.get_sim_state()
	_show_cursor(int(s.get("gx", 20)), int(s.get("gz", 20)), int(s.get("size", 3)), valid)


func _show_cursor(gx: int, gz: int, size: int, valid: bool) -> void:
	_ensure_cursor()
	if _cursor_mesh == null:
		return
	_cursor_mesh.visible = edit_mode or selected_building != null
	_cursor_mesh.position = GridMath.cell_to_world(gx, gz, size) + Vector3(0.0, 0.06, 0.0)
	var span := float(size) * GridMath.CELL_SIZE * 0.98
	(_cursor_mesh.mesh as PlaneMesh).size = Vector2(span, span)
	_cursor_mat.albedo_color = Color(0.18, 0.90, 0.42, 0.42) if valid else Color(0.95, 0.22, 0.18, 0.48)


func _ensure_cursor() -> void:
	if _cursor_mesh != null:
		return
	_cursor_mesh = MeshInstance3D.new()
	_cursor_mesh.name = "PlacementCursor"
	_cursor_mesh.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var pm := PlaneMesh.new()
	pm.size = Vector2(4.4, 4.4)
	_cursor_mesh.mesh = pm
	_cursor_mat = StandardMaterial3D.new()
	_cursor_mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	_cursor_mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	_cursor_mat.albedo_color = Color(0.18, 0.90, 0.42, 0.40)
	_cursor_mesh.material_override = _cursor_mat
	_cursor_mesh.visible = false
	add_child(_cursor_mesh)
