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
varying vec3 v_local_normal;

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
	vec3 center_planet = displaced_point(UV);
	vec3 du_planet = displaced_point(clamp(UV + vec2(1.0 / 32.0, 0.0), vec2(0.0), vec2(1.0)));
	vec3 dv_planet = displaced_point(clamp(UV + vec2(0.0, 1.0 / 32.0), vec2(0.0), vec2(1.0)));
	vec3 local_center = center_planet - chunk_origin_planet;
	vec3 local_du = du_planet - chunk_origin_planet;
	vec3 local_dv = dv_planet - chunk_origin_planet;
	v_local_normal = normalize(cross(local_du - local_center, local_dv - local_center));
	VERTEX = local_center;
	NORMAL = v_local_normal;
}

void fragment() {
	vec4 material_sample = texture(material_tile, tile_uv(v_chunk_uv));
	vec3 lowland = vec3(0.18, 0.37, 0.17);
	vec3 upland = vec3(0.47, 0.43, 0.30);
	vec3 rocky = vec3(0.48, 0.48, 0.50);
	vec3 biome_mix = mix(lowland, upland, clamp(material_sample.y, 0.0, 1.0));
	ALBEDO = mix(biome_mix, rocky, clamp(material_sample.z, 0.0, 1.0));
	NORMAL = normalize(v_local_normal);
	ROUGHNESS = 0.96;
	SPECULAR = 0.1;
}
