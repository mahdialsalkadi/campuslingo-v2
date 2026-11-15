class_name RuneMortar
extends DefenseTurretBase
## SEC-D03: Rune Mortar — long-range parabolic artillery with 4-cell inner deadzone & 2-cell splash.

const SECTION_ID := "SEC-D03"
var _pending_impacts: Array[Dictionary] = []

func _init() -> void:
	archetype = "rune_mortar"

func step_combat(dt: float, units: Array, vfx: Node) -> void:
	super.step_combat(dt, units, vfx)
	for i in range(_pending_impacts.size() - 1, -1, -1):
		_pending_impacts[i]["t"] = float(_pending_impacts[i]["t"]) - dt
		if float(_pending_impacts[i]["t"]) <= 0.0:
			_emit_juice("mortar_impact", _pending_impacts[i]["pos"] as Vector3, 1.0)
			_pending_impacts.remove_at(i)

func _on_fired_fx(tgt: Node3D, vfx: Node) -> void:
	var muzzle := global_position + Vector3(0.0, _muzzle_y, 0.0)
	var flight_s := 0.717
	var apex := _profile.measure_mortar_apex_60hz() if _profile != null else 1.804
	if vfx != null:
		vfx.call("spawn_projectile", "mortar", muzzle, tgt.global_position + Vector3(0.0, 0.25, 0.0), flight_s, apex)
	_pending_impacts.append({"t": flight_s, "pos": tgt.global_position})
	_emit_juice("mortar_fire", muzzle, 1.0)
