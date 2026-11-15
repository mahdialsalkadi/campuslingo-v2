class_name StudyFlashcardDrawer
extends PanelContainer
## SEC-UI03: Bilingual FSRS-6 Question, Sandbox equation slider & Overcharge study drawer.

const SECTION_ID := "SEC-UI03"

signal study_submitted(outcome: Dictionary)
signal drawer_closed

@onready var header_lbl: Label = $VBox/TopRow/StudyHeaderLbl
@onready var node_badge_lbl: Label = $VBox/NodeBadgeLbl
@onready var prompt_lbl: Label = $VBox/PromptLbl
@onready var sandbox_lbl: Label = $VBox/SandboxBox/SandboxValLbl
@onready var sandbox_slider: HSlider = $VBox/SandboxBox/SandboxSlider
@onready var overcharge_btn: CheckButton = $VBox/ControlsRow/OverchargeBtn
@onready var feedback_lbl: Label = $VBox/FeedbackLbl
@onready var close_btn: Button = $VBox/TopRow/CloseStudyBtn

var _study_api: VillageStudyAPI = null
var _challenge: Dictionary = {}
var _target_node_id: String = ""
var _selected_opt: int = -1
var _fsrs_rating: int = 3
var _is_rtl: bool = false
var _sandbox_solved: bool = true
var _challenges: Array = []
var _challenge_idx: int = 0
var _live_loaded: bool = false

func _ready() -> void:
	visible = false
	close_btn.pressed.connect(func(): visible = false; drawer_closed.emit())
	sandbox_slider.value_changed.connect(_on_sandbox_changed)
	for i in range(4):
		var ob := get_node_or_null("VBox/OptionsGrid/Opt%dBtn" % i) as Button
		if ob != null:
			ob.pressed.connect(_on_option_pressed.bind(i))
	for r in range(1, 5):
		var rb := get_node_or_null("VBox/ControlsRow/Rate%dBtn" % r) as Button
		if rb != null:
			rb.pressed.connect(_on_rate_pressed.bind(r))
	get_node("VBox/SubmitStudyBtn").pressed.connect(_on_submit_pressed)

func bind_api(api: VillageStudyAPI) -> void:
	_study_api = api


func open_next(node_id: String = "") -> void:
	if _study_api != null and not _live_loaded:
		_live_loaded = true
		var live := await _study_api.fetch_content()
		if live.get("data") is Dictionary and (live["data"] as Dictionary).get("challenges") is Array:
			var arr: Array = (live["data"] as Dictionary)["challenges"]
			if not arr.is_empty():
				_challenges = arr
	if _challenges.is_empty() and _study_api != null:
		_challenges = _study_api.load_offline_curriculum().get("challenges", [])
	if not _challenges.is_empty():
		open_challenge(_challenges[_challenge_idx % _challenges.size()] as Dictionary, node_id)
		_challenge_idx += 1


func apply_outcome_rewards(out: Dictionary, selected_building: Node, bar: BuildingActionBar) -> Vector2i:
	if not bool(out.get("correct", false)):
		return Vector2i.ZERO
	if selected_building != null and selected_building.has_method("repair_brownout"):
		selected_building.call("repair_brownout")
		bar.bind_building(selected_building)
	return Vector2i(45, 35)

func set_rtl(rtl: bool) -> void:
	_is_rtl = rtl
	_refresh_texts()

func open_challenge(ch: Dictionary, node_id: String = "") -> void:
	_challenge = ch.duplicate(true)
	_target_node_id = node_id if node_id != "" else String(ch.get("node_id", ""))
	_selected_opt = 0
	_fsrs_rating = 3
	overcharge_btn.button_pressed = false
	visible = true
	feedback_lbl.text = ""
	var sb: Dictionary = _challenge.get("sandbox_param", {}) if _challenge.get("sandbox_param") is Dictionary else {}
	sandbox_slider.min_value = float(sb.get("min", 10.0))
	sandbox_slider.max_value = float(sb.get("max", 120.0))
	sandbox_slider.step = float(sb.get("step", 1.0))
	sandbox_slider.value = float(sb.get("default", 80.0))
	_on_sandbox_changed(sandbox_slider.value)
	_refresh_texts()

