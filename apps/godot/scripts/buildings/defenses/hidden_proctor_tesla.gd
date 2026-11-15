class_name HiddenProctorTesla
extends DefenseTurretBase
## SEC-D04: Hidden Proctor Tesla — concealed underground spire rising via clip_y_plane at <= 4.0 cells.

const SECTION_ID := "SEC-D04"
var _spire_node: Node3D = null
var _reveal_progress: float = 0.0

func _init() -> void:
	archetype = "hidden_proctor_tesla"

func _mount_meshes(cfg: Dictionary) -> void:
	super._mount_meshes(cfg)
	if _spire_node == null and cfg.has("spire_model"):
		_spire_node = GridMath.mount_glb(self, "%s_lod0.glb" % String(cfg["spire_model"]), float(cfg.get("target_h", 2.55)), Vector3.ONE, Color.WHITE, float(cfg.get("max_xz", 2.0)))
		_apply_spire_clip()
		GridMath.apply_lod_visibility(_spire_node)
	_reveal_progress = 1.0 if revealed else 0.0
	_update_spire_transform()

func _apply_spire_clip() -> void:
	_set_clip_recursive(_spire_node)

func _set_clip_recursive(n: Node) -> void:
	if n is MeshInstance3D and (n as MeshInstance3D).mesh != null:
		var mi := n as MeshInstance3D
		for i in range(mi.mesh.get_surface_count()):
			var base_mat := mi.mesh.surface_get_material(i)
			var key := String(base_mat.resource_name).to_lower() if base_mat != null else "metal"
			var m := PbrMiniatureMaterial.get_pbr_material(key, false).duplicate() as ShaderMaterial
			m.set_shader_parameter("clip_y_plane", 0.02)
			mi.set_surface_override_material(i, m)
	for c in n.get_children():
		_set_clip_recursive(c)

func step_combat(dt: float, units: Array, vfx: Node) -> void:
	super.step_combat(dt, units, vfx)
	var tgt_p := 1.0 if revealed else 0.0
	_reveal_progress = move_toward(_reveal_progress, tgt_p, dt * 3.8)
	_update_spire_transform()

func _update_spire_transform() -> void:
	if _spire_node != null:
		_spire_node.position.y = lerpf(-2.65, 0.0, _reveal_progress)
		_spire_node.visible = _reveal_progress > 0.04

func _on_fired_fx(tgt: Node3D, vfx: Node) -> void:
	var muzzle := global_position + Vector3(0.0, _muzzle_y, 0.0)
	if vfx != null:
		vfx.call("spawn_tesla_arc", muzzle, tgt.global_position + Vector3(0.0, 0.65, 0.0))
	_emit_juice("tesla_zap", muzzle, 0.85)
