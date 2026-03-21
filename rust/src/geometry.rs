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
    pub detail_frequency: f64,
    pub detail_weight: f64,
}

impl Default for TerrainFieldSettings {
    fn default() -> Self {
        Self {
            planet_radius: 1_000.0,
            height_amplitude: 120.0,
            base_frequency: 0.006,
            detail_frequency: 0.021,
            detail_weight: 0.28,
        }
    }
}

impl TerrainFieldSettings {
    pub fn sample_height(&self, unit_dir: DVec3) -> f64 {
        let planet_space = unit_dir * self.planet_radius;
        let primary = harmonic_signal(planet_space * self.base_frequency);
        let detail = ridge_signal(planet_space * self.detail_frequency);
        let signal = (0.78 * primary + self.detail_weight * detail).clamp(-1.0, 1.0);

        signal * self.height_amplitude
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

fn harmonic_signal(p: DVec3) -> f64 {
    let octave_0 = (p.dot(DVec3::new(0.971, 1.113, 1.357))).sin();
    let octave_1 = (p.dot(DVec3::new(-1.947, 0.613, 1.731))).sin();
    let octave_2 = ((p.x * 0.37).cos() + (p.y * 0.53).sin() + (p.z * 0.79).cos()) / 3.0;

    (0.52 * octave_0 + 0.33 * octave_1 + 0.15 * octave_2).clamp(-1.0, 1.0)
}

fn ridge_signal(p: DVec3) -> f64 {
    1.0 - 2.0 * harmonic_signal(p).abs()
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
