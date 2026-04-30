use glam::{DVec2, DVec3};

#[cfg(test)]
use crate::runtime::Edge;
use crate::runtime::{ChunkKey, Face};

pub const FACE_COORD_EPSILON: f64 = 1.0e-12;
pub const UNIT_DIRECTION_EPSILON: f64 = 1.0e-12;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceBasis {
    pub n: DVec3,
    pub u: DVec3,
    pub v: DVec3,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CubeProjection {
    Normalized,
    #[default]
    Spherified,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainFieldSettings {
    pub planet_radius: f64,
    pub height_amplitude: f64,
    pub base_frequency: f64,
    pub hill_frequency: f64,
    pub hill_strength: f64,
    pub mountain_frequency: f64,
    pub mountain_strength: f64,
    pub detail_frequency: f64,
    pub detail_weight: f64,
    pub sea_level_meters: f64,
}

impl Default for TerrainFieldSettings {
    fn default() -> Self {
        Self {
            planet_radius: 1_000.0,
            height_amplitude: 120.0,
            base_frequency: 2.1,
            hill_frequency: 10.5,
            hill_strength: 0.22,
            mountain_frequency: 27.0,
            mountain_strength: 0.32,
            detail_frequency: 58.0,
            detail_weight: 0.035,
            sea_level_meters: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainFieldSample {
    pub height: f64,
    pub height_norm: f32,
    pub moisture: f32,
    pub land_mask: f32,
}

impl TerrainFieldSettings {
    pub fn sample(&self, unit_dir: DVec3) -> TerrainFieldSample {
        let height_amplitude = self.height_amplitude.max(0.0);
        let continent_raw = fbm_value(
            unit_dir * self.base_frequency + DVec3::new(11.3, -7.1, 19.7),
            5,
            0.53,
            2.03,
        );
        let continent_mask = smoothstep(-0.18, 0.22, continent_raw);
        let continent_height = lerp(-0.50, 0.24, continent_mask) + continent_raw * 0.10;

        let hills = fbm_value(
            unit_dir * self.hill_frequency + DVec3::new(-31.9, 4.7, 8.2),
            4,
            0.48,
            2.01,
        ) * self.hill_strength
            * 0.26
            * continent_mask;

        let mountain_area = smoothstep(
            0.10,
            0.55,
            fbm_value(
                unit_dir * (self.base_frequency * 2.4) + DVec3::new(5.4, 37.0, -12.8),
                4,
                0.52,
                2.0,
            ),
        ) * smoothstep(0.08, 0.36, continent_raw);
        let ridge_source = fbm_value(
            unit_dir * self.mountain_frequency + DVec3::new(73.2, -16.0, 41.5),
            4,
            0.50,
            2.02,
        );
        let ridge = smoothstep(0.50, 0.86, 1.0 - ridge_source.abs()).powf(1.35);
        let mountains = mountain_area * ridge * self.mountain_strength * 0.56;

        let detail = fbm_value(
            unit_dir * self.detail_frequency + DVec3::new(-3.0, 91.0, 25.0),
            3,
            0.42,
            2.0,
        ) * self.detail_weight
            * continent_mask;
        let signal = (continent_height + hills + mountains + detail).clamp(-1.0, 1.0);
        let height = signal * height_amplitude;
        let height_norm = if height_amplitude <= f64::EPSILON {
            0.5
        } else {
            ((height / height_amplitude) * 0.5 + 0.5).clamp(0.0, 1.0)
        } as f32;
        let moisture_base = value_noise3(unit_dir * 5.0 + DVec3::new(6.0, -14.0, 28.0)) * 0.5 + 0.5;
        let moisture = (moisture_base * (1.0 - 0.25 * mountain_area)).clamp(0.0, 1.0) as f32;
        let land_mask = self.land_mask_for_height(height);

        TerrainFieldSample {
            height,
            height_norm,
            moisture,
            land_mask,
        }
    }

    pub fn sample_height(&self, unit_dir: DVec3) -> f64 {
        self.sample(unit_dir).height
    }

    pub fn land_mask_for_height(&self, height: f64) -> f32 {
        smoothstep_f32(
            self.sea_level_meters as f32,
            (self.sea_level_meters + self.height_amplitude.max(1.0) * 0.02) as f32,
            height as f32,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainSample {
    pub chunk_uv: DVec2,
    pub face_uv: DVec2,
    pub face_st: DVec2,
    pub cube_point: DVec3,
    pub unit_dir: DVec3,
    pub planet_point: DVec3,
    pub height: f64,
    pub displaced_point: DVec3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleError {
    InvalidChunkKey,
    SampleGridTooSmall,
    SampleOutOfRange,
}

pub fn face_basis(face: Face) -> FaceBasis {
    match face {
        Face::Px => FaceBasis {
            n: DVec3::new(1.0, 0.0, 0.0),
            u: DVec3::new(0.0, 0.0, -1.0),
            v: DVec3::new(0.0, 1.0, 0.0),
        },
        Face::Nx => FaceBasis {
            n: DVec3::new(-1.0, 0.0, 0.0),
            u: DVec3::new(0.0, 0.0, 1.0),
            v: DVec3::new(0.0, 1.0, 0.0),
        },
        Face::Py => FaceBasis {
            n: DVec3::new(0.0, 1.0, 0.0),
            u: DVec3::new(1.0, 0.0, 0.0),
            v: DVec3::new(0.0, 0.0, -1.0),
        },
        Face::Ny => FaceBasis {
            n: DVec3::new(0.0, -1.0, 0.0),
            u: DVec3::new(1.0, 0.0, 0.0),
            v: DVec3::new(0.0, 0.0, 1.0),
        },
        Face::Pz => FaceBasis {
            n: DVec3::new(0.0, 0.0, 1.0),
            u: DVec3::new(1.0, 0.0, 0.0),
            v: DVec3::new(0.0, 1.0, 0.0),
        },
        Face::Nz => FaceBasis {
            n: DVec3::new(0.0, 0.0, -1.0),
            u: DVec3::new(-1.0, 0.0, 0.0),
            v: DVec3::new(0.0, 1.0, 0.0),
        },
    }
}

pub fn local_sample_to_chunk_uv(
    i: u32,
    j: u32,
    samples_per_edge: u32,
) -> Result<DVec2, SampleError> {
    if samples_per_edge < 2 {
        return Err(SampleError::SampleGridTooSmall);
    }
    if i >= samples_per_edge || j >= samples_per_edge {
        return Err(SampleError::SampleOutOfRange);
    }

    let denom = f64::from(samples_per_edge - 1);
    Ok(DVec2::new(f64::from(i) / denom, f64::from(j) / denom))
}

pub fn chunk_uv_to_face_uv(key: ChunkKey, chunk_uv: DVec2) -> Result<DVec2, SampleError> {
    if !key.is_valid_for_lod() {
        return Err(SampleError::InvalidChunkKey);
    }

    let resolution = f64::from(ChunkKey::resolution_for_lod(key.lod));
    Ok(DVec2::new(
        snap_unit_interval((f64::from(key.x) + chunk_uv.x) / resolution),
        snap_unit_interval((f64::from(key.y) + chunk_uv.y) / resolution),
    ))
}

pub fn face_uv_to_signed_coords(face_uv: DVec2) -> DVec2 {
    DVec2::new(
        snap_signed_coord(face_uv.x * 2.0 - 1.0),
        snap_signed_coord(face_uv.y * 2.0 - 1.0),
    )
}

pub fn cube_point_for_face(face: Face, face_st: DVec2) -> DVec3 {
    let basis = face_basis(face);
    basis.n + basis.u * face_st.x + basis.v * face_st.y
}

pub fn sample_chunk_terrain(
    key: ChunkKey,
    i: u32,
    j: u32,
    samples_per_edge: u32,
    projection: CubeProjection,
    field_settings: &TerrainFieldSettings,
) -> Result<TerrainSample, SampleError> {
    let chunk_uv = local_sample_to_chunk_uv(i, j, samples_per_edge)?;
    let face_uv = chunk_uv_to_face_uv(key, chunk_uv)?;
    let face_st = face_uv_to_signed_coords(face_uv);
    let cube_point = cube_point_for_face(key.face, face_st);
    let unit_dir = projection.project_cube_point(cube_point);
    let planet_point = unit_dir * field_settings.planet_radius;
    let height = field_settings.sample_height(unit_dir);
    let displaced_point = unit_dir * (field_settings.planet_radius + height);

    Ok(TerrainSample {
        chunk_uv,
        face_uv,
        face_st,
        cube_point,
        unit_dir,
        planet_point,
        height,
        displaced_point,
    })
}

impl CubeProjection {
    pub fn project_cube_point(self, cube_point: DVec3) -> DVec3 {
        match self {
            Self::Normalized => normalize_direction(cube_point),
            Self::Spherified => {
                let x2 = cube_point.x * cube_point.x;
                let y2 = cube_point.y * cube_point.y;
                let z2 = cube_point.z * cube_point.z;

                let warped = DVec3::new(
                    cube_point.x * (1.0 - 0.5 * y2 - 0.5 * z2 + (y2 * z2) / 3.0).sqrt(),
                    cube_point.y * (1.0 - 0.5 * z2 - 0.5 * x2 + (z2 * x2) / 3.0).sqrt(),
                    cube_point.z * (1.0 - 0.5 * x2 - 0.5 * y2 + (x2 * y2) / 3.0).sqrt(),
                );

                normalize_direction(warped)
            }
        }
    }
}

fn smoothstep(edge0: f64, edge1: f64, value: f64) -> f64 {
    let t = ((value - edge0) / (edge1 - edge0).max(f64::EPSILON)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn smootherstep(value: f64) -> f64 {
    let t = value.clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn fbm_value(mut p: DVec3, octaves: u32, persistence: f64, lacunarity: f64) -> f64 {
    let mut amplitude = 1.0;
    let mut sum = 0.0;
    let mut norm = 0.0;

    for _ in 0..octaves.max(1) {
        sum += value_noise3(p) * amplitude;
        norm += amplitude;
        amplitude *= persistence;
        p *= lacunarity;
    }

    if norm <= f64::EPSILON {
        0.0
    } else {
        (sum / norm).clamp(-1.0, 1.0)
    }
}

fn value_noise3(p: DVec3) -> f64 {
    let cell = p.floor();
    let frac = p - cell;
    let sx = smootherstep(frac.x);
    let sy = smootherstep(frac.y);
    let sz = smootherstep(frac.z);
    let ix = cell.x as i32;
    let iy = cell.y as i32;
    let iz = cell.z as i32;

    let x00 = lerp(hash_lattice(ix, iy, iz), hash_lattice(ix + 1, iy, iz), sx);
    let x10 = lerp(
        hash_lattice(ix, iy + 1, iz),
        hash_lattice(ix + 1, iy + 1, iz),
        sx,
    );
    let x01 = lerp(
        hash_lattice(ix, iy, iz + 1),
        hash_lattice(ix + 1, iy, iz + 1),
        sx,
    );
    let x11 = lerp(
        hash_lattice(ix, iy + 1, iz + 1),
        hash_lattice(ix + 1, iy + 1, iz + 1),
        sx,
    );
    let y0 = lerp(x00, x10, sy);
    let y1 = lerp(x01, x11, sy);
    lerp(y0, y1, sz)
}

fn hash_lattice(x: i32, y: i32, z: i32) -> f64 {
    let mut value = x as u64;
    value ^= (y as u64).rotate_left(21);
    value ^= (z as u64).rotate_left(42);
    value = splitmix64(value);
    let normalized = ((value >> 11) as f64) * (1.0 / ((1_u64 << 53) as f64));
    normalized * 2.0 - 1.0
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = value;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn smoothstep_f32(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0).max(f32::EPSILON)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn normalize_direction(direction: DVec3) -> DVec3 {
    let len_sq = direction.length_squared();
    if len_sq <= FACE_COORD_EPSILON {
        DVec3::ZERO
    } else {
        direction / len_sq.sqrt()
    }
}

fn snap_unit_interval(value: f64) -> f64 {
    if value.abs() <= FACE_COORD_EPSILON {
        0.0
    } else if (value - 1.0).abs() <= FACE_COORD_EPSILON {
        1.0
    } else {
        value
    }
}

fn snap_signed_coord(value: f64) -> f64 {
    if value.abs() <= FACE_COORD_EPSILON {
        0.0
    } else if (value - 1.0).abs() <= FACE_COORD_EPSILON {
        1.0
    } else if (value + 1.0).abs() <= FACE_COORD_EPSILON {
        -1.0
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f64, b: f64, epsilon: f64) {
        assert!(
            (a - b).abs() <= epsilon,
            "expected {a} ~= {b} within {epsilon}"
        );
    }

    fn approx_eq_vec2(a: DVec2, b: DVec2, epsilon: f64) {
        approx_eq(a.x, b.x, epsilon);
        approx_eq(a.y, b.y, epsilon);
    }

    fn approx_eq_vec3(a: DVec3, b: DVec3, epsilon: f64) {
        approx_eq(a.x, b.x, epsilon);
        approx_eq(a.y, b.y, epsilon);
        approx_eq(a.z, b.z, epsilon);
    }

    fn root_key(face: Face) -> ChunkKey {
        ChunkKey::new(face, 0, 0, 0)
    }

    fn edge_sample_indices(edge: Edge, sample_index: u32, samples_per_edge: u32) -> (u32, u32) {
        let last = samples_per_edge - 1;

        match edge {
            Edge::NegU => (0, sample_index),
            Edge::PosU => (last, sample_index),
            Edge::NegV => (sample_index, 0),
            Edge::PosV => (sample_index, last),
        }
    }

    fn cube_point_on_edge(face: Face, edge: Edge, q: f64) -> DVec3 {
        let basis = face_basis(face);
        let (s, t) = match edge {
            Edge::NegU => (-1.0, q * 2.0 - 1.0),
            Edge::PosU => (1.0, q * 2.0 - 1.0),
            Edge::NegV => (q * 2.0 - 1.0, -1.0),
            Edge::PosV => (q * 2.0 - 1.0, 1.0),
        };

        basis.n + basis.u * s + basis.v * t
    }

    fn face_from_normal(normal: DVec3) -> Face {
        if normal.x > 0.5 {
            Face::Px
        } else if normal.x < -0.5 {
            Face::Nx
        } else if normal.y > 0.5 {
            Face::Py
        } else if normal.y < -0.5 {
            Face::Ny
        } else if normal.z > 0.5 {
            Face::Pz
        } else if normal.z < -0.5 {
            Face::Nz
        } else {
            panic!("normal was not axis aligned: {normal:?}");
        }
    }

    fn edge_param_to_neighbor(face: Face, edge: Edge, q: f64) -> (Face, Edge, f64) {
        let basis = face_basis(face);
        let outward = match edge {
            Edge::NegU => -basis.u,
            Edge::PosU => basis.u,
            Edge::NegV => -basis.v,
            Edge::PosV => basis.v,
        };
        let cube_point = cube_point_on_edge(face, edge, q);
        let neighbor_face = face_from_normal(outward);
        let neighbor_basis = face_basis(neighbor_face);
        let s2 = snap_signed_coord(cube_point.dot(neighbor_basis.u));
        let t2 = snap_signed_coord(cube_point.dot(neighbor_basis.v));

        if (s2 + 1.0).abs() <= FACE_COORD_EPSILON {
            (
                neighbor_face,
                Edge::NegU,
                snap_unit_interval((t2 + 1.0) * 0.5),
            )
        } else if (s2 - 1.0).abs() <= FACE_COORD_EPSILON {
            (
                neighbor_face,
                Edge::PosU,
                snap_unit_interval((t2 + 1.0) * 0.5),
            )
        } else if (t2 + 1.0).abs() <= FACE_COORD_EPSILON {
            (
                neighbor_face,
                Edge::NegV,
                snap_unit_interval((s2 + 1.0) * 0.5),
            )
        } else if (t2 - 1.0).abs() <= FACE_COORD_EPSILON {
            (
                neighbor_face,
                Edge::PosV,
                snap_unit_interval((s2 + 1.0) * 0.5),
            )
        } else {
            panic!(
                "cube edge did not project to a neighboring face edge: face={face:?} edge={edge:?} q={q}"
            );
        }
    }

    #[test]
    fn face_bases_are_orthonormal_and_right_handed() {
        for face in Face::ALL {
            let basis = face_basis(face);

            approx_eq(basis.n.length(), 1.0, UNIT_DIRECTION_EPSILON);
            approx_eq(basis.u.length(), 1.0, UNIT_DIRECTION_EPSILON);
            approx_eq(basis.v.length(), 1.0, UNIT_DIRECTION_EPSILON);
            approx_eq(basis.n.dot(basis.u), 0.0, UNIT_DIRECTION_EPSILON);
            approx_eq(basis.n.dot(basis.v), 0.0, UNIT_DIRECTION_EPSILON);
            approx_eq(basis.u.dot(basis.v), 0.0, UNIT_DIRECTION_EPSILON);
            approx_eq_vec3(basis.u.cross(basis.v), basis.n, UNIT_DIRECTION_EPSILON);
        }
    }

    #[test]
    fn chunk_local_samples_map_to_expected_face_corners() {
        let key = ChunkKey::new(Face::Px, 1, 1, 0);
        let field = TerrainFieldSettings::default();
        let projection = CubeProjection::default();

        let lower_left = sample_chunk_terrain(key, 0, 0, 3, projection, &field).unwrap();
        let upper_right = sample_chunk_terrain(key, 2, 2, 3, projection, &field).unwrap();

        approx_eq_vec2(
            lower_left.chunk_uv,
            DVec2::new(0.0, 0.0),
            FACE_COORD_EPSILON,
        );
        approx_eq_vec2(lower_left.face_uv, DVec2::new(0.5, 0.0), FACE_COORD_EPSILON);
        approx_eq_vec2(
            lower_left.face_st,
            DVec2::new(0.0, -1.0),
            FACE_COORD_EPSILON,
        );
        approx_eq_vec2(
            upper_right.chunk_uv,
            DVec2::new(1.0, 1.0),
            FACE_COORD_EPSILON,
        );
        approx_eq_vec2(
            upper_right.face_uv,
            DVec2::new(1.0, 0.5),
            FACE_COORD_EPSILON,
        );
        approx_eq_vec2(
            upper_right.face_st,
            DVec2::new(1.0, 0.0),
            FACE_COORD_EPSILON,
        );
    }

    #[test]
    fn sampled_unit_directions_stay_normalized_across_faces_and_lods() {
        let field = TerrainFieldSettings::default();
        let sample_keys = [
            ChunkKey::new(Face::Px, 0, 0, 0),
            ChunkKey::new(Face::Nx, 1, 1, 1),
            ChunkKey::new(Face::Py, 2, 2, 1),
            ChunkKey::new(Face::Ny, 2, 1, 2),
            ChunkKey::new(Face::Pz, 3, 5, 1),
            ChunkKey::new(Face::Nz, 3, 6, 7),
        ];

        for key in sample_keys {
            for i in 0..5 {
                for j in 0..5 {
                    let sample =
                        sample_chunk_terrain(key, i, j, 5, CubeProjection::default(), &field)
                            .unwrap();

                    approx_eq(sample.unit_dir.length(), 1.0, 1.0e-12);
                }
            }
        }
    }

    #[test]
    fn shared_border_samples_match_across_all_faces() {
        let field = TerrainFieldSettings::default();
        let samples_per_edge = 9;
        let last = samples_per_edge - 1;

        for face in Face::ALL {
            for edge in Edge::ALL {
                for sample_index in 0..samples_per_edge {
                    let q = f64::from(sample_index) / f64::from(last);
                    let (neighbor_face, neighbor_edge, neighbor_q) =
                        edge_param_to_neighbor(face, edge, q);
                    let neighbor_index = (neighbor_q * f64::from(last))
                        .round()
                        .clamp(0.0, f64::from(last))
                        as u32;

                    let (i, j) = edge_sample_indices(edge, sample_index, samples_per_edge);
                    let (ni, nj) =
                        edge_sample_indices(neighbor_edge, neighbor_index, samples_per_edge);

                    let sample = sample_chunk_terrain(
                        root_key(face),
                        i,
                        j,
                        samples_per_edge,
                        CubeProjection::default(),
                        &field,
                    )
                    .unwrap();
                    let neighbor_sample = sample_chunk_terrain(
                        root_key(neighbor_face),
                        ni,
                        nj,
                        samples_per_edge,
                        CubeProjection::default(),
                        &field,
                    )
                    .unwrap();

                    approx_eq_vec3(sample.unit_dir, neighbor_sample.unit_dir, 1.0e-12);
                    approx_eq(sample.height, neighbor_sample.height, 1.0e-9);
                    approx_eq_vec3(
                        sample.displaced_point,
                        neighbor_sample.displaced_point,
                        1.0e-9,
                    );
                }
            }
        }
    }
}
