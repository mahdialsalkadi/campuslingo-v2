class_name FormulaGiant
extends TroopAgentBase
## SEC-T04: Formula Giant — armored siege juggernaut prioritizing DEFENSE buildings (540 HP, 26 DPS).

const SECTION_ID := "SEC-T04"

func _init() -> void:
	unit_id = "formula_giant"

func _strike_target(tgt: Node, s: Dictionary, vfx: Node) -> void:
	super._strike_target(tgt, s, vfx)
	var bc := GridMath.cell_to_world(int(s["gx"]), int(s["gz"]), int(s["size"]))
	if vfx != null:
		vfx.call("spawn_shockwave", bc + Vector3(0.0, 0.08, 0.0), 1.9, Color(1.0, 0.82, 0.42, 0.8), 0.26)
