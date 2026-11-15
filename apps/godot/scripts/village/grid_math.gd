class_name GridMath
extends RefCounted
## SEC-G01: 40x40 Isometric Grid Math, GLB Cleaner, Animation & Material Helpers.

const GRID_SIZE: int = 40
const CELL_SIZE: float = 1.5
const HALF_ARENA_M: float = (GRID_SIZE * CELL_SIZE) * 0.5 ## 30.0 m
const CORE_MIN: int = 4
const CORE_MAX: int = 35 ## [4..35] = 32x32 buildable interior

static func cell_to_world(gx: int, gz: int, footprint_cells: int = 1) -> Vector3:
	var half_span: float = GRID_SIZE * 0.5
	return Vector3(((float(gx) + float(footprint_cells) * 0.5) - half_span) * CELL_SIZE, 0.0, ((float(gz) + float(footprint_cells) * 0.5) - half_span) * CELL_SIZE)

static func world_to_cell(pos: Vector3) -> Vector2i:
	var half_span: float = GRID_SIZE * 0.5
	return Vector2i(clampi(floori(pos.x / CELL_SIZE + half_span), 0, GRID_SIZE - 1), clampi(floori(pos.z / CELL_SIZE + half_span), 0, GRID_SIZE - 1))

static func is_in_bounds(gx: int, gz: int, size: int = 1) -> bool:
	return gx >= 0 and gz >= 0 and (gx + size) <= GRID_SIZE and (gz + size) <= GRID_SIZE

static func is_core_buildable(gx: int, gz: int, size: int = 1) -> bool:
	return size > 0 and gx >= CORE_MIN and gz >= CORE_MIN and (gx + size - 1) <= CORE_MAX and (gz + size - 1) <= CORE_MAX

static func is_deploy_perimeter(gx: int, gz: int) -> bool:
	return is_in_bounds(gx, gz, 1) and not is_core_buildable(gx, gz, 1)

static func create_occupancy_grid() -> PackedByteArray:
	var arr := PackedByteArray(); arr.resize(GRID_SIZE * GRID_SIZE); arr.fill(0); return arr

static func can_place_footprint(gx: int, gz: int, size: int, occupied: PackedByteArray) -> bool:
	if not is_core_buildable(gx, gz, size): return false
	for dz in range(size):
		for dx in range(size):
			if occupied[(gz + dz) * GRID_SIZE + (gx + dx)] != 0: return false
	return true

static func set_footprint(occupied: PackedByteArray, gx: int, gz: int, size: int, val: int) -> void:
	for dz in range(size):
		for dx in range(size):
			if is_in_bounds(gx + dx, gz + dz, 1): occupied[(gz + dz) * GRID_SIZE + (gx + dx)] = val

static func can_deploy_troop(world_pos: Vector3, occupied: PackedByteArray) -> bool:
	if absf(world_pos.x) > HALF_ARENA_M or absf(world_pos.z) > HALF_ARENA_M: return false
	var cell: Vector2i = world_to_cell(world_pos)
	if occupied.size() == GRID_SIZE * GRID_SIZE and occupied[cell.y * GRID_SIZE + cell.x] != 0: return false
	return is_deploy_perimeter(cell.x, cell.y)

static func cell_distance(a: Vector3, b: Vector3) -> float:
	return Vector2(a.x - b.x, a.z - b.z).length() / CELL_SIZE
static func dist_to_footprint(pos: Vector3, b: Dictionary) -> float:
	var half: float = GRID_SIZE * 0.5
	var cx: float = clampf(pos.x, (float(b["gx"]) - half) * CELL_SIZE, (float(b["gx"]) + float(b["size"]) - half) * CELL_SIZE)
	var cz: float = clampf(pos.z, (float(b["gz"]) - half) * CELL_SIZE, (float(b["gz"]) + float(b["size"]) - half) * CELL_SIZE)
	return Vector2(pos.x - cx, pos.z - cz).length() / CELL_SIZE

