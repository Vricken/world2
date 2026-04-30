use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChunkRidState {
    pub mesh_rid: Option<Rid>,
    pub render_instance_rid: Option<Rid>,
    pub physics_body_rid: Option<Rid>,
    pub physics_shape_rid: Option<Rid>,
    pub asset_multimesh_rids: Vec<Rid>,
    pub asset_instance_rids: Vec<Rid>,
    pub render_resident: bool,
    pub physics_resident: bool,
    pub active_surface_class: Option<SurfaceClassKey>,
    pub gd_staging: Option<GdPackedStaging>,
    pub pooled_surface_class: Option<SurfaceClassKey>,
}

#[derive(Debug)]
pub struct CanonicalRenderMeshEntry {
    pub surface_class: SurfaceClassKey,
    pub mesh: Option<Gd<ArrayMesh>>,
}

#[derive(Debug, Default)]
pub struct WaterRenderState {
    pub mesh: Option<Gd<ArrayMesh>>,
    pub material: Option<Gd<StandardMaterial3D>>,
    pub instance_rid: Option<Rid>,
    pub radius: f64,
    pub active: bool,
}

#[derive(Debug)]
pub struct GpuMaterialPoolEntry {
    pub surface_class: SurfaceClassKey,
    pub shader_material: Option<Gd<ShaderMaterial>>,
    pub height_texture: Option<Gd<ImageTexture>>,
    pub material_texture: Option<Gd<ImageTexture>>,
    pub height_image: Option<Gd<Image>>,
    pub material_image: Option<Gd<Image>>,
    pub height_bytes: Option<PackedByteArray>,
    pub material_bytes: Option<PackedByteArray>,
}

