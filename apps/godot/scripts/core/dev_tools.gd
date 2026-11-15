extends Node
## SEC-C02: DevTools (3rd Autoload).
## Parses CLI args after `--` for M0..M5 headless checks, StoryCam capture, and section inspection.

const SECTION_ID := "SEC-C02"

var capture_path: String = ""
var wait_frames: int = 30
var camera_name: String = ""
var hide_hud: bool = false
var goto_mode: String = ""
var spawn_marker: String = "Default"
var mission_id: String = ""
var inspect_section_id: String = ""
var unlock_all: bool = false

var _frame_counter: int = 0
var _capture_started: bool = false


func _ready() -> void:
	var args: PackedStringArray = OS.get_cmdline_user_args()
	for raw_arg in args:
		var arg := String(raw_arg)
		if arg.begins_with("--capture="):
			capture_path = arg.trim_prefix("--capture=")
		elif arg.begins_with("--wait-frames="):
			wait_frames = maxi(2, int(arg.trim_prefix("--wait-frames=")))
		elif arg.begins_with("--cam="):
			camera_name = arg.trim_prefix("--cam=")
		elif arg == "--hide-hud":
			hide_hud = true
		elif arg.begins_with("--goto="):
			goto_mode = arg.trim_prefix("--goto=")
		elif arg.begins_with("--spawn="):
			spawn_marker = arg.trim_prefix("--spawn=")
		elif arg.begins_with("--mission="):
			mission_id = arg.trim_prefix("--mission=")
		elif arg.begins_with("--section="):
			inspect_section_id = arg.trim_prefix("--section=").to_upper()
		elif arg == "--unlock-all":
			unlock_all = true


func _process(_delta: float) -> void:
	if _frame_counter == 2:
		_apply_runtime_dev_hooks()
	if capture_path.is_empty():
		_frame_counter += 1
		return
	_frame_counter += 1
	if _frame_counter >= wait_frames and not _capture_started:
		_capture_started = true
		_do_capture_and_quit()


func _apply_runtime_dev_hooks() -> void:
	var root := get_tree().current_scene
	if root == null:
		return
	if not camera_name.is_empty():
		_activate_story_camera(root, camera_name)
	if hide_hud:
		for hud_name in ["SectionDebugInspector", "CampusHUD"]:
			var hud_node := root.find_child(hud_name, true, false)
			if hud_node and hud_node is Control:
				(hud_node as Control).visible = false
	if not inspect_section_id.is_empty():
		var insp2 := root.find_child("SectionDebugInspector", true, false)
		if insp2 and insp2.has_method("focus_section"):
			insp2.focus_section(inspect_section_id)


func _activate_story_camera(root: Node, cam_name: String) -> void:
	var cam := root.find_child(cam_name, true, false)
	if cam == null or not (cam is Camera3D):
		return
	var story_cam: Camera3D = cam
	var f3d := root.find_child("Fortress3DView", true, false)
	if f3d and f3d.has_method("set_story_camera_override"):
		f3d.set_story_camera_override(story_cam.transform, story_cam.fov)
	else:
		story_cam.make_current()


func _do_capture_and_quit() -> void:
	await RenderingServer.frame_post_draw
	var tex := get_viewport().get_texture()
	if tex != null:
		var img := tex.get_image()
		if img != null:
			img.save_png(capture_path)
			print("[DevTools] Captured viewport to: ", capture_path)
	get_tree().quit(0)
