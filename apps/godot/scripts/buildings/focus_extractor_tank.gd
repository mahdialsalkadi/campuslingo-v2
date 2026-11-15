class_name FocusExtractorTank
extends VillageBuildingBase
## SEC-B03: 3x3 Focus Elixir Extractor & Tank (Lv0->Lv3, powers troop training & overcharge).

const SECTION_ID := "SEC-B03"

var stored_focus: float = 90.0


func _init() -> void:
	archetype = "focus_extractor_tank"
	kind = "RESOURCE"
	footprint_cells = 3


func step_production(dt: float) -> void:
	if not alive:
		return
	var cap := 250.0 * float(tier + 1)
	var rate := 2.0 * (1.0 + 0.35 * float(tier)) * (0.65 if browned_out else 1.0)
	stored_focus = minf(cap, stored_focus + rate * dt)


func collect_focus() -> int:
	var amt := int(floorf(stored_focus))
	stored_focus -= float(amt)
	if amt > 0:
		_emit_juice("collect_elixir", global_position + Vector3(0.0, 1.5, 0.0), 0.9)
	return amt


func _mount_visuals() -> void:
	super._mount_visuals()
	if _mount == null:
		return
	var glass := StandardMaterial3D.new()
	glass.albedo_color = Color(0.36, 0.12, 0.62, 0.90)
	glass.metallic = 0.18
	glass.roughness = 0.14
	glass.emission_enabled = true
	glass.emission = Color(0.38, 0.10, 0.68)
	glass.emission_energy_multiplier = 0.24
	for node in _mount.find_children("*", "MeshInstance3D", true, false):
		var mi := node as MeshInstance3D
		if mi.mesh != null:
			for sidx in range(mi.mesh.get_surface_count()):
				var mat := mi.mesh.surface_get_material(sidx)
				if (mat != null and mat.resource_name.to_lower().contains("elixir")) or (sidx == 2 and mi.mesh.get_surface_count() >= 5):
					mi.set_surface_override_material(sidx, glass)
