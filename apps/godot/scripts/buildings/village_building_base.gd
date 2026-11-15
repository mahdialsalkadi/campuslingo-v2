class_name VillageBuildingBase
extends Node3D
## Shared upgradable building base: tier GLBs, TierHP, Brownout visuals, contact shadow & upgrade FX.

const STATS_PATH := "res://data/building_stats.json"
static var _stats_cache: Dictionary = {}

var archetype: String = "gold_mine_vault"
var kind: String = "RESOURCE"
var node_id: String = ""
var title: String = ""
var tier: int = 1
var grid_pos: Vector2i = Vector2i(14, 14)
var footprint_cells: int = 3
var hp: float = 450.0
var max_hp: float = 450.0
var browned_out: bool = false
var raid_immune: bool = false
var alive: bool = true

var _mount: Node3D = null
var _shadow: Node3D = null
var _cfg: Dictionary = {}

static func load_spec(arch: String) -> Dictionary:
	if _stats_cache.is_empty() and FileAccess.file_exists(STATS_PATH):
		var p: Variant = JSON.parse_string(FileAccess.get_file_as_string(STATS_PATH))
		if p is Dictionary:
			_stats_cache = (p as Dictionary).get("buildings", {})
	return (_stats_cache.get(arch, {}) as Dictionary).duplicate(true)

func init_building(spec: Dictionary) -> void:
	archetype = String(spec.get("archetype", archetype))
	_cfg = load_spec(archetype)
	kind = String(_cfg.get("kind", kind))
	node_id = String(spec.get("node_id", spec.get("id", "")))
	title = String(spec.get("title", archetype.capitalize()))
	tier = clampi(int(spec.get("tier", tier)), 0, 3)
	footprint_cells = int(_cfg.get("size", 3))
	grid_pos = Vector2i(int(spec.get("gx", grid_pos.x)), int(spec.get("gz", grid_pos.y)))
	browned_out = bool(spec.get("browned_out", false))
	raid_immune = bool(spec.get("raid_immune", tier == 0))
	position = GridMath.cell_to_world(grid_pos.x, grid_pos.y, footprint_cells)
	_recompute_hp(spec.has("hp"))
	if spec.has("hp"):
		hp = float(spec["hp"])
	alive = hp > 0.0
	visible = alive
	_on_cfg_loaded(spec)
	_mount_visuals()

func _on_cfg_loaded(_spec: Dictionary) -> void:
	pass

func _recompute_hp(keep_hp: bool = false) -> void:
	var hp_arr: Array = _cfg.get("hp_by_tier", [])
	var base := float(hp_arr[mini(tier, hp_arr.size() - 1)]) if not hp_arr.is_empty() else float(_cfg.get("base_hp", 450)) * (1.0 + 0.45 * float(tier))
	max_hp = roundf(base * (0.70 if browned_out else 1.0))
	if not keep_hp:
		hp = max_hp

func _mount_visuals() -> void:
	if _mount != null:
		_mount.queue_free()
	_mount = Node3D.new()
	_mount.name = "MeshMount"
	add_child(_mount)
	var lv_arr: Array = _cfg.get("model_lv", [])
	var model_path := String(lv_arr[clampi(tier, 0, lv_arr.size() - 1)]) if not lv_arr.is_empty() else String(_cfg.get("model", ""))
	if model_path != "":
		var n := GridMath.mount_glb(_mount, "%s_lod0.glb" % model_path, float(_cfg.get("target_h", 2.8)) * (1.0 + 0.04 * float(tier)), Vector3.ONE * 2.2, Color.WHITE, float(_cfg.get("max_xz", 3.6)))
		PbrMiniatureMaterial.apply_material_map(n, {}, false)
		GridMath.apply_lod_visibility(n)
	if _shadow == null:
		_shadow = PbrMiniatureMaterial.attach_contact_shadow(self, Vector2.ONE * (float(footprint_cells) * GridMath.CELL_SIZE * 0.88), 0.44, 0.46)
	_apply_brownout_visual()

func _apply_brownout_visual() -> void:
	if _mount != null:
		GridMath.set_color_mul(_mount, Color(0.56, 0.48, 0.43, 1.0) if browned_out else Color.WHITE)

func set_grid_pos(gx: int, gz: int) -> void:
	grid_pos = Vector2i(gx, gz)
	position = GridMath.cell_to_world(gx, gz, footprint_cells)

func upgrade_tier(next_tier: int = -1) -> bool:
	var target := (tier + 1) if next_tier < 0 else clampi(next_tier, 0, 3)
	if target <= tier or target > 3:
		return false
	tier = target
	raid_immune = (tier == 0)
	_recompute_hp(false)
	_on_cfg_loaded({})
	_mount_visuals()
	_emit_juice("build_upgrade", global_position, 1.1)
	return true

func repair_brownout() -> void:
	browned_out = false
	_recompute_hp(false)
	_apply_brownout_visual()
	_emit_juice("build_upgrade", global_position, 0.9)

func apply_shield_snapshot(shield: Dictionary) -> void:
	var nt := clampi(int(shield.get("tier", tier)), 0, 3)
	var nb := bool(shield.get("is_brownout", browned_out))
	raid_immune = bool(shield.get("raid_immune", nt == 0))
	if nt != tier:
		tier = nt
		browned_out = nb
		_recompute_hp(false)
		_on_cfg_loaded({})
		_mount_visuals()
	elif nb != browned_out:
		browned_out = nb
		_recompute_hp(false)
		_apply_brownout_visual()

func upgrade_cost() -> int:
	if tier >= 3:
		return 0
	var costs: Array = _cfg.get("upgrade_cost_gold", [0, 200, 450, 850])
	return int(costs[mini(tier + 1, costs.size() - 1)]) if not costs.is_empty() else 250

func take_damage(dmg: float) -> void:
	if not alive:
		return
	hp = maxf(0.0, hp - dmg)
	if hp <= 0.0:
		alive = false
		visible = false
		_emit_juice("destroy", global_position, 1.15)

func reset_for_raid() -> void:
	_recompute_hp(false)
	alive = true
	visible = true

func get_sim_state() -> Dictionary:
	return {
		"archetype": archetype, "kind": kind, "node_id": node_id, "title": title,
		"gx": grid_pos.x, "gz": grid_pos.y, "size": footprint_cells, "tier": tier,
		"hp": int(roundf(hp)), "max_hp": int(roundf(max_hp)),
		"browned_out": browned_out, "raid_immune": raid_immune, "alive": alive,
	}

func _emit_juice(ev: String, pos: Vector3, scale_f: float = 1.0) -> void:
	var j := get_node_or_null("/root/Juice") if is_inside_tree() else null
	if j != null:
		j.call("emit_event", ev, pos, Vector3.UP, scale_f)
