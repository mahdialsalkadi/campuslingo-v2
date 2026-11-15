class_name VillageProfile
extends Resource
## REA-derived physics, simulation, and isometric camera tuning profile (analogous to mak_profile.tres).

@export_group("Simulation & REA Correction")
@export var sim_tick_hz: float = 20.0
@export var shot_phase_offset_ratio: float = 0.50
@export var mortar_launch_vy_mps: float = 10.31 # REA discrete-step +0.70 m/s correction
@export var mortar_gravity_mps2: float = 28.12

@export_group("Troop Kinematics (60Hz Continuous)")
@export var accel_start_mps2: float = 7.73
@export var accel_top_taper_mps2: float = 2.50
@export var braking_decel_mps2: float = 28.10
@export var turn_rate_deg_s: float = 337.50

@export_group("Isometric Camera")
@export var camera_pitch_deg: float = -45.0
@export var camera_yaw_deg: float = 45.0
@export var camera_zoom_default_m: float = 34.0
@export var camera_pan_damping: float = 8.0


static func load_default() -> VillageProfile:
	var path: String = "res://scenes/combat/village_profile.tres"
	if ResourceLoader.exists(path):
		var res: Resource = ResourceLoader.load(path)
		if res is VillageProfile:
			return res as VillageProfile
	return VillageProfile.new()


func measure_troop_ramp_60hz(top_speed_mps: float) -> Dictionary:
	var dt: float = 1.0 / 60.0
	var v: float = 0.0
	var t: float = 0.0
	var t_90: float = -1.0
	var target_90: float = top_speed_mps * 0.90
	while t < 2.0:
		if v >= target_90 and t_90 < 0.0:
			t_90 = t
		var frac: float = minf(1.0, v / maxf(0.01, top_speed_mps))
		var a: float = accel_start_mps2 - (accel_start_mps2 - accel_top_taper_mps2) * frac
		v = minf(top_speed_mps, v + a * dt)
		t += dt
	return {"top_speed_mps": v, "time_to_90pct_s": t_90}


func measure_mortar_apex_60hz() -> float:
	var dt: float = 1.0 / 60.0
	var y: float = 0.0
	var vy: float = mortar_launch_vy_mps
	var apex: float = 0.0
	for _i in range(180):
		vy -= mortar_gravity_mps2 * dt # Velocity-first Godot integration
		y += vy * dt
		if y > apex:
			apex = y
		if y <= 0.0:
			break
	return apex
