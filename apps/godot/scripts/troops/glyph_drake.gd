class_name GlyphDrake
extends TroopAgentBase
## SEC-T05: Glyph Drake — flying arcane dragon bypassing walls (620 HP, 42 DPS, AIR layer y=2.4m).

const SECTION_ID := "SEC-T05"

func _init() -> void:
	unit_id = "glyph_drake"

func _strike_target(tgt: Node, s: Dictionary, vfx: Node) -> void:
	super._strike_target(tgt, s, vfx)
	var bc := GridMath.cell_to_world(int(s["gx"]), int(s["gz"]), int(s["size"]))
	if vfx != null:
		vfx.call("spawn_additive_halo", global_position + Vector3(0.0, hover_y + 0.4, 0.0), 2.2, Color(0.28, 0.88, 1.0), 1.3, 0.24)
		vfx.call("spawn_shockwave", bc + Vector3(0.0, 0.6, 0.0), 2.2, Color(0.32, 0.90, 1.0, 0.85), 0.30)
