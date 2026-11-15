class_name FortressWallTile
extends Node3D
## SEC-B04: 1x1 Auto-connecting (N/S/E/W) Fortress Wall Lv0->Lv3 batched via MultiMeshInstance3D.

const SECTION_ID := "SEC-B04"
const TIER_HP := [300.0, 480.0, 680.0, 900.0]

var archetype: String = "fortress_wall_tile"
var kind: String = "WALL"
var grid_pos: Vector2i = Vector2i(12, 12)
var footprint_cells: int = 1
var tier: int = 1
var hp: float = 480.0
var max_hp: float = 480.0
var browned_out: bool = false
var alive: bool = true

static var _mesh_cache: Dictionary = {}


func init_building(spec: Dictionary) -> void:
	grid_pos = Vector2i(int(spec.get("gx", grid_pos.x)), int(spec.get("gz", grid_pos.y)))
	tier = clampi(int(spec.get("tier", tier)), 0, 3)
	browned_out = bool(spec.get("browned_out", false))
	max_hp = float(TIER_HP[tier])
	hp = float(spec.get("hp", max_hp))
	position = GridMath.cell_to_world(grid_pos.x, grid_pos.y, 1)
	alive = hp > 0.0


func set_grid_pos(gx: int, gz: int) -> void:
	grid_pos = Vector2i(gx, gz)
	position = GridMath.cell_to_world(gx, gz, 1)


func upgrade_tier(next_tier: int = -1) -> bool:
	var target := (tier + 1) if next_tier < 0 else clampi(next_tier, 0, 3)
	if target <= tier or target > 3:
		return false
	tier = target
	max_hp = float(TIER_HP[tier])
	hp = max_hp
	var j := get_node_or_null("/root/Juice") if is_inside_tree() else null
	if j != null:
		j.call("emit_event", "build_upgrade", global_position, Vector3.UP, 0.7)
	return true


func take_damage(dmg: float) -> void:
	if not alive:
		return
	hp = maxf(0.0, hp - dmg)
	if hp <= 0.0:
		alive = false
		var j := get_node_or_null("/root/Juice") if is_inside_tree() else null
		if j != null:
			j.call("emit_event", "wall_breach", global_position, Vector3.UP, 1.0)


func reset_for_raid() -> void:
	max_hp = float(TIER_HP[tier])
	hp = max_hp
	alive = true


func upgrade_cost() -> int:
	return [0, 40, 100, 200][mini(tier + 1, 3)] if tier < 3 else 0


func get_sim_state() -> Dictionary:
	return {
		"archetype": archetype, "kind": kind, "gx": grid_pos.x, "gz": grid_pos.y,
		"size": 1, "tier": tier, "hp": int(roundf(hp)), "max_hp": int(roundf(max_hp)),
		"browned_out": browned_out, "alive": alive,
	}


static func rebuild_wall_batches(walls: Array, host: Node3D) -> void:
	if host == null:
		return
	var cells_by_pos: Dictionary = {}
	for w in walls:
		if w != null and w is FortressWallTile and (w as FortressWallTile).alive:
			var wt := w as FortressWallTile
			cells_by_pos[wt.grid_pos] = wt
	var shadow_xfs: Array[Transform3D] = []
	for t in range(4):
		var p_xfs: Array[Transform3D] = []
		var c_xfs: Array[Transform3D] = []
		for pos: Vector2i in cells_by_pos.keys():
			var wt: FortressWallTile = cells_by_pos[pos]
			if wt.tier != t:
				continue
			var wp := GridMath.cell_to_world(pos.x, pos.y, 1)
			p_xfs.append(Transform3D(Basis.IDENTITY, wp))
			shadow_xfs.append(Transform3D(Basis().scaled(Vector3(1.5, 1.0, 1.5)), wp + Vector3(0.0, 0.015, 0.0)))
			if cells_by_pos.has(Vector2i(pos.x + 1, pos.y)):
				c_xfs.append(Transform3D(Basis.IDENTITY, wp + Vector3(0.75, 0.0, 0.0)))
				shadow_xfs.append(Transform3D(Basis().scaled(Vector3(1.6, 1.0, 1.1)), wp + Vector3(0.75, 0.015, 0.0)))
			if cells_by_pos.has(Vector2i(pos.x, pos.y + 1)):
				c_xfs.append(Transform3D(Basis(Vector3.UP, PI * 0.5), wp + Vector3(0.0, 0.0, 0.75)))
				shadow_xfs.append(Transform3D(Basis().scaled(Vector3(1.1, 1.0, 1.6)), wp + Vector3(0.0, 0.015, 0.75)))
		_assign_batch(host, "WallPillarLv%d" % t, "res://assets/models/buildings/wall_lv%d_pillar_lod0.glb" % t, p_xfs)
		_assign_batch(host, "WallConnLv%d" % t, "res://assets/models/buildings/wall_lv%d_connector_lod0.glb" % t, c_xfs)
	var sh := _ensure_mm(host, "WallShadows", GeometryInstance3D.SHADOW_CASTING_SETTING_OFF)
	if sh.material_override == null:
		sh.material_override = PbrMiniatureMaterial.create_contact_shadow_material(Vector2(0.34, 0.34), 0.42)
	var uq := PlaneMesh.new()
	uq.size = Vector2.ONE
	GridMath.populate_multimesh(sh, uq, shadow_xfs)


static func _assign_batch(host: Node3D, node_name: String, glb_path: String, xfs: Array[Transform3D]) -> void:
	var mm := _ensure_mm(host, node_name, GeometryInstance3D.SHADOW_CASTING_SETTING_ON)
	mm.visible = not xfs.is_empty()
	if xfs.is_empty():
		return
	if not _mesh_cache.has(glb_path):
		_mesh_cache[glb_path] = PbrMiniatureMaterial.load_skinned_glb_mesh(glb_path, {})
	GridMath.populate_multimesh(mm, _mesh_cache[glb_path] as Mesh, xfs)


static func _ensure_mm(host: Node3D, node_name: String, shadow_mode: int) -> MultiMeshInstance3D:
	var existing := host.get_node_or_null(node_name)
	if existing is MultiMeshInstance3D:
		return existing as MultiMeshInstance3D
	var n := MultiMeshInstance3D.new()
	n.name = node_name
	n.cast_shadow = shadow_mode
	host.add_child(n)
	return n
