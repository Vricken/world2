use std::array;
use std::sync::OnceLock;

use crate::runtime::Edge;

pub const CANONICAL_TOPOLOGY_CLASS: u16 = 1;
pub const QUADS_PER_EDGE: u32 = 32;
pub const VISIBLE_VERTICES_PER_EDGE: u32 = QUADS_PER_EDGE + 1;
pub const BORDER_RING_QUADS: u32 = 1;
pub const SAMPLED_VERTICES_PER_EDGE: u32 = VISIBLE_VERTICES_PER_EDGE + BORDER_RING_QUADS * 2;
pub const STITCH_VARIANT_COUNT: usize = 16;
pub const BASE_STITCH_MASK: u8 = 0;
pub const STITCH_MASK_ALL: u8 = 0b1111;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StitchError {
    InvalidStitchMask(u8),
    UnsupportedNeighborLodDelta {
        edge: Edge,
        chunk_lod: u8,
        neighbor_lod: u8,
    },
}

#[derive(Debug)]
pub struct CanonicalChunkTopology {
    base_indices: Vec<i32>,
    stitch_variants: [Vec<i32>; STITCH_VARIANT_COUNT],
}

impl CanonicalChunkTopology {
    pub fn vertex_count(&self) -> u32 {
        VISIBLE_VERTICES_PER_EDGE * VISIBLE_VERTICES_PER_EDGE
    }

    pub fn sampled_vertex_count(&self) -> u32 {
        SAMPLED_VERTICES_PER_EDGE * SAMPLED_VERTICES_PER_EDGE
    }

    pub fn base_indices(&self) -> &[i32] {
        &self.base_indices
    }

    pub fn stitch_indices(&self, stitch_mask: u8) -> Result<&[i32], StitchError> {
        validate_stitch_mask(stitch_mask)?;
        Ok(&self.stitch_variants[stitch_mask as usize])
    }

    pub fn index_count_for_mask(&self, stitch_mask: u8) -> Result<u32, StitchError> {
        Ok(self.stitch_indices(stitch_mask)?.len() as u32)
    }
}

pub fn canonical_chunk_topology() -> &'static CanonicalChunkTopology {
    static CANONICAL_TOPOLOGY: OnceLock<CanonicalChunkTopology> = OnceLock::new();

    CANONICAL_TOPOLOGY.get_or_init(|| CanonicalChunkTopology {
        base_indices: build_variant_indices(BASE_STITCH_MASK)
            .expect("base stitch mask must be valid"),
        stitch_variants: array::from_fn(|mask| {
            build_variant_indices(mask as u8).expect("canonical stitch masks must be valid")
        }),
    })
}

pub fn stitch_mask_bit(edge: Edge) -> u8 {
    match edge {
        Edge::NegU => 1 << 0,
        Edge::PosU => 1 << 1,
        Edge::NegV => 1 << 2,
        Edge::PosV => 1 << 3,
    }
}

pub fn stitch_mask_contains(stitch_mask: u8, edge: Edge) -> Result<bool, StitchError> {
    validate_stitch_mask(stitch_mask)?;
    Ok(stitch_mask & stitch_mask_bit(edge) != 0)
}

pub fn validate_stitch_mask(stitch_mask: u8) -> Result<(), StitchError> {
    if stitch_mask & !STITCH_MASK_ALL != 0 {
        Err(StitchError::InvalidStitchMask(stitch_mask))
    } else {
        Ok(())
    }
}

pub fn stitch_mask_from_neighbor_lods(
    chunk_lod: u8,
    neighbor_lods: [u8; 4],
) -> Result<u8, StitchError> {
    let mut stitch_mask = BASE_STITCH_MASK;

    for (edge, neighbor_lod) in Edge::ALL.into_iter().zip(neighbor_lods) {
        match chunk_lod.cmp(&neighbor_lod) {
            std::cmp::Ordering::Equal => {}
            std::cmp::Ordering::Greater if chunk_lod - neighbor_lod == 1 => {
                stitch_mask |= stitch_mask_bit(edge);
            }
            std::cmp::Ordering::Less if neighbor_lod - chunk_lod == 1 => {}
            _ => {
                return Err(StitchError::UnsupportedNeighborLodDelta {
                    edge,
                    chunk_lod,
                    neighbor_lod,
                });
            }
        }
    }

    Ok(stitch_mask)
}

