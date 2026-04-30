use super::super::*;

#[derive(Clone, Debug)]
pub(crate) struct RenderPayloadRequest {
    pub sequence: usize,
    pub epoch: u64,
    pub key: ChunkKey,
    pub surface_class: SurfaceClassKey,
    pub requirements: PayloadBuildRequirements,
    pub chunk_origin_planet: DVec3,
    pub geometric_error: f32,
    pub config: RuntimeConfig,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WorkerScratchJobMetrics {
    pub sample_reuse: bool,
    pub mesh_reuse: bool,
    pub pack_reuse: bool,
    pub growth_events: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedRenderPayload {
    pub sequence: usize,
    pub epoch: u64,
    pub key: ChunkKey,
    pub surface_class: SurfaceClassKey,
    pub sample_count: usize,
    pub asset_candidate_count: usize,
    pub asset_rejected_count: usize,
    pub chunk_origin_planet: DVec3,
    pub gpu_custom_aabb: Option<CachedAabb>,
    pub render_tile: ChunkRenderTilePayload,
    pub mesh: CpuMeshBuffers,
    pub assets: Vec<AssetInstance>,
    pub packed_regions: Option<PackedMeshRegions>,
    pub requirements: PayloadBuildRequirements,
    pub scratch_metrics: WorkerScratchJobMetrics,
}

#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct PreparedPayloadBatch {
    pub results: Vec<PreparedRenderPayload>,
    pub queue_peak: usize,
    pub result_wait_count: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SubmittedPayloadBatch {
    pub submitted_jobs: usize,
    pub superseded_jobs: usize,
    pub queue_peak: usize,
}

#[derive(Debug, Default)]
pub(crate) struct ReadyPayloadBatch {
    pub results: Vec<PreparedRenderPayload>,
}

#[derive(Default)]
struct WorkerScratch {
    samples: Vec<ChunkSample>,
    heights: Vec<f32>,
    mesh: CpuMeshBuffers,
    vertex_region: Vec<u8>,
    attribute_region: Vec<u8>,
    index_region: Vec<u8>,
}

impl WorkerScratch {
    fn prepare_samples(&mut self, required: usize) -> (bool, usize) {
        let reused = self.samples.capacity() >= required;
        if !reused {
            self.samples
                .reserve(required.saturating_sub(self.samples.capacity()));
        }
        self.samples.clear();
        (reused, usize::from(!reused))
    }

    fn prepare_mesh(&mut self, vertex_count: usize, index_count: usize) -> (bool, usize) {
        let mut growth_events = 0usize;
        let mut all_reused = true;

        for buffer in [&mut self.mesh.positions, &mut self.mesh.normals] {
            let reused = buffer.capacity() >= vertex_count;
            if !reused {
                buffer.reserve(vertex_count.saturating_sub(buffer.capacity()));
                growth_events += 1;
                all_reused = false;
            }
            buffer.clear();
        }

        let tangents_reused = self.mesh.tangents.capacity() >= vertex_count;
        if !tangents_reused {
            self.mesh
                .tangents
                .reserve(vertex_count.saturating_sub(self.mesh.tangents.capacity()));
            growth_events += 1;
            all_reused = false;
        }
        self.mesh.tangents.clear();

        let uvs_reused = self.mesh.uvs.capacity() >= vertex_count;
        if !uvs_reused {
            self.mesh
                .uvs
                .reserve(vertex_count.saturating_sub(self.mesh.uvs.capacity()));
            growth_events += 1;
            all_reused = false;
        }
        self.mesh.uvs.clear();

        let colors_reused = self.mesh.colors.capacity() >= vertex_count;
        if !colors_reused {
            self.mesh
                .colors
                .reserve(vertex_count.saturating_sub(self.mesh.colors.capacity()));
            growth_events += 1;
            all_reused = false;
        }
        self.mesh.colors.clear();

        let indices_reused = self.mesh.indices.capacity() >= index_count;
        if !indices_reused {
            self.mesh
                .indices
                .reserve(index_count.saturating_sub(self.mesh.indices.capacity()));
            growth_events += 1;
            all_reused = false;
        }
        self.mesh.indices.clear();

        (all_reused, growth_events)
    }

    fn prepare_packed_regions(
        &mut self,
        vertex_bytes: usize,
        attribute_bytes: usize,
        index_bytes: usize,
    ) -> (bool, usize) {
        let mut growth_events = 0usize;
        let mut all_reused = true;

        for (buffer, required) in [
            (&mut self.vertex_region, vertex_bytes),
            (&mut self.attribute_region, attribute_bytes),
            (&mut self.index_region, index_bytes),
        ] {
            let reused = buffer.capacity() >= required;
            if !reused {
                buffer.reserve(required.saturating_sub(buffer.capacity()));
                growth_events += 1;
                all_reused = false;
            }
            buffer.clear();
            // `pack_mesh_regions_into()` overwrites every byte that participates in the
            // configured surface layout, so we avoid zero-filling here and only clear
            // explicit stride padding inside the packer when a class actually has padding.
            unsafe {
                buffer.set_len(required);
            }
        }

        (all_reused, growth_events)
    }
}

#[derive(Default)]
struct WorkerQueueState {
    jobs: VecDeque<RenderPayloadRequest>,
    shutdown: bool,
}

struct WorkerQueue {
    state: Mutex<WorkerQueueState>,
    wake: Condvar,
}

pub(crate) struct ThreadedPayloadGenerator {
    worker_count: usize,
    queue: Arc<WorkerQueue>,
    result_rx: mpsc::Receiver<PreparedRenderPayload>,
    workers: Vec<JoinHandle<()>>,
}

impl std::fmt::Debug for ThreadedPayloadGenerator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThreadedPayloadGenerator")
            .field("worker_count", &self.worker_count)
            .finish()
    }
}

