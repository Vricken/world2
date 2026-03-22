use std::sync::OnceLock;

use glam::DVec3;

use crate::geometry::{face_basis, FACE_COORD_EPSILON};
use crate::runtime::{ChunkKey, ChunkNeighbors, Edge, Face};

pub const DEFAULT_MAX_LOD: u8 = 10;
pub const MAX_SUPPORTED_MAX_LOD: u8 = 16;
pub const DIRECTED_EDGE_TRANSFORM_COUNT: usize = Face::ALL.len() * Edge::ALL.len();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EdgeTransform {
    pub neighbor_face: Face,
    pub neighbor_edge: Edge,
    pub flip: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopologyError {
    InvalidChunkKey,
}

pub fn edge_transform(face: Face, edge: Edge) -> EdgeTransform {
    edge_transform_table()[face_index(face)][edge_index(edge)]
}

pub fn same_lod_neighbor(key: ChunkKey, edge: Edge) -> Result<ChunkKey, TopologyError> {
    if !key.is_valid_for_lod() {
        return Err(TopologyError::InvalidChunkKey);
    }

    let resolution = ChunkKey::resolution_for_lod(key.lod);

    match edge {
        Edge::NegU if key.x > 0 => Ok(ChunkKey::new(key.face, key.lod, key.x - 1, key.y)),
        Edge::PosU if key.x + 1 < resolution => {
            Ok(ChunkKey::new(key.face, key.lod, key.x + 1, key.y))
        }
        Edge::NegV if key.y > 0 => Ok(ChunkKey::new(key.face, key.lod, key.x, key.y - 1)),
        Edge::PosV if key.y + 1 < resolution => {
            Ok(ChunkKey::new(key.face, key.lod, key.x, key.y + 1))
        }
        _ => {
            let xform = edge_transform(key.face, edge);
            let seam_param = match edge {
                Edge::NegU | Edge::PosU => key.y,
                Edge::NegV | Edge::PosV => key.x,
            };
            let mapped_param = if xform.flip {
                (resolution - 1) - seam_param
            } else {
                seam_param
            };

            let (neighbor_x, neighbor_y) = match xform.neighbor_edge {
                Edge::NegU => (0, mapped_param),
                Edge::PosU => (resolution - 1, mapped_param),
                Edge::NegV => (mapped_param, 0),
                Edge::PosV => (mapped_param, resolution - 1),
            };

            Ok(ChunkKey::new(
                xform.neighbor_face,
                key.lod,
                neighbor_x,
                neighbor_y,
            ))
        }
    }
}

pub fn same_lod_neighbors(key: ChunkKey) -> Result<ChunkNeighbors, TopologyError> {
    Ok(ChunkNeighbors::new([
        same_lod_neighbor(key, Edge::NegU)?,
        same_lod_neighbor(key, Edge::PosU)?,
        same_lod_neighbor(key, Edge::NegV)?,
        same_lod_neighbor(key, Edge::PosV)?,
    ]))
}

fn edge_transform_table() -> &'static [[EdgeTransform; Edge::ALL.len()]; Face::ALL.len()] {
    static EDGE_TRANSFORM_TABLE: OnceLock<[[EdgeTransform; Edge::ALL.len()]; Face::ALL.len()]> =
        OnceLock::new();

    EDGE_TRANSFORM_TABLE.get_or_init(|| {
        let mut table = [[EdgeTransform {
            neighbor_face: Face::Px,
            neighbor_edge: Edge::NegU,
            flip: false,
        }; Edge::ALL.len()]; Face::ALL.len()];

        for face in Face::ALL {
            for edge in Edge::ALL {
                table[face_index(face)][edge_index(edge)] = build_edge_transform(face, edge);
            }
        }

        table
    })
}

fn build_edge_transform(face: Face, edge: Edge) -> EdgeTransform {
    let (neighbor_face_0, neighbor_edge_0, q0) = edge_param_to_neighbor(face, edge, 0.25);
    let (neighbor_face_1, neighbor_edge_1, q1) = edge_param_to_neighbor(face, edge, 0.75);

    assert_eq!(neighbor_face_0, neighbor_face_1);
    assert_eq!(neighbor_edge_0, neighbor_edge_1);

    EdgeTransform {
        neighbor_face: neighbor_face_0,
        neighbor_edge: neighbor_edge_0,
        flip: q1 < q0,
    }
}

