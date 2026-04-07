mod assets;
mod core;
mod data;
mod legacy;
mod math;
mod pipeline;
mod strategy;
#[cfg(test)]
mod tests;
mod workers;

use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};
use std::f64::consts::PI;
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

use glam::{DVec2, DVec3};
use godot::builtin::{
    Aabb, Array, Basis, Color, Dictionary, PackedByteArray, PackedColorArray, PackedInt32Array,
    PackedVector2Array, PackedVector3Array, Plane, Rid, StringName, Transform3D, Variant, Vector2,
    Vector3,
};
use godot::classes::physics_server_3d::{BodyMode, BodyState};
use godot::classes::rendering_server::{MultimeshTransformFormat, PrimitiveType};
use godot::classes::{
    ArrayMesh, Image, ImageTexture, PhysicsServer3D, RenderingServer, Shader, ShaderMaterial,
};
use godot::meta::ToGodot;
use godot::obj::{Gd, NewGd, Singleton};
use godot::tools::load;

use crate::geometry::{
    chunk_uv_to_face_uv, cube_point_for_face, face_uv_to_signed_coords, CubeProjection,
    TerrainFieldSettings, TerrainSample,
};
use crate::mesh_topology::{self, StitchError, CANONICAL_TOPOLOGY_CLASS};
use crate::topology::{self, TopologyError};

use math::*;
use workers::{
    ChunkMetaBuildRequest, DesiredAssetGroupsBuildRequest, ThreadedAssetGroupGenerator,
    ThreadedMetadataGenerator,
};
use workers::{PreparedRenderPayload, RenderPayloadRequest, ThreadedPayloadGenerator};

pub use assets::*;
pub use data::*;
pub use pipeline::SelectionFrameState;
pub use strategy::*;

/// Treat Rust->Godot packed-array transfer as copy-possible unless the docs
/// explicitly guarantee a zero-copy ownership handoff for the exact API used.
pub const DEFAULT_METADATA_PRECOMPUTE_MAX_LOD: u8 = 5;
pub const DEFAULT_DENSE_METADATA_PREBUILD_MAX_LOD: u8 = 8;
pub const PAYLOAD_PRECOMPUTE_MAX_LOD: u8 = 5;
pub const DEFAULT_RENDER_LOD_REFERENCE_HEIGHT_PX: f32 = 1080.0;
pub const DEFAULT_TARGET_RENDER_CHUNKS: usize = 160;
pub const DEFAULT_HARD_RENDER_CHUNK_CAP: usize = 224;
pub const DEFAULT_SPLIT_THRESHOLD_PX: f32 = 8.0;
pub const DEFAULT_MERGE_THRESHOLD_PX: f32 = 4.0;
pub const DEFAULT_HORIZON_SAFETY_MARGIN: f64 = 16.0;
pub const DEFAULT_PHYSICS_ACTIVATION_RADIUS: f64 = 512.0;
pub const DEFAULT_PHYSICS_MAX_ACTIVE_CHUNKS: usize = 12;
pub const DEFAULT_ORIGIN_RECENTER_DISTANCE: f64 = 1_024.0;
pub const DEFAULT_COMMIT_BUDGET_PER_FRAME: usize = 24;
pub const DEFAULT_UPLOAD_BUDGET_BYTES_PER_FRAME: usize = 1024 * 1024;
pub const DEFAULT_RENDER_ACTIVATION_BUDGET_PER_FRAME: usize = 6;
pub const DEFAULT_RENDER_UPDATE_BUDGET_PER_FRAME: usize = 4;
pub const DEFAULT_RENDER_DEACTIVATION_BUDGET_PER_FRAME: usize = 8;
pub const DEFAULT_RENDER_SERVICE_STARVATION_LIMIT_FRAMES: u32 = 30;
pub const DEFAULT_PHYSICS_ACTIVATION_BUDGET_PER_FRAME: usize = 2;
pub const DEFAULT_PHYSICS_DEACTIVATION_BUDGET_PER_FRAME: usize = 4;
pub const DEFAULT_RENDER_MATERIAL_CLASS: u8 = 0;
pub const DEFAULT_RENDER_FORMAT_MASK: u64 = 0x1B;
pub const DEFAULT_RENDER_VERTEX_STRIDE: usize = 12;
pub const DEFAULT_RENDER_ATTRIBUTE_STRIDE: usize = 24;
pub const DEFAULT_RENDER_INDEX_STRIDE: usize = 4;
pub const DEFAULT_RENDER_POOL_WATERMARK_PER_CLASS: usize = 8;
pub const DEFAULT_PHYSICS_POOL_WATERMARK: usize = 4;
pub const DEFAULT_MAX_WORKER_THREADS: usize = 4;
pub const DEFAULT_MIN_AVERAGE_CHUNK_SURFACE_SPAN_METERS: f64 = 32.0;
pub const CURRENT_IMPLEMENTED_PHASE: u8 = 15;
pub const CURRENT_IMPLEMENTED_PHASE_LABEL: &str = "Phase 15 strategy-layer refinement";
pub const NEXT_PHASE_LABEL: &str = "none";
pub const PHASE4_TERRAIN_SHADER_PATH: &str = "res://shaders/terrain_gpu_chunk.gdshader";
const PACKED_NORMAL_BYTES: usize = 12;
const PACKED_UV_BYTES: usize = 8;
const PACKED_COLOR_BYTES: usize = 4;
const PACKED_COLOR_OFFSET: usize = PACKED_NORMAL_BYTES + PACKED_UV_BYTES;

