class_name CameraPanZoom
extends Camera3D
## SEC-G03: Clash-of-Clans Isometric Pan/Zoom Camera (Pitch -45 deg, Yaw +45 deg).

const FIXED_PITCH_DEG: float = -45.0
const FIXED_YAW_DEG: float = 45.0
const ZOOM_MIN_M: float = 18.0
const ZOOM_MAX_M: float = 54.0
const ZOOM_DEFAULT_M: float = 34.0
const ZOOM_STEP_M: float = 2.5
const MAX_PAN_SPEED_MPS: float = 28.0
const INERTIA_DAMPING: float = 8.0
const BOUNDS_LIMIT_M: float = 34.0

var focus_target := Vector3.ZERO
var zoom_size_m: float = ZOOM_DEFAULT_M
var _pitch_deg: float = FIXED_PITCH_DEG
var _yaw_deg: float = FIXED_YAW_DEG
var _damping: float = INERTIA_DAMPING
var pan_velocity := Vector2.ZERO
var _dragging: bool = false
var _last_mouse := Vector2.ZERO
var _shake_trauma: float = 0.0


var _sun_light: DirectionalLight3D = null

func _ready() -> void:
	var profile := VillageProfile.load_default()
	zoom_size_m = profile.camera_zoom_default_m
	_pitch_deg = profile.camera_pitch_deg
	_yaw_deg = profile.camera_yaw_deg
	_damping = profile.camera_pan_damping
	projection = PROJECTION_ORTHOGONAL
	size = zoom_size_m
	near = 0.5
	far = 220.0
	rotation_degrees = Vector3(_pitch_deg, _yaw_deg, 0.0)
	_apply_transform()
	sync_sun_shadow_distance()


func _unhandled_input(event: InputEvent) -> void:
	if not current:
		return
	if event is InputEventMouseButton:
		var mb := event as InputEventMouseButton
		if mb.button_index == MOUSE_BUTTON_WHEEL_UP and mb.pressed:
			set_zoom_size(zoom_size_m - ZOOM_STEP_M)
		elif mb.button_index == MOUSE_BUTTON_WHEEL_DOWN and mb.pressed:
			set_zoom_size(zoom_size_m + ZOOM_STEP_M)
		elif mb.button_index == MOUSE_BUTTON_MIDDLE or mb.button_index == MOUSE_BUTTON_RIGHT:
			_dragging = mb.pressed
			_last_mouse = mb.position
	elif event is InputEventMouseMotion and _dragging:
		var mm := event as InputEventMouseMotion
		var delta: Vector2 = mm.position - _last_mouse
		_last_mouse = mm.position
		var world_scale: float = zoom_size_m / 650.0
		_add_screen_pan(-delta * world_scale * 14.0)


func _process(delta: float) -> void:
	if not current:
		return
	var dir := Vector2.ZERO
	if Input.is_action_pressed("ui_left") or Input.is_key_pressed(KEY_A):
		dir.x -= 1.0
	if Input.is_action_pressed("ui_right") or Input.is_key_pressed(KEY_D):
		dir.x += 1.0
	if Input.is_action_pressed("ui_up") or Input.is_key_pressed(KEY_W):
		dir.y -= 1.0
	if Input.is_action_pressed("ui_down") or Input.is_key_pressed(KEY_S):
		dir.y += 1.0

	if dir.length() > 0.01:
		dir = dir.normalized()
		var right_xz := Vector2(cos(deg_to_rad(_yaw_deg)), -sin(deg_to_rad(_yaw_deg)))
		var fwd_xz := Vector2(-sin(deg_to_rad(_yaw_deg)), -cos(deg_to_rad(_yaw_deg)))
		pan_velocity = (right_xz * dir.x - fwd_xz * dir.y) * MAX_PAN_SPEED_MPS
	else:
		pan_velocity = pan_velocity.lerp(Vector2.ZERO, clampf(_damping * delta, 0.0, 1.0))

	if pan_velocity.length() > MAX_PAN_SPEED_MPS:
		pan_velocity = pan_velocity.normalized() * MAX_PAN_SPEED_MPS

	focus_target.x = clampf(focus_target.x + pan_velocity.x * delta, -BOUNDS_LIMIT_M, BOUNDS_LIMIT_M)
	focus_target.z = clampf(focus_target.z + pan_velocity.y * delta, -BOUNDS_LIMIT_M, BOUNDS_LIMIT_M)
	_shake_trauma = maxf(0.0, _shake_trauma - delta * 2.8)
	_apply_transform()


func _add_screen_pan(delta_world: Vector2) -> void:
	var right_xz := Vector2(cos(deg_to_rad(FIXED_YAW_DEG)), -sin(deg_to_rad(FIXED_YAW_DEG)))
	var fwd_xz := Vector2(-sin(deg_to_rad(FIXED_YAW_DEG)), -cos(deg_to_rad(FIXED_YAW_DEG)))
	pan_velocity = (right_xz * delta_world.x - fwd_xz * delta_world.y)


func set_zoom_size(new_size: float) -> void:
	zoom_size_m = clampf(new_size, ZOOM_MIN_M, ZOOM_MAX_M)
	size = zoom_size_m
	sync_sun_shadow_distance()

func sync_sun_shadow_distance(sun: DirectionalLight3D = null) -> void:
	if sun != null:
		_sun_light = sun
	elif _sun_light == null and get_parent() != null:
		_sun_light = get_parent().get_node_or_null("SunLight") as DirectionalLight3D
	if _sun_light == null:
		return
	_sun_light.directional_shadow_mode = DirectionalLight3D.SHADOW_PARALLEL_2_SPLITS
	_sun_light.directional_shadow_split_1 = 0.30
	_sun_light.directional_shadow_max_distance = clampf(zoom_size_m * 1.9, 65.0, 105.0)


func add_trauma(amount: float) -> void:
	_shake_trauma = clampf(_shake_trauma + amount, 0.0, 1.0)


func _apply_transform() -> void:
	var dist: float = 55.0
	var offset := Vector3(dist * 0.577, dist * 0.707, dist * 0.577)
	var shake := Vector3.ZERO
	if _shake_trauma > 0.005:
		var amp: float = _shake_trauma * _shake_trauma * 0.45
		shake = Vector3(randf_range(-amp, amp), randf_range(-amp, amp), 0.0)
	global_position = focus_target + offset + shake
	look_at(focus_target, Vector3.UP)
	rotation_degrees.x = FIXED_PITCH_DEG
	rotation_degrees.y = FIXED_YAW_DEG
	rotation_degrees.z = 0.0