fn edge_param_to_neighbor(face: Face, edge: Edge, q: f64) -> (Face, Edge, f64) {
    let basis = face_basis(face);
    let (s, t, outward) = match edge {
        Edge::NegU => (-1.0, q * 2.0 - 1.0, -basis.u),
        Edge::PosU => (1.0, q * 2.0 - 1.0, basis.u),
        Edge::NegV => (q * 2.0 - 1.0, -1.0, -basis.v),
        Edge::PosV => (q * 2.0 - 1.0, 1.0, basis.v),
    };

    let cube_point = basis.n + basis.u * s + basis.v * t;
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

fn face_from_normal(normal: DVec3) -> Face {
    match (
        normal.x.round() as i32,
        normal.y.round() as i32,
        normal.z.round() as i32,
    ) {
        (1, 0, 0) => Face::Px,
        (-1, 0, 0) => Face::Nx,
        (0, 1, 0) => Face::Py,
        (0, -1, 0) => Face::Ny,
        (0, 0, 1) => Face::Pz,
        (0, 0, -1) => Face::Nz,
        _ => panic!("normal was not axis aligned: {normal:?}"),
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

const fn face_index(face: Face) -> usize {
    match face {
        Face::Px => 0,
        Face::Nx => 1,
        Face::Py => 2,
        Face::Ny => 3,
        Face::Pz => 4,
        Face::Nz => 5,
    }
}

const fn edge_index(edge: Edge) -> usize {
    match edge {
        Edge::NegU => 0,
        Edge::PosU => 1,
        Edge::NegV => 2,
        Edge::PosV => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opposite_edge(edge: Edge) -> Edge {
        match edge {
            Edge::NegU => Edge::PosU,
            Edge::PosU => Edge::NegU,
            Edge::NegV => Edge::PosV,
            Edge::PosV => Edge::NegV,
        }
    }

    fn expected_chunk_count(max_lod: u8) -> usize {
        let mut total = 0usize;

        for lod in 0..=max_lod {
            let resolution = ChunkKey::resolution_for_lod(lod) as usize;
            total += Face::ALL.len() * resolution * resolution;
        }

        total
    }

    #[test]
    fn basis_derived_transform_table_contains_all_24_directed_entries() {
        let mut seen = 0usize;

        for face in Face::ALL {
            for edge in Edge::ALL {
                let xform = edge_transform(face, edge);
                assert_ne!(xform.neighbor_face, face);
                seen += 1;
            }
        }

        assert_eq!(seen, DIRECTED_EDGE_TRANSFORM_COUNT);
    }

    #[test]
    fn directed_edge_transforms_round_trip_across_shared_edges() {
        for face in Face::ALL {
            for edge in Edge::ALL {
                let xform = edge_transform(face, edge);
                let reverse = edge_transform(xform.neighbor_face, xform.neighbor_edge);

                assert_eq!(reverse.neighbor_face, face);
                assert_eq!(reverse.neighbor_edge, edge);
                assert_eq!(reverse.flip, xform.flip);
            }
        }
    }

    #[test]
    fn same_lod_neighbors_match_internal_and_cross_face_cases() {
        let interior = ChunkKey::new(Face::Px, 3, 4, 2);
        assert_eq!(
            same_lod_neighbor(interior, Edge::NegU).unwrap(),
            ChunkKey::new(Face::Px, 3, 3, 2)
        );
        assert_eq!(
            same_lod_neighbor(interior, Edge::PosV).unwrap(),
            ChunkKey::new(Face::Px, 3, 4, 3)
        );

        let cross_face = ChunkKey::new(Face::Px, 2, 0, 0);
        assert_eq!(
            same_lod_neighbor(cross_face, Edge::NegU).unwrap(),
            ChunkKey::new(Face::Pz, 2, 3, 0)
        );
        assert_eq!(
            same_lod_neighbor(cross_face, Edge::NegV).unwrap(),
            ChunkKey::new(Face::Ny, 2, 3, 3)
        );
    }

    #[test]
    fn same_lod_neighbor_rejects_invalid_chunk_keys() {
        let invalid = ChunkKey::new(Face::Px, 2, 4, 0);

        assert_eq!(
            same_lod_neighbor(invalid, Edge::NegU),
            Err(TopologyError::InvalidChunkKey)
        );
        assert_eq!(
            same_lod_neighbors(invalid),
            Err(TopologyError::InvalidChunkKey)
        );
    }

    #[test]
    fn full_grid_round_trip_stays_deterministic_to_default_max_lod() {
        let mut visited = 0usize;

        for lod in 0..=DEFAULT_MAX_LOD {
            let resolution = ChunkKey::resolution_for_lod(lod);

            for face in Face::ALL {
                for x in 0..resolution {
                    for y in 0..resolution {
                        let key = ChunkKey::new(face, lod, x, y);
                        let neighbors = same_lod_neighbors(key).unwrap();

                        for edge in Edge::ALL {
                            let neighbor = neighbors.get(edge);
                            let reverse_edge = if neighbor.face == face {
                                opposite_edge(edge)
                            } else {
                                edge_transform(face, edge).neighbor_edge
                            };

                            assert_eq!(same_lod_neighbor(neighbor, reverse_edge).unwrap(), key);
                        }

                        visited += 1;
                    }
                }
            }
        }

        assert_eq!(visited, expected_chunk_count(DEFAULT_MAX_LOD));
    }
}
