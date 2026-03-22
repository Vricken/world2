mod core;
mod data;
mod math;
mod pipeline;
#[cfg(test)]
mod tests;
mod workers;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

use glam::{DVec2, DVec3};
use godot::builtin::{
    Array, Color, Dictionary, PackedByteArray, PackedColorArray, PackedInt32Array,
    PackedVector2Array, PackedVector3Array, Plane, Rid, StringName, Transform3D, Variant, Vector2,
    Vector3,
};
use godot::classes::physics_server_3d::{BodyMode, BodyState};
use godot::classes::rendering_server::PrimitiveType;
use godot::classes::{PhysicsServer3D, RenderingServer};
use godot::meta::ToGodot;
use godot::obj::Singleton;

use crate::geometry::{
    chunk_uv_to_face_uv, cube_point_for_face, face_uv_to_signed_coords, CubeProjection,
    TerrainFieldSettings,
};
use crate::mesh_topology::{self, StitchError, CANONICAL_TOPOLOGY_CLASS};
use crate::topology::{self, TopologyError};

use math::*;
use workers::{PreparedRenderPayload, RenderPayloadRequest, ThreadedPayloadGenerator};

pub use data::*;
pub use pipeline::SelectionFrameState;

/// Treat Rust->Godot packed-array transfer as copy-possible unless the docs
/// explicitly guarantee a zero-copy ownership handoff for the exact API used.
pub const DEFAULT_METADATA_PRECOMPUTE_MAX_LOD: u8 = 5;
pub const PAYLOAD_PRECOMPUTE_MAX_LOD: u8 = 5;
pub const DEFAULT_SPLIT_THRESHOLD_PX: f32 = 8.0;
pub const DEFAULT_MERGE_THRESHOLD_PX: f32 = 4.0;
pub const DEFAULT_PHYSICS_ACTIVATION_RADIUS: f64 = 3_000.0;
pub const DEFAULT_ORIGIN_RECENTER_DISTANCE: f64 = 1_024.0;
pub const DEFAULT_COMMIT_BUDGET_PER_FRAME: usize = 24;
pub const DEFAULT_UPLOAD_BUDGET_BYTES_PER_FRAME: usize = 8 * 1024 * 1024;
pub const DEFAULT_RENDER_MATERIAL_CLASS: u8 = 0;
pub const DEFAULT_RENDER_FORMAT_MASK: u64 = 0x1B;
pub const DEFAULT_RENDER_VERTEX_STRIDE: usize = 12;
pub const DEFAULT_RENDER_ATTRIBUTE_STRIDE: usize = 24;
pub const DEFAULT_RENDER_INDEX_STRIDE: usize = 4;
pub const DEFAULT_RENDER_POOL_WATERMARK_PER_CLASS: usize = 8;
pub const DEFAULT_PHYSICS_POOL_WATERMARK: usize = 32;
pub const DEFAULT_MAX_WORKER_THREADS: usize = 4;
const PACKED_NORMAL_BYTES: usize = 12;
const PACKED_UV_BYTES: usize = 8;
const PACKED_COLOR_BYTES: usize = 4;
const PACKED_COLOR_OFFSET: usize = PACKED_NORMAL_BYTES + PACKED_UV_BYTES;

#[derive(Debug)]
pub struct PlanetRuntime {
    pub config: RuntimeConfig,
    pub scenario_rid: Rid,
    pub physics_space_rid: Rid,
    pub meta: HashMap<ChunkKey, ChunkMeta>,
    pub active_render: HashSet<ChunkKey>,
    pub active_physics: HashSet<ChunkKey>,
    pub resident_payloads: HashMap<ChunkKey, ChunkPayload>,
    pub rid_state: HashMap<ChunkKey, ChunkRidState>,
    pub render_pool: HashMap<SurfaceClassKey, VecDeque<RenderPoolEntry>>,
    pub physics_pool: VecDeque<PhysicsPoolEntry>,
    threaded_payload_generator: ThreadedPayloadGenerator,
    pub origin_snapshot: OriginSnapshot,
    pub frame_state: SelectionFrameState,
    pub deferred_starvation: HashMap<DeferredOpKey, u32>,
    origin_shift_pending_rebind: bool,
}