impl ThreadedPayloadGenerator {
    pub(crate) fn new(worker_count: usize) -> Self {
        let worker_count = worker_count.max(1);
        let queue = Arc::new(WorkerQueue {
            state: Mutex::new(WorkerQueueState::default()),
            wake: Condvar::new(),
        });
        let (result_tx, result_rx) = mpsc::channel();
        let mut workers = Vec::with_capacity(worker_count);

        for worker_index in 0..worker_count {
            let worker_queue = Arc::clone(&queue);
            let worker_results = result_tx.clone();
            let worker_name = format!("planet-worker-{worker_index}");
            let worker = thread::Builder::new()
                .name(worker_name)
                .spawn(move || render_payload_worker_loop(worker_queue, worker_results))
                .expect("phase 9 worker threads must spawn");
            workers.push(worker);
        }
        drop(result_tx);

        Self {
            worker_count,
            queue,
            result_rx,
            workers,
        }
    }

    pub(crate) fn worker_count(&self) -> usize {
        self.worker_count
    }

    #[cfg(test)]
    pub(crate) fn generate(&self, jobs: Vec<RenderPayloadRequest>) -> PreparedPayloadBatch {
        if jobs.is_empty() {
            return PreparedPayloadBatch::default();
        }

        let submitted = self.submit(jobs);
        let expected_results = submitted.submitted_jobs;
        if expected_results == 0 {
            return PreparedPayloadBatch {
                results: Vec::new(),
                queue_peak: submitted.queue_peak,
                result_wait_count: 0,
            };
        }

        let mut results = Vec::with_capacity(expected_results);
        let mut result_wait_count = 0usize;
        while results.len() < expected_results {
            match self.result_rx.try_recv() {
                Ok(result) => results.push(result),
                Err(mpsc::TryRecvError::Empty) => {
                    result_wait_count += 1;
                    results.push(
                        self.result_rx
                            .recv()
                            .expect("phase 9 worker results channel must stay connected"),
                    );
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    panic!("phase 9 worker results channel disconnected unexpectedly");
                }
            }
        }
        results.sort_by_key(|result| result.sequence);

        PreparedPayloadBatch {
            results,
            queue_peak: submitted.queue_peak,
            result_wait_count,
        }
    }

