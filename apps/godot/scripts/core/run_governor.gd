extends Node
## SEC-C01: RunGovernor (1st Autoload).
## Enforces D006 machine hardware cap (`MAX_INSTANCES = 1`) and quiet background execution.

const SECTION_ID := "SEC-C01"
const MAX_INSTANCES := 1


func _enter_tree() -> void:
	var owner_flag := ProjectSettings.globalize_path("res://tools/.owner_using_pc")
	if FileAccess.file_exists(owner_flag) and DisplayServer.get_name() != "headless":
		if DisplayServer.get_name() == "X11" or OS.get_name() == "Windows":
			DisplayServer.window_set_position(Vector2i(-32000, -32000))
		DisplayServer.window_set_flag(DisplayServer.WINDOW_FLAG_NO_FOCUS, true)
		var master_idx := AudioServer.get_bus_index("Master")
		if master_idx >= 0:
			AudioServer.set_bus_mute(master_idx, true)


func get_instance_cap() -> int:
	return MAX_INSTANCES
