class_name CoreArchiveTownHall
extends VillageBuildingBase
## SEC-B01: 4x4 Core Archive / Town Hall (Lv0->Lv3 visual evolution, awards 1* on destroy).

const SECTION_ID := "SEC-B01"

signal townhall_destroyed


func _init() -> void:
	archetype = "core_archive_townhall"
	kind = "TOWNHALL"
	footprint_cells = 4
	tier = 2


func take_damage(dmg: float) -> void:
	var was_alive := alive
	super.take_damage(dmg)
	if was_alive and not alive:
		townhall_destroyed.emit()
