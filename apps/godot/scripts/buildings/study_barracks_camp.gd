class_name StudyBarracksCamp
extends VillageBuildingBase
## SEC-B05: 3x3 Study Barracks & Army Camp (housing capacity 20->80 by tier).

const SECTION_ID := "SEC-B05"
const HOUSING_CAP := [20, 40, 60, 80]


func _init() -> void:
	archetype = "study_barracks_camp"
	kind = "ARMY"
	footprint_cells = 3


func housing_capacity() -> int:
	return int(HOUSING_CAP[clampi(tier, 0, 3)])
