class_name ResourceTopBar
extends PanelContainer
## SEC-UI01: Top HUD bar (Gold, Focus Elixir, Trophies, Shield Status, Mode Nav, EN/AR RTL toggle).

const SECTION_ID := "SEC-UI01"

signal mode_requested(mode_name: String)
signal language_changed(is_rtl: bool)
signal connect_requested

@onready var gold_lbl: Label = $HBox/Counters/GoldLbl
@onready var focus_lbl: Label = $HBox/Counters/FocusLbl
@onready var trophy_lbl: Label = $HBox/Counters/TrophyLbl
@onready var shield_lbl: Label = $HBox/Counters/ShieldLbl
@onready var conn_lbl: Label = $HBox/Counters/ConnLbl
@onready var lang_btn: Button = $HBox/RightBtns/LangBtn
@onready var connect_btn: Button = $HBox/RightBtns/ConnectBtn

var is_rtl: bool = false
var _shared_font: FontVariation = null

const NAV_LABELS := {
	"VillageBtn": ["Village", "القرية"],
	"EditBtn": ["Edit", "تعديل"],
	"StudyBtn": ["Study", "دراسة"],
	"RaidBtn": ["PvP Raid", "غارة"],
	"BossBtn": ["Boss", "الزعيم"],
	"DuelBtn": ["Duel", "مبارزة"],
}

func _ready() -> void:
	_setup_bilingual_theme()
	for btn_name in NAV_LABELS.keys():
		var b := get_node_or_null("HBox/NavBtns/" + btn_name) as Button
		if b != null:
			var target_mode := String(btn_name).trim_suffix("Btn").to_lower()
			b.pressed.connect(func(): mode_requested.emit(target_mode))
	lang_btn.pressed.connect(toggle_language)
	connect_btn.pressed.connect(func(): connect_requested.emit())
	refresh_counters(1450, 620, 120, "Shield Active", false)
	apply_localization()

func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and not event.echo:
		var k := event as InputEventKey
		if k.keycode == KEY_L and k.ctrl_pressed:
			toggle_language()
			get_viewport().set_input_as_handled()

func _setup_bilingual_theme() -> void:
	if _shared_font != null:
		return
	var f_en := ResourceLoader.load("res://assets/fonts/NotoSans-Regular.ttf") as FontFile
	var f_ar := ResourceLoader.load("res://assets/fonts/NotoNaskhArabic-Regular.ttf") as FontFile
	if f_en != null:
		_shared_font = FontVariation.new()
		_shared_font.base_font = f_en
		if f_ar != null:
			f_en.fallbacks = [f_ar]
		var hud := get_parent() as Control
		if hud != null:
			var th := Theme.new()
			th.default_font = _shared_font
			th.default_font_size = 15
			hud.theme = th

func toggle_language() -> void:
	set_rtl(not is_rtl)

func set_rtl(enabled: bool) -> void:
	is_rtl = enabled
	var hud := get_parent() as Control
	if hud != null:
		hud.layout_direction = Control.LAYOUT_DIRECTION_RTL if is_rtl else Control.LAYOUT_DIRECTION_LTR
	apply_localization()
	language_changed.emit(is_rtl)

func apply_localization() -> void:
	lang_btn.text = "EN | عربي" if not is_rtl else "عربي | EN"
	connect_btn.text = "متصل" if is_rtl else "Sync"
	for btn_name in NAV_LABELS.keys():
		var b := get_node_or_null("HBox/NavBtns/" + btn_name) as Button
		if b != null:
			b.text = String(NAV_LABELS[btn_name][1 if is_rtl else 0])

func refresh_counters(gold: int, focus: int, trophies: int, shield_text: String, live_online: bool) -> void:
	if is_rtl:
		gold_lbl.text = "الذهب: %d" % gold
		focus_lbl.text = "الإكسير: %d" % focus
		trophy_lbl.text = "الكؤوس: %d" % trophies
		shield_lbl.text = shield_text
		conn_lbl.text = "الخادم: متصل" if live_online else "وضع محلي"
	else:
		gold_lbl.text = "Gold: %d" % gold
		focus_lbl.text = "Focus: %d" % focus
		trophy_lbl.text = "Trophies: %d" % trophies
		shield_lbl.text = shield_text
		conn_lbl.text = "LIVE" if live_online else "LOCAL"
	conn_lbl.modulate = Color(0.45, 0.96, 0.55) if live_online else Color(0.95, 0.82, 0.45)
