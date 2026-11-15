class_name MainVillage
extends Node3D
## Root Village & Raid Coordinator wiring all 40 SEC-* modules.

@onready var _board: GridBoardRenderer = $IslandBoard
@onready var _camera: CameraPanZoom = $CameraPanZoom
@onready var _vfx: CombatBuildParticles = $CombatBuildParticles
@onready var _drag: BuildDragPlacement = $BuildDragPlacement
@onready var _wall_batches: Node3D = $WallBatches
@onready var _buildings_root: Node3D = $Buildings
@onready var _troops_root: Node3D = $Troops
@onready var _auth_api: AuthCoursesAPI = $Services/AuthCoursesAPI
@onready var _study_api: VillageStudyAPI = $Services/VillageStudyAPI
@onready var _raid_api: RaidsDuelsAPI = $Services/RaidsDuelsAPI
@onready var _pvp_ctrl: PvpRaidController = $Services/PvpRaidController
@onready var _boss_ctrl: BossRaidController = $Services/BossRaidController
@onready var _duel_ctrl: LiveDuelController = $Services/LiveDuelController
@onready var _top_bar: ResourceTopBar = $HudCanvas/CampusHUD/ResourceTopBar
@onready var _action_bar: BuildingActionBar = $HudCanvas/CampusHUD/BuildingActionBar
@onready var _study_drawer: StudyFlashcardDrawer = $HudCanvas/CampusHUD/StudyFlashcardDrawer
@onready var _army_drawer: ArmyTrainingDrawer = $HudCanvas/CampusHUD/ArmyTrainingDrawer
@onready var _mode_modal: PanelContainer = $HudCanvas/CampusHUD/ModeModalDrawer

var gold: int = 1450
var focus_elixir: int = 620
var trophies: int = 120
var mode: String = "village"
var _buildings: Array = []
var _troops: Array = []
var _selected_building: Node = null
var _toast_timer: float = 0.0

func _ready() -> void:
	_drag.bind_board(_board)
	_pvp_ctrl.bind_services(_raid_api)
	_boss_ctrl.bind_api(_raid_api)
	_duel_ctrl.bind_api(_raid_api)
	_study_drawer.bind_api(_study_api)
	_connect_signals()
	_buildings = VillageFactory.populate_village(_auth_api.get_course_id(), _buildings_root, _wall_batches, _army_drawer)
	_refresh_hud()
	VillageFactory.apply_cli_flags(self, _top_bar)

func _connect_signals() -> void:
	_top_bar.mode_requested.connect(_on_mode_requested)
	_top_bar.language_changed.connect(func(r: bool): _action_bar.set_rtl(r); _study_drawer.set_rtl(r); _army_drawer.set_rtl(r); _refresh_hud())
	_top_bar.connect_requested.connect(_sync_live_backend)
	_action_bar.upgrade_requested.connect(_on_upgrade_building)
	_action_bar.repair_requested.connect(_on_repair_building)
	_action_bar.move_requested.connect(func(_b): _on_mode_requested("edit"))
	_action_bar.collect_requested.connect(func(b): var y := VillageFactory.collect_building_yield(b); gold += y.x; focus_elixir += y.y; _refresh_hud())
	_study_drawer.study_submitted.connect(_on_study_submitted)
	_army_drawer.train_requested.connect(_on_train_unit)
	_army_drawer.hero_rally_requested.connect(func(): for t in _troops: if t is FieldScholarHero: t.trigger_rally(_troops, _vfx))
	_army_drawer.end_raid_requested.connect(_finish_raid)
	_pvp_ctrl.raid_updated.connect(func(st: int, pct: int, rem: float): _army_drawer.update_raid_hud(st, pct, rem))
	_mode_modal.get_node("VBox/TopRow/CloseModeBtn").pressed.connect(func(): _mode_modal.visible = false)

func _physics_process(delta: float) -> void:
	_board.sync_camera_lighting(get_viewport().get_camera_3d())
	_duel_ctrl.poll(delta)
	if _toast_timer > 0.0:
		_toast_timer = maxf(0.0, _toast_timer - delta)
		if _toast_timer == 0.0:
			_refresh_hud()
	if mode == "raid" and _pvp_ctrl.active:
		if _pvp_ctrl.step_raid(delta, _buildings, _troops, _wall_batches, _vfx):
			_finish_raid()
	else:
		for b in _buildings:
			if b.has_method("step_production"):
				b.call("step_production", delta)