    pub(crate) fn submit(&self, jobs: Vec<RenderPayloadRequest>) -> SubmittedPayloadBatch {
        if jobs.is_empty() {
            return SubmittedPayloadBatch::default();
        }

        let mut queue_peak = 0usize;
        let mut superseded_jobs = 0usize;
        let submitted_jobs = jobs.len();

        {
            let mut queue_state = self
                .queue
                .state
                .lock()
                .expect("phase 9 worker queue lock should not poison");
            for job in jobs {
                let before_len = queue_state.jobs.len();
                queue_state.jobs.retain(|queued| {
                    !(queued.epoch < job.epoch && keys_overlap_hierarchically(queued.key, job.key))
                });
                superseded_jobs += before_len.saturating_sub(queue_state.jobs.len());
                queue_state.jobs.push_back(job);
                queue_peak = queue_peak.max(queue_state.jobs.len());
            }
        }
        self.queue.wake.notify_all();

        SubmittedPayloadBatch {
            submitted_jobs,
            superseded_jobs,
            queue_peak,
        }
    }

    pub(crate) fn drain_ready(&self) -> ReadyPayloadBatch {
        let mut results = Vec::new();

        loop {
            match self.result_rx.try_recv() {
                Ok(result) => results.push(result),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    panic!("phase 9 worker results channel disconnected unexpectedly");
                }
            }
        }

        results.sort_by(|a, b| a.epoch.cmp(&b.epoch).then(a.sequence.cmp(&b.sequence)));
        ReadyPayloadBatch { results }
    }
}

