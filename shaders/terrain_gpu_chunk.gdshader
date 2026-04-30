shader_type spatial;
render_mode cull_back, depth_draw_opaque;

uniform sampler2D height_tile : repeat_disable, filter_linear;
uniform sampler2D material_tile : repeat_disable, filter_linear;
uniform float planet_radius = 1000.0;
uniform float face_resolution = 1.0;
uniform int face_index = 0;
uniform float chunk_x = 0.0;
uniform float chunk_y = 0.0;
uniform vec3 chunk_origin_planet = vec3(0.0);

varying vec2 v_chunk_uv;

vec3 face_normal(int face) {
	if (face == 0) {
		return vec3(1.0, 0.0, 0.0);
	}
	if (face == 1) {
		return vec3(-1.0, 0.0, 0.0);
	}
	if (face == 2) {
		return vec3(0.0, 1.0, 0.0);
	}
	if (face == 3) {
		return vec3(0.0, -1.0, 0.0);
	}
	if (face == 4) {
		return vec3(0.0, 0.0, 1.0);
	}
	return vec3(0.0, 0.0, -1.0);
}

vec3 face_u(int face) {
	if (face == 0) {
		return vec3(0.0, 0.0, -1.0);
	}
	if (face == 1) {
		return vec3(0.0, 0.0, 1.0);
	}
	if (face == 2 || face == 3 || face == 4) {
		return vec3(1.0, 0.0, 0.0);
	}
	return vec3(-1.0, 0.0, 0.0);
}

vec3 face_v(int face) {
	if (face == 2) {
		return vec3(0.0, 0.0, -1.0);
	}
	if (face == 3) {
		return vec3(0.0, 0.0, 1.0);
	}
	return vec3(0.0, 1.0, 0.0);
}

vec2 tile_uv(vec2 chunk_uv) {
	vec2 tile_coord = chunk_uv * 32.0 + vec2(1.0);
	return (tile_coord + vec2(0.5)) / 35.0;
}

vec3 project_spherified(vec3 cube_point) {
	vec3 p2 = cube_point * cube_point;
	vec3 warped = vec3(
		cube_point.x * sqrt(max(0.0, 1.0 - 0.5 * p2.y - 0.5 * p2.z + (p2.y * p2.z) / 3.0)),
		cube_point.y * sqrt(max(0.0, 1.0 - 0.5 * p2.z - 0.5 * p2.x + (p2.z * p2.x) / 3.0)),
		cube_point.z * sqrt(max(0.0, 1.0 - 0.5 * p2.x - 0.5 * p2.y + (p2.x * p2.y) / 3.0))
	);
	return normalize(warped);
}

vec3 displaced_point(vec2 chunk_uv) {
	vec2 face_uv = (vec2(chunk_x, chunk_y) + chunk_uv) / max(face_resolution, 1.0);
	vec2 face_st = face_uv * 2.0 - 1.0;
	vec3 cube_point = face_normal(face_index) + face_u(face_index) * face_st.x + face_v(face_index) * face_st.y;
	vec3 unit_dir = project_spherified(cube_point);
	float height = texture(height_tile, tile_uv(chunk_uv)).r;
	return unit_dir * (planet_radius + height);
}

void vertex() {
	v_chunk_uv = UV;
	float uv_step = 1.0 / 32.0;
	vec3 center_planet = displaced_point(UV);
	vec3 left_planet = displaced_point(clamp(UV - vec2(uv_step, 0.0), vec2(-uv_step), vec2(1.0 + uv_step)));
	vec3 right_planet = displaced_point(clamp(UV + vec2(uv_step, 0.0), vec2(-uv_step), vec2(1.0 + uv_step)));
	vec3 down_planet = displaced_point(clamp(UV - vec2(0.0, uv_step), vec2(-uv_step), vec2(1.0 + uv_step)));
	vec3 up_planet = displaced_point(clamp(UV + vec2(0.0, uv_step), vec2(-uv_step), vec2(1.0 + uv_step)));
	vec3 local_center = center_planet - chunk_origin_planet;
	vec3 tangent_u = normalize(right_planet - left_planet);
	vec3 tangent_v = normalize(up_planet - down_planet);
	VERTEX = local_center;
	NORMAL = normalize(cross(tangent_u, tangent_v));
}

void fragment() {
	vec4 material_sample = texture(material_tile, tile_uv(v_chunk_uv));
	float height_norm = clamp(material_sample.r, 0.0, 1.0);
	float slope = clamp(material_sample.g, 0.0, 1.0);
	float moisture = clamp(material_sample.b, 0.0, 1.0);
	float land_mask = clamp(material_sample.a, 0.0, 1.0);
	vec3 seabed = vec3(0.07, 0.16, 0.14);
	vec3 beach = vec3(0.62, 0.55, 0.38);
	vec3 grass_dry = vec3(0.25, 0.34, 0.18);
	vec3 grass_wet = vec3(0.12, 0.32, 0.16);
	vec3 upland = vec3(0.42, 0.39, 0.28);
	vec3 rock = vec3(0.46, 0.46, 0.44);
	vec3 snow = vec3(0.78, 0.80, 0.76);
	vec3 lowland = mix(grass_dry, grass_wet, moisture);
	vec3 base = mix(beach, lowland, smoothstep(0.50, 0.58, height_norm));
	base = mix(base, upland, smoothstep(0.60, 0.78, height_norm));
	base = mix(base, rock, max(smoothstep(0.16, 0.42, slope), smoothstep(0.74, 0.90, height_norm)));
	base = mix(base, snow, smoothstep(0.88, 0.98, height_norm) * (1.0 - smoothstep(0.35, 0.70, moisture)));
	ALBEDO = mix(seabed, base, land_mask);
	ROUGHNESS = 0.96;
	SPECULAR = 0.1;
}
