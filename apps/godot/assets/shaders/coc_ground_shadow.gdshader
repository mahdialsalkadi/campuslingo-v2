shader_type spatial;
render_mode blend_mix, depth_draw_never, cull_back, unshaded;

uniform vec2 half_extents = vec2(0.28, 0.28);
uniform float softness : hint_range(0.02, 0.35) = 0.18;
uniform float opacity : hint_range(0.0, 0.95) = 0.36;
uniform vec3 shadow_color : source_color = vec3(0.06, 0.07, 0.04);
uniform vec3 soil_color : source_color = vec3(0.18, 0.16, 0.10);
uniform float soil_ring_strength : hint_range(0.0, 0.6) = 0.12;

void fragment() {
	vec2 centered = abs(UV - vec2(0.5));
	vec2 clamped_he = min(half_extents, vec2(0.30));
	float r = 0.11;
	vec2 q = centered - max(clamped_he - vec2(r), vec2(0.0));
	float d = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - r;
	float edge_fade = 1.0 - smoothstep(0.30, 0.48, max(centered.x, centered.y));
	float core = (1.0 - smoothstep(-0.04, softness, d)) * edge_fade;
	float soil_band = (1.0 - smoothstep(0.0, softness * 0.9, abs(d - 0.02))) * soil_ring_strength * edge_fade;
	vec3 col = mix(soil_color, shadow_color, clamp(core * 1.25, 0.0, 1.0));
	float alpha = clamp(core * opacity + soil_band * (1.0 - core * 0.75), 0.0, 0.46) * edge_fade;
	ALBEDO = col;
	ALPHA = alpha;
}