func _refresh_texts() -> void:
	var dir_tag: Control.TextDirection = Control.TEXT_DIRECTION_RTL if _is_rtl else Control.TEXT_DIRECTION_LTR
	header_lbl.text = "مراجعة FSRS-6 ومختبر المعادلات" if _is_rtl else "FSRS-6 Recall & Sandbox Lab"
	close_btn.text = "إغلاق" if _is_rtl else "Close"
	node_badge_lbl.text = ("العقدة المستهدفة: %s" if _is_rtl else "Target Node: %s") % (_target_node_id if _target_node_id != "" else "Pipeline")
	var prompt_txt := String(_challenge.get("prompt_ar", "")) if (_is_rtl and _challenge.has("prompt_ar")) else String(_challenge.get("prompt", "Solve the recall drill."))
	prompt_lbl.text = prompt_txt
	prompt_lbl.text_direction = dir_tag
	var opts: Array = _challenge.get("options_ar", []) if (_is_rtl and _challenge.get("options_ar") is Array) else _challenge.get("options", ["Option A", "Option B", "Option C", "Option D"])
	for i in range(4):
		var ob := get_node_or_null("VBox/OptionsGrid/Opt%dBtn" % i) as Button
		if ob != null:
			ob.text = String(opts[i]) if i < opts.size() else "-"
			ob.text_direction = dir_tag
			ob.modulate = Color(0.55, 0.95, 0.65) if i == _selected_opt else Color.WHITE
	overcharge_btn.text = "شحن مضاعف (×2)" if _is_rtl else "Overcharge (2x)"
	get_node("VBox/SubmitStudyBtn").text = "إرسال الإجابة والتقييم" if _is_rtl else "Submit Recall & Rating"
	var rate_ar := ["١ مجدداً", "٢ صعب", "٣ جيد", "٤ سهل"]
	var rate_en := ["1 Again", "2 Hard", "3 Good", "4 Easy"]
	for r in range(1, 5):
		var rb := get_node_or_null("VBox/ControlsRow/Rate%dBtn" % r) as Button
		if rb != null:
			rb.text = rate_ar[r - 1] if _is_rtl else rate_en[r - 1]

func _on_sandbox_changed(val: float) -> void:
	var sb: Dictionary = _challenge.get("sandbox_param", {}) if _challenge.get("sandbox_param") is Dictionary else {}
	var mult := float(sb.get("mult", 1.25))
	var res := val * mult
	var tmin := float(sb.get("target_min", 95.0))
	var tmax := float(sb.get("target_max", 110.0))
	_sandbox_solved = (res >= tmin and res <= tmax)
	var pname := String(sb.get("name", "W_base"))
	sandbox_lbl.text = "%s=%.1f -> Eff=%.1f [Target %.0f..%.0f] %s" % [pname, val, res, tmin, tmax, "OK" if _sandbox_solved else "TUNE"]
	sandbox_lbl.modulate = Color(0.45, 0.96, 0.55) if _sandbox_solved else Color(0.98, 0.72, 0.35)

func _on_option_pressed(idx: int) -> void:
	_selected_opt = idx
	_refresh_texts()

func _on_rate_pressed(rating: int) -> void:
	_fsrs_rating = rating
	for r in range(1, 5):
		var rb := get_node_or_null("VBox/ControlsRow/Rate%dBtn" % r) as Button
		if rb != null:
			rb.modulate = Color(0.98, 0.82, 0.32) if r == rating else Color.WHITE

func _on_submit_pressed() -> void:
	var ch_id := String(_challenge.get("id", "ch-pipe-01"))
	var out: Dictionary = {"correct": _selected_opt == 0, "fsrs_rating": _fsrs_rating, "overcharged": overcharge_btn.button_pressed, "e_delta": 750}
	if _study_api != null:
		var res := await _study_api.submit_answer(ch_id, maxi(_selected_opt, 0), _fsrs_rating, overcharge_btn.button_pressed)
		if res.get("data") is Dictionary:
			out = (res["data"] as Dictionary).duplicate()
	out["node_id"] = _target_node_id
	out["sandbox_solved"] = _sandbox_solved
	var ok := bool(out.get("correct", true))
	feedback_lbl.text = ("صحيح! +%d E_u" if _is_rtl else "Correct! Delta E_u: +%d") % int(out.get("e_delta", 750)) if ok else ("تحتاج مراجعة (%d)" % int(out.get("e_delta", -300)))
	feedback_lbl.modulate = Color(0.45, 0.96, 0.55) if ok else Color(0.96, 0.42, 0.38)
	study_submitted.emit(out)
