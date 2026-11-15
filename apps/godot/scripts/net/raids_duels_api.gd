class_name RaidsDuelsAPI
extends Node
## SEC-C06: Raids, Bosses, Clans, Leagues & Duels REST + WebSocket slice (/v1/*).

const SECTION_ID := "SEC-C06"

var _client: ApiHttpClient = null


func _ready() -> void:
	_ensure_client()


func _ensure_client() -> ApiHttpClient:
	if _client == null:
		_client = ApiHttpClient.ensure(self)
	return _client


func opt_in_raids(opt_in: bool = true, course_id: String = "") -> Dictionary:
	var cid := course_id if course_id != "" else _ensure_client().course_id
	return await _ensure_client().request_json(HTTPClient.METHOD_POST, "/v1/courses/%s/raids/opt-in" % cid, {"opt_in": opt_in})


func scan_raids(course_id: String = "") -> Dictionary:
	var cid := course_id if course_id != "" else _ensure_client().course_id
	var res := await _ensure_client().request_json(HTTPClient.METHOD_GET, "/v1/courses/%s/raids/scanner" % cid)
	if bool(res.get("ok", false)):
		return res
	var seed: Variant = JSON.parse_string(FileAccess.get_file_as_string("res://data/offline_curriculum.json")) if FileAccess.file_exists("res://data/offline_curriculum.json") else {}
	var cands: Array = (seed as Dictionary).get("offline_raids", []) if seed is Dictionary else []
	return {"ok": true, "offline": true, "data": {"course_id": cid, "candidates": cands, "limits": {"outbound_per_day": 10}}}


func start_raid(defender_user_id: String, defender_node_id: String, is_revenge: bool = false, revenge_source_raid_id: Variant = null, course_id: String = "") -> Dictionary:
	var cid := course_id if course_id != "" else _ensure_client().course_id
	var payload := {"defender_user_id": defender_user_id, "defender_node_id": defender_node_id, "is_revenge": is_revenge}
	if revenge_source_raid_id != null and String(revenge_source_raid_id) != "":
		payload["revenge_source_raid_id"] = String(revenge_source_raid_id)
	var res := await _ensure_client().request_json(HTTPClient.METHOD_POST, "/v1/courses/%s/raids/start" % cid, payload)
	if bool(res.get("ok", false)):
		return res
	return {"ok": true, "offline": true, "data": {"raid": {"raid_id": "offline-raid-%d" % Time.get_ticks_msec()}}}


func settle_raid(raid_id: String, sandbox_solved: bool, drills_passed: int, base_score_points: int, course_id: String = "") -> Dictionary:
	var cid := course_id if course_id != "" else _ensure_client().course_id
	var payload := {"sandbox_solved": sandbox_solved, "drills_passed": drills_passed, "base_score_points": base_score_points}
	var res := await _ensure_client().request_json(HTTPClient.METHOD_POST, "/v1/courses/%s/raids/%s/settle" % [cid, raid_id], payload)
	if bool(res.get("ok", false)):
		return res
	var won := sandbox_solved and drills_passed >= 2
	return {"ok": true, "offline": true, "data": {"raid_id": raid_id, "settlement": {"status": "WON" if won else "LOST", "attacker_trophy_gain": 15 if won else 0, "gold_awarded": 120 if won else 25, "loot_chest_tier": "APEX_RAID_CHEST" if drills_passed >= 3 else ("STANDARD_RAID_CHEST" if won else null)}}}


func list_bosses(course_id: String = "") -> Dictionary:
	var cid := course_id if course_id != "" else _ensure_client().course_id
	var res := await _ensure_client().request_json(HTTPClient.METHOD_GET, "/v1/courses/%s/bosses" % cid)
	if bool(res.get("ok", false)):
		return res
	var seed: Variant = JSON.parse_string(FileAccess.get_file_as_string("res://data/offline_curriculum.json")) if FileAccess.file_exists("res://data/offline_curriculum.json") else {}
	var b: Dictionary = (seed as Dictionary).get("offline_boss", {}) if seed is Dictionary else {}
	return {"ok": true, "offline": true, "data": {"bosses": [b] if not b.is_empty() else []}}


func join_boss_lobby(boss_id: String, lobby_id: String = "", course_id: String = "") -> Dictionary:
	var cid := course_id if course_id != "" else _ensure_client().course_id
	var payload := {"lobby_id": lobby_id} if lobby_id != "" else {}
	return await _ensure_client().request_json(HTTPClient.METHOD_POST, "/v1/courses/%s/bosses/%s/lobbies" % [cid, boss_id], payload)


func strike_boss(boss_id: String, lobby_id: String, seq: int, rating: int, overcharged: bool, base_damage: int, settlement_key: String, course_id: String = "") -> Dictionary:
	var cid := course_id if course_id != "" else _ensure_client().course_id
	var payload := {"lobby_id": lobby_id, "sequence": seq, "rating": rating, "overcharged": overcharged, "base_damage": base_damage, "settlement_key": settlement_key}
	return await _ensure_client().request_json(HTTPClient.METHOD_POST, "/v1/courses/%s/bosses/%s/strike" % [cid, boss_id], payload)


func claim_boss_chest(boss_id: String, settlement_key: String, course_id: String = "") -> Dictionary:
	var cid := course_id if course_id != "" else _ensure_client().course_id
	return await _ensure_client().request_json(HTTPClient.METHOD_POST, "/v1/courses/%s/bosses/%s/claim" % [cid, boss_id], {"settlement_key": settlement_key})


func get_economy(course_id: String = "") -> Dictionary:
	var cid := course_id if course_id != "" else _ensure_client().course_id
	return await _ensure_client().request_json(HTTPClient.METHOD_GET, "/v1/courses/%s/economy" % cid)


func get_league_bracket(week_key: String = "2026-W46", tier: int = 1, course_id: String = "") -> Dictionary:
	var cid := course_id if course_id != "" else _ensure_client().course_id
	return await _ensure_client().request_json(HTTPClient.METHOD_GET, "/v1/courses/%s/leagues/%s?tier=%d" % [cid, week_key, tier])


func matchmake_duel(prefer_mode: String = "BOT", course_id: String = "") -> Dictionary:
	var cid := course_id if course_id != "" else _ensure_client().course_id
	return await _ensure_client().request_json(HTTPClient.METHOD_POST, "/v1/courses/%s/duels/matchmake" % cid, {"wait_budget_ms": 0, "prefer_mode": prefer_mode})


func open_ws(path_with_query: String) -> WebSocketPeer:
	var c := _ensure_client()
	var peer := WebSocketPeer.new()
	var hdrs := c.build_headers(false)
	if not hdrs.is_empty():
		peer.handshake_headers = hdrs
	peer.connect_to_url(c.ws_url(path_with_query))
	return peer
