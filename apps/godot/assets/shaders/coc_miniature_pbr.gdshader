shader_type spatial;
render_mode blend_mix, depth_draw_opaque, cull_back, diffuse_burley, specular_schlick_ggx;

uniform sampler2D albedo_tex : source_color, filter_linear_mipmap, repeat_enable;
uniform sampler2D normal_tex : hint_normal, filter_linear_mipmap, repeat_enable;
uniform sampler2D orm_tex : filter_linear_mipmap, repeat_enable;
uniform sampler2D emission_tex : source_color, filter_linear_mipmap, repeat_enable;

uniform bool has_albedo_tex = false;
uniform bool has_normal_tex = false;
uniform bool has_orm_tex = false;
uniform bool has_emission_tex = false;

uniform vec4 albedo_tint : source_color = vec4(1.0);
uniform vec2 uv_scale = vec2(1.0, 1.0);
uniform float normal_scale : hint_range(0.0, 2.0) = 1.0;
uniform float roughness_scale : hint_range(0.0, 2.0) = 1.0;
uniform float metallic_scale : hint_range(0.0, 2.0) = 1.0;
uniform float base_roughness : hint_range(0.04, 1.0) = 0.78;
uniform float base_metallic : hint_range(0.0, 1.0) = 0.0;
uniform float ao_strength : hint_range(0.0, 1.0) = 0.78;
uniform float vertex_ao_strength : hint_range(0.0, 1.0) = 0.85;

uniform vec3 emission_color : source_color = vec3(0.0);
uniform float emission_energy : hint_range(0.0, 8.0) = 0.0;
uniform vec4 color_mul : source_color = vec4(1.0);
uniform vec4 color_add : source_color = vec4(0.0);

uniform vec3 rim_color : source_color = vec3(0.32, 0.88, 1.0);
uniform float rim_strength : hint_range(0.0, 2.5) = 0.0;
uniform float clip_y_plane = -100.0;
uniform bool use_instance_data = false;

varying vec3 v_world_pos;
varying vec4 v_custom;
varying float v_vert_ao;

void vertex() {
	v_world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	v_vert_ao = clamp(COLOR.r, 0.12, 1.0);
	v_custom = use_instance_data ? INSTANCE_CUSTOM : vec4(1.0, 0.0, 0.0, 1.0);
}

void fragment() {
	if (v_world_pos.y < clip_y_plane) {
		discard;
	}
	vec2 uv = UV * uv_scale;
	vec3 base_albedo = has_albedo_tex ? texture(albedo_tex, uv).rgb * albedo_tint.rgb : albedo_tint.rgb;
	base_albedo *= mix(0.64, 1.0, clamp(v_vert_ao * (1.0 - vertex_ao_strength) + vertex_ao_strength * v_vert_ao, 0.0, 1.0));
	float ao_map = 1.0;
	float rough_val = base_roughness;
	float metal_val = base_metallic;
	if (has_orm_tex) {
		vec3 orm = texture(orm_tex, uv).rgb;
		ao_map = orm.r;
		rough_val = clamp(orm.g * roughness_scale, 0.06, 1.0);
		metal_val = clamp(orm.b * metallic_scale, 0.0, 1.0);
	}
	vec3 tinted = base_albedo * color_mul.rgb + color_add.rgb;
	float brownout = use_instance_data ? clamp(v_custom.b, 0.0, 1.0) : 0.0;
	float lum = dot(tinted, vec3(0.299, 0.587, 0.114));
	tinted = mix(tinted, vec3(lum * 0.65, lum * 0.62, lum * 0.58), brownout * 0.72);

	ALBEDO = tinted;
	ROUGHNESS = rough_val;
	METALLIC = metal_val;
	AO = mix(1.0, ao_map, ao_strength) * mix(1.0, v_vert_ao, vertex_ao_strength);

	if (has_normal_tex) {
		NORMAL_MAP = texture(normal_tex, uv).rgb;
		NORMAL_MAP_DEPTH = normal_scale;
	}

	float fresnel = pow(1.0 - clamp(dot(NORMAL, VIEW), 0.0, 1.0), 3.0);
	float sel_rim = max(rim_strength, use_instance_data ? v_custom.g * 1.25 : 0.0);
	vec3 em = emission_color * emission_energy;
	if (has_emission_tex) {
		em *= texture(emission_tex, uv).rgb;
	}
	EMISSION = em + rim_color * (fresnel * sel_rim);
}
