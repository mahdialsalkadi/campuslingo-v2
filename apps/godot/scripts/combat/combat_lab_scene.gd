class_name CombatLabScene
extends Node3D
## Interactive & captured M1 Combat Lab benchmarking Defenses (SEC-D01..D04), Troops (SEC-T01..T05), Walls & VFX.

@onready var _board: GridBoardRenderer = $IslandBoard
@onready var _camera: CameraPanZoom = $CameraPanZoom
@onready var _vfx: CombatBuildParticles = $CombatBuildParticles
@onready var _buildings_root: Node3D = $Buildings
@onready var _troops_root: Node3D = $Troops
@onready var _wall_pillars: MultiMeshInstance3D = $WallPillarBatch
@onready var _wall_conns: MultiMeshInstance3D = $WallConnectorBatch
@onready var _wall_shadows: MultiMeshInstance3D = $WallShadowBatch

var _buildings: Array = []
var _troops: Array = []
var _wall_map: Dictionary = {}
var _objective_anchor: Node3D = null

class WallProxy extends Node3D:
	var gx: int; var gz: int; var hp: float = 420.0; var max_hp: float = 420.0; var alive: bool = true
	func _init(x: int, z: int) -> void:
		gx = x; gz = z; position = GridMath.cell_to_world(gx, gz, 1)
	func take_damage(dmg: float) -> void:
		if not alive: return
		hp = maxf(0.0, hp - dmg)
		if hp <= 0.0:
			alive = false; visible = false
			var j := get_node_or_null("/root/Juice") if is_inside_tree() else null
			if j != null: j.call("emit_event", "wall_breach", global_position, Vector3.UP, 1.0)
	func get_sim_state() -> Dictionary:
		return {"archetype": "fortress_wall_tile", "kind": "WALL", "gx": gx, "gz": gz, "size": 1, "hp": int(roundf(hp)), "max_hp": int(roundf(max_hp)), "alive": alive}

class ObjectiveDummy extends Node3D:
	var gx: int = 18; var gz: int = 17; var size: int = 4; var hp: float = 1800.0; var max_hp: float = 1800.0; var alive: bool = true
	func take_damage(dmg: float) -> void:
		if not alive: return
		hp = maxf(0.0, hp - dmg)
		if hp <= 0.0:
			alive = false; visible = false
			var j := get_node_or_null("/root/Juice") if is_inside_tree() else null
			if j != null: j.call("emit_event", "destroy", global_position, Vector3.UP, 1.2)
	func get_sim_state() -> Dictionary:
		return {"archetype": "core_archive_townhall", "kind": "TOWNHALL", "gx": gx, "gz": gz, "size": size, "hp": int(roundf(hp)), "max_hp": int(roundf(max_hp)), "alive": alive}

func _ready() -> void:
	var j := get_node_or_null("/root/Juice")
	if j != null: j.call("bind_scene_targets", _camera, _vfx)
	_build_objective_citadel()
	_build_walls()
	_spawn_defenses()
	_spawn_troops()
	_wall_map = AStarWallPathfinder.build_wall_map(_buildings.map(func(b): return b.get_sim_state()))

func _physics_process(delta: float) -> void:
	_board.sync_camera_lighting(get_viewport().get_camera_3d())
	_wall_map = AStarWallPathfinder.build_wall_map(_buildings.map(func(b): return b.get_sim_state()))
	for b in _buildings:
		if b.has_method("step_combat"): b.step_combat(delta, _troops, _vfx)
	for t in _troops:
		if t.has_method("step_unit"): t.step_unit(delta, _buildings, _wall_map, _vfx)

func _build_objective_citadel() -> void:
	var obj := ObjectiveDummy.new(); obj.name = "ObjectiveCitadel"
	obj.position = GridMath.cell_to_world(obj.gx, obj.gz, obj.size); _buildings_root.add_child(obj)
	var m := GridMath.mount_glb(obj, "res://assets/models/buildings/core_archive_townhall_lv2_lod0.glb", 4.1, Vector3.ONE * 4.0, Color.WHITE, 5.2)
	PbrMiniatureMaterial.apply_material_map(m, {}, false)
	PbrMiniatureMaterial.attach_contact_shadow(obj, Vector2(5.2, 5.2), 0.45, 0.46)
	_buildings.append(obj); _objective_anchor = obj