fn build_variant_indices(stitch_mask: u8) -> Result<Vec<i32>, StitchError> {
    validate_stitch_mask(stitch_mask)?;

    let stitch_neg_u = stitch_mask_contains(stitch_mask, Edge::NegU)?;
    let stitch_pos_u = stitch_mask_contains(stitch_mask, Edge::PosU)?;
    let stitch_neg_v = stitch_mask_contains(stitch_mask, Edge::NegV)?;
    let stitch_pos_v = stitch_mask_contains(stitch_mask, Edge::PosV)?;

    let mut covered = vec![false; (QUADS_PER_EDGE * QUADS_PER_EDGE) as usize];
    let mut indices = Vec::with_capacity(base_index_count_hint(stitch_mask));

    if stitch_neg_u && stitch_neg_v {
        add_corner_block(&mut indices, &mut covered, 0, 0);
    }
    if stitch_pos_u && stitch_neg_v {
        add_corner_block(&mut indices, &mut covered, QUADS_PER_EDGE - 2, 0);
    }
    if stitch_neg_u && stitch_pos_v {
        add_corner_block(&mut indices, &mut covered, 0, QUADS_PER_EDGE - 2);
    }
    if stitch_pos_u && stitch_pos_v {
        add_corner_block(
            &mut indices,
            &mut covered,
            QUADS_PER_EDGE - 2,
            QUADS_PER_EDGE - 2,
        );
    }

    if stitch_neg_u {
        let start_y = if stitch_neg_v { 2 } else { 0 };
        let end_y = QUADS_PER_EDGE - if stitch_pos_v { 2 } else { 0 };
        for y in (start_y..end_y).step_by(2) {
            add_neg_u_stitch_pair(&mut indices, &mut covered, y);
        }
    }

    if stitch_pos_u {
        let start_y = if stitch_neg_v { 2 } else { 0 };
        let end_y = QUADS_PER_EDGE - if stitch_pos_v { 2 } else { 0 };
        for y in (start_y..end_y).step_by(2) {
            add_pos_u_stitch_pair(&mut indices, &mut covered, y);
        }
    }

    if stitch_neg_v {
        let start_x = if stitch_neg_u { 2 } else { 0 };
        let end_x = QUADS_PER_EDGE - if stitch_pos_u { 2 } else { 0 };
        for x in (start_x..end_x).step_by(2) {
            add_neg_v_stitch_pair(&mut indices, &mut covered, x);
        }
    }

    if stitch_pos_v {
        let start_x = if stitch_neg_u { 2 } else { 0 };
        let end_x = QUADS_PER_EDGE - if stitch_pos_u { 2 } else { 0 };
        for x in (start_x..end_x).step_by(2) {
            add_pos_v_stitch_pair(&mut indices, &mut covered, x);
        }
    }

    for y in 0..QUADS_PER_EDGE {
        for x in 0..QUADS_PER_EDGE {
            if is_covered(&covered, x, y) {
                continue;
            }

            add_base_cell(&mut indices, x, y);
            cover_cell(&mut covered, x, y);
        }
    }

    Ok(indices)
}

fn base_index_count_hint(stitch_mask: u8) -> usize {
    let base_indices = (QUADS_PER_EDGE * QUADS_PER_EDGE * 6) as usize;
    let stitched_edges = stitch_mask.count_ones() as usize;
    base_indices.saturating_sub(stitched_edges * 48)
}

fn add_base_cell(indices: &mut Vec<i32>, x: u32, y: u32) {
    let a = vertex_index(x, y);
    let b = vertex_index(x + 1, y);
    let c = vertex_index(x, y + 1);
    let d = vertex_index(x + 1, y + 1);

    push_triangle(indices, a, d, c);
    push_triangle(indices, a, b, d);
}

fn add_corner_block(indices: &mut Vec<i32>, covered: &mut [bool], x0: u32, y0: u32) {
    let a = vertex_index(x0, y0);
    let b = vertex_index(x0 + 2, y0);
    let c = vertex_index(x0 + 2, y0 + 2);
    let d = vertex_index(x0, y0 + 2);
    let center = vertex_index(x0 + 1, y0 + 1);

    push_triangle(indices, a, b, center);
    push_triangle(indices, b, c, center);
    push_triangle(indices, c, d, center);
    push_triangle(indices, d, a, center);

    cover_cell(covered, x0, y0);
    cover_cell(covered, x0 + 1, y0);
    cover_cell(covered, x0, y0 + 1);
    cover_cell(covered, x0 + 1, y0 + 1);
}

fn add_neg_u_stitch_pair(indices: &mut Vec<i32>, covered: &mut [bool], y: u32) {
    let p0 = vertex_index(0, y);
    let i0 = vertex_index(1, y);
    let i1 = vertex_index(1, y + 1);
    let p2 = vertex_index(0, y + 2);
    let i2 = vertex_index(1, y + 2);

    push_triangle(indices, p0, i0, i1);
    push_triangle(indices, p0, i1, p2);
    push_triangle(indices, p2, i1, i2);

    cover_cell(covered, 0, y);
    cover_cell(covered, 0, y + 1);
}

fn add_pos_u_stitch_pair(indices: &mut Vec<i32>, covered: &mut [bool], y: u32) {
    let x = QUADS_PER_EDGE - 1;
    let p0 = vertex_index(x + 1, y);
    let i0 = vertex_index(x, y);
    let i1 = vertex_index(x, y + 1);
    let p2 = vertex_index(x + 1, y + 2);
    let i2 = vertex_index(x, y + 2);

    push_triangle(indices, p0, i1, i0);
    push_triangle(indices, p0, p2, i1);
    push_triangle(indices, p2, i2, i1);

    cover_cell(covered, x, y);
    cover_cell(covered, x, y + 1);
}

