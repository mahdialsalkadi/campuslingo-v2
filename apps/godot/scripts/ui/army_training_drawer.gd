class_name ArmyTrainingDrawer
extends PanelContainer
## SEC-UI04: Army housing cap, troop training queue, active deploy selector & live Raid HUD bar.

const SECTION_ID := "SEC-UI04"

signal train_requested(unit_id: String, cost_elixir: int)
signal unit_selected(unit_id: String)
signal hero_rally_requested
signal end_raid_requested

const UNIT_ORDER := ["squire_footman", "lexi_archer", "thesis_breaker", "formula_giant", "glyph_drake", "field_scholar_hero"]
const SHORT_NAMES := {"squire_footman": "Squire", "lexi_archer": "Archer", "thesis_breaker": "Breaker", "formula_giant": "Giant", "glyph_drake": "Drake", "field_scholar_hero": "Hero"}

@onready var cap_lbl: Label = $VBox/TopRow/CapLbl
@onready var raid_lbl: Label = $VBox/TopRow/RaidLbl
@onready var rally_btn: Button = $VBox/TopRow/RallyBtn
@onready var end_raid_btn: Button = $VBox/TopRow/EndRaidBtn

var housing_cap: int = 40
var active_unit: String = "squire_footman"
var _raid_mode: bool = false
var _is_rtl: bool = false
var counts: Dictionary = {"squire_footman": 8, "lexi_archer": 6, "thesis_breaker": 3, "formula_giant": 2, "glyph_drake": 1, "field_scholar_hero": 1}

const SHORT_NAMES_AR := {"squire_footman": "فارس", "lexi_archer": "رامي", "thesis_breaker": "مفجر", "formula_giant": "عملاق", "glyph_drake": "تنين", "field_scholar_hero": "البطل"}

func _ready() -> void:
	for uid in UNIT_ORDER:
		var btn := get_node_or_null("VBox/UnitsRow/%sBtn" % SHORT_NAMES[uid]) as Button
		if btn != null:
			btn.pressed.connect(_on_unit_btn.bind(uid))
	rally_btn.pressed.connect(func(): hero_rally_requested.emit())
	end_raid_btn.pressed.connect(func(): end_raid_requested.emit())
	refresh_ui()

func set_housing_cap(cap: int) -> void:
	housing_cap = maxi(20, cap)
	refresh_ui()

func set_raid_mode(enabled: bool) -> void:
	_raid_mode = enabled
	rally_btn.visible = enabled
	end_raid_btn.visible = enabled
	raid_lbl.visible = enabled
	refresh_ui()

func update_raid_hud(stars: int, pct: int, remain_s: float) -> void:
	raid_lbl.text = "%d* | %d%% | %ds" % [stars, pct, int(ceilf(maxf(0.0, remain_s)))]

func used_housing() -> int:
	var total := 0
	for uid in UNIT_ORDER:
		var spec: Dictionary = TroopAgentBase.load_spec(uid)
		total += int(counts.get(uid, 0)) * int(spec.get("housing", 0))
	return total

func try_add_unit(uid: String, available_focus: int) -> Dictionary:
	var spec: Dictionary = TroopAgentBase.load_spec(uid)
	var h := int(spec.get("housing", 1))
	var cost := int(spec.get("cost_elixir", 25))
	if used_housing() + h > housing_cap or available_focus < cost:
		return {"ok": false, "cost": cost}
	counts[uid] = int(counts.get(uid, 0)) + 1
	refresh_ui()
	return {"ok": true, "cost": cost}

func consume_deploy_unit(uid: String) -> bool:
	var cur := int(counts.get(uid, 0))
	if cur <= 0:
		return false
	counts[uid] = cur - 1
	refresh_ui()
	return true

func set_rtl(enabled: bool) -> void:
	_is_rtl = enabled
	refresh_ui()


func refresh_ui() -> void:
	if _is_rtl:
		cap_lbl.text = ("نشر الجيش (%d/%d):" if _raid_mode else "معسكر التدريب (%d/%d):") % [used_housing(), housing_cap]
		rally_btn.text = "حشد!"
		end_raid_btn.text = "إنهاء"
	else:
		cap_lbl.text = ("Deploy Army (%d/%d):" if _raid_mode else "Train Camp (%d/%d):") % [used_housing(), housing_cap]
		rally_btn.text = "Rally!"
		end_raid_btn.text = "Finish Raid"
	for uid in UNIT_ORDER:
		var btn := get_node_or_null("VBox/UnitsRow/%sBtn" % SHORT_NAMES[uid]) as Button
		if btn == null:
			continue
		var c := int(counts.get(uid, 0))
		var lbl := String(SHORT_NAMES_AR[uid] if _is_rtl else SHORT_NAMES[uid])
		btn.text = "%s x%d" % [lbl, c]
		btn.modulate = Color(0.98, 0.85, 0.35) if uid == active_unit else Color.WHITE

func _on_unit_btn(uid: String) -> void:
	active_unit = uid
	if _raid_mode:
		unit_selected.emit(uid)
	else:
		var cost := int(TroopAgentBase.load_spec(uid).get("cost_elixir", 25))
		train_requested.emit(uid, cost)
	refresh_ui()
