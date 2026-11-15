class_name LayoutPersistence
extends RefCounted
## SEC-G05: Atomic village JSON persistence (user://village_layout_<course>.json) & DAG starter layout.

const SECTION_ID := "SEC-G05"


static func layout_path(course_id: String) -> String:
	var clean := ""
	for i in range(course_id.length()):
		var ch := course_id.unicode_at(i)
		var ok := (ch >= 48 and ch <= 57) or (ch >= 97 and ch <= 122) or (ch >= 65 and ch <= 90) or ch == 95 or ch == 45
		clean += course_id.substr(i, 1) if ok else "_"
	if clean == "":
		clean = "default"
	return "user://village_layout_%s.json" % clean


static func save_layout(course_id: String, buildings: Array) -> bool:
	var path := layout_path(course_id)
	var rows: Array = []
	for item in buildings:
		if item is Dictionary:
			rows.append((item as Dictionary).duplicate())
		elif item != null and item.has_method("get_sim_state"):
			var s: Dictionary = item.get_sim_state()
			rows.append({
				"id": String(s.get("node_id", "")),
				"archetype": String(s.get("archetype", "")),
				"gx": int(s.get("gx", 18)),
				"gz": int(s.get("gz", 18)),
				"tier": int(s.get("tier", 1)),
				"browned_out": bool(s.get("browned_out", false)),
			})
	var payload := {"version": 1, "course_id": course_id, "buildings": rows}
	var json_str := JSON.stringify(payload, "\t")
	var tmp := path + ".tmp"
	var f := FileAccess.open(tmp, FileAccess.WRITE)
	if f == null:
		return false
	f.store_string(json_str)
	f.close()
	var bak := path + ".bak"
	if FileAccess.file_exists(path):
		DirAccess.copy_absolute(ProjectSettings.globalize_path(path), ProjectSettings.globalize_path(bak))
	var err := DirAccess.rename_absolute(ProjectSettings.globalize_path(tmp), ProjectSettings.globalize_path(path))
	return err == OK


static func load_layout(course_id: String) -> Array:
	var path := layout_path(course_id)
	for candidate in [path, path + ".bak"]:
		if not FileAccess.file_exists(candidate):
			continue
		var parsed: Variant = JSON.parse_string(FileAccess.get_file_as_string(candidate))
		if parsed is Dictionary and (parsed as Dictionary).get("buildings") is Array:
			return (parsed as Dictionary)["buildings"] as Array
	return []


static func build_starter_village(fortress_nodes: Array = []) -> Array:
	var specs: Array = [
		{"archetype": "core_archive_townhall", "gx": 18, "gz": 17, "tier": 2},
		{"archetype": "gold_mine_vault", "gx": 14, "gz": 14, "tier": 2},
		{"archetype": "focus_extractor_tank", "gx": 23, "gz": 14, "tier": 1},
		{"archetype": "study_barracks_camp", "gx": 14, "gz": 21, "tier": 1},
		{"archetype": "clan_hall_shield_dome", "gx": 23, "gz": 21, "tier": 2},
		{"archetype": "quill_archer_spire", "gx": 8, "gz": 15, "tier": 1},
		{"archetype": "ink_cannon", "gx": 30, "gz": 15, "tier": 1},
		{"archetype": "ink_cannon", "gx": 8, "gz": 21, "tier": 1, "browned_out": true},
		{"archetype": "rune_mortar", "gx": 30, "gz": 21, "tier": 1},
		{"archetype": "hidden_proctor_tesla", "gx": 19, "gz": 27, "tier": 1, "revealed": false},
	]
	for idx in range(mini(fortress_nodes.size(), specs.size())):
		if fortress_nodes[idx] is Dictionary:
			var fn: Dictionary = fortress_nodes[idx]
			var sh: Dictionary = fn.get("shield", {}) if fn.get("shield") is Dictionary else {}
			specs[idx]["node_id"] = String(fn.get("node_id", ""))
			specs[idx]["title"] = String(fn.get("title", ""))
			if sh.has("tier") and sh["tier"] != null:
				specs[idx]["tier"] = clampi(int(sh["tier"]), 0, 3)
			if sh.has("is_brownout") and sh["is_brownout"] != null:
				specs[idx]["browned_out"] = bool(sh["is_brownout"])
			if sh.has("raid_immune"):
				specs[idx]["raid_immune"] = bool(sh["raid_immune"])
	for x in range(12, 29):
		specs.append({"archetype": "fortress_wall_tile", "gx": x, "gz": 25, "tier": 1})
		if x < 18 or x > 22:
			specs.append({"archetype": "fortress_wall_tile", "gx": x, "gz": 12, "tier": 1})
	for z in range(13, 25):
		specs.append({"archetype": "fortress_wall_tile", "gx": 12, "gz": z, "tier": 1})
		specs.append({"archetype": "fortress_wall_tile", "gx": 28, "gz": z, "tier": 1})
	return specs
