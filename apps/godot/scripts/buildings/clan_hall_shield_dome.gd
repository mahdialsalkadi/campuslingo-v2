class_name ClanHallShieldDome
extends VillageBuildingBase
## SEC-B06: 3x3 Clan Hall, ProtectionKind Shield Dome & Brownout indicator overlay.

const SECTION_ID := "SEC-B06"

var _dome: MeshInstance3D = null
var _dome_mat: StandardMaterial3D = null


func _init() -> void:
	archetype = "clan_hall_shield_dome"
	kind = "SHIELD"
	footprint_cells = 3


func _mount_visuals() -> void:
	super._mount_visuals()
	_ensure_shield_dome()
	_sync_dome_state()


func _apply_brownout_visual() -> void:
	super._apply_brownout_visual()
	_sync_dome_state()


func _ensure_shield_dome() -> void:
	if _dome != null:
		return
	_dome = MeshInstance3D.new()
	_dome.name = "ProtectionShieldDome"
	_dome.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var sm := SphereMesh.new()
	sm.radius = 2.65
	sm.height = 5.3
	sm.radial_segments = 20
	sm.rings = 10
	_dome.mesh = sm
	_dome.position = Vector3(0.0, 0.25, 0.0)
	_dome_mat = StandardMaterial3D.new()
	_dome_mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	_dome_mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	_dome_mat.blend_mode = BaseMaterial3D.BLEND_MODE_ADD
	_dome_mat.cull_mode = BaseMaterial3D.CULL_DISABLED
	_dome_mat.albedo_color = Color(0.24, 0.78, 1.0, 0.16)
	_dome.material_override = _dome_mat
	add_child(_dome)


func _sync_dome_state() -> void:
	if _dome == null or _dome_mat == null:
		return
	_dome.visible = alive and (raid_immune or not browned_out)
	_dome_mat.albedo_color = Color(0.32, 0.88, 1.0, 0.22) if raid_immune else Color(0.24, 0.72, 0.95, 0.10)
