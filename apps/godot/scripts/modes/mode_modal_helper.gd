class_name ModeModalHelper
extends RefCounted
## Binds Chapter Boss Raid and Live 2-Lane Duel controls inside CampusHUD/ModeModalDrawer.


static func open_boss(modal: PanelContainer, boss_ctrl: BossRaidController, on_reward: Callable) -> void:
	var b := await boss_ctrl.load_boss()
	var hp_now := int(b.get("current_hp", 300))
	var hp_max := int(b.get("max_hp", 300))
	var status_txt := "%s — HP %d/%d (%s)" % [String(b.get("title", "Boss")), hp_now, hp_max, String(b.get("status", "ACTIVE"))]
	var act1 := func() -> void:
		var st := await boss_ctrl.execute_strike(3, false)
		modal.get_node("VBox/ModeLogLbl").text = "Strike: %d dmg | Boss HP %d" % [int(st.get("damage", 0)), int(st.get("boss_hp_after", 0))]
		await open_boss(modal, boss_ctrl, on_reward)
	var act2 := func() -> void:
		var st := await boss_ctrl.execute_strike(4, true)
		modal.get_node("VBox/ModeLogLbl").text = "Overcharge 200%: %d dmg | Boss HP %d" % [int(st.get("damage", 0)), int(st.get("boss_hp_after", 0))]
		await open_boss(modal, boss_ctrl, on_reward)
	var act3 := func() -> void:
		var cl := await boss_ctrl.claim_chest()
		modal.get_node("VBox/ModeLogLbl").text = "Chest: +%dg" % int(cl.get("gold_awarded", 0))
		on_reward.call(int(cl.get("gold_awarded", 0)))
	_bind_modal(modal, "Chapter Boss Raid", status_txt, "Strike (3 Good)", "Overcharge (200%)", "Claim Chest", act1, act2, act3)


static func open_duel(modal: PanelContainer, duel_ctrl: LiveDuelController) -> void:
	var d := await duel_ctrl.start_duel("BOT")
	var ar: Dictionary = d.get("arena", {})
	var kind := String(ar.get("opponent_kind", "BOT"))
	var act1 := func() -> void:
		duel_ctrl.send_strike(0, 0, false)
		refresh_duel_status(modal, duel_ctrl)
	var act2 := func() -> void:
		duel_ctrl.send_strike(1, 0, true)
		refresh_duel_status(modal, duel_ctrl)
	var act3 := func() -> void:
		duel_ctrl.advance_clock(30000)
		refresh_duel_status(modal, duel_ctrl)
	_bind_modal(modal, "Live 90s 2-Lane Duel (%s)" % kind, "", "Strike Lane 0", "Blitz Lane 1", "+30s Clock", act1, act2, act3)
	refresh_duel_status(modal, duel_ctrl)


static func refresh_duel_status(modal: PanelContainer, duel_ctrl: LiveDuelController) -> void:
	var ar: Dictionary = duel_ctrl.arena
	var sf: Dictionary = ar.get("self", {})
	var opp: Dictionary = ar.get("opponent", {})
	var opp_thp: Array = opp.get("tower_hp", [180, 180])
	var sec := int(ar.get("remaining_ms", 0)) / 1000
	modal.get_node("VBox/ModeStatusLbl").text = "Self Core %d | Opp Core %d | Towers [%s,%s] | %ds" % [
		int(sf.get("core_hp", 400)), int(opp.get("core_hp", 400)), str(opp_thp[0]), str(opp_thp[1]), sec
	]


static func _bind_modal(modal: PanelContainer, title_t: String, status_t: String, b1: String, b2: String, b3: String, f1: Callable, f2: Callable, f3: Callable) -> void:
	modal.get_node("VBox/TopRow/ModeTitleLbl").text = title_t
	modal.get_node("VBox/ModeStatusLbl").text = status_t
	var pairs := [
		[modal.get_node("VBox/Btns/Action1Btn"), b1, f1],
		[modal.get_node("VBox/Btns/Action2Btn"), b2, f2],
		[modal.get_node("VBox/Btns/Action3Btn"), b3, f3],
	]
	for pair in pairs:
		var btn: Button = pair[0]
		btn.text = String(pair[1])
		for conn in btn.pressed.get_connections():
			btn.pressed.disconnect(conn["callable"])
		btn.pressed.connect(pair[2])
