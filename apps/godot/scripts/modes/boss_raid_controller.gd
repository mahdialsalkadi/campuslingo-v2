class_name BossRaidController
extends Node
## SEC-M02: Co-op Chapter Boss (/bosses, /lobbies, /strike, /claim) with exact rating parity.

const SECTION_ID := "SEC-M02"

signal boss_updated(boss: Dictionary)
signal boss_chest_claimed(claim: Dictionary)

var _api: RaidsDuelsAPI = null
var active_boss: Dictionary = {"id": "00000000-0000-4000-8000-000000000501", "title": "Warden of Stalled Pipelines", "max_hp": 300, "current_hp": 300, "status": "ACTIVE"}
var lobby_id: String = ""
var strike_seq: int = 1


func bind_api(api: RaidsDuelsAPI) -> void:
	_api = api


static func eval_boss_strike_damage(rating: int, overcharged: bool, base_damage: int, hp_before: int) -> Dictionary:
	return CombatParityMath.evaluate_boss_strike_local(rating, overcharged, base_damage, hp_before)


func load_boss() -> Dictionary:
	if _api != null:
		var res := await _api.list_bosses()
		var data: Variant = res.get("data")
		if data is Dictionary and (data as Dictionary).get("bosses") is Array:
			var arr: Array = (data as Dictionary)["bosses"]
			if not arr.is_empty() and arr[0] is Dictionary:
				active_boss = (arr[0] as Dictionary).duplicate(true)
		var bid := String(active_boss.get("id", ""))
		if bid != "":
			var lb := await _api.join_boss_lobby(bid)
			if lb.get("data") is Dictionary and (lb["data"] as Dictionary).get("lobby") is Dictionary:
				lobby_id = String(((lb["data"] as Dictionary)["lobby"] as Dictionary).get("id", ""))
	boss_updated.emit(active_boss)
	return active_boss


func execute_strike(rating: int, overcharged: bool, base_damage: int = 60) -> Dictionary:
	var hp_now := int(active_boss.get("current_hp", 300))
	var pred := eval_boss_strike_damage(rating, overcharged, base_damage, hp_now)
	var bid := String(active_boss.get("id", ""))
	var seq := strike_seq
	strike_seq += 1
	if _api != null and lobby_id != "" and bid != "":
		var skey := "boss-%s-%d" % [lobby_id, seq]
		var res := await _api.strike_boss(bid, lobby_id, seq, rating, overcharged, base_damage, skey)
		if bool(res.get("ok", false)) and res.get("data") is Dictionary and (res["data"] as Dictionary).get("strike") is Dictionary:
			var st: Dictionary = (res["data"] as Dictionary)["strike"]
			active_boss["current_hp"] = int(st.get("boss_hp_after", pred["boss_hp_after"]))
			active_boss["status"] = String(st.get("boss_status", pred["boss_status"]))
			boss_updated.emit(active_boss)
			return st
	active_boss["current_hp"] = int(pred["boss_hp_after"])
	active_boss["status"] = String(pred["boss_status"])
	boss_updated.emit(active_boss)
	return pred


func claim_chest() -> Dictionary:
	var bid := String(active_boss.get("id", ""))
	var skey := "claim-%s-%d" % [bid, Time.get_ticks_msec()]
	if _api != null and lobby_id != "" and bid != "":
		var res := await _api.claim_boss_chest(bid, skey)
		if bool(res.get("ok", false)) and res.get("data") is Dictionary and (res["data"] as Dictionary).get("claim") is Dictionary:
			var cl: Dictionary = (res["data"] as Dictionary)["claim"]
			boss_chest_claimed.emit(cl)
			return cl
	var offline_cl := {"claimed_now": int(active_boss.get("current_hp", 1)) <= 0, "reward_tier": "APEX_BOSS_CHEST", "gold_awarded": int(active_boss.get("gold_reward", 240))}
	boss_chest_claimed.emit(offline_cl)
	return offline_cl
