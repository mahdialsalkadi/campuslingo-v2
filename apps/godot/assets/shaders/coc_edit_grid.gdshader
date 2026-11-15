shader_type spatial;
render_mode blend_mix, depth_draw_never, cull_back, unshaded;

uniform float cell_size = 1.5;
uniform float board_half_extent = 30.0;
uniform float core_half_extent = 24.0;
uniform float overlay_alpha : hint_range(0.0, 1.0) = 0.0;
uniform vec3 grid_color : source_color = vec3(0.95, 0.92, 0.78);
uniform vec3 core_border_color : source_color = vec3(0.98, 0.78, 0.28);

varying vec3 v_world_pos;

void vertex() {
	v_world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

void fragment() {
	if (overlay_alpha <= 0.01) {
		discard;
	}
	vec2 wp = v_world_pos.xz + vec2(board_half_extent);
	vec2 cell_uv = fract(wp / cell_size);
	float lx = 1.0 - smoothstep(0.0, 0.045, min(cell_uv.x, 1.0 - cell_uv.x));
	float lz = 1.0 - smoothstep(0.0, 0.045, min(cell_uv.y, 1.0 - cell_uv.y));
	float line = max(lx, lz);

	float core_edge = abs(max(abs(v_world_pos.x), abs(v_world_pos.z)) - core_half_extent);
	float border_mask = 1.0 - smoothstep(0.04, 0.24, core_edge);

	vec3 col = mix(grid_color, core_border_color, border_mask);
	float alpha = clamp(max(line * 0.42, border_mask * 0.85) * overlay_alpha, 0.0, 0.9);
	ALBEDO = col;
	ALPHA = alpha;
}
