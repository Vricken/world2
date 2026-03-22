use super::super::*;

use std::collections::VecDeque;
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

#[derive(Clone, Debug)]
pub(crate) struct ChunkMetaBuildRequest {
    pub epoch: u64,
    pub key: ChunkKey,
    pub config: RuntimeConfig,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedChunkMeta {
    pub epoch: u64,
    pub key: ChunkKey,
    pub meta: ChunkMeta,
}

#[derive(Default)]
struct MetadataQueueState {
    jobs: VecDeque<ChunkMetaBuildRequest>,
    shutdown: bool,
}

struct MetadataQueue {
    state: Mutex<MetadataQueueState>,
    wake: Condvar,
}

pub(crate) struct ThreadedMetadataGenerator {
    queue: Arc<MetadataQueue>,
    result_rx: mpsc::Receiver<PreparedChunkMeta>,
    workers: Vec<JoinHandle<()>>,
}

impl std::fmt::Debug for ThreadedMetadataGenerator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThreadedMetadataGenerator")
            .field("worker_count", &self.workers.len())
            .finish()
    }
}

impl ThreadedMetadataGenerator {
    pub(crate) fn new(worker_count: usize) -> Self {
        let worker_count = worker_count.max(1);
        let queue = Arc::new(MetadataQueue {
            state: Mutex::new(MetadataQueueState::default()),
            wake: Condvar::new(),
        });
        let (result_tx, result_rx) = mpsc::channel();
        let mut workers = Vec::with_capacity(worker_count);

        for worker_index in 0..worker_count {
            let worker_queue = Arc::clone(&queue);
            let worker_results = result_tx.clone();
            let worker_name = format!("planet-meta-worker-{worker_index}");
            let worker = thread::Builder::new()
                .name(worker_name)
                .spawn(move || metadata_worker_loop(worker_queue, worker_results))
                .expect("metadata worker threads must spawn");
            workers.push(worker);
        }
        drop(result_tx);

        Self {
            queue,
            result_rx,
            workers,
        }
    }

    pub(crate) fn submit(&self, job: ChunkMetaBuildRequest) {
        let mut queue_state = self
            .queue
            .state
            .lock()
            .expect("metadata worker queue lock should not poison");
        queue_state.jobs.retain(|queued| queued.key != job.key);
        queue_state.jobs.push_back(job);
        drop(queue_state);
        self.queue.wake.notify_one();
    }

    pub(crate) fn drain_ready(&self) -> Vec<PreparedChunkMeta> {
        let mut results = Vec::new();

        loop {
            match self.result_rx.try_recv() {
                Ok(result) => results.push(result),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    panic!("metadata worker results channel disconnected unexpectedly");
                }
            }
        }

        results
    }
}

impl Drop for ThreadedMetadataGenerator {
    fn drop(&mut self) {
        {
            let mut queue_state = self
                .queue
                .state
                .lock()
                .expect("metadata worker queue lock should not poison");
            queue_state.shutdown = true;
        }
        self.queue.wake.notify_all();

        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn metadata_worker_loop(queue: Arc<MetadataQueue>, result_tx: mpsc::Sender<PreparedChunkMeta>) {
    loop {
        let job = {
            let mut queue_state = queue
                .state
                .lock()
                .expect("metadata worker queue lock should not poison");
            while queue_state.jobs.is_empty() && !queue_state.shutdown {
                queue_state = queue
                    .wake
                    .wait(queue_state)
                    .expect("metadata worker queue wait should not poison");
            }
            if queue_state.shutdown && queue_state.jobs.is_empty() {
                return;
            }
            queue_state.jobs.pop_front()
        };

        let Some(job) = job else {
            continue;
        };

        let prepared = PreparedChunkMeta {
            epoch: job.epoch,
            key: job.key,
            meta: build_chunk_meta_for_config(&job.config, job.key)
                .expect("metadata worker requests must use valid keys"),
        };
        if result_tx.send(prepared).is_err() {
            return;
        }
    }
}

fn build_chunk_meta_for_config(
    config: &RuntimeConfig,
    key: ChunkKey,
) -> Result<ChunkMeta, TopologyError> {
    if !key.is_valid_for_lod() {
        return Err(TopologyError::InvalidChunkKey);
    }

    let sample_dirs = chunk_sample_directions_for_config(config, key, 5)?;
    let center_dir = sample_dirs
        .iter()
        .copied()
        .fold(DVec3::ZERO, |sum, dir| sum + dir)
        .normalize_or_zero();
    let terrain = TerrainFieldSettings {
        planet_radius: config.planet_radius,
        height_amplitude: config.height_amplitude,
        ..TerrainFieldSettings::default()
    };
    let center_planet = center_dir * config.planet_radius;

    let angular_radius = sample_dirs
        .iter()
        .copied()
        .map(|dir| center_dir.angle_between(dir))
        .fold(0.0_f64, f64::max);
    let mut min_height = config.height_amplitude as f32;
    let mut max_height = -(config.height_amplitude as f32);

    for dir in &sample_dirs {
        let height = terrain
            .sample_height(*dir)
            .clamp(-config.height_amplitude, config.height_amplitude) as f32;
        min_height = min_height.min(height);
        max_height = max_height.max(height);
    }

    let raw_max_radius = config.planet_radius + f64::from(max_height);
    let geometric_error =
        (2.0 * raw_max_radius * angular_radius / f64::from(mesh_topology::QUADS_PER_EDGE)) as f32;
    let height_padding = geometric_error.min(config.height_amplitude as f32);
    min_height = (min_height - height_padding).max(-(config.height_amplitude as f32));
    max_height = (max_height + height_padding).min(config.height_amplitude as f32);
    let min_radius = (config.planet_radius + f64::from(min_height)).max(1.0);
    let max_radius = config.planet_radius + f64::from(max_height);
    let radius = sample_dirs
        .iter()
        .copied()
        .flat_map(|dir| {
            [
                (dir * min_radius - center_planet).length(),
                (dir * max_radius - center_planet).length(),
            ]
        })
        .fold(0.0_f64, f64::max);

    let surface_class = SurfaceClassKey::canonical_chunk(
        mesh_topology::BASE_STITCH_MASK,
        config.render_material_class,
        config.render_format_mask,
        config.render_vertex_stride,
        config.render_attribute_stride,
        config.render_index_stride,
    )
    .expect("base stitch mask must always yield a surface class");

    ChunkMeta::new(
        key,
        ChunkBounds::new(
            center_planet,
            radius,
            min_height,
            max_height,
            min_radius,
            max_radius,
        ),
        ChunkMetrics::new(geometric_error, 0.0, angular_radius as f32),
        surface_class,
    )
}

fn chunk_sample_directions_for_config(
    config: &RuntimeConfig,
    key: ChunkKey,
    samples_per_edge: u32,
) -> Result<Vec<DVec3>, TopologyError> {
    let mut sample_dirs = Vec::with_capacity((samples_per_edge * samples_per_edge) as usize);
    let last = samples_per_edge.saturating_sub(1).max(1);

    for y in 0..samples_per_edge {
        for x in 0..samples_per_edge {
            let u = f64::from(x) / f64::from(last);
            let v = f64::from(y) / f64::from(last);
            let face_uv = chunk_uv_to_face_uv(key, glam::DVec2::new(u, v))
                .map_err(|_| TopologyError::InvalidChunkKey)?;
            let face_st = face_uv_to_signed_coords(face_uv);
            let cube_point = cube_point_for_face(key.face, face_st);
            sample_dirs.push(config.cube_projection.project(cube_point));
        }
    }

    Ok(sample_dirs)
}