func _build_walls() -> void:
	var cells: Dictionary = {}
	for x in range(12, 28):
		cells[Vector2i(x, 24)] = true
		if x < 17 or x > 22: cells[Vector2i(x, 13)] = true
	for z in range(14, 25):
		cells[Vector2i(12, z)] = true; cells[Vector2i(27, z)] = true
	var p_xfs: Array[Transform3D] = []; var c_xfs: Array[Transform3D] = []; var s_xfs: Array[Transform3D] = []
	for cell: Vector2i in cells.keys():
		var wp := GridMath.cell_to_world(cell.x, cell.y, 1)
		var proxy := WallProxy.new(cell.x, cell.y); _buildings_root.add_child(proxy); _buildings.append(proxy)
		p_xfs.append(Transform3D(Basis.IDENTITY, wp))
		s_xfs.append(Transform3D(Basis().scaled(Vector3(1.5, 1.0, 1.5)), wp + Vector3(0.0, 0.015, 0.0)))
		if cells.has(Vector2i(cell.x + 1, cell.y)):
			c_xfs.append(Transform3D(Basis.IDENTITY, wp + Vector3(0.75, 0.0, 0.0)))
			s_xfs.append(Transform3D(Basis().scaled(Vector3(1.6, 1.0, 1.1)), wp + Vector3(0.75, 0.015, 0.0)))
		if cells.has(Vector2i(cell.x, cell.y + 1)):
			c_xfs.append(Transform3D(Basis(Vector3.UP, PI * 0.5), wp + Vector3(0.0, 0.0, 0.75)))
			s_xfs.append(Transform3D(Basis().scaled(Vector3(1.1, 1.0, 1.6)), wp + Vector3(0.0, 0.015, 0.75)))
	GridMath.populate_multimesh(_wall_pillars, PbrMiniatureMaterial.load_skinned_glb_mesh("res://assets/models/buildings/wall_lv1_pillar_lod0.glb", {}), p_xfs)
	GridMath.populate_multimesh(_wall_conns, PbrMiniatureMaterial.load_skinned_glb_mesh("res://assets/models/buildings/wall_lv1_connector_lod0.glb", {}), c_xfs)
	var uq := PlaneMesh.new(); uq.size = Vector2.ONE
	_wall_shadows.material_override = PbrMiniatureMaterial.create_contact_shadow_material(Vector2(0.34, 0.34), 0.42)
	GridMath.populate_multimesh(_wall_shadows, uq, s_xfs)

func _spawn_defenses() -> void:
	var specs := [
		[QuillArcherSpire, {"gx": 14, "gz": 15, "tier": 1}],
		[InkCannon, {"gx": 23, "gz": 15, "tier": 1}],
		[InkCannon, {"gx": 14, "gz": 20, "tier": 0, "browned_out": true}],
		[RuneMortar, {"gx": 23, "gz": 20, "tier": 1}],
		[HiddenProctorTesla, {"gx": 19, "gz": 25, "tier": 1, "revealed": true}],
	]
	for item in specs:
		var d: DefenseTurretBase = item[0].new()
		_buildings_root.add_child(d); d.init_building(item[1]); _buildings.append(d)

func _spawn_troops() -> void:
	var wave := [
		[SquireFootman, 16, 27], [SquireFootman, 19, 28], [SquireFootman, 22, 27],
		[LexiArcher, 16, 29], [LexiArcher, 20, 29], [LexiArcher, 23, 29],
		[ThesisBreaker, 17, 26], [ThesisBreaker, 22, 26],
		[FormulaGiant, 24, 27], [GlyphDrake, 14, 27],
	]
	for item in wave:
		var t: TroopAgentBase = item[0].new()
		_troops_root.add_child(t)
		t.init_unit({"pos": GridMath.cell_to_world(int(item[1]), int(item[2]), 1)})
		_troops.append(t)
