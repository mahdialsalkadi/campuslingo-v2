shader_type spatial;
render_mode blend_add, depth_draw_never, cull_back, unshaded;

uniform vec3 halo_color : source_color = vec3(0.18, 0.82, 1.0);
uniform float intensity : hint_range(0.0, 4.0) = 0.85;
uniform float softness : hint_range(0.5, 5.0) = 2.4;
uniform float pulse_speed : hint_range(0.0, 8.0) = 1.8;

void vertex() {
	// Camera-facing spherical billboard preserving local scale
	mat4 mv = VIEW_MATRIX * MODEL_MATRIX;
	float sx = length(vec3(MODEL_MATRIX[0].xyz));
	float sy = length(vec3(MODEL_MATRIX[1].xyz));
	float sz = length(vec3(MODEL_MATRIX[2].xyz));
	MODELVIEW_MATRIX = mat4(
		vec4(sx, 0.0, 0.0, 0.0),
		vec4(0.0, sy, 0.0, 0.0),
		vec4(0.0, 0.0, sz, 0.0),
		mv[3]
	);
}

void fragment() {
	vec2 d = UV * 2.0 - vec2(1.0);
	float r = length(d);
	float glow = pow(clamp(1.0 - r, 0.0, 1.0), softness);
	float pulse = 0.90 + 0.10 * sin(TIME * pulse_speed);
	ALBEDO = halo_color * (glow * intensity * pulse);
	ALPHA = glow * 0.72;
}