fn add_neg_v_stitch_pair(indices: &mut Vec<i32>, covered: &mut [bool], x: u32) {
    let p0 = vertex_index(x, 0);
    let i0 = vertex_index(x, 1);
    let i1 = vertex_index(x + 1, 1);
    let p2 = vertex_index(x + 2, 0);
    let i2 = vertex_index(x + 2, 1);

    push_triangle(indices, p0, i1, i0);
    push_triangle(indices, p0, p2, i1);
    push_triangle(indices, p2, i2, i1);

    cover_cell(covered, x, 0);
    cover_cell(covered, x + 1, 0);
}

fn add_pos_v_stitch_pair(indices: &mut Vec<i32>, covered: &mut [bool], x: u32) {
    let y = QUADS_PER_EDGE - 1;
    let p0 = vertex_index(x, y + 1);
    let i0 = vertex_index(x, y);
    let i1 = vertex_index(x + 1, y);
    let p2 = vertex_index(x + 2, y + 1);
    let i2 = vertex_index(x + 2, y);

    push_triangle(indices, p0, i0, i1);
    push_triangle(indices, p0, i1, p2);
    push_triangle(indices, p2, i1, i2);

    cover_cell(covered, x, y);
    cover_cell(covered, x + 1, y);
}

fn push_triangle(indices: &mut Vec<i32>, a: i32, b: i32, c: i32) {
    indices.extend_from_slice(&[a, b, c]);
}

fn vertex_index(x: u32, y: u32) -> i32 {
    (y * VISIBLE_VERTICES_PER_EDGE + x) as i32
}

fn cover_cell(covered: &mut [bool], x: u32, y: u32) {
    let index = (y * QUADS_PER_EDGE + x) as usize;
    covered[index] = true;
}

fn is_covered(covered: &[bool], x: u32, y: u32) -> bool {
    covered[(y * QUADS_PER_EDGE + x) as usize]
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn edge_vertex_coords(edge: Edge) -> impl Iterator<Item = (u32, u32)> {
        (0..VISIBLE_VERTICES_PER_EDGE).map(move |step| match edge {
            Edge::NegU => (0, step),
            Edge::PosU => (VISIBLE_VERTICES_PER_EDGE - 1, step),
            Edge::NegV => (step, 0),
            Edge::PosV => (step, VISIBLE_VERTICES_PER_EDGE - 1),
        })
    }

    #[test]
    fn canonical_constants_match_phase_five_contract() {
        let topology = canonical_chunk_topology();

        assert_eq!(QUADS_PER_EDGE, 32);
        assert_eq!(VISIBLE_VERTICES_PER_EDGE, 33);
        assert_eq!(SAMPLED_VERTICES_PER_EDGE, 35);
        assert_eq!(topology.vertex_count(), 1_089);
        assert_eq!(topology.sampled_vertex_count(), 1_225);
        assert_eq!(topology.base_indices().len(), 6_144);
    }

    #[test]
    fn stitch_variants_exist_for_all_16_masks_and_stay_in_range() {
        let topology = canonical_chunk_topology();
        let max_vertex = topology.vertex_count() as i32;

        for stitch_mask in 0..STITCH_VARIANT_COUNT as u8 {
            let indices = topology.stitch_indices(stitch_mask).unwrap();

            assert!(!indices.is_empty());
            assert!(indices
                .iter()
                .all(|index| *index >= 0 && *index < max_vertex));
            assert_eq!(
                topology.index_count_for_mask(stitch_mask).unwrap() as usize,
                indices.len()
            );
        }
    }

    #[test]
    fn stitched_edges_only_keep_even_boundary_vertices() {
        let topology = canonical_chunk_topology();

        for edge in Edge::ALL {
            let stitch_mask = stitch_mask_bit(edge);
            let indices = topology.stitch_indices(stitch_mask).unwrap();
            let used_vertices = indices.iter().copied().collect::<HashSet<_>>();

            for (step, (x, y)) in edge_vertex_coords(edge).enumerate() {
                let vertex = vertex_index(x, y);
                if step % 2 == 1 {
                    assert!(
                        !used_vertices.contains(&vertex),
                        "odd boundary vertex {vertex} unexpectedly used for {edge:?}"
                    );
                } else {
                    assert!(
                        used_vertices.contains(&vertex),
                        "even boundary vertex {vertex} missing for {edge:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn neighbor_lod_to_stitch_mask_only_marks_fine_to_coarse_edges() {
        let mask = stitch_mask_from_neighbor_lods(5, [4, 5, 6, 4]).unwrap();

        assert_eq!(
            mask,
            stitch_mask_bit(Edge::NegU) | stitch_mask_bit(Edge::PosV)
        );

        let unsupported = stitch_mask_from_neighbor_lods(5, [3, 5, 5, 5]);
        assert_eq!(
            unsupported,
            Err(StitchError::UnsupportedNeighborLodDelta {
                edge: Edge::NegU,
                chunk_lod: 5,
                neighbor_lod: 3,
            })
        );
    }
}
