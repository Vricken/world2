use super::super::*;

#[derive(Clone, Debug)]
pub(crate) struct RenderPayloadRequest {
    pub sequence: usize,
    pub epoch: u64,
    pub key: ChunkKey,
    pub surface_class: SurfaceClassKey,
    pub chunk_origin_planet: DVec3,
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
    pub mesh: CpuMeshBuffers,
    pub assets: Vec<AssetInstance>,
    pub collider_vertices: Vec<[f32; 3]>,
    pub collider_indices: Vec<i32>,
    pub collider_faces: Vec<[f32; 3]>,
    pub packed_regions: PackedMeshRegions,
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
            buffer.resize(required, 0);
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
                    !(queued.epoch < job.epoch
                        && keys_overlap_hierarchically(queued.key, job.key))
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

    let vertex_count = request.surface_class.vertex_count as usize;
    let index_count = request.surface_class.index_count as usize;
    let (mesh_reuse, mesh_growth) = scratch.prepare_mesh(vertex_count, index_count);
    growth_events += mesh_growth;
    derive_cpu_mesh_buffers_into(
        &request.config,
        &scratch.samples,
        samples_per_edge,
        request.surface_class.stitch_mask,
        request.chunk_origin_planet,
        &mut scratch.mesh,
    );

    let (pack_reuse, pack_growth) = scratch.prepare_packed_regions(
        request.surface_class.vertex_bytes,
        request.surface_class.attribute_bytes,
        request.surface_class.index_bytes,
    );
    growth_events += pack_growth;
    pack_mesh_regions_into(
        &scratch.mesh,
        &request.surface_class,
        &mut scratch.vertex_region,
        &mut scratch.attribute_region,
        &mut scratch.index_region,
    );
    let placement = build_chunk_asset_placement(&request.config, request.key);
    let collider_vertices = scratch.mesh.positions.clone();
    let collider_indices = scratch.mesh.indices.clone();
    let collider_faces = collider_face_vertices_from_indices(&collider_vertices, &collider_indices);

    PreparedRenderPayload {
        sequence: request.sequence,
        epoch: request.epoch,
        key: request.key,
        surface_class: request.surface_class.clone(),
        sample_count,
        asset_candidate_count: placement.candidate_count,
        asset_rejected_count: placement.rejected_count,
        chunk_origin_planet: request.chunk_origin_planet,
        mesh: scratch.mesh.clone(),
        assets: placement.assets,
        collider_vertices,
        collider_indices,
        collider_faces,
        packed_regions: PackedMeshRegions {
            vertex_region: scratch.vertex_region.clone(),
            attribute_region: scratch.attribute_region.clone(),
            index_region: scratch.index_region.clone(),
            vertex_stride: request.surface_class.vertex_stride,
            attribute_stride: request.surface_class.attribute_stride,
            index_stride: request.surface_class.index_stride,
        },
        scratch_metrics: WorkerScratchJobMetrics {
            sample_reuse,
            mesh_reuse,
            pack_reuse,
            growth_events,
        },
    }
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
    let visible_quads = f64::from(mesh_topology::QUADS_PER_EDGE);
    let border = f64::from(mesh_topology::BORDER_RING_QUADS);
    let terrain = TerrainFieldSettings {
        planet_radius: config.planet_radius,
        height_amplitude: config.height_amplitude,
        ..TerrainFieldSettings::default()
    };

    for y in 0..samples_per_edge {
        for x in 0..samples_per_edge {
            let chunk_uv = DVec2::new(
                (f64::from(x) - border) / visible_quads,
                (f64::from(y) - border) / visible_quads,
            );
            let face_uv =
                chunk_uv_to_face_uv(key, chunk_uv).expect("phase 9 worker keys must be valid");
            let cube_point = cube_point_for_face(key.face, face_uv_to_signed_coords(face_uv));
            let unit_dir = config
                .cube_projection
                .project(normalize_to_cube_surface(cube_point));
            let height = terrain
                .sample_height(unit_dir)
                .clamp(-config.height_amplitude, config.height_amplitude)
                as f32;
            let temperature = (1.0 - unit_dir.y.abs()) as f32;
            let moisture_signal =
                (unit_dir.dot(DVec3::new(1.731, -0.613, 0.947)).sin() * 0.5 + 0.5) as f32;
            let biome0 = moisture_signal.clamp(0.0, 1.0);
            let biome1 = ((temperature * 0.75)
                + ((height / config.height_amplitude as f32) * 0.25 + 0.25))
                .clamp(0.0, 1.0);

            scratch.samples.push(ChunkSample {
                unit_dir,
                height,
                biome0,
                biome1,
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
            mesh.uvs.push([
                x as f32 / mesh_topology::QUADS_PER_EDGE as f32,
                y as f32 / mesh_topology::QUADS_PER_EDGE as f32,
            ]);
            mesh.colors
                .push([sample.biome0, sample.biome1, sample.slope_hint, 1.0]);
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
    for (index, position) in mesh.positions.iter().enumerate() {
        let offset = index * surface_class.vertex_stride;
        write_f32x3(
            &mut vertex_region[offset..offset + DEFAULT_RENDER_VERTEX_STRIDE],
            *position,
        );
    }

    for index in 0..mesh.vertex_count() {
        let offset = index * surface_class.attribute_stride;
        write_f32x3(
            &mut attribute_region[offset..offset + PACKED_NORMAL_BYTES],
            mesh.normals[index],
        );
        write_f32x2(
            &mut attribute_region
                [offset + PACKED_NORMAL_BYTES..offset + PACKED_NORMAL_BYTES + PACKED_UV_BYTES],
            mesh.uvs[index],
        );
        if surface_class.attribute_stride >= PACKED_COLOR_OFFSET + PACKED_COLOR_BYTES {
            write_rgba8(
                &mut attribute_region[offset + PACKED_COLOR_OFFSET
                    ..offset + PACKED_COLOR_OFFSET + PACKED_COLOR_BYTES],
                mesh.colors[index],
            );
        }
    }

    for (index, triangle_index) in mesh.indices.iter().copied().enumerate() {
        let offset = index * surface_class.index_stride;
        index_region[offset..offset + DEFAULT_RENDER_INDEX_STRIDE]
            .copy_from_slice(&triangle_index.to_le_bytes());
    }
}
