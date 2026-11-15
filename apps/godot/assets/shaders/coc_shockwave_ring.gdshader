shader_type spatial;
render_mode unshaded, blend_add, cull_disabled, depth_draw_never;

// Clash-style radial ground shockwave pulse inspired by custom_shockwave_sprite.
uniform vec4 ring_color : source_color = vec4(1.0, 0.72, 0.24, 1.0);
uniform float progress : hint_range(0.0, 1.0) = 0.25;

void fragment() {
	vec2 p = UV * 2.0 - vec2(1.0);
	float r = length(p);
	float radius = mix(0.14, 0.94, progress);
	float width = mix(0.18, 0.05, progress);
	float ring = smoothstep(radius - width, radius, r) * (1.0 - smoothstep(radius, radius + width * 0.6, r));
	float inner_glow = smoothstep(radius, max(radius - width * 1.6, 0.0), r) * 0.35 * (1.0 - progress);
	float alpha = (ring + inner_glow) * (1.0 - progress) * step(r, 1.0);
	ALBEDO = ring_color.rgb * (1.2 + ring * 0.9);
	ALPHA = clamp(alpha * ring_color.a, 0.0, 1.0);
}