impl GpuMaterialPoolEntry {
    pub fn new(surface_class: SurfaceClassKey) -> Self {
        Self {
            surface_class,
            shader_material: None,
            height_texture: None,
            material_texture: None,
            height_image: None,
            material_image: None,
            height_bytes: None,
            material_bytes: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderResidencyEntry {
    pub desired: bool,
    pub active: bool,
    pub required_surface_class: Option<SurfaceClassKey>,
    pub resident_surface_class: Option<SurfaceClassKey>,
    pub last_selected_tick: u64,
    pub last_service_tick: Option<u64>,
    pub last_unused_tick: u64,
    pub refinement_benefit_px: f32,
    pub distance_key_mm: u64,
    pub selected_starvation_frames: u32,
    pub starvation_failure_reported: bool,
}

impl Default for RenderResidencyEntry {
    fn default() -> Self {
        Self {
            desired: false,
            active: false,
            required_surface_class: None,
            resident_surface_class: None,
            last_selected_tick: 0,
            last_service_tick: None,
            last_unused_tick: 0,
            refinement_benefit_px: 0.0,
            distance_key_mm: u64::MAX,
            selected_starvation_frames: 0,
            starvation_failure_reported: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct RenderTileHandle {
    pub slot: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderTileSlotEntry {
    pub handle: RenderTileHandle,
    pub key: ChunkKey,
    pub sample_count: usize,
    pub byte_len: usize,
    pub last_touched_tick: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenderTilePoolState {
    pub slots: Vec<Option<RenderTileSlotEntry>>,
    pub key_to_slot: HashMap<ChunkKey, usize>,
    pub free_slots: Vec<usize>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderTilePoolSnapshot {
    pub total_slots: usize,
    pub active_slots: usize,
    pub free_slots: usize,
    pub resident_bytes: usize,
    pub eviction_ready_slots: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CollisionResidencySnapshot {
    pub entries: usize,
    pub bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeamDebugSnapshot {
    pub active_render_chunks: usize,
    pub active_chunks_with_surface_class: usize,
    pub active_chunks_missing_surface_class: usize,
    pub active_stitched_chunks: usize,
    pub pending_surface_class_mismatch_chunks: usize,
    pub active_stitch_mask_counts: [usize; mesh_topology::STITCH_VARIANT_COUNT],
    pub active_stitched_edge_counts: [usize; Edge::ALL.len()],
    pub pooled_render_entries: usize,
    pub pooled_stitch_mask_counts: [usize; mesh_topology::STITCH_VARIANT_COUNT],
}

impl Default for SeamDebugSnapshot {
    fn default() -> Self {
        Self {
            active_render_chunks: 0,
            active_chunks_with_surface_class: 0,
            active_chunks_missing_surface_class: 0,
            active_stitched_chunks: 0,
            pending_surface_class_mismatch_chunks: 0,
            active_stitch_mask_counts: [0; mesh_topology::STITCH_VARIANT_COUNT],
            active_stitched_edge_counts: [0; Edge::ALL.len()],
            pooled_render_entries: 0,
            pooled_stitch_mask_counts: [0; mesh_topology::STITCH_VARIANT_COUNT],
        }
    }
}

impl SeamDebugSnapshot {
    pub fn active_stitch_mask_summary(&self) -> String {
        stitch_mask_summary(&self.active_stitch_mask_counts)
    }

    pub fn pooled_stitch_mask_summary(&self) -> String {
        stitch_mask_summary(&self.pooled_stitch_mask_counts)
    }

    pub fn active_stitched_edge_summary(&self) -> String {
        Edge::ALL
            .into_iter()
            .enumerate()
            .map(|(index, edge)| {
                format!(
                    "{}:{}",
                    edge.label(),
                    self.active_stitched_edge_counts[index]
                )
            })
            .collect::<Vec<_>>()
            .join("|")
    }

    pub(crate) fn record_active_mask(&mut self, stitch_mask: u8) {
        self.active_chunks_with_surface_class += 1;
        record_mask(
            stitch_mask,
            &mut self.active_stitch_mask_counts,
            Some((
                &mut self.active_stitched_chunks,
                &mut self.active_stitched_edge_counts,
            )),
        );
    }

    pub(crate) fn record_pooled_mask(&mut self, stitch_mask: u8) {
        self.pooled_render_entries += 1;
        record_mask(stitch_mask, &mut self.pooled_stitch_mask_counts, None);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderPoolEntry {
    pub mesh_rid: Rid,
    pub render_instance_rid: Rid,
    pub surface_class: SurfaceClassKey,
    pub gd_staging: Option<GdPackedStaging>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PhysicsPoolEntry {
    pub physics_body_rid: Rid,
    pub physics_shape_rid: Rid,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RenderWarmPath {
    ReuseCurrentSurface,
    ReusePooledSurface(RenderPoolEntry),
    ColdPath(RenderFallbackReason),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderFallbackReason {
    MissingCurrentSurfaceClass,
    IncompatibleCurrentSurfaceClass(Vec<&'static str>),
    NoCompatiblePooledSurface,
}

fn stitch_mask_summary(counts: &[usize; mesh_topology::STITCH_VARIANT_COUNT]) -> String {
    let mut parts = Vec::new();

    for (mask, count) in counts.iter().copied().enumerate() {
        if count > 0 {
            parts.push(format!("{mask}:{count}"));
        }
    }

    if parts.is_empty() {
        "none".to_string()
    } else {
        parts.join("|")
    }
}

fn record_mask(
    stitch_mask: u8,
    counts: &mut [usize; mesh_topology::STITCH_VARIANT_COUNT],
    stitched_detail: Option<(&mut usize, &mut [usize; Edge::ALL.len()])>,
) {
    let mask_index = usize::from(stitch_mask);
    if mask_index >= counts.len() {
        return;
    }

    counts[mask_index] += 1;

    let Some((stitched_chunks, stitched_edges)) = stitched_detail else {
        return;
    };
    if stitch_mask == mesh_topology::BASE_STITCH_MASK {
        return;
    }

    *stitched_chunks += 1;
    for (index, edge) in Edge::ALL.into_iter().enumerate() {
        if stitch_mask & mesh_topology::stitch_mask_bit(edge) != 0 {
            stitched_edges[index] += 1;
        }
    }
}
