extends Control
## SEC-UI05: SectionDebugInspector (F2 / button overlay).
## Lets owner inspect all 40 Micro-Sections (`SEC-*`) live and copy section codes for feedback.

const SECTION_ID := "SEC-UI05"

var _toggle_btn: Button
var _drawer: PanelContainer
var _track_filter: String = "ALL"
var _selected_id: String = "SEC-UI05"
var _list_box: VBoxContainer
var _detail_label: RichTextLabel
var _header_label: Label


func _ready() -> void:
	z_index = 100
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_build_overlay()
	call_deferred("move_to_front")


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and not event.echo:
		var key_ev: InputEventKey = event
		if key_ev.keycode == KEY_F2:
			_toggle_drawer()
			get_viewport().set_input_as_handled()


func _build_overlay() -> void:
	_toggle_btn = Button.new()
	_toggle_btn.text = "Sections (F2)"
	_toggle_btn.custom_minimum_size = Vector2(130, 32)
	_toggle_btn.anchor_left = 0.0
	_toggle_btn.anchor_right = 0.0
	_toggle_btn.anchor_top = 1.0
	_toggle_btn.anchor_bottom = 1.0
	_toggle_btn.offset_left = 10
	_toggle_btn.offset_top = -118
	_toggle_btn.offset_right = 144
	_toggle_btn.offset_bottom = -86
	_toggle_btn.pressed.connect(_toggle_drawer)
	add_child(_toggle_btn)

	_drawer = PanelContainer.new()
	_drawer.visible = false
	_drawer.anchor_left = 0.03
	_drawer.anchor_top = 0.10
	_drawer.anchor_right = 0.56
	_drawer.anchor_bottom = 0.90
	add_child(_drawer)

	var vbox := VBoxContainer.new()
	vbox.add_theme_constant_override("separation", 6)
	_drawer.add_child(vbox)

	var top_h := HBoxContainer.new()
	vbox.add_child(top_h)
	_header_label = Label.new()
	_header_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	top_h.add_child(_header_label)
	var close_btn := Button.new()
	close_btn.text = "Close"
	close_btn.pressed.connect(_toggle_drawer)
	top_h.add_child(close_btn)

	var filters_flow := HFlowContainer.new()
	vbox.add_child(filters_flow)
	for trk in ["ALL", "core_net", "village_grid", "buildings_upgrades", "study_economy_ui", "troops_raid_sim", "raid_modes", "vfx_audio"]:
		var b := Button.new()
		b.text = trk
		b.pressed.connect(func():
			_track_filter = trk
			_refresh_list()
		)
		filters_flow.add_child(b)

	var scroll := ScrollContainer.new()
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	vbox.add_child(scroll)
	_list_box = VBoxContainer.new()
	_list_box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(_list_box)

	_detail_label = RichTextLabel.new()
	_detail_label.custom_minimum_size = Vector2(0, 74)
	_detail_label.bbcode_enabled = true
	vbox.add_child(_detail_label)
	_refresh_list()


func _toggle_drawer() -> void:
	_drawer.visible = not _drawer.visible
	if _drawer.visible:
		_refresh_list()


func focus_section(sec_id: String) -> void:
	_selected_id = sec_id.to_upper()
	_track_filter = "ALL"
	_drawer.visible = true
	_refresh_list()


func _refresh_list() -> void:
	for ch in _list_box.get_children():
		ch.queue_free()
	var reg := get_node_or_null("/root/SectionRegistry")
	if reg == null:
		return
	var counts: Dictionary = reg.get_summary_counts()
	_header_label.text = "Micro-Sections Inspector — %d Total (%d Live / %d Planned)" % [int(counts["total"]), int(counts["live"]), int(counts["planned"])]
	for item in reg.get_all_sections():
		if _track_filter != "ALL" and String(item["track"]) != _track_filter:
			continue
		var sid := String(item["id"])
		var btn := Button.new()
		btn.alignment = HORIZONTAL_ALIGNMENT_LEFT
		btn.text = "[%s] %s (%s) — %s" % [sid, String(item["name"]), String(item["state"]), String(item["track"])]
		btn.pressed.connect(func():
			_selected_id = sid
			_update_detail(item)
		)
		_list_box.add_child(btn)
		if sid == _selected_id:
			_update_detail(item)


func _update_detail(item: Dictionary) -> void:
	var sid := String(item["id"])
	_detail_label.text = "[b]%s — %s[/b] [%s]\nFile: [code]%s[/code]\nRole: %s" % [
		sid, String(item["name"]), String(item["state"]), String(item["path"]), String(item["role"])
	]