static func mount_glb(host: Node3D, glb_path: String, target_h: float, fallback_size: Vector3, fallback_color: Color, max_xz_m: float = 0.0) -> Node3D:
	if glb_path != "" and ResourceLoader.exists(glb_path):
		var packed: Resource = ResourceLoader.load(glb_path)
		if packed is PackedScene:
			var inst: Node = (packed as PackedScene).instantiate()
			if inst is Node3D:
				var node3d := inst as Node3D
				var aabb: AABB = _measure_aabb(node3d, Transform3D.IDENTITY)
				var s: float = target_h / maxf(aabb.size.y, 0.05)
				if max_xz_m > 0.0:
					s = minf(s, max_xz_m / maxf(maxf(aabb.size.x, aabb.size.z), 0.05))
				node3d.scale = Vector3.ONE * s
				node3d.position = Vector3(-(aabb.position.x + aabb.size.x * 0.5) * s, -aabb.position.y * s, -(aabb.position.z + aabb.size.z * 0.5) * s)
				host.add_child(node3d)
				return node3d
	var mesh_inst := MeshInstance3D.new()
	var box := BoxMesh.new(); box.size = fallback_size
	var mat := StandardMaterial3D.new()
	mat.albedo_color = fallback_color; mat.roughness = 0.78; mat.metallic = 0.0
	mesh_inst.mesh = box; mesh_inst.material_override = mat
	mesh_inst.position = Vector3(0.0, fallback_size.y * 0.5, 0.0)
	host.add_child(mesh_inst)
	return mesh_inst
static func apply_lod_visibility(lod0: Node3D, lod1: Node3D = null, lod2: Node3D = null, begin_lod1: float = 28.0, begin_lod2: float = 42.0) -> void:
	_set_vis_range(lod0, 0.0, begin_lod1 if lod1 != null else 0.0)
	if lod1 != null:
		_set_vis_range(lod1, begin_lod1, begin_lod2 if lod2 != null else 0.0)
	if lod2 != null:
		_set_vis_range(lod2, begin_lod2, 0.0)

static func _set_vis_range(root: Node, r_begin: float, r_end: float) -> void:
	if root is GeometryInstance3D:
		var gi := root as GeometryInstance3D
		gi.visibility_range_begin = r_begin
		gi.visibility_range_end = r_end
	for c in root.get_children():
		_set_vis_range(c, r_begin, r_end)

static func play_anim(root: Node, anim_substr: String) -> void:
	var ap: AnimationPlayer = _find_anim_player(root)
	if ap == null: return
	var q: String = anim_substr.to_lower(); var chosen: String = ""
	for a in ap.get_animation_list():
		if String(a).to_lower().find(q) >= 0: chosen = String(a); break
	if chosen == "" and ap.has_animation("Idle"): chosen = "Idle"
	if chosen != "":
		var anim: Animation = ap.get_animation(chosen)
		if anim != null: anim.loop_mode = Animation.LOOP_LINEAR
		if ap.current_animation != chosen: ap.play(chosen)

static func tint_meshes(root: Node, mat: Material) -> void:
	## Applies a miniature PBR override material without mutating metallic or roughness properties.
	if root is MeshInstance3D: (root as MeshInstance3D).material_override = mat
	for c in root.get_children(): tint_meshes(c, mat)

static func set_color_mul(root: Node, mul: Color) -> void:
	if root is MeshInstance3D and (root as MeshInstance3D).mesh != null:
		var mi := root as MeshInstance3D
		for i in range(mi.mesh.get_surface_count()):
			var sm := mi.get_surface_override_material(i)
			if sm is ShaderMaterial:
				var d := (sm as ShaderMaterial).duplicate() as ShaderMaterial
				d.set_shader_parameter("color_mul", mul); mi.set_surface_override_material(i, d)
	for c in root.get_children(): set_color_mul(c, mul)
static func populate_multimesh(node: MultiMeshInstance3D, mesh: Mesh, xfs: Array[Transform3D]) -> void:
	var mm := MultiMesh.new()
	mm.transform_format = MultiMesh.TRANSFORM_3D
	mm.use_custom_data = true
	mm.mesh = mesh
	mm.instance_count = xfs.size()
	for i in range(xfs.size()):
		mm.set_instance_transform(i, xfs[i]); mm.set_instance_custom_data(i, Color(1.0, 0.0, 0.0, 1.0))
	node.multimesh = mm

static func _find_anim_player(n: Node) -> AnimationPlayer:
	if n is AnimationPlayer: return n as AnimationPlayer
	for c in n.get_children():
		var found := _find_anim_player(c)
		if found != null: return found
	return null

static func _measure_aabb(node: Node3D, xf: Transform3D) -> AABB:
	if not node.visible: return AABB()
	var cur: Transform3D = xf * node.transform if node.get_parent() != null else Transform3D.IDENTITY
	var combined := AABB(); var has_box: bool = false
	if node is VisualInstance3D:
		var b: AABB = (node as VisualInstance3D).get_aabb()
		if b.size.length() > 0.001: combined = cur * b; has_box = true
	for child in node.get_children():
		if child is Node3D:
			var cb: AABB = _measure_aabb(child as Node3D, cur)
			if cb.size.length() > 0.001: combined = combined.merge(cb) if has_box else cb; has_box = true
	return combined if has_box else AABB(Vector3(-0.5, 0.0, -0.5), Vector3(1.0, 1.0, 1.0))
