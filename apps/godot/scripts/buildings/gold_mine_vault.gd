class_name GoldMineVault
extends VillageBuildingBase
## SEC-B02: 3x3 Knowledge Gold Collector & Vault (Lv0->Lv3, Brownout reduces output -35%).

const SECTION_ID := "SEC-B02"

var stored_gold: float = 120.0


func _init() -> void:
	archetype = "gold_mine_vault"
	kind = "RESOURCE"
	footprint_cells = 3


func step_production(dt: float) -> void:
	if not alive:
		return
	var cap := 300.0 * float(tier + 1)
	var rate := 2.4 * (1.0 + 0.35 * float(tier)) * (0.65 if browned_out else 1.0)
	stored_gold = minf(cap, stored_gold + rate * dt)


func collect_gold() -> int:
	var amt := int(floorf(stored_gold))
	stored_gold -= float(amt)
	if amt > 0:
		_emit_juice("collect_gold", global_position + Vector3(0.0, 1.5, 0.0), 0.9)
	return amt