func _unhandled_input(event: InputEvent) -> void:
	if not (event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT):
		return
	var cam := get_viewport().get_camera_3d()
	if mode == "raid":
		_pvp_ctrl.deploy_troop(_drag.screen_to_cell(cam, event.position, 1), _buildings, _drag, _army_drawer, _troops_root, _troops, _vfx)
	elif mode == "edit" and _selected_building != null:
		VillageFactory.try_move_building(_selected_building, event.position, cam, _drag, _buildings, _wall_batches, _auth_api.get_course_id())
	else:
		_selected_building = VillageFactory.find_building_at_cell(_buildings, _drag.screen_to_cell(cam, event.position, 1))
		_drag.select_building(_selected_building)
		_action_bar.bind_building(_selected_building)

func _on_mode_requested(next_mode: String) -> void:
	if mode == "raid" and next_mode != "raid":
		await _finish_raid()
	mode = next_mode
	_drag.set_edit_mode(mode == "edit")
	_army_drawer.set_raid_mode(mode == "raid")
	_study_drawer.visible = (mode == "study")
	_mode_modal.visible = (mode == "boss" or mode == "duel")
	if mode == "study":
		_study_drawer.open_next("")
	elif mode == "raid":
		var starter := await _pvp_ctrl.start_next_raid(_buildings)
		if bool(starter.get("ok", false)):
			_top_bar.shield_lbl.text = "RAIDING TARGET"
	elif mode == "boss":
		await ModeModalHelper.open_boss(_mode_modal, _boss_ctrl, func(g: int): gold += g; _refresh_hud())
	elif mode == "duel":
		await ModeModalHelper.open_duel(_mode_modal, _duel_ctrl)

func _sync_live_backend() -> void:
	var bal := await VillageFactory.sync_backend_village(_auth_api, _study_api, _raid_api, _buildings)
	gold = int(bal.get("gold", gold))
	trophies = int(bal.get("trophies", trophies))
	_refresh_hud()

func _finish_raid() -> void:
	var res := await _pvp_ctrl.finish_and_reset(_troops, _buildings, _wall_batches, _army_drawer)
	gold += int(res.get("gold_awarded", 90))
	trophies += int(res.get("attacker_trophy_gain", 15))
	mode = "village"
	_toast_timer = 3.5
	_refresh_hud()
	_top_bar.shield_lbl.text = ("فوز +%dذ +%dك" % [int(res.get("gold_awarded", 90)), int(res.get("attacker_trophy_gain", 15))]) if _top_bar.is_rtl else ("RAID +%dG +%dT" % [int(res.get("gold_awarded", 90)), int(res.get("attacker_trophy_gain", 15))])

func _on_upgrade_building(b: Node) -> void:
	var rem := VillageFactory.try_upgrade_building(b, gold, _buildings, _wall_batches, _auth_api.get_course_id(), _army_drawer, _action_bar)
	if rem >= 0:
		gold = rem
		_refresh_hud()

func _on_repair_building(b: Node) -> void:
	_selected_building = b
	_study_drawer.open_next(String(b.get_sim_state().get("node_id", "")) if b.has_method("get_sim_state") else "")

func _on_study_submitted(out: Dictionary) -> void:
	var rw := _study_drawer.apply_outcome_rewards(out, _selected_building, _action_bar)
	gold += rw.x
	focus_elixir += rw.y
	_refresh_hud()

func _on_train_unit(uid: String, _cost: int) -> void:
	var res := _army_drawer.try_add_unit(uid, focus_elixir)
	if bool(res.get("ok", false)):
		focus_elixir -= int(res.get("cost", 0))
		_refresh_hud()

func _refresh_hud() -> void:
	var sh_txt := _top_bar.shield_lbl.text if _toast_timer > 0.0 else ("درع نشط" if _top_bar.is_rtl else "Shield Active")
	_top_bar.refresh_counters(gold, focus_elixir, trophies, sh_txt, _auth_api.is_online())
