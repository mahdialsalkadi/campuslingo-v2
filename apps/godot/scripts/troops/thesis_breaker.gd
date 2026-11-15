class_name ThesisBreaker
extends TroopAgentBase
## SEC-T03: Thesis Breaker — wall-breaching demolition sapper (75 HP, 25 DPS / 100 DPS vs Wall).

const SECTION_ID := "SEC-T03"

func _init() -> void:
	unit_id = "thesis_breaker"

func _strike_target(tgt: Node, s: Dictionary, vfx: Node) -> void:
	super._strike_target(tgt, s, vfx)
	if String(s.get("kind", "")) == "WALL":
		var bc := GridMath.cell_to_world(int(s["gx"]), int(s["gz"]), int(s["size"]))
		_emit_juice("wall_breach", bc, 0.9)
		if vfx != null:
			vfx.call("spawn_additive_halo", bc + Vector3(0.0, 0.8, 0.0), 2.1, Color(1.0, 0.56, 0.18), 1.35, 0.25)
