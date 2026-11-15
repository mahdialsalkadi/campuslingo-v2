extends SceneTree
## Gate M3 Live Server Verification: Full Auth -> Study -> Raid -> Boss -> WS Duel -> Restart Persistence.


func _initialize() -> void:
	call_deferred("_run_all")


func _run_all() -> void:
	var host := Node.new()
	root.add_child(host)
	var auth := AuthCoursesAPI.new()
	var study := VillageStudyAPI.new()
	var raids := RaidsDuelsAPI.new()
	var boss := BossRaidController.new()
	var duel := LiveDuelController.new()
	for n in [auth, study, raids, boss, duel]:
		host.add_child(n)
	boss.bind_api(raids)
	duel.bind_api(raids)
	await process_frame

	var checks := {}
	var cid := ApiHttpClient.DEFAULT_COURSE_ID
	var email := "m3_live_%d@campuslingo.local" % Time.get_unix_time_from_system()
	var passw := "CampusM3#2026!"
	var reg := await auth.register(email, passw)
	ApiHttpClient.ensure(host).course_id = cid
	var enr := await auth.enroll_course(cid)
	checks["auth_register_enroll"] = bool(reg.get("ok", false)) and bool(enr.get("ok", false)) and auth.is_online()

	var fort := await study.fetch_fortress(cid)
	var fnodes: Array = (fort.get("data") as Dictionary).get("nodes", []) if fort.get("data") is Dictionary else []
	checks["fortress_fetch"] = bool(fort.get("ok", false)) and not bool(fort.get("offline", false)) and not fnodes.is_empty()

	var cont := await study.fetch_content(cid)
	var chs: Array = (cont.get("data") as Dictionary).get("challenges", []) if cont.get("data") is Dictionary else []
	var ch_id := String((chs[0] as Dictionary).get("id", "")) if not chs.is_empty() and chs[0] is Dictionary else ""
	var sub_res := await study.submit_answer(ch_id, 0, 3, false, cid) if ch_id != "" else {"ok": false}
	var sub_data: Variant = sub_res.get("data")
	checks["study_attempt_live"] = bool(sub_res.get("ok", false)) and sub_data is Dictionary and not bool((sub_data as Dictionary).get("offline", true))

	await raids.opt_in_raids(true, cid)
	var scan := await raids.scan_raids(cid)
	var cands: Array = (scan.get("data") as Dictionary).get("candidates", []) if scan.get("data") is Dictionary else []
	var raid_ok := false
	for c in cands:
		if c is Dictionary and not bool((c as Dictionary).get("raid_immune", false)):
			var cand := c as Dictionary
			var st := await raids.start_raid(String(cand.get("defender_user_id", "")), String(cand.get("node_id", "")), false, null, cid)
			if not bool(st.get("offline", false)) and st.get("data") is Dictionary:
				var rid := String(((st["data"] as Dictionary).get("raid", {}) as Dictionary).get("raid_id", ""))
				if rid != "":
					var stl := await raids.settle_raid(rid, true, 3, 45, cid)
					raid_ok = bool(stl.get("ok", false)) and not bool(stl.get("offline", false))
			break
	checks["raid_live_flow"] = raid_ok

	var b_init := await boss.load_boss()
	var b_hit := await boss.execute_strike(4, true, 60)
	checks["boss_live_strike"] = boss.lobby_id != "" and b_hit.has("raw_damage") and b_init.has("id")

	var d_start := await duel.start_duel("BOT")
	var ws_ok := false
	for _i in range(45):
		duel.poll(0.05)
		await create_timer(0.03).timeout
		if duel._round_started:
			ws_ok = true
			break
	if ws_ok:
		duel.send_strike(0, 0, true)
		for _j in range(15):
			duel.poll(0.05)
			await create_timer(0.03).timeout
		duel.advance_clock(30000)
		for _k in range(10):
			duel.poll(0.05)
			await create_timer(0.03).timeout
	duel.close_duel()
	checks["duel_ws_live"] = ws_ok and not bool(d_start.get("offline", true)) and int(duel.arena.get("remaining_ms", 90000)) < 90000

	await auth.logout()
	await auth.login(email, passw)
	var econ := await raids.get_economy(cid)
	var bal: Variant = (econ.get("data") as Dictionary).get("balance") if econ.get("data") is Dictionary else null
	checks["economy_restart_persisted"] = bal is Dictionary and int((bal as Dictionary).get("gold", 0)) >= 90 and int((bal as Dictionary).get("trophies", 0)) >= 15

	var passed := 0
	for k in checks.keys():
		if bool(checks[k]):
			passed += 1
	print("VERIFY_M3_LIVE ", JSON.stringify({"passed": passed, "total": checks.size(), "checks": checks, "balance": bal}))
	host.queue_free()
	ApiHttpClient._shared = null
	quit(0 if passed == checks.size() else 1)
