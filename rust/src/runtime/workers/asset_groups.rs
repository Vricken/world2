use super::super::*;

use std::collections::{HashMap, VecDeque};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

#[derive(Clone, Debug)]
pub(crate) struct AssetGroupChunkInput {
    pub key: ChunkKey,
    pub chunk_origin_planet: DVec3,
    pub assets: Vec<AssetInstance>,
}

#[derive(Clone, Debug)]
pub(crate) struct DesiredAssetGroupsBuildRequest {
    pub epoch: u64,
    pub config: RuntimeConfig,
    pub chunks: Vec<AssetGroupChunkInput>,
    pub anchor_origins: HashMap<ChunkKey, DVec3>,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedAssetGroups {
    pub epoch: u64,
    pub groups: HashMap<AssetGroupKey, DesiredAssetGroup>,
}

#[derive(Default)]
struct AssetGroupQueueState {
    jobs: VecDeque<DesiredAssetGroupsBuildRequest>,
    shutdown: bool,
}

struct AssetGroupQueue {
    state: Mutex<AssetGroupQueueState>,
    wake: Condvar,
}

pub(crate) struct ThreadedAssetGroupGenerator {
    queue: Arc<AssetGroupQueue>,
    result_rx: mpsc::Receiver<PreparedAssetGroups>,
    workers: Vec<JoinHandle<()>>,
}

impl std::fmt::Debug for ThreadedAssetGroupGenerator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThreadedAssetGroupGenerator")
            .field("worker_count", &self.workers.len())
            .finish()
    }
}

impl ThreadedAssetGroupGenerator {
    pub(crate) fn new(worker_count: usize) -> Self {
        let worker_count = worker_count.max(1);
        let queue = Arc::new(AssetGroupQueue {
            state: Mutex::new(AssetGroupQueueState::default()),
            wake: Condvar::new(),
        });
        let (result_tx, result_rx) = mpsc::channel();
        let mut workers = Vec::with_capacity(worker_count);

        for worker_index in 0..worker_count {
            let worker_queue = Arc::clone(&queue);
            let worker_results = result_tx.clone();
            let worker_name = format!("planet-asset-group-worker-{worker_index}");
            let worker = thread::Builder::new()
                .name(worker_name)
                .spawn(move || asset_group_worker_loop(worker_queue, worker_results))
                .expect("asset-group worker threads must spawn");
            workers.push(worker);
        }
        drop(result_tx);

        Self {
            queue,
            result_rx,
            workers,
        }
    }

    pub(crate) fn submit_latest(&self, job: DesiredAssetGroupsBuildRequest) {
        let mut queue_state = self
            .queue
            .state
            .lock()
            .expect("asset-group worker queue lock should not poison");
        queue_state.jobs.clear();
        queue_state.jobs.push_back(job);
        drop(queue_state);
        self.queue.wake.notify_one();
    }

    pub(crate) fn drain_ready(&self) -> Vec<PreparedAssetGroups> {
        let mut results = Vec::new();

        loop {
            match self.result_rx.try_recv() {
                Ok(result) => results.push(result),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    panic!("asset-group worker results channel disconnected unexpectedly");
                }
            }
        }

        results.sort_by_key(|result| result.epoch);
        results
    }
}

impl Drop for ThreadedAssetGroupGenerator {
    fn drop(&mut self) {
        {
            let mut queue_state = self
                .queue
                .state
                .lock()
                .expect("asset-group worker queue lock should not poison");
            queue_state.shutdown = true;
        }
        self.queue.wake.notify_all();

        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn asset_group_worker_loop(
    queue: Arc<AssetGroupQueue>,
    result_tx: mpsc::Sender<PreparedAssetGroups>,
) {
    loop {
        let job = {
            let mut queue_state = queue
                .state
                .lock()
                .expect("asset-group worker queue lock should not poison");
            while queue_state.jobs.is_empty() && !queue_state.shutdown {
                queue_state = queue
                    .wake
                    .wait(queue_state)
                    .expect("asset-group worker queue wait should not poison");
            }
            if queue_state.shutdown && queue_state.jobs.is_empty() {
                return;
            }
            queue_state.jobs.pop_front()
        };

        let Some(job) = job else {
            continue;
        };

        let groups = build_desired_asset_groups_for_snapshot(&job);
        if result_tx
            .send(PreparedAssetGroups {
                epoch: job.epoch,
                groups,
            })
            .is_err()
        {
            return;
        }
    }
}

fn build_desired_asset_groups_for_snapshot(
    request: &DesiredAssetGroupsBuildRequest,
) -> HashMap<AssetGroupKey, DesiredAssetGroup> {
    let mut groups = HashMap::new();

    for chunk in &request.chunks {
        for asset in &chunk.assets {
            let group_key = asset_group_key_for_chunk(
                chunk.key,
                asset.family_id,
                request.config.asset_group_chunk_span,
            );
            let entry = groups.entry(group_key).or_insert_with(|| {
                let anchor_key =
                    asset_group_anchor_key(group_key, request.config.asset_group_chunk_span);
                let group_origin_planet = request
                    .anchor_origins
                    .get(&anchor_key)
                    .copied()
                    .unwrap_or(chunk.chunk_origin_planet);
                DesiredAssetGroup {
                    key: group_key,
                    group_origin_planet,
                    source_chunks: Vec::new(),
                    assets: Vec::new(),
                    local_bounds: None,
                }
            });
            if entry.source_chunks.last().copied() != Some(chunk.key) {
                entry.source_chunks.push(chunk.key);
            }
            entry.assets.push(asset.clone());
        }
    }

    for group in groups.values_mut() {
        group.source_chunks.sort_unstable();
        group.local_bounds = asset_group_local_bounds(group.group_origin_planet, &group.assets);
    }

    groups
}
