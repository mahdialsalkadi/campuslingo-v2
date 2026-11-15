class_name BuildingActionBar
extends PanelContainer
## SEC-UI02: Contextual bottom bar (Info, Upgrade, Repair Brownout, Move, Collect).

const SECTION_ID := "SEC-UI02"

signal upgrade_requested(building: Node)
signal repair_requested(building: Node)
signal move_requested(building: Node)
signal collect_requested(building: Node)

@onready var title_lbl: Label = $VBox/InfoRow/SelTitleLbl
@onready var stats_lbl: Label = $VBox/InfoRow/SelStatsLbl
@onready var info_btn: Button = $VBox/BtnRow/InfoBtn
@onready var upgrade_btn: Button = $VBox/BtnRow/UpgradeBtn
@onready var repair_btn: Button = $VBox/BtnRow/RepairBtn
@onready var move_btn: Button = $VBox/BtnRow/MoveBtn
@onready var collect_btn: Button = $VBox/BtnRow/CollectBtn

var _selected: Node = null
var _is_rtl: bool = false

func _ready() -> void:
	info_btn.pressed.connect(_on_info)
	upgrade_btn.pressed.connect(func(): if _selected != null: upgrade_requested.emit(_selected))
	repair_btn.pressed.connect(func(): if _selected != null: repair_requested.emit(_selected))
	move_btn.pressed.connect(func(): if _selected != null: move_requested.emit(_selected))
	collect_btn.pressed.connect(func(): if _selected != null: collect_requested.emit(_selected))
	bind_building(null)

func set_rtl(rtl: bool) -> void:
	_is_rtl = rtl
	bind_building(_selected)

func bind_building(b: Node) -> void:
	_selected = b
	visible = (b != null)
	if b == null or not b.has_method("get_sim_state"):
		return
	var s: Dictionary = b.get_sim_state()
	var arch := String(s.get("archetype", "building")).replace("_", " ").capitalize()
	var t := int(s.get("tier", 1))
	var hp_now := int(s.get("hp", 0))
	var hp_max := int(s.get("max_hp", 0))
	var bo := bool(s.get("browned_out", false))
	var cost := int(b.call("upgrade_cost")) if b.has_method("upgrade_cost") else 0
	var bname := String(s.get("title", "")) if String(s.get("title", "")) != "" else arch
	if _is_rtl:
		title_lbl.text = "%s — المستوى %d" % [bname, t]
		stats_lbl.text = "الصحة: %d/%d | %s" % [hp_now, hp_max, "انخفاض جهد (-35%)" if bo else "درع سليم"]
		info_btn.text = "معلومات"
		upgrade_btn.text = "أقصى مستوى" if t >= 3 else "ترقية (%d)" % cost
		repair_btn.text = "إصلاح (FSRS)"
		move_btn.text = "تحريك"
		collect_btn.text = "جمع"
	else:
		title_lbl.text = "%s (Lv%d)" % [bname, t]
		stats_lbl.text = "HP %d/%d | %s" % [hp_now, hp_max, "BROWNOUT -35%" if bo else "ACTIVE"]
		info_btn.text = "Info"
		upgrade_btn.text = "MAX Lv3" if t >= 3 else "Upgrade (%dg)" % cost
		repair_btn.text = "Study Repair"
		move_btn.text = "Move"
		collect_btn.text = "Collect"
	upgrade_btn.disabled = (t >= 3)
	repair_btn.visible = bo
	collect_btn.visible = String(s.get("kind", "")) == "RESOURCE"

func _on_info() -> void:
	if _selected == null or not _selected.has_method("get_sim_state"):
		return
	var s: Dictionary = _selected.get_sim_state()
	stats_lbl.text = "Grid (%d,%d) %dx%d | Kind: %s" % [int(s.get("gx", 0)), int(s.get("gz", 0)), int(s.get("size", 1)), int(s.get("size", 1)), String(s.get("kind", ""))]
