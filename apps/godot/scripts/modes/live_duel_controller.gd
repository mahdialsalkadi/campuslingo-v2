class_name LiveDuelController
extends Node
## SEC-M03: Live 90s 2-Lane Duel WebSocket client (/duels/matchmake + /duels/ws) & local fallback.

const SECTION_ID := "SEC-M03"

signal duel_state_changed(arena: Dictionary)

var _api: RaidsDuelsAPI = null
var _ws: WebSocketPeer = null
var match_id: String = ""
var side: String = "PLAYER_A"
var opponent_kind: String = "BOT"
var seq: int = 1
var arena: Dictionary = {}
var _hb_timer: float = 0.0
var _round_started: bool = false


func bind_api(api: RaidsDuelsAPI) -> void:
	_api = api


func start_duel(prefer_mode: String = "BOT") -> Dictionary:
	close_duel()
	seq = 1
	_round_started = false
	if _api != null:
		var res := await _api.matchmake_duel(prefer_mode)
		if bool(res.get("ok", false)) and res.get("data") is Dictionary:
			var d: Dictionary = res["data"]
			match_id = String(d.get("match_id", ""))
			side = String(d.get("side", "PLAYER_A"))
			opponent_kind = String(d.get("opponent_kind", "BOT"))
			if d.get("arena") is Dictionary:
				arena = (d["arena"] as Dictionary).duplicate(true)
			if match_id != "":
				_ws = _api.open_ws("/v1/duels/ws?match_id=%s" % match_id)
			duel_state_changed.emit(arena)
			return {"ok": true, "match_id": match_id, "arena": arena, "offline": false}
	match_id = "offline-duel-1"
	arena = {
		"status": "IN_PROGRESS", "remaining_ms": 90000, "opponent_kind": "BOT",
		"self": {"core_hp": 400, "tower_hp": [180, 180], "overcharge_count": 0, "energy_milli": 5000},
		"opponent": {"core_hp": 400, "tower_hp": [180, 180], "overcharge_count": 0},
	}
	duel_state_changed.emit(arena)
	return {"ok": true, "match_id": match_id, "arena": arena, "offline": true}


func poll(dt: float) -> void:
	if _ws == null:
		return
	_ws.poll()
	var st := _ws.get_ready_state()
	if st == WebSocketPeer.STATE_OPEN:
		if not _round_started:
			_round_started = true
			_send_cmd({"type": "START_ROUND", "sequence": _next_seq()})
		_hb_timer += dt
		if _hb_timer >= 2.0:
			_hb_timer = 0.0
			_send_cmd({"type": "HEARTBEAT", "client_ts_ms": Time.get_ticks_msec()})
		while _ws.get_available_packet_count() > 0:
			var txt := _ws.get_packet().get_string_from_utf8()
			var parsed: Variant = JSON.parse_string(txt)
			if parsed is Dictionary:
				_handle_frame(parsed as Dictionary)


func send_strike(lane: int = 0, selected_index: int = 0, quick_blitz: bool = false) -> Dictionary:
	if _ws != null and _ws.get_ready_state() == WebSocketPeer.STATE_OPEN:
		var cid := _pick_card_id()
		if cid != "":
			_send_cmd({"type": "SELECT_CARD", "sequence": _next_seq(), "card_id": cid})
		_send_cmd({
			"type": "STRIKE", "sequence": _next_seq(), "lane": lane, "card_id": cid,
			"selected_index": selected_index, "use_quick_blitz_chain": quick_blitz, "client_elapsed_ms": 950,
		})
		return arena
	var opp: Dictionary = arena.get("opponent", {})
	var thp: Array = opp.get("tower_hp", [180, 180])
	var idx := clampi(lane, 0, 1)
	var dmg := 95 if quick_blitz else 65
	thp[idx] = maxi(0, int(thp[idx]) - dmg)
	opp["tower_hp"] = thp
	if int(thp[idx]) <= 0:
		opp["core_hp"] = maxi(0, int(opp.get("core_hp", 400)) - 120)
	arena["opponent"] = opp
	arena["remaining_ms"] = maxi(0, int(arena.get("remaining_ms", 90000)) - 6000)
	duel_state_changed.emit(arena)
	return arena


func advance_clock(delta_ms: int = 30000) -> void:
	if _ws != null and _ws.get_ready_state() == WebSocketPeer.STATE_OPEN:
		_send_cmd({"type": "TICK_ADVANCE", "delta_ms": delta_ms})
	else:
		arena["remaining_ms"] = maxi(0, int(arena.get("remaining_ms", 90000)) - delta_ms)
		duel_state_changed.emit(arena)


func close_duel() -> void:
	if _ws != null:
		_ws.close()
		_ws = null


func _pick_card_id() -> String:
	var sf: Dictionary = arena.get("self", {})
	var hand: Array = sf.get("hand", [])
	if not hand.is_empty() and hand[0] is Dictionary:
		return String((hand[0] as Dictionary).get("card_id", ""))
	return "card-0"


func _next_seq() -> int:
	var s := seq
	seq += 1
	return s


func _send_cmd(payload: Dictionary) -> void:
	if _ws != null and _ws.get_ready_state() == WebSocketPeer.STATE_OPEN:
		_ws.send_text(JSON.stringify(payload))


func _handle_frame(frame: Dictionary) -> void:
	var ftype := String(frame.get("type", ""))
	if frame.get("payload") is Dictionary:
		var pl: Dictionary = frame["payload"]
		if pl.has("arena") and pl["arena"] is Dictionary:
			arena = (pl["arena"] as Dictionary).duplicate(true)
			duel_state_changed.emit(arena)
		elif pl.has("self") and pl.has("opponent"):
			arena = pl.duplicate(true)
			duel_state_changed.emit(arena)
	elif ftype == "ERROR":
		pass
