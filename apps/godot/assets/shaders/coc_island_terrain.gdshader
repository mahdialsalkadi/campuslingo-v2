shader_type spatial;
render_mode blend_mix, depth_draw_opaque, cull_back, diffuse_burley, specular_schlick_ggx;

uniform sampler2D grass_albedo : source_color, filter_linear_mipmap, repeat_enable;
uniform sampler2D grass_normal : hint_normal, filter_linear_mipmap, repeat_enable;
uniform sampler2D grass_orm : filter_linear_mipmap, repeat_enable;

uniform sampler2D cobble_albedo : source_color, filter_linear_mipmap, repeat_enable;
uniform sampler2D cobble_normal : hint_normal, filter_linear_mipmap, repeat_enable;
uniform sampler2D cobble_orm : filter_linear_mipmap, repeat_enable;

uniform sampler2D sand_albedo : source_color, filter_linear_mipmap, repeat_enable;
uniform sampler2D sand_normal : hint_normal, filter_linear_mipmap, repeat_enable;
uniform sampler2D sand_orm : filter_linear_mipmap, repeat_enable;

uniform float world_uv_scale = 0.22;
uniform float cobble_uv_scale = 0.30;
uniform float island_half_extent = 33.5;
uniform float core_half_extent = 24.0;
uniform float plaza_radius = 6.2;

varying vec3 v_world_pos;
varying vec3 v_world_nrm;

float hash21(vec2 p) {
	p = fract(p * vec2(123.34, 456.21));
	p += dot(p, p + 45.32);
	return fract(p.x * p.y);
}

float vnoise(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	f = f * f * (3.0 - 2.0 * f);
	return mix(mix(hash21(i), hash21(i + vec2(1.0, 0.0)), f.x), mix(hash21(i + vec2(0.0, 1.0)), hash21(i + vec2(1.0, 1.0)), f.x), f.y);
}

void vertex() {
	v_world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	v_world_nrm = normalize((MODEL_MATRIX * vec4(NORMAL, 0.0)).xyz);
}

void fragment() {
	vec2 wp = v_world_pos.xz;
	vec2 uv_g = wp * world_uv_scale;
	vec2 uv_c = wp * cobble_uv_scale;

	float macro_a = vnoise(wp * 0.055);
	float macro_b = vnoise(wp * 0.17 + vec2(19.3, 7.1));
	float macro_mix = clamp(macro_a * 0.65 + macro_b * 0.35, 0.0, 1.0);

	vec3 g_alb = texture(grass_albedo, uv_g).rgb;
	vec3 g_nrm = texture(grass_normal, uv_g).rgb;
	vec3 g_orm = texture(grass_orm, uv_g).rgb;

	// Extract surface grain luminance to drive lush saturated CoC meadow greens
	float g_luma = clamp(dot(g_alb, vec3(0.299, 0.587, 0.114)) * 1.15, 0.42, 1.12);
	vec3 meadow_cool = vec3(0.18, 0.38, 0.11) * g_luma;
	vec3 meadow_warm = vec3(0.32, 0.52, 0.15) * g_luma;
	vec3 grass_col = mix(meadow_cool, meadow_warm, macro_mix);

	vec3 c_alb = texture(cobble_albedo, uv_c).rgb * vec3(0.68, 0.65, 0.58);
	vec3 c_nrm = texture(cobble_normal, uv_c).rgb;
	vec3 c_orm = texture(cobble_orm, uv_c).rgb;

	vec3 s_alb = texture(sand_albedo, uv_g * 1.15).rgb * vec3(0.64, 0.56, 0.42);
	vec3 s_nrm = texture(sand_normal, uv_g * 1.15).rgb;
	vec3 s_orm = texture(sand_orm, uv_g * 1.15).rgb;

	float r_plaza = length(wp) + (macro_b - 0.5) * 1.25;
	float plaza_mask = 1.0 - smoothstep(plaza_radius - 1.1, plaza_radius + 0.85, r_plaza);
	float path_x = (1.0 - smoothstep(1.1, 2.0, abs(wp.x))) * (1.0 - smoothstep(13.5, 19.5, abs(wp.y)));
	float path_z = (1.0 - smoothstep(1.1, 2.0, abs(wp.y))) * (1.0 - smoothstep(13.5, 19.5, abs(wp.x)));
	float cobble_mask = clamp(max(plaza_mask, max(path_x, path_z) * 0.85), 0.0, 1.0);

	float edge_d = max(abs(wp.x), abs(wp.y)) + (macro_a - 0.5) * 1.8;
	float shore_mask = smoothstep(island_half_extent - 3.6, island_half_extent + 0.3, edge_d);
	float cliff_wall = 1.0 - smoothstep(0.55, 0.85, v_world_nrm.y);
	float sand_mask = clamp(shore_mask, 0.0, 1.0);

	float in_core = 1.0 - smoothstep(core_half_extent - 1.5, core_half_extent + 1.5, max(abs(wp.x), abs(wp.y)));
	grass_col *= mix(0.84, 1.05, in_core);

	vec3 col = mix(grass_col, c_alb, cobble_mask);
	col = mix(col, s_alb, sand_mask);
	col = mix(col, vec3(0.26, 0.24, 0.21) * (0.75 + 0.4 * g_luma), cliff_wall);

	vec3 nrm = mix(g_nrm, c_nrm, cobble_mask);
	nrm = mix(nrm, s_nrm, sand_mask);
	vec3 orm = mix(g_orm, c_orm, cobble_mask);
	orm = mix(orm, s_orm, sand_mask);

	ALBEDO = col;
	NORMAL_MAP = nrm;
	NORMAL_MAP_DEPTH = mix(0.9, 1.2, cobble_mask);
	AO = mix(0.7, 1.0, orm.r);
	ROUGHNESS = clamp(orm.g * 0.96, 0.45, 0.98);
	METALLIC = 0.0;
}