const PHASE11_EMPHASIS_STEPS: [u8; 3] = [5, 8, 19];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildOrderStage {
    pub step: u8,
    pub slug: &'static str,
    pub description: &'static str,
}

impl BuildOrderStage {
    pub const fn new(step: u8, slug: &'static str, description: &'static str) -> Self {
        Self {
            step,
            slug,
            description,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhaseBuildHandoff {
    pub phase_label: &'static str,
    pub covered_step_range: Option<(u8, u8)>,
    pub emphasized_steps: &'static [u8],
    pub role: &'static str,
}

impl PhaseBuildHandoff {
    pub const fn new(
        phase_label: &'static str,
        covered_step_range: Option<(u8, u8)>,
        emphasized_steps: &'static [u8],
        role: &'static str,
    ) -> Self {
        Self {
            phase_label,
            covered_step_range,
            emphasized_steps,
            role,
        }
    }

    fn summary_token(&self) -> String {
        let mut token = String::from(self.phase_label);
        token.push('=');

        if let Some((start, end)) = self.covered_step_range {
            if start == end {
                token.push_str(&start.to_string());
            } else {
                token.push_str(&format!("{start}-{end}"));
            }
        } else {
            token.push_str("doc");
        }

        if !self.emphasized_steps.is_empty() {
            let emphasis = self
                .emphasized_steps
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join("/");
            token.push_str(&format!("+{emphasis}"));
        }

        token
    }
}

pub const BUILD_ORDER_STAGES: [BuildOrderStage; 23] = [
    BuildOrderStage::new(
        1,
        "face-basis-neighbors",
        "face basis + chunk key + neighbor mapping",
    ),
    BuildOrderStage::new(
        2,
        "projection",
        "default modified / spherified cube projection",
    ),
    BuildOrderStage::new(3, "face-samples", "cube-face sample coordinates"),
    BuildOrderStage::new(4, "terrain-field", "3D noise displacement on sphere"),
    BuildOrderStage::new(5, "ring-normals", "border ring + normal generation"),
    BuildOrderStage::new(6, "base-mesh", "base chunk mesh generation"),
    BuildOrderStage::new(
        7,
        "same-lod-validation",
        "same-LOD neighbor validation across face edges",
    ),
    BuildOrderStage::new(8, "stitch-indices", "stitch index buffers"),
    BuildOrderStage::new(
        9,
        "metadata-tree",
        "metadata tree + bounds + angular radius + surface class",
    ),
    BuildOrderStage::new(10, "horizon-culling", "horizon culling"),
    BuildOrderStage::new(11, "frustum-culling", "frustum culling"),
    BuildOrderStage::new(12, "lod-selection", "projected-error LOD selection"),
    BuildOrderStage::new(
        13,
        "active-set-separation",
        "render/physics active-set separation",
    ),
    BuildOrderStage::new(14, "cold-render", "cold server-side render commit path"),
    BuildOrderStage::new(15, "warm-render", "warm pooled render path"),
    BuildOrderStage::new(16, "packing-helpers", "Rust byte-region packing helpers"),
    BuildOrderStage::new(
        17,
        "staging-buffers",
        "reusable Godot packed staging buffers",
    ),
    BuildOrderStage::new(
        18,
        "in-place-staging",
        "in-place staging fills via resize() + as_mut_slice()",
    ),
    BuildOrderStage::new(
        19,
        "byte-region-updates",
        "byte-region vertex / attribute / index updates",
    ),
    BuildOrderStage::new(20, "physics-commit", "server-side physics commit path"),
    BuildOrderStage::new(21, "asset-multimesh", "chunk-group asset multimesh path"),
    BuildOrderStage::new(22, "worker-scratch", "worker scratch reuse"),
    BuildOrderStage::new(
        23,
        "budgeting-polish",
        "commit budgeting / upload budgeting / pool watermarks / hysteresis / caching polish",
    ),
];

pub const BUILD_ORDER_HANDOFFS: [PhaseBuildHandoff; 5] = [
    PhaseBuildHandoff::new(
        "phases01-10",
        Some((1, 20)),
        &[],
        "correctness-to-backend implementation",
    ),
    PhaseBuildHandoff::new(
        "phase11",
        None,
        &PHASE11_EMPHASIS_STEPS,
        "seam hardening over already-shipped mesh/update steps",
    ),
    PhaseBuildHandoff::new("phase12", Some((21, 21)), &[], "asset residency ownership"),
    PhaseBuildHandoff::new(
        "phase09",
        Some((22, 22)),
        &[],
        "threaded worker scratch reuse policy",
    ),
    PhaseBuildHandoff::new(
        "phase13",
        Some((23, 23)),
        &[],
        "default numbers and bounded-churn controls",
    ),
];

pub fn average_chunk_surface_span_meters(planet_radius: f64, lod: u8) -> f64 {
    if !planet_radius.is_finite() || planet_radius <= 0.0 {
        return 0.0;
    }

    let chunk_count = 6.0 * 4.0_f64.powi(i32::from(lod));
    ((4.0 * PI * planet_radius * planet_radius) / chunk_count).sqrt()
}

pub fn radius_derived_max_lod_for_planet_radius(
    planet_radius: f64,
    min_average_chunk_surface_span_meters: f64,
    max_lod_cap: u8,
) -> u8 {
    if !planet_radius.is_finite()
        || planet_radius <= 0.0
        || !min_average_chunk_surface_span_meters.is_finite()
        || min_average_chunk_surface_span_meters <= 0.0
    {
        return 0;
    }

    let mut derived_max_lod = 0;
    let capped_max_lod = max_lod_cap.min(topology::MAX_SUPPORTED_MAX_LOD);
    for lod in 0..=capped_max_lod {
        let average_span = average_chunk_surface_span_meters(planet_radius, lod);
        if average_span < min_average_chunk_surface_span_meters {
            break;
        }
        derived_max_lod = lod;
    }

    derived_max_lod
}

#[derive(Debug)]
pub struct PlanetRuntime {
    pub config: RuntimeConfig,
    pub scenario_rid: Rid,
    pub physics_space_rid: Rid,
    pub meta: MetadataStore,
    pub active_render: HashSet<ChunkKey>,
    pub active_physics: HashSet<ChunkKey>,
    pub render_residency: HashMap<ChunkKey, RenderResidencyEntry>,
    pub render_tile_pool: RenderTilePoolState,
    pub resident_payloads: HashMap<ChunkKey, ChunkPayload>,
    pub rid_state: HashMap<ChunkKey, ChunkRidState>,
    pub render_pool: HashMap<SurfaceClassKey, VecDeque<RenderPoolEntry>>,
    pub canonical_render_meshes: HashMap<SurfaceClassKey, CanonicalRenderMeshEntry>,
    pub gpu_material_pool: HashMap<SurfaceClassKey, VecDeque<GpuMaterialPoolEntry>>,
    pub gpu_active_materials: HashMap<ChunkKey, GpuMaterialPoolEntry>,
    pub gpu_render_instance_pool: VecDeque<Rid>,
    pub terrain_shader: Option<Gd<Shader>>,
    pub physics_pool: VecDeque<PhysicsPoolEntry>,
    pub asset_groups: HashMap<AssetGroupKey, AssetGroupState>,
    pub asset_family_meshes: HashMap<u16, Rid>,
    threaded_payload_generator: ThreadedPayloadGenerator,
    threaded_metadata_generator: ThreadedMetadataGenerator,
    threaded_asset_group_generator: ThreadedAssetGroupGenerator,
    pending_meta_requests: HashMap<ChunkKey, u64>,
    pending_payload_requests: HashMap<ChunkKey, PendingPayloadRequest>,
    pending_asset_group_epoch: Option<u64>,
    asset_groups_dirty: bool,
    next_meta_request_epoch: u64,
    next_payload_request_epoch: u64,
    next_asset_group_epoch: u64,
    pub origin_snapshot: OriginSnapshot,
    pub frame_state: SelectionFrameState,
    pub deferred_starvation: HashMap<DeferredOpKey, u32>,
    origin_shift_pending_rebind: bool,
    pending_origin_rebases: usize,
    pending_render_transform_rebinds: usize,
    pending_physics_transform_rebinds: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingPayloadRequest {
    epoch: u64,
    surface_class: SurfaceClassKey,
    requirements: PayloadBuildRequirements,
}

impl PlanetRuntime {
    pub fn build_order_stages() -> &'static [BuildOrderStage] {
        &BUILD_ORDER_STAGES
    }

    pub fn build_order_handoffs() -> &'static [PhaseBuildHandoff] {
        &BUILD_ORDER_HANDOFFS
    }

    pub fn build_order_stage_count() -> usize {
        BUILD_ORDER_STAGES.len()
    }

    pub fn build_order_is_contiguous() -> bool {
        BUILD_ORDER_STAGES
            .iter()
            .enumerate()
            .all(|(index, stage)| stage.step == (index + 1) as u8)
    }

    pub fn build_order_summary(&self) -> String {
        let handoff = Self::build_order_handoffs()
            .iter()
            .map(PhaseBuildHandoff::summary_token)
            .collect::<Vec<_>>()
            .join(",");

        format!(
            "phase={} steps=1-{} handoff={} next={}",
            CURRENT_IMPLEMENTED_PHASE,
            Self::build_order_stage_count(),
            handoff,
            NEXT_PHASE_LABEL
        )
    }

    pub fn strategy_summary(&self) -> String {
        format!(
            "projection={} visibility={} frustum_culling={} coarse_lod_fallback={} render_backend={} staging={}",
            ProjectionStrategy::label(&self.config.cube_projection),
            ChunkVisibilityStrategy::label(&self.config.visibility_strategy),
            self.config.enable_frustum_culling,
            self.config.keep_coarse_lod_chunks_rendered,
            ChunkRenderBackend::label(&self.config.render_backend),
            PackedStagingPolicy::label(&self.config.staging_policy),
        )
    }
}
