shader_type spatial;
render_mode blend_mix, depth_draw_opaque, cull_back, diffuse_burley, specular_schlick_ggx;

uniform sampler2D water_normal_tex : hint_normal, filter_linear_mipmap, repeat_enable;
uniform vec3 shallow_color : source_color = vec3(0.14, 0.54, 0.62);
uniform vec3 deep_color : source_color = vec3(0.05, 0.22, 0.38);
uniform vec3 foam_color : source_color = vec3(0.86, 0.94, 0.96);
uniform float shore_half_extent = 33.8;
uniform float shore_width = 7.5;
uniform float uv_scale = 0.085;
uniform float normal_depth : hint_range(0.0, 1.5) = 0.48;
uniform float roughness_val : hint_range(0.04, 0.5) = 0.14;

varying vec3 v_world_pos;

void vertex() {
	v_world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

void fragment() {
	vec2 wp = v_world_pos.xz;
	vec2 uv1 = wp * uv_scale + TIME * vec2(0.016, 0.011);
	vec2 uv2 = wp * (uv_scale * 1.42) + TIME * vec2(-0.012, 0.019);
	vec3 n1 = texture(water_normal_tex, uv1).rgb;
	vec3 n2 = texture(water_normal_tex, uv2).rgb;
	vec3 blended_n = normalize(mix(n1, n2, 0.5));

	float box_d = max(abs(wp.x), abs(wp.y)) - shore_half_extent;
	float radial_shift = sin(wp.x * 0.22) * 0.55 + cos(wp.y * 0.19) * 0.55;
	float shore_dist = max(0.0, box_d + radial_shift);
	float depth_mix = smoothstep(0.0, shore_width, shore_dist);

	float wave_crest = 1.0 - smoothstep(0.0, 2.2, shore_dist);
	float ripple = 0.5 + 0.5 * sin(shore_dist * 4.5 - TIME * 2.1 + n1.r * 2.0);
	float foam = clamp(wave_crest * smoothstep(0.45, 0.92, ripple) * 0.78 + (1.0 - smoothstep(0.0, 0.65, shore_dist)) * 0.62, 0.0, 1.0);

	vec3 water_col = mix(shallow_color, deep_color, depth_mix);
	float fresnel = pow(1.0 - clamp(dot(NORMAL, VIEW), 0.0, 1.0), 3.2);
	water_col = mix(water_col, vec3(0.48, 0.72, 0.85), fresnel * 0.35);

	ALBEDO = mix(water_col, foam_color, foam);
	NORMAL_MAP = blended_n;
	NORMAL_MAP_DEPTH = normal_depth;
	ROUGHNESS = mix(roughness_val, 0.55, foam);
	SPECULAR = 0.78;
	METALLIC = 0.04;
}
