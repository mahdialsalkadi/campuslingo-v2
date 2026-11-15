class_name FieldScholarHero
extends TroopAgentBase
## SEC-T06: Field Scholar Hero (950 HP, 55 DPS, Rally Recital +30% atk/spd within 5.0 cells for 6.0 s).

const SECTION_ID := "SEC-T06"

var rally_cooldown: float = 0.0
var rally_count: int = 0


func _init() -> void:
	unit_id = "field_scholar_hero"


func step_unit(dt: float, buildings: Array, wall_map: Dictionary, vfx: Node) -> void:
	rally_cooldown = maxf(0.0, rally_cooldown - dt)
	super.step_unit(dt, buildings, wall_map, vfx)


func trigger_rally(allies: Array, vfx: Node) -> int:
	if not alive or rally_cooldown > 0.0:
		return 0
	rally_cooldown = 12.0
	rally_count += 1
	var cfg := load_spec(unit_id)
	var radius := float(cfg.get("rally_radius_cells", 5.0))
	var mult := float(cfg.get("rally_buff_mult", 1.30))
	var dur := float(cfg.get("rally_dur", 6.0))
	var buffed := 0
	for a in allies:
		if a != null and a.has_method("is_alive") and a.is_alive():
			if GridMath.cell_distance(global_position, (a as Node3D).global_position) <= radius:
				a.call("apply_rally_buff", mult, dur)
				buffed += 1
	if vfx != null:
		vfx.call("spawn_event", "hero_rally", global_position, Vector3.UP, 1.0)
	_emit_juice("hero_rally", global_position, 1.1)
	return buffed
