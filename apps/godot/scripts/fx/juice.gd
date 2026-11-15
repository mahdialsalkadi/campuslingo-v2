extends Node
## SEC-FX01: Juice & Audio Bus Manager (4th Autoload) — pooled 60ms-gated SFX, shake & VFX routing.

const SECTION_ID := "SEC-FX01"
const MAX_HISTORY := 32
const POOL_SIZE := 8
const COOLDOWN_MS := 60

const EVENT_TO_AUDIO := {
	"archer_fire": "res://assets/audio/sfx_archer.ogg",
	"cannon_fire": "res://assets/audio/sfx_cannon.ogg",
	"mortar_fire": "res://assets/audio/sfx_cannon.ogg",
	"mortar_impact": "res://assets/audio/sfx_mortar_break.ogg",
	"tesla_zap": "res://assets/audio/sfx_archer.ogg",
	"troop_deploy": "res://assets/audio/sfx_deploy.ogg",
	"hit": "res://assets/audio/sfx_hit.ogg",
	"destroy": "res://assets/audio/sfx_destroy.ogg",
	"wall_breach": "res://assets/audio/sfx_mortar_break.ogg",
	"star": "res://assets/audio/sfx_star.ogg",
	"upgrade": "res://assets/audio/sfx_star.ogg",
}

signal juice_triggered(event_id: String, world_pos: Vector3, normal: Vector3, scale_factor: float)

var _recent_events: Array[Dictionary] = []
var _streams: Dictionary = {}
var _players: Array[AudioStreamPlayer] = []
var _pool_cursor: int = 0
var _last_cue_ms: Dictionary = {}
var _camera_target: Node = null
var _vfx_target: Node = null


func _ready() -> void:
	_setup_audio_buses()
	_preload_streams()
	for i in range(POOL_SIZE):
		var p := AudioStreamPlayer.new()
		p.name = "JuiceSFX_%d" % i
		p.bus = "SFX" if AudioServer.get_bus_index("SFX") >= 0 else "Master"
		p.volume_db = -6.0
		add_child(p)
		_players.append(p)


func bind_scene_targets(cam: Node, vfx: Node) -> void:
	_camera_target = cam
	_vfx_target = vfx


func _setup_audio_buses() -> void:
	for bus_name in ["Music", "SFX", "UI"]:
		if AudioServer.get_bus_index(bus_name) < 0:
			var idx := AudioServer.bus_count
			AudioServer.add_bus(idx)
			AudioServer.set_bus_name(idx, bus_name)
			AudioServer.set_bus_send(idx, "Master")


func _preload_streams() -> void:
	for cue in EVENT_TO_AUDIO:
		var path := String(EVENT_TO_AUDIO[cue])
		if not _streams.has(path) and ResourceLoader.exists(path):
			_streams[path] = ResourceLoader.load(path)


func emit_event(event_id: String, world_pos: Vector3, normal: Vector3 = Vector3.UP, scale_factor: float = 1.0) -> void:
	var now_ms := Time.get_ticks_msec()
	_recent_events.append({"id": event_id, "pos": world_pos, "normal": normal, "scale": scale_factor, "tick_ms": now_ms})
	if _recent_events.size() > MAX_HISTORY:
		_recent_events.pop_front()
	emit_signal("juice_triggered", event_id, world_pos, normal, scale_factor)
	if _vfx_target != null and _vfx_target.has_method("spawn_event"):
		_vfx_target.call("spawn_event", event_id, world_pos, normal, scale_factor)
	if _camera_target != null and _camera_target.has_method("shake"):
		var amt := 0.22 * scale_factor if event_id in ["mortar_impact", "destroy", "wall_breach"] else (0.10 * scale_factor if event_id == "cannon_fire" else 0.0)
		if amt > 0.0:
			_camera_target.call("shake", amt)
	_play_cue_gated(event_id, now_ms, scale_factor)


func _play_cue_gated(cue: String, now_ms: int, scale_factor: float) -> void:
	var path := String(EVENT_TO_AUDIO.get(cue, ""))
	if path == "" or not _streams.has(path):
		return
	var prev_ms := int(_last_cue_ms.get(cue, -COOLDOWN_MS))
	if now_ms - prev_ms < COOLDOWN_MS:
		return
	_last_cue_ms[cue] = now_ms
	var p := _players[_pool_cursor % _players.size()]
	_pool_cursor = (_pool_cursor + 1) % _players.size()
	p.stream = _streams[path] as AudioStream
	p.pitch_scale = clampf(1.0 + (float(now_ms % 13) - 6.0) * 0.015, 0.88, 1.14)
	p.volume_db = clampf(-7.0 + log(maxf(scale_factor, 0.3)) * 2.0, -14.0, 0.0)
	p.play()


func get_recent_events() -> Array[Dictionary]:
	return _recent_events.duplicate()
