class_name VillageStudyAPI
extends Node
## SEC-C05: Fortress (/fortress, /shields/recompute) & FSRS-6 Study (/content, /practice, /attempts).

const SECTION_ID := "SEC-C05"
const OFFLINE_PATH := "res://data/offline_curriculum.json"

signal fortress_synced(dashboard: Dictionary)
signal attempt_graded(outcome: Dictionary)

var _client: ApiHttpClient = null
var _offline_cache: Dictionary = {}


func _ready() -> void:
	_ensure_client()


func _ensure_client() -> ApiHttpClient:
	if _client == null:
		_client = ApiHttpClient.ensure(self)
	return _client


func load_offline_curriculum() -> Dictionary:
	if _offline_cache.is_empty() and FileAccess.file_exists(OFFLINE_PATH):
		var parsed: Variant = JSON.parse_string(FileAccess.get_file_as_string(OFFLINE_PATH))
		if parsed is Dictionary:
			_offline_cache = parsed as Dictionary
	return _offline_cache


func fetch_fortress(course_id: String = "") -> Dictionary:
	var cid := course_id if course_id != "" else _ensure_client().course_id
	var res := await _ensure_client().request_json(HTTPClient.METHOD_GET, "/v1/courses/%s/fortress" % cid)
	if bool(res.get("ok", false)) and res.get("data") is Dictionary:
		fortress_synced.emit(res["data"] as Dictionary)
		return res
	var fallback := {"course_id": cid, "nodes": _build_offline_fortress_nodes(), "edges": [], "offline": true}
	fortress_synced.emit(fallback)
	return {"ok": true, "status": 0, "offline": true, "data": fallback}


func recompute_shields(course_id: String = "") -> Dictionary:
	var cid := course_id if course_id != "" else _ensure_client().course_id
	var res := await _ensure_client().request_json(HTTPClient.METHOD_POST, "/v1/courses/%s/shields/recompute" % cid)
	if bool(res.get("ok", false)) and res.get("data") is Dictionary:
		fortress_synced.emit(res["data"] as Dictionary)
	return res


func fetch_content(course_id: String = "") -> Dictionary:
	var cid := course_id if course_id != "" else _ensure_client().course_id
	var res := await _ensure_client().request_json(HTTPClient.METHOD_GET, "/v1/courses/%s/content" % cid)
	if bool(res.get("ok", false)) and res.get("data") is Dictionary:
		return res
	var seed := load_offline_curriculum()
	return {"ok": true, "offline": true, "data": {"nodes": seed.get("nodes", []), "challenges": seed.get("challenges", [])}}


func start_practice(challenge_id: String, course_id: String = "") -> Dictionary:
	var cid := course_id if course_id != "" else _ensure_client().course_id
	var res := await _ensure_client().request_json(HTTPClient.METHOD_GET, "/v1/courses/%s/challenges/%s/practice" % [cid, challenge_id])
	if bool(res.get("ok", false)):
		return res
	for ch in load_offline_curriculum().get("challenges", []):
		if String((ch as Dictionary).get("id", "")) == challenge_id:
			return {"ok": true, "offline": true, "data": ch}
	return res


func submit_answer(challenge_id: String, selected_index: int, fsrs_rating: int = 3, overcharged: bool = false, course_id: String = "") -> Dictionary:
	var cid := course_id if course_id != "" else _ensure_client().course_id
	var c := _ensure_client()
	if c.online:
		var att := await c.request_json(HTTPClient.METHOD_POST, "/v1/courses/%s/challenges/%s/attempts" % [cid, challenge_id], {})
		if bool(att.get("ok", false)) and att.get("data") is Dictionary:
			var att_id := String((att["data"] as Dictionary).get("id", (att["data"] as Dictionary).get("attempt_id", "")))
			if att_id != "":
				var ans := await c.request_json(HTTPClient.METHOD_POST, "/v1/attempts/%s/answer" % att_id, {"selected_index": selected_index})
				if bool(ans.get("ok", false)) and ans.get("data") is Dictionary:
					var out: Dictionary = (ans["data"] as Dictionary).duplicate()
					out["fsrs_rating"] = fsrs_rating
					out["overcharged"] = overcharged
					out["offline"] = false
					await recompute_shields(cid)
					attempt_graded.emit(out)
					return {"ok": true, "status": int(ans.get("status", 200)), "data": out}
	var offline_out := _grade_offline(challenge_id, selected_index, fsrs_rating, overcharged)
	attempt_graded.emit(offline_out)
	return {"ok": true, "offline": true, "data": offline_out}


func report_node(node_id: String, reason: String = "BROKEN_QUESTION", details: String = "Godot report", course_id: String = "") -> Dictionary:
	var cid := course_id if course_id != "" else _ensure_client().course_id
	return await _ensure_client().request_json(HTTPClient.METHOD_POST, "/v1/courses/%s/nodes/%s/report" % [cid, node_id], {"reason": reason, "details": details})


func _grade_offline(challenge_id: String, selected_index: int, fsrs_rating: int, overcharged: bool) -> Dictionary:
	var correct := selected_index == 0
	for ch in load_offline_curriculum().get("challenges", []):
		if String((ch as Dictionary).get("id", "")) == challenge_id:
			correct = (selected_index == int((ch as Dictionary).get("correct_index", 0)))
			break
	var bonus := 750 if (correct and fsrs_rating >= 3) else -300
	if overcharged and correct:
		bonus += 500
	return {"correct": correct, "fsrs_rating": fsrs_rating, "overcharged": overcharged, "e_delta": bonus, "offline": true}


func _build_offline_fortress_nodes() -> Array:
	var out: Array = []
	for raw in load_offline_curriculum().get("nodes", []):
		var n: Dictionary = raw
		out.append({
			"node_id": String(n.get("id", "")),
			"title": String(n.get("title", "")),
			"archetype": String(n.get("archetype", "PIPELINE")),
			"shield": {"state": "ACTIVE", "tier": int(n.get("tier", 1)), "e_u_ticks": int(n.get("e_u_ticks", 7200)), "is_brownout": bool(n.get("is_brownout", false)), "raid_immune": false}
		})
	return out
