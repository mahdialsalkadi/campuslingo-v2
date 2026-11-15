class_name InkCannon
extends DefenseTurretBase
## SEC-D02: Ink Cannon — ground-only naval carriage cannon with Brownout -35% fire rate penalty.

const SECTION_ID := "SEC-D02"

func _init() -> void:
	archetype = "ink_cannon"

func _on_fired_fx(tgt: Node3D, vfx: Node) -> void:
	var muzzle := global_position + Vector3(0.0, _muzzle_y, 0.0)
	if vfx != null:
		vfx.call("spawn_projectile", "cannonball", muzzle, tgt.global_position + Vector3(0.0, 0.45, 0.0), 0.24, 0.14)
	_emit_juice("cannon_fire", muzzle, 0.95)