impl Drop for ThreadedPayloadGenerator {
    fn drop(&mut self) {
        {
            let mut queue_state = self
                .queue
                .state
                .lock()
                .expect("phase 9 worker queue lock should not poison");
            queue_state.shutdown = true;
        }
        self.queue.wake.notify_all();

        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn render_payload_worker_loop(
    queue: Arc<WorkerQueue>,
    result_tx: mpsc::Sender<PreparedRenderPayload>,
) {
    let mut scratch = WorkerScratch::default();

    loop {
        let job = {
            let mut queue_state = queue
                .state
                .lock()
                .expect("phase 9 worker queue lock should not poison");
            while queue_state.jobs.is_empty() && !queue_state.shutdown {
                queue_state = queue
                    .wake
                    .wait(queue_state)
                    .expect("phase 9 worker queue wait should not poison");
            }
            if queue_state.shutdown && queue_state.jobs.is_empty() {
                return;
            }
            queue_state.jobs.pop_front()
        };

        let Some(job) = job else {
            continue;
        };

        let payload = build_render_payload_with_scratch(&job, &mut scratch);
        if result_tx.send(payload).is_err() {
            return;
        }
    }
}

fn build_render_payload_with_scratch(
    request: &RenderPayloadRequest,
    scratch: &mut WorkerScratch,
) -> PreparedRenderPayload {
    let samples_per_edge = mesh_topology::SAMPLED_VERTICES_PER_EDGE;
    let sample_count = (samples_per_edge * samples_per_edge) as usize;
    let (sample_reuse, mut growth_events) = scratch.prepare_samples(sample_count);

    sample_chunk_scalar_field_into(&request.config, request.key, samples_per_edge, scratch);
    fill_sample_slope_hints_for(
        &mut scratch.heights,
        &mut scratch.samples,
        samples_per_edge,
        request.config.height_amplitude,
    );

    let render_tile = ChunkRenderTilePayload::from_samples(samples_per_edge, &scratch.samples);
    let gpu_custom_aabb = cached_gpu_chunk_aabb_from_samples(
        request.config.planet_radius,
        request.chunk_origin_planet,
        request.geometric_error,
        samples_per_edge,
        &scratch.samples,
    );
    let mut mesh_reuse = false;
    let mut pack_reuse = false;
    let mut mesh = CpuMeshBuffers::default();
    let mut packed_regions = None;

    if request.requirements.requires_cpu_mesh() {
        let vertex_count = request.surface_class.vertex_count as usize;
        let index_count = request.surface_class.index_count as usize;
        let (prepared_mesh_reuse, mesh_growth) = scratch.prepare_mesh(vertex_count, index_count);
        mesh_reuse = prepared_mesh_reuse;
        growth_events += mesh_growth;
        derive_cpu_mesh_buffers_into(
            &request.config,
            &scratch.samples,
            samples_per_edge,
            request.surface_class.stitch_mask,
            request.chunk_origin_planet,
            &mut scratch.mesh,
        );

        if request.requirements.cpu_render_data {
            let (prepared_pack_reuse, pack_growth) = scratch.prepare_packed_regions(
                request.surface_class.vertex_bytes,
                request.surface_class.attribute_bytes,
                request.surface_class.index_bytes,
            );
            pack_reuse = prepared_pack_reuse;
            growth_events += pack_growth;
            pack_mesh_regions_into(
                &scratch.mesh,
                &request.surface_class,
                &mut scratch.vertex_region,
                &mut scratch.attribute_region,
                &mut scratch.index_region,
            );
            packed_regions = Some(PackedMeshRegions {
                vertex_region: scratch.vertex_region.clone(),
                attribute_region: scratch.attribute_region.clone(),
                index_region: scratch.index_region.clone(),
                vertex_stride: request.surface_class.vertex_stride,
                attribute_stride: request.surface_class.attribute_stride,
                index_stride: request.surface_class.index_stride,
            });
            mesh = scratch.mesh.clone();
        } else if request.requirements.collision_mesh {
            mesh.positions = scratch.mesh.positions.clone();
            mesh.indices = scratch.mesh.indices.clone();
        }
    }

    let placement = build_chunk_asset_placement(&request.config, request.key);

    PreparedRenderPayload {
        sequence: request.sequence,
        epoch: request.epoch,
        key: request.key,
        surface_class: request.surface_class.clone(),
        sample_count,
        asset_candidate_count: placement.candidate_count,
        asset_rejected_count: placement.rejected_count,
        chunk_origin_planet: request.chunk_origin_planet,
        gpu_custom_aabb,
        render_tile,
        mesh,
        assets: placement.assets,
        packed_regions,
        requirements: request.requirements,
        scratch_metrics: WorkerScratchJobMetrics {
            sample_reuse,
            mesh_reuse,
            pack_reuse,
            growth_events,
        },
    }
}

fn cached_gpu_chunk_aabb_from_samples(
    planet_radius: f64,
    chunk_origin_planet: DVec3,
    geometric_error: f32,
    samples_per_edge: u32,
    samples: &[ChunkSample],
) -> Option<CachedAabb> {
    if samples_per_edge != mesh_topology::SAMPLED_VERTICES_PER_EDGE {
        return None;
    }

    let visible_last = mesh_topology::QUADS_PER_EDGE;
    let border = mesh_topology::BORDER_RING_QUADS;
    let mut min = Vector3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let mut max = Vector3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);

    for y in 0..=visible_last {
        for x in 0..=visible_last {
            let sample_index = ((y + border) * samples_per_edge + (x + border)) as usize;
            let sample = samples.get(sample_index)?;
            let displaced_point = sample.displaced_point(planet_radius);
            let local = planet_to_chunk_local_f32(displaced_point, chunk_origin_planet);
            min.x = min.x.min(local[0]);
            min.y = min.y.min(local[1]);
            min.z = min.z.min(local[2]);
            max.x = max.x.max(local[0]);
            max.y = max.y.max(local[1]);
            max.z = max.z.max(local[2]);
        }
    }

    let padding = geometric_error.max(0.5);
    min -= Vector3::new(padding, padding, padding);
    max += Vector3::new(padding, padding, padding);
    Some(CachedAabb::from_min_max(min, max))
}

fn keys_overlap_hierarchically(a: ChunkKey, b: ChunkKey) -> bool {
    a.face == b.face && (a.is_descendant_of(&b) || b.is_descendant_of(&a))
}

fn sample_chunk_scalar_field_into(
    config: &RuntimeConfig,
    key: ChunkKey,
    samples_per_edge: u32,
    scratch: &mut WorkerScratch,
) {
    let visible_quads_recip = 1.0 / f64::from(mesh_topology::QUADS_PER_EDGE);
    let border = f64::from(mesh_topology::BORDER_RING_QUADS);
    let height_amplitude = config.height_amplitude;
    let terrain = config.terrain_settings();

    for y in 0..samples_per_edge {
        for x in 0..samples_per_edge {
            let chunk_uv = DVec2::new(
                (f64::from(x) - border) * visible_quads_recip,
                (f64::from(y) - border) * visible_quads_recip,
            );
            let face_uv =
                chunk_uv_to_face_uv(key, chunk_uv).expect("phase 9 worker keys must be valid");
            let cube_point = cube_point_for_face(key.face, face_uv_to_signed_coords(face_uv));
            let unit_dir = config
                .cube_projection
                .project(normalize_to_cube_surface(cube_point));
            let terrain_sample = terrain.sample(unit_dir);
            let height = terrain_sample
                .height
                .clamp(-height_amplitude, height_amplitude) as f32;

            scratch.samples.push(ChunkSample {
                unit_dir,
                height,
                height_norm: terrain_sample.height_norm,
                moisture: terrain_sample.moisture,
                land_mask: terrain_sample.land_mask,
                slope_hint: 0.0,
            });
        }
    }
}

fn fill_sample_slope_hints_for(
    height_scratch: &mut Vec<f32>,
    samples: &mut [ChunkSample],
    samples_per_edge: u32,
    height_amplitude: f64,
) {
    if height_scratch.capacity() < samples.len() {
        height_scratch.reserve(samples.len().saturating_sub(height_scratch.capacity()));
    }
    height_scratch.clear();
    height_scratch.extend(samples.iter().map(|sample| sample.height));

    for y in 0..samples_per_edge {
        for x in 0..samples_per_edge {
            let left = height_scratch[(clamp_grid_index(x as i32 - 1, samples_per_edge) as u32
                + y * samples_per_edge) as usize];
            let right = height_scratch[(clamp_grid_index(x as i32 + 1, samples_per_edge) as u32
                + y * samples_per_edge) as usize];
            let down = height_scratch[(x + clamp_grid_index(y as i32 - 1, samples_per_edge) as u32
                * samples_per_edge) as usize];
            let up = height_scratch[(x + clamp_grid_index(y as i32 + 1, samples_per_edge) as u32
                * samples_per_edge) as usize];
            let gradient = ((right - left).powi(2) + (up - down).powi(2)).sqrt();
            let slope_hint = if height_amplitude <= f64::from(f32::EPSILON) {
                0.0
            } else {
                (gradient / (height_amplitude as f32 * 2.0)).clamp(0.0, 1.0)
            };
            let index = (y * samples_per_edge + x) as usize;
            samples[index].slope_hint = slope_hint;
        }
    }
}

fn derive_cpu_mesh_buffers_into(
    config: &RuntimeConfig,
    samples: &[ChunkSample],
    samples_per_edge: u32,
    stitch_mask: u8,
    chunk_origin_planet: DVec3,
    mesh: &mut CpuMeshBuffers,
) {
    let topology = mesh_topology::canonical_chunk_topology();
    let visible_edge = mesh_topology::VISIBLE_VERTICES_PER_EDGE;
    let uv_scale = 1.0 / mesh_topology::QUADS_PER_EDGE as f32;
    mesh.indices.extend_from_slice(
        topology
            .stitch_indices(stitch_mask)
            .expect("phase 9 normalized stitch masks must stay valid"),
    );

    for y in 0..visible_edge {
        for x in 0..visible_edge {
            let sample_x = x + mesh_topology::BORDER_RING_QUADS;
            let sample_y = y + mesh_topology::BORDER_RING_QUADS;
            let sample = sample_grid_get(samples, samples_per_edge, sample_x, sample_y);
            let displaced = sample.displaced_point(config.planet_radius);
            let left = sample_grid_get(samples, samples_per_edge, sample_x - 1, sample_y)
                .displaced_point(config.planet_radius);
            let right = sample_grid_get(samples, samples_per_edge, sample_x + 1, sample_y)
                .displaced_point(config.planet_radius);
            let down = sample_grid_get(samples, samples_per_edge, sample_x, sample_y - 1)
                .displaced_point(config.planet_radius);
            let up = sample_grid_get(samples, samples_per_edge, sample_x, sample_y + 1)
                .displaced_point(config.planet_radius);
            let tangent_u = (right - left).normalize_or_zero();
            let tangent_v = (up - down).normalize_or_zero();
            let normal = tangent_u.cross(tangent_v).normalize_or_zero();

            mesh.positions
                .push(planet_to_chunk_local_f32(displaced, chunk_origin_planet));
            mesh.normals.push(dvec3_to_f32_array(normal));
            mesh.tangents.push([
                tangent_u.x as f32,
                tangent_u.y as f32,
                tangent_u.z as f32,
                1.0,
            ]);
            mesh.uvs.push([x as f32 * uv_scale, y as f32 * uv_scale]);
            mesh.colors.push([
                sample.height_norm,
                sample.slope_hint,
                sample.moisture,
                sample.land_mask,
            ]);
        }
    }
}

fn sample_grid_get(samples: &[ChunkSample], samples_per_edge: u32, x: u32, y: u32) -> &ChunkSample {
    &samples[(y * samples_per_edge + x) as usize]
}

fn pack_mesh_regions_into(
    mesh: &CpuMeshBuffers,
    surface_class: &SurfaceClassKey,
    vertex_region: &mut [u8],
    attribute_region: &mut [u8],
    index_region: &mut [u8],
) {
    if surface_class.vertex_stride > DEFAULT_RENDER_VERTEX_STRIDE {
        vertex_region.fill(0);
    }
    let attribute_bytes_written =
        if surface_class.attribute_stride >= PACKED_COLOR_OFFSET + PACKED_COLOR_BYTES {
            PACKED_COLOR_OFFSET + PACKED_COLOR_BYTES
        } else {
            PACKED_NORMAL_BYTES + PACKED_UV_BYTES
        };
    if surface_class.attribute_stride > attribute_bytes_written {
        attribute_region.fill(0);
    }
    if surface_class.index_stride > DEFAULT_RENDER_INDEX_STRIDE {
        index_region.fill(0);
    }

    for (index, position) in mesh.positions.iter().enumerate() {
        let offset = index * surface_class.vertex_stride;
        write_f32x3(
            &mut vertex_region[offset..offset + DEFAULT_RENDER_VERTEX_STRIDE],
            *position,
        );
    }

    for (index, ((normal, uv), color)) in mesh
        .normals
        .iter()
        .copied()
        .zip(mesh.uvs.iter().copied())
        .zip(mesh.colors.iter().copied())
        .enumerate()
    {
        let offset = index * surface_class.attribute_stride;
        write_f32x3(
            &mut attribute_region[offset..offset + PACKED_NORMAL_BYTES],
            normal,
        );
        write_f32x2(
            &mut attribute_region
                [offset + PACKED_NORMAL_BYTES..offset + PACKED_NORMAL_BYTES + PACKED_UV_BYTES],
            uv,
        );
        if surface_class.attribute_stride >= PACKED_COLOR_OFFSET + PACKED_COLOR_BYTES {
            write_rgba8(
                &mut attribute_region[offset + PACKED_COLOR_OFFSET
                    ..offset + PACKED_COLOR_OFFSET + PACKED_COLOR_BYTES],
                color,
            );
        }
    }

    for (index, triangle_index) in mesh.indices.iter().copied().enumerate() {
        let offset = index * surface_class.index_stride;
        index_region[offset..offset + DEFAULT_RENDER_INDEX_STRIDE]
            .copy_from_slice(&triangle_index.to_le_bytes());
    }
}
