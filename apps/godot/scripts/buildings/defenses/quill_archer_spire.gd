class_name QuillArcherSpire
extends DefenseTurretBase
## SEC-D01: Quill Archer Spire — elevated wooden tower firing repeating bolts at Ground & Air troops.

const SECTION_ID := "SEC-D01"

func _init() -> void:
	archetype = "quill_archer_spire"

func _on_fired_fx(tgt: Node3D, vfx: Node) -> void:
	var muzzle := global_position + Vector3(0.0, _muzzle_y, 0.0)
	if vfx != null:
		vfx.call("spawn_projectile", "arrow", muzzle, tgt.global_position + Vector3(0.0, 0.75, 0.0), 0.22, 0.28)
	_emit_juice("archer_fire", muzzle, 0.9)
