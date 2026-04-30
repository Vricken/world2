use super::super::*;

const METADATA_SAMPLE_GRID_EDGE: u32 = 5;
const MAX_NEIGHBOR_NORMALIZATION_PASSES_PER_FRAME: usize = 64;
const MAX_NEIGHBOR_NORMALIZATION_WORK_ITEMS_PER_FRAME: usize = 16_384;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectionFrameState {
    pub tick: u64,
    pub desired_render_count: usize,
    pub desired_physics_count: usize,
    pub gpu_tile_upload_bytes: usize,
    pub gpu_material_binds: usize,
    pub active_gpu_render_chunks: usize,
    pub canonical_render_meshes: usize,
    pub render_residency_entries: usize,
    pub render_residency_evictions: usize,
    pub render_tile_bytes: usize,
    pub render_tile_pool_slots: usize,
    pub render_tile_pool_active_slots: usize,
    pub render_tile_pool_free_slots: usize,
    pub render_tile_eviction_ready_slots: usize,
    pub collision_residency_entries: usize,
    pub collision_residency_bytes: usize,
    pub selected_render_starved_chunks: usize,
    pub selected_render_starvation_failures: usize,
    pub max_selected_render_starvation_frames: u32,
    pub horizon_survivor_count: usize,
    pub frustum_survivor_count: usize,
    pub selected_candidates: usize,
    pub refinement_iterations: usize,
    pub selection_cap_hits: usize,
    pub selected_leaf_count: usize,
    pub neighbor_split_count: usize,
    pub queued_commit_ops: usize,
    pub committed_commit_ops: usize,
    pub deferred_commit_ops: usize,
    pub upload_bytes_committed: usize,
    pub upload_bytes_deferred: usize,
    pub max_deferred_starvation_frames: u32,
    pub phase7_sampled_chunks: usize,
    pub phase7_meshed_chunks: usize,
    pub phase7_packed_chunks: usize,
    pub phase7_staged_chunks: usize,
    pub phase7_commit_payloads: usize,
    pub phase7_lifecycle_commands: usize,
    pub phase7_warm_current_reuse_hits: usize,
    pub phase7_warm_pool_reuse_hits: usize,
    pub phase7_cold_fallbacks: usize,
    pub phase8_render_warm_current_commits: usize,
    pub phase8_render_warm_pool_commits: usize,
    pub phase8_render_cold_commits: usize,
    pub phase8_physics_commits: usize,
    pub phase8_fallback_missing_current_surface_class: usize,
    pub phase8_fallback_incompatible_current_surface_class: usize,
    pub phase8_fallback_no_compatible_pooled_surface: usize,
    pub phase9_worker_threads: usize,
    pub phase9_meta_requests_submitted: usize,
    pub phase9_meta_results_installed: usize,
    pub phase9_submitted_jobs: usize,
    pub phase9_generation_jobs: usize,
    pub phase9_ready_results: usize,
    pub phase9_stale_results_dropped: usize,
    pub phase9_superseded_jobs: usize,
    pub phase9_inflight_jobs: usize,
    pub phase9_queue_peak: usize,
    pub phase9_result_wait_count: usize,
    pub phase9_sample_scratch_reuse_hits: usize,
    pub phase9_mesh_scratch_reuse_hits: usize,
    pub phase9_pack_scratch_reuse_hits: usize,
    pub phase9_scratch_growth_events: usize,
    pub phase10_origin_rebases: usize,
    pub phase10_render_transform_rebinds: usize,
    pub phase10_physics_transform_rebinds: usize,
    pub phase12_chunks_with_asset_payloads: usize,
    pub phase12_asset_candidate_count: usize,
    pub phase12_asset_rejected_count: usize,
    pub phase12_asset_accepted_count: usize,
    pub phase12_active_groups: usize,
    pub phase12_active_instances: usize,
    pub sparse_meta_entries: usize,
    pub render_pool_entries: usize,
    pub physics_pool_entries: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RefinementCandidate {
    key: ChunkKey,
    split_benefit_px: f32,
    error_px: f32,
}

impl Eq for RefinementCandidate {}

impl Ord for RefinementCandidate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.split_benefit_px
            .total_cmp(&other.split_benefit_px)
            .then_with(|| self.error_px.total_cmp(&other.error_px))
            .then_with(|| self.key.lod.cmp(&other.key.lod))
            .then_with(|| other.key.cmp(&self.key))
    }
}

impl PartialOrd for RefinementCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PlanetRuntime {
    pub fn flush_pending_origin_rebinds(&mut self) {
        if !self.origin_shift_pending_rebind {
            return;
        }

        let mut frame_state = SelectionFrameState {
            phase10_origin_rebases: 1,
            ..SelectionFrameState::default()
        };
        self.rebind_active_relative_transforms(&mut frame_state);
        self.origin_shift_pending_rebind = false;
        self.frame_state.phase10_origin_rebases = self
            .frame_state
            .phase10_origin_rebases
            .saturating_add(frame_state.phase10_origin_rebases);
        self.pending_origin_rebases = self
            .pending_origin_rebases
            .saturating_add(frame_state.phase10_origin_rebases);
        self.frame_state.phase10_render_transform_rebinds = self
            .frame_state
            .phase10_render_transform_rebinds
            .saturating_add(frame_state.phase10_render_transform_rebinds);
        self.pending_render_transform_rebinds = self
            .pending_render_transform_rebinds
            .saturating_add(frame_state.phase10_render_transform_rebinds);
        self.frame_state.phase10_physics_transform_rebinds = self
            .frame_state
            .phase10_physics_transform_rebinds
            .saturating_add(frame_state.phase10_physics_transform_rebinds);
        self.pending_physics_transform_rebinds = self
            .pending_physics_transform_rebinds
            .saturating_add(frame_state.phase10_physics_transform_rebinds);
    }

    pub fn step_visibility_selection(&mut self, camera: &CameraState) -> Result<(), TopologyError> {
        let mut frame_state = SelectionFrameState {
            tick: self.frame_state.tick.saturating_add(1),
            phase9_worker_threads: self.threaded_payload_generator.worker_count(),
            phase10_origin_rebases: self.pending_origin_rebases,
            phase10_render_transform_rebinds: self.pending_render_transform_rebinds,
            phase10_physics_transform_rebinds: self.pending_physics_transform_rebinds,
            ..SelectionFrameState::default()
        };
        self.pending_origin_rebases = 0;
        self.pending_render_transform_rebinds = 0;
        self.pending_physics_transform_rebinds = 0;
        frame_state.phase9_meta_results_installed = self.drain_ready_chunk_meta();
        if self.origin_shift_pending_rebind {
            frame_state.phase10_origin_rebases += 1;
            self.rebind_active_relative_transforms(&mut frame_state);
            self.origin_shift_pending_rebind = false;
        }
        let desired_render = self.select_render_set(camera, &mut frame_state)?;
        let desired_physics = self.select_physics_set(camera, &desired_render)?;

        frame_state.desired_render_count = desired_render.len();
        frame_state.desired_physics_count = desired_physics.len();

        self.apply_budgeted_diffs(&desired_render, &desired_physics, camera, &mut frame_state)?;
        frame_state.sparse_meta_entries = self.meta.sparse_count();
        self.frame_state = frame_state;

        Ok(())
    }

    pub(crate) fn drain_ready_chunk_meta(&mut self) -> usize {
        let mut installed = 0usize;
        for prepared in self.threaded_metadata_generator.drain_ready() {
            let Some(expected_epoch) = self.pending_meta_requests.get(&prepared.key).copied()
            else {
                continue;
            };
            if expected_epoch != prepared.epoch {
                continue;
            }
            self.pending_meta_requests.remove(&prepared.key);
            let _ = self.meta.insert_chunk_meta(prepared.meta, true);
            installed += 1;
        }
        installed
    }

    pub(crate) fn request_chunk_meta_if_missing(
        &mut self,
        key: ChunkKey,
        frame_state: &mut SelectionFrameState,
    ) {
        if self.meta.contains_key(&key) || self.pending_meta_requests.contains_key(&key) {
            return;
        }

        let epoch = self.next_meta_request_epoch;
        self.next_meta_request_epoch = self.next_meta_request_epoch.saturating_add(1);
        self.pending_meta_requests.insert(key, epoch);
        self.threaded_metadata_generator
            .submit(ChunkMetaBuildRequest {
                epoch,
                key,
                config: self.config.clone(),
            });
        frame_state.phase9_meta_requests_submitted += 1;
    }

    pub(crate) fn build_chunk_meta(&self, key: ChunkKey) -> Result<ChunkMeta, TopologyError> {
        let sample_dirs = self.chunk_sample_directions(key, METADATA_SAMPLE_GRID_EDGE)?;
        let center_dir = sample_dirs
            .iter()
            .copied()
            .fold(DVec3::ZERO, |sum, dir| sum + dir)
            .normalize_or_zero();
        let terrain = self.config.terrain_settings();
        let center_planet = center_dir * self.config.planet_radius;

        let angular_radius = sample_dirs
            .iter()
            .copied()
            .map(|dir| center_dir.angle_between(dir))
            .fold(0.0_f64, f64::max);
        let mut min_height = self.config.height_amplitude as f32;
        let mut max_height = -(self.config.height_amplitude as f32);

        for dir in &sample_dirs {
            let height = terrain
                .sample_height(*dir)
                .clamp(-self.config.height_amplitude, self.config.height_amplitude)
                as f32;
            min_height = min_height.min(height);
            max_height = max_height.max(height);
        }
        let raw_max_radius = self.config.planet_radius + f64::from(max_height);
        let geometric_error = (2.0 * raw_max_radius * angular_radius
            / f64::from(mesh_topology::QUADS_PER_EDGE)) as f32;
        let height_padding = geometric_error.min(self.config.height_amplitude as f32);
        min_height = (min_height - height_padding).max(-(self.config.height_amplitude as f32));
        max_height = (max_height + height_padding).min(self.config.height_amplitude as f32);
        let min_radius = (self.config.planet_radius + f64::from(min_height)).max(1.0);
        let max_radius = self.config.planet_radius + f64::from(max_height);
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
            self.config.render_material_class,
            self.config.render_format_mask,
            self.config.render_vertex_stride,
            self.config.render_attribute_stride,
            self.config.render_index_stride,
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

    fn chunk_sample_directions(
        &self,
        key: ChunkKey,
        samples_per_edge: u32,
    ) -> Result<Vec<DVec3>, TopologyError> {
        if !key.is_valid_for_lod() {
            return Err(TopologyError::InvalidChunkKey);
        }

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
                sample_dirs.push(self.config.cube_projection.project(cube_point));
            }
        }

        Ok(sample_dirs)
    }

    #[cfg(test)]
    pub(crate) fn sample_chunk_scalar_field(
        &self,
        key: ChunkKey,
    ) -> Result<ChunkSampleGrid, TopologyError> {
        if !key.is_valid_for_lod() {
            return Err(TopologyError::InvalidChunkKey);
        }

        let samples_per_edge = mesh_topology::SAMPLED_VERTICES_PER_EDGE;
        let visible_quads = f64::from(mesh_topology::QUADS_PER_EDGE);
        let border = f64::from(mesh_topology::BORDER_RING_QUADS);
        let terrain = self.config.terrain_settings();
        let mut samples = Vec::with_capacity((samples_per_edge * samples_per_edge) as usize);

        for y in 0..samples_per_edge {
            for x in 0..samples_per_edge {
                let chunk_uv = DVec2::new(
                    (f64::from(x) - border) / visible_quads,
                    (f64::from(y) - border) / visible_quads,
                );
                let face_uv = chunk_uv_to_face_uv(key, chunk_uv)
                    .map_err(|_| TopologyError::InvalidChunkKey)?;
                let cube_point = cube_point_for_face(key.face, face_uv_to_signed_coords(face_uv));
                let unit_dir = self
                    .config
                    .cube_projection
                    .project(normalize_to_cube_surface(cube_point));
                let terrain_sample = terrain.sample(unit_dir);
                let height = terrain_sample
                    .height
                    .clamp(-self.config.height_amplitude, self.config.height_amplitude)
                    as f32;

                samples.push(ChunkSample {
                    unit_dir,
                    height,
                    height_norm: terrain_sample.height_norm,
                    moisture: terrain_sample.moisture,
                    land_mask: terrain_sample.land_mask,
                    slope_hint: 0.0,
                });
            }
        }

        let mut grid = ChunkSampleGrid::new(samples_per_edge, samples);
        self.fill_sample_slope_hints(&mut grid);

        Ok(grid)
    }

    #[cfg(test)]
    fn fill_sample_slope_hints(&self, grid: &mut ChunkSampleGrid) {
        let samples_per_edge = grid.samples_per_edge;
        let heights = grid
            .samples
            .iter()
            .map(|sample| sample.height)
            .collect::<Vec<_>>();

        for y in 0..samples_per_edge {
            for x in 0..samples_per_edge {
                let left = heights[(clamp_grid_index(x as i32 - 1, samples_per_edge) as u32
                    + y * samples_per_edge) as usize];
                let right = heights[(clamp_grid_index(x as i32 + 1, samples_per_edge) as u32
                    + y * samples_per_edge) as usize];
                let down = heights[(x + clamp_grid_index(y as i32 - 1, samples_per_edge) as u32
                    * samples_per_edge) as usize];
                let up = heights[(x + clamp_grid_index(y as i32 + 1, samples_per_edge) as u32
                    * samples_per_edge) as usize];
                let gradient = ((right - left).powi(2) + (up - down).powi(2)).sqrt();
                let slope_hint = if self.config.height_amplitude <= f64::from(f32::EPSILON) {
                    0.0
                } else {
                    (gradient / (self.config.height_amplitude as f32 * 2.0)).clamp(0.0, 1.0)
                };
                let index = (y * samples_per_edge + x) as usize;
                grid.samples[index].slope_hint = slope_hint;
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn derive_cpu_mesh_buffers(
        &self,
        samples: &ChunkSampleGrid,
        stitch_mask: u8,
        chunk_origin_planet: DVec3,
    ) -> Result<CpuMeshBuffers, StitchError> {
        let topology = mesh_topology::canonical_chunk_topology();
        let visible_edge = mesh_topology::VISIBLE_VERTICES_PER_EDGE;
        let mut mesh = CpuMeshBuffers {
            positions: Vec::with_capacity(topology.vertex_count() as usize),
            normals: Vec::with_capacity(topology.vertex_count() as usize),
            tangents: Vec::with_capacity(topology.vertex_count() as usize),
            uvs: Vec::with_capacity(topology.vertex_count() as usize),
            colors: Vec::with_capacity(topology.vertex_count() as usize),
            indices: topology.stitch_indices(stitch_mask)?.to_vec(),
        };

        for y in 0..visible_edge {
            for x in 0..visible_edge {
                let sample_x = x + mesh_topology::BORDER_RING_QUADS;
                let sample_y = y + mesh_topology::BORDER_RING_QUADS;
                let sample = samples.get(sample_x, sample_y);
                let displaced = sample.displaced_point(self.config.planet_radius);
                let left = samples
                    .get(sample_x - 1, sample_y)
                    .displaced_point(self.config.planet_radius);
                let right = samples
                    .get(sample_x + 1, sample_y)
                    .displaced_point(self.config.planet_radius);
                let down = samples
                    .get(sample_x, sample_y - 1)
                    .displaced_point(self.config.planet_radius);
                let up = samples
                    .get(sample_x, sample_y + 1)
                    .displaced_point(self.config.planet_radius);
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
                mesh.colors.push([
                    sample.height_norm,
                    sample.slope_hint,
                    sample.moisture,
                    sample.land_mask,
                ]);
            }
        }

        Ok(mesh)
    }

    #[cfg(test)]
    fn pack_mesh_regions(
        &self,
        mesh: &CpuMeshBuffers,
        surface_class: &SurfaceClassKey,
    ) -> Result<PackedMeshRegions, &'static str> {
        if mesh.vertex_count() != surface_class.vertex_count as usize {
            return Err("vertex_count");
        }
        if mesh.index_count() != surface_class.index_count as usize {
            return Err("index_count");
        }
        if surface_class.vertex_stride < DEFAULT_RENDER_VERTEX_STRIDE {
            return Err("vertex_stride");
        }
        if surface_class.attribute_stride < PACKED_NORMAL_BYTES + PACKED_UV_BYTES {
            return Err("attribute_stride");
        }
        if surface_class.index_stride < DEFAULT_RENDER_INDEX_STRIDE {
            return Err("index_stride");
        }

        let mut packed = PackedMeshRegions {
            vertex_region: vec![0; surface_class.vertex_bytes],
            attribute_region: vec![0; surface_class.attribute_bytes],
            index_region: vec![0; surface_class.index_bytes],
            vertex_stride: surface_class.vertex_stride,
            attribute_stride: surface_class.attribute_stride,
            index_stride: surface_class.index_stride,
        };

        for (index, position) in mesh.positions.iter().enumerate() {
            let offset = index * surface_class.vertex_stride;
            write_f32x3(
                &mut packed.vertex_region[offset..offset + DEFAULT_RENDER_VERTEX_STRIDE],
                *position,
            );
        }

        for index in 0..mesh.vertex_count() {
            let offset = index * surface_class.attribute_stride;
            write_f32x3(
                &mut packed.attribute_region[offset..offset + PACKED_NORMAL_BYTES],
                mesh.normals[index],
            );
            write_f32x2(
                &mut packed.attribute_region
                    [offset + PACKED_NORMAL_BYTES..offset + PACKED_NORMAL_BYTES + PACKED_UV_BYTES],
                mesh.uvs[index],
            );
            if surface_class.attribute_stride >= PACKED_COLOR_OFFSET + PACKED_COLOR_BYTES {
                write_rgba8(
                    &mut packed.attribute_region[offset + PACKED_COLOR_OFFSET
                        ..offset + PACKED_COLOR_OFFSET + PACKED_COLOR_BYTES],
                    mesh.colors[index],
                );
            }
        }

        for (index, triangle_index) in mesh.indices.iter().copied().enumerate() {
            let offset = index * surface_class.index_stride;
            packed.index_region[offset..offset + DEFAULT_RENDER_INDEX_STRIDE]
                .copy_from_slice(&triangle_index.to_le_bytes());
        }

        Ok(packed)
    }

    pub(crate) fn select_render_set(
        &mut self,
        camera: &CameraState,
        frame_state: &mut SelectionFrameState,
    ) -> Result<HashSet<ChunkKey>, TopologyError> {
        let mut selected = HashSet::with_capacity(self.active_render.len().max(Face::ALL.len()));
        let mut refinement_queue = BinaryHeap::new();
        let split_ancestors = self.build_current_split_ancestors();

        for face in Face::ALL {
            let key = ChunkKey::new(face, 0, 0, 0);
            if self.select_visible_chunk(
                key,
                camera,
                &mut selected,
                &mut refinement_queue,
                &split_ancestors,
                frame_state,
            )? {
                continue;
            }
            if self.config.keep_coarse_lod_chunks_rendered {
                selected.insert(key);
            }
        }

        let refinement_soft_cap = self
            .config
            .target_render_chunks
            .min(self.config.hard_render_chunk_cap);
        while let Some(candidate) = refinement_queue.pop() {
            frame_state.refinement_iterations += 1;
            if !selected.contains(&candidate.key) {
                continue;
            }

            let children = candidate
                .key
                .children()
                .expect("queued refinement candidates must not be leaves");
            let mut child_entries = Vec::with_capacity(children.len());
            let mut child_count = 0usize;

            for child in children {
                if self.select_visible_chunk(
                    child,
                    camera,
                    &mut HashSet::new(),
                    &mut refinement_queue,
                    &split_ancestors,
                    frame_state,
                )? {
                    child_entries.push((child, true));
                    child_count += 1;
                } else if self.config.keep_coarse_lod_chunks_rendered {
                    child_entries.push((child, false));
                    child_count += 1;
                }
            }

            if child_entries.is_empty() {
                continue;
            }

            let projected_count = selected.len().saturating_sub(1).saturating_add(child_count);
            if projected_count > refinement_soft_cap
                || projected_count > self.config.hard_render_chunk_cap
            {
                frame_state.selection_cap_hits += 1;
                continue;
            }

            selected.remove(&candidate.key);
            for (child, was_visible) in child_entries {
                selected.insert(child);
                if !was_visible {
                    continue;
                }
            }
        }

        frame_state.neighbor_split_count =
            self.normalize_neighbor_lod_delta(&mut selected, frame_state)?;
        frame_state.selected_leaf_count = selected.len();

        Ok(selected)
    }

    fn select_visible_chunk(
        &mut self,
        key: ChunkKey,
        camera: &CameraState,
        selected: &mut HashSet<ChunkKey>,
        refinement_queue: &mut BinaryHeap<RefinementCandidate>,
        split_ancestors: &HashSet<ChunkKey>,
        frame_state: &mut SelectionFrameState,
    ) -> Result<bool, TopologyError> {
        let meta = self.ensure_chunk_meta(key)?;

        if !self.horizon_visible(camera, &meta) {
            return Ok(false);
        }
        frame_state.horizon_survivor_count += 1;

        if !self.frustum_visible(camera, &meta) {
            return Ok(false);
        }
        frame_state.frustum_survivor_count += 1;

        let error_px = self.projected_error_px(camera, &meta);
        selected.insert(key);
        self.queue_refinement_candidate(
            key,
            error_px,
            refinement_queue,
            split_ancestors,
            frame_state,
        )?;

        Ok(true)
    }

    fn build_current_split_ancestors(&self) -> HashSet<ChunkKey> {
        let mut split_ancestors = HashSet::with_capacity(
            self.active_render
                .len()
                .saturating_mul(usize::from(self.config.max_lod)),
        );

        for key in &self.active_render {
            let mut ancestor = key.parent();
            while let Some(parent) = ancestor {
                split_ancestors.insert(parent);
                ancestor = parent.parent();
            }
        }

        split_ancestors
    }

    fn should_split_chunk(
        &self,
        key: ChunkKey,
        error_px: f32,
        split_ancestors: &HashSet<ChunkKey>,
    ) -> bool {
        let is_currently_split = split_ancestors.contains(&key);

        if is_currently_split {
            error_px >= self.config.merge_threshold_px
        } else {
            error_px > self.config.split_threshold_px
        }
    }

    fn queue_refinement_candidate(
        &mut self,
        key: ChunkKey,
        error_px: f32,
        refinement_queue: &mut BinaryHeap<RefinementCandidate>,
        split_ancestors: &HashSet<ChunkKey>,
        frame_state: &mut SelectionFrameState,
    ) -> Result<(), TopologyError> {
        let Some(candidate) =
            self.build_refinement_candidate(key, error_px, split_ancestors, frame_state)?
        else {
            return Ok(());
        };

        refinement_queue.push(candidate);
        frame_state.selected_candidates += 1;
        Ok(())
    }

    fn build_refinement_candidate(
        &mut self,
        key: ChunkKey,
        error_px: f32,
        split_ancestors: &HashSet<ChunkKey>,
        frame_state: &mut SelectionFrameState,
    ) -> Result<Option<RefinementCandidate>, TopologyError> {
        if key.lod >= self.config.max_lod
            || !self.should_split_chunk(key, error_px, split_ancestors)
        {
            return Ok(None);
        }

        let children = key
            .children()
            .expect("child keys must exist while below configured max lod");
        let mut all_children_ready = true;
        for child in children {
            if self.meta.contains_key(&child) {
                continue;
            }
            self.request_chunk_meta_if_missing(child, frame_state);
            all_children_ready = false;
        }
        if !all_children_ready {
            return Ok(None);
        }

        let split_threshold_px = if split_ancestors.contains(&key) {
            self.config.merge_threshold_px
        } else {
            self.config.split_threshold_px
        };
        let split_benefit_px = error_px - split_threshold_px;

        Ok((split_benefit_px > 0.0).then_some(RefinementCandidate {
            key,
            split_benefit_px,
            error_px,
        }))
    }

    pub(crate) fn normalize_neighbor_lod_delta(
        &mut self,
        selected: &mut HashSet<ChunkKey>,
        frame_state: &mut SelectionFrameState,
    ) -> Result<usize, TopologyError> {
        self.normalize_neighbor_lod_delta_with_limits(
            selected,
            frame_state,
            MAX_NEIGHBOR_NORMALIZATION_PASSES_PER_FRAME,
            MAX_NEIGHBOR_NORMALIZATION_WORK_ITEMS_PER_FRAME,
            self.config.hard_render_chunk_cap,
        )
    }

    pub(crate) fn normalize_neighbor_lod_delta_with_limits(
        &mut self,
        selected: &mut HashSet<ChunkKey>,
        frame_state: &mut SelectionFrameState,
        max_passes: usize,
        max_work_items: usize,
        hard_chunk_cap: usize,
    ) -> Result<usize, TopologyError> {
        let mut splits_applied = 0usize;
        let mut seen_states = HashSet::new();
        let mut work_items = 0usize;

        for _ in 0..max_passes {
            let state = Self::normalized_selection_state(selected);
            if !seen_states.insert(state) {
                self.force_collapse_neighbor_lod_delta(selected)?;
                return Ok(splits_applied);
            }

            let mut split_targets = HashSet::new();
            let mut collapse_targets = HashSet::new();

            for key in selected.iter().copied().collect::<Vec<_>>() {
                let neighbors = self
                    .meta
                    .neighbors(&key)
                    .unwrap_or(topology::same_lod_neighbors(key)?);
                for neighbor_same_lod in neighbors.same_lod {
                    work_items = work_items.saturating_add(1);
                    if work_items > max_work_items {
                        self.force_collapse_neighbor_lod_delta(selected)?;
                        return Ok(splits_applied);
                    }
                    if let Some(active_ancestor) =
                        Self::find_active_ancestor_covering(neighbor_same_lod, selected)
                    {
                        if key.lod > active_ancestor.lod + 1 {
                            let coarse_children = active_ancestor
                                .children()
                                .expect("normalization only splits non-leaf chunks");
                            let mut coarse_children_ready = true;
                            for child in coarse_children {
                                if !self.meta.contains_key(&child) {
                                    self.request_chunk_meta_if_missing(child, frame_state);
                                    coarse_children_ready = false;
                                }
                            }

                            if coarse_children_ready {
                                split_targets.insert(active_ancestor);
                            } else if let Some(collapse_target) =
                                key.ancestor_at_lod(active_ancestor.lod + 1)
                            {
                                collapse_targets.insert(collapse_target);
                            }
                        }
                    }
                }
            }

            if split_targets.is_empty() && collapse_targets.is_empty() {
                break;
            }

            let mut progressed_this_pass = false;
            let mut hard_cap_blocked = false;
            for coarse_key in split_targets {
                if selected.len().saturating_add(3) > hard_chunk_cap {
                    hard_cap_blocked = true;
                    continue;
                }
                if !selected.remove(&coarse_key) {
                    continue;
                }

                let mut all_children_ready = true;
                for child in coarse_key
                    .children()
                    .expect("normalization only splits non-leaf chunks")
                {
                    if self.meta.contains_key(&child) {
                        selected.insert(child);
                    } else {
                        self.request_chunk_meta_if_missing(child, frame_state);
                        all_children_ready = false;
                    }
                }

                if !all_children_ready {
                    selected.insert(coarse_key);
                } else {
                    splits_applied += 1;
                    progressed_this_pass = true;
                }
            }

            for collapse_target in collapse_targets {
                let descendants = selected
                    .iter()
                    .copied()
                    .filter(|candidate| candidate.is_descendant_of(&collapse_target))
                    .collect::<Vec<_>>();
                if descendants.is_empty() {
                    continue;
                }

                for descendant in descendants {
                    selected.remove(&descendant);
                }
                progressed_this_pass |= selected.insert(collapse_target);
            }

            if !progressed_this_pass {
                if hard_cap_blocked {
                    self.force_collapse_neighbor_lod_delta(selected)?;
                }
                return Ok(splits_applied);
            }
        }

        self.force_collapse_neighbor_lod_delta(selected)?;
        Ok(splits_applied)
    }

    pub(crate) fn find_active_ancestor_covering(
        mut key: ChunkKey,
        active_set: &HashSet<ChunkKey>,
    ) -> Option<ChunkKey> {
        loop {
            if active_set.contains(&key) {
                return Some(key);
            }

            key = key.parent()?;
        }
    }

    fn collect_neighbor_collapse_targets(
        &self,
        selected: &HashSet<ChunkKey>,
    ) -> Result<HashSet<ChunkKey>, TopologyError> {
        let mut collapse_targets = HashSet::new();

        for key in selected.iter().copied().collect::<Vec<_>>() {
            let neighbors = self
                .meta
                .neighbors(&key)
                .unwrap_or(topology::same_lod_neighbors(key)?);
            for neighbor_same_lod in neighbors.same_lod {
                if let Some(active_ancestor) =
                    Self::find_active_ancestor_covering(neighbor_same_lod, selected)
                {
                    if key.lod > active_ancestor.lod + 1 {
                        if let Some(collapse_target) = key.ancestor_at_lod(active_ancestor.lod + 1)
                        {
                            collapse_targets.insert(collapse_target);
                        }
                    }
                }
            }
        }

        Ok(collapse_targets)
    }

    fn force_collapse_neighbor_lod_delta(
        &self,
        selected: &mut HashSet<ChunkKey>,
    ) -> Result<(), TopologyError> {
        loop {
            let collapse_targets = self.collect_neighbor_collapse_targets(selected)?;
            if collapse_targets.is_empty() {
                return Ok(());
            }

            let mut progressed_this_pass = false;
            for collapse_target in collapse_targets {
                progressed_this_pass |=
                    Self::collapse_selected_descendants(selected, collapse_target);
            }

            if !progressed_this_pass {
                return Ok(());
            }
        }
    }

    fn collapse_selected_descendants(
        selected: &mut HashSet<ChunkKey>,
        collapse_target: ChunkKey,
    ) -> bool {
        let descendants = selected
            .iter()
            .copied()
            .filter(|candidate| candidate.is_descendant_of(&collapse_target))
            .collect::<Vec<_>>();
        if descendants.is_empty() {
            return false;
        }

        for descendant in descendants {
            selected.remove(&descendant);
        }
        selected.insert(collapse_target)
    }

    fn normalized_selection_state(selected: &HashSet<ChunkKey>) -> Vec<ChunkKey> {
        let mut state = selected.iter().copied().collect::<Vec<_>>();
        state.sort_unstable();
        state
    }

    pub(crate) fn select_physics_set(
        &mut self,
        camera: &CameraState,
        desired_render: &HashSet<ChunkKey>,
    ) -> Result<HashSet<ChunkKey>, TopologyError> {
        let render_keys = desired_render.iter().copied().collect::<Vec<_>>();
        let mut candidates = Vec::with_capacity(render_keys.len());

        for key in render_keys {
            let meta = self.ensure_chunk_meta(key)?;
            let distance = self.chunk_camera_distance(camera, &meta);
            candidates.push((key, distance));
        }
        candidates.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));

        let mut physics = HashSet::new();
        for (key, distance) in candidates {
            if distance <= self.config.physics_activation_radius
                && physics.len() < self.config.physics_max_active_chunks
            {
                physics.insert(key);
            }
        }

        Ok(physics)
    }

    pub(crate) fn required_surface_class_for_selection(
        &mut self,
        key: ChunkKey,
        desired_render: &HashSet<ChunkKey>,
    ) -> Result<SurfaceClassKey, TopologyError> {
        let neighbors = self
            .meta
            .neighbors(&key)
            .unwrap_or(topology::same_lod_neighbors(key)?);
        let mut neighbor_lods = [key.lod; Edge::ALL.len()];

        for (index, edge) in Edge::ALL.into_iter().enumerate() {
            let neighbor_same_lod = neighbors.get(edge);
            if let Some(covering_key) =
                Self::find_active_ancestor_covering(neighbor_same_lod, desired_render)
            {
                neighbor_lods[index] = covering_key.lod;
            }
        }

        let stitch_mask = mesh_topology::stitch_mask_from_neighbor_lods(key.lod, neighbor_lods)
            .expect("selection normalization must keep neighbor lod deltas within one");

        SurfaceClassKey::canonical_chunk(
            stitch_mask,
            self.config.render_material_class,
            self.config.render_format_mask,
            self.config.render_vertex_stride,
            self.config.render_attribute_stride,
            self.config.render_index_stride,
        )
        .map_err(|_| TopologyError::InvalidChunkKey)
    }

    #[cfg(test)]
    pub(crate) fn ensure_render_payload_for_selection(
        &mut self,
        key: ChunkKey,
        desired_render: &HashSet<ChunkKey>,
        frame_state: &mut SelectionFrameState,
    ) -> Result<usize, TopologyError> {
        let desired_physics = HashSet::new();
        let Some(request) =
            self.prepare_render_payload_request(0, 0, key, desired_render, &desired_physics)?
        else {
            return Ok(self.payload_upload_bytes(key));
        };

        let samples = self.sample_chunk_scalar_field(key)?;
        let mesh = self
            .derive_cpu_mesh_buffers(
                &samples,
                request.surface_class.stitch_mask,
                request.chunk_origin_planet,
            )
            .expect("normalized stitch masks must map to canonical topology");
        let packed_regions = self
            .pack_mesh_regions(&mesh, &request.surface_class)
            .expect("phase 7 packer must match configured surface strides");
        let placement = build_chunk_asset_placement(&self.config, key);
        let prepared = PreparedRenderPayload {
            sequence: request.sequence,
            epoch: request.epoch,
            key,
            surface_class: request.surface_class,
            sample_count: samples.len(),
            asset_candidate_count: placement.candidate_count,
            asset_rejected_count: placement.rejected_count,
            chunk_origin_planet: request.chunk_origin_planet,
            gpu_custom_aabb: None,
            render_tile: samples.to_render_tile_payload(),
            mesh,
            assets: placement.assets,
            packed_regions: Some(packed_regions),
            requirements: request.requirements,
            scratch_metrics: super::super::workers::payloads::WorkerScratchJobMetrics::default(),
        };
        Ok(self.install_prepared_render_payload(prepared, frame_state))
    }

    fn payload_requirements_for_key(
        &self,
        desired_physics: &HashSet<ChunkKey>,
        key: ChunkKey,
    ) -> PayloadBuildRequirements {
        PayloadBuildRequirements::new(
            self.config.render_backend == RenderBackendKind::ServerPool,
            desired_physics.contains(&key) || self.active_physics.contains(&key),
        )
    }

    fn payload_satisfies_requirements(
        &self,
        payload: &ChunkPayload,
        surface_class: &SurfaceClassKey,
        requirements: PayloadBuildRequirements,
    ) -> bool {
        payload.surface_class == *surface_class
            && payload.render_tile.validate_layout().is_ok()
            && payload.build_requirements().satisfies(requirements)
    }

    pub(crate) fn prepare_render_payload_request(
        &mut self,
        sequence: usize,
        epoch: u64,
        key: ChunkKey,
        desired_render: &HashSet<ChunkKey>,
        desired_physics: &HashSet<ChunkKey>,
    ) -> Result<Option<RenderPayloadRequest>, TopologyError> {
        let surface_class = self.required_surface_class_for_selection(key, desired_render)?;
        let requirements = self.payload_requirements_for_key(desired_physics, key);
        let existing_matches = self
            .resident_payloads
            .get(&key)
            .map(|payload| {
                self.payload_satisfies_requirements(payload, &surface_class, requirements)
            })
            .unwrap_or(false);
        if existing_matches {
            return Ok(None);
        }

        let meta = self.ensure_chunk_meta(key)?;
        Ok(Some(RenderPayloadRequest {
            sequence,
            epoch,
            key,
            surface_class,
            requirements,
            chunk_origin_planet: meta.bounds.center_planet,
            geometric_error: meta.metrics.geometric_error,
            config: self.config.clone(),
        }))
    }

    pub(crate) fn request_render_payloads_for_selection(
        &mut self,
        keys: &[ChunkKey],
        desired_render: &HashSet<ChunkKey>,
        desired_physics: &HashSet<ChunkKey>,
        frame_state: &mut SelectionFrameState,
    ) -> Result<(), TopologyError> {
        let mut sorted_keys = keys.to_vec();
        sorted_keys.sort_unstable();
        sorted_keys.dedup();
        let refresh_set = sorted_keys.iter().copied().collect::<HashSet<_>>();
        self.pending_payload_requests
            .retain(|key, _| refresh_set.contains(key));

        let mut requests = Vec::new();
        for key in sorted_keys {
            let surface_class = self.required_surface_class_for_selection(key, desired_render)?;
            let requirements = self.payload_requirements_for_key(desired_physics, key);
            let existing_matches = self
                .resident_payloads
                .get(&key)
                .map(|payload| {
                    self.payload_satisfies_requirements(payload, &surface_class, requirements)
                })
                .unwrap_or(false);
            if existing_matches {
                self.pending_payload_requests.remove(&key);
                continue;
            }

            if self
                .pending_payload_requests
                .get(&key)
                .map(|pending| {
                    pending.surface_class == surface_class
                        && pending.requirements.satisfies(requirements)
                })
                .unwrap_or(false)
            {
                continue;
            }

            let epoch = self.next_payload_request_epoch;
            self.next_payload_request_epoch = self.next_payload_request_epoch.saturating_add(1);
            if let Some(request) = self.prepare_render_payload_request(
                requests.len(),
                epoch,
                key,
                desired_render,
                desired_physics,
            )? {
                self.pending_payload_requests.insert(
                    key,
                    PendingPayloadRequest {
                        epoch,
                        surface_class: surface_class.clone(),
                        requirements,
                    },
                );
                requests.push(request);
            }
        }

        let submitted = self.threaded_payload_generator.submit(requests);
        frame_state.phase9_worker_threads = self.threaded_payload_generator.worker_count();
        frame_state.phase9_submitted_jobs += submitted.submitted_jobs;
        frame_state.phase9_superseded_jobs += submitted.superseded_jobs;
        frame_state.phase9_queue_peak = frame_state.phase9_queue_peak.max(submitted.queue_peak);
        frame_state.phase9_inflight_jobs = self.pending_payload_requests.len();

        Ok(())
    }

    pub(crate) fn drain_ready_render_payloads(&mut self, frame_state: &mut SelectionFrameState) {
        let batch = self.threaded_payload_generator.drain_ready();
        frame_state.phase9_worker_threads = self.threaded_payload_generator.worker_count();
        frame_state.phase9_ready_results += batch.results.len();
        frame_state.phase9_generation_jobs += batch.results.len();

        for prepared in batch.results {
            self.accept_prepared_render_payload(prepared, frame_state);
        }

        frame_state.phase9_inflight_jobs = self.pending_payload_requests.len();
    }

    pub(crate) fn accept_prepared_render_payload(
        &mut self,
        prepared: PreparedRenderPayload,
        frame_state: &mut SelectionFrameState,
    ) -> bool {
        let Some(pending) = self.pending_payload_requests.get(&prepared.key) else {
            frame_state.phase9_stale_results_dropped += 1;
            frame_state.phase9_inflight_jobs = self.pending_payload_requests.len();
            return false;
        };
        if pending.epoch != prepared.epoch
            || pending.surface_class != prepared.surface_class
            || pending.requirements != prepared.requirements
        {
            frame_state.phase9_stale_results_dropped += 1;
            frame_state.phase9_inflight_jobs = self.pending_payload_requests.len();
            return false;
        }

        self.pending_payload_requests.remove(&prepared.key);
        frame_state.phase9_sample_scratch_reuse_hits +=
            usize::from(prepared.scratch_metrics.sample_reuse);
        frame_state.phase9_mesh_scratch_reuse_hits +=
            usize::from(prepared.scratch_metrics.mesh_reuse);
        frame_state.phase9_pack_scratch_reuse_hits +=
            usize::from(prepared.scratch_metrics.pack_reuse);
        frame_state.phase9_scratch_growth_events += prepared.scratch_metrics.growth_events;
        self.install_prepared_render_payload(prepared, frame_state);
        frame_state.phase9_inflight_jobs = self.pending_payload_requests.len();
        true
    }

    pub(crate) fn payload_matches_selection(
        &mut self,
        key: ChunkKey,
        desired_render: &HashSet<ChunkKey>,
        desired_physics: &HashSet<ChunkKey>,
    ) -> Result<bool, TopologyError> {
        let required_surface_class =
            self.required_surface_class_for_selection(key, desired_render)?;
        let requirements = self.payload_requirements_for_key(desired_physics, key);
        Ok(self
            .resident_payloads
            .get(&key)
            .map(|payload| {
                self.payload_satisfies_requirements(payload, &required_surface_class, requirements)
            })
            .unwrap_or(false))
    }

    #[cfg(test)]
    pub(crate) fn desired_keys_intersecting(
        key: ChunkKey,
        desired: &HashSet<ChunkKey>,
    ) -> Vec<ChunkKey> {
        desired
            .iter()
            .copied()
            .filter(|candidate| {
                candidate.face == key.face
                    && (candidate.is_descendant_of(&key) || key.is_descendant_of(candidate))
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn coverage_ready_for_key(
        key: ChunkKey,
        desired: &HashSet<ChunkKey>,
        ready_desired: &HashSet<ChunkKey>,
    ) -> bool {
        let intersections = Self::desired_keys_intersecting(key, desired);
        intersections.is_empty()
            || intersections
                .into_iter()
                .all(|replacement| ready_desired.contains(&replacement))
    }

    fn install_prepared_render_payload(
        &mut self,
        prepared: PreparedRenderPayload,
        frame_state: &mut SelectionFrameState,
    ) -> usize {
        let PreparedRenderPayload {
            epoch,
            key,
            surface_class,
            sample_count,
            asset_candidate_count,
            asset_rejected_count,
            chunk_origin_planet,
            gpu_custom_aabb,
            render_tile,
            mesh,
            assets,
            packed_regions,
            requirements,
            ..
        } = prepared;

        frame_state.phase7_sampled_chunks += 1;
        if requirements.requires_cpu_mesh() {
            frame_state.phase7_meshed_chunks += 1;
        }
        if packed_regions.is_some() {
            frame_state.phase7_packed_chunks += 1;
        }
        frame_state.phase12_asset_candidate_count += asset_candidate_count;
        frame_state.phase12_asset_rejected_count += asset_rejected_count;
        frame_state.phase12_asset_accepted_count += assets.len();
        frame_state.phase12_chunks_with_asset_payloads += 1;

        let current_surface_class = self
            .rid_state
            .get(&key)
            .and_then(|state| {
                state
                    .render_resident
                    .then(|| state.active_surface_class.clone())
            })
            .flatten();
        let warm_path =
            self.choose_render_warm_path(current_surface_class.as_ref(), &surface_class);
        frame_state.phase7_commit_payloads += 1;
        frame_state.phase7_lifecycle_commands += 1;

        let render_lifecycle = match &warm_path {
            RenderWarmPath::ReuseCurrentSurface => {
                frame_state.phase7_warm_current_reuse_hits += 1;
                RenderLifecycleCommand::WarmReuseCurrent
            }
            RenderWarmPath::ReusePooledSurface(_) => {
                frame_state.phase7_warm_pool_reuse_hits += 1;
                RenderLifecycleCommand::WarmReusePooled
            }
            RenderWarmPath::ColdPath(reason) => {
                frame_state.phase7_cold_fallbacks += 1;
                match reason {
                    RenderFallbackReason::MissingCurrentSurfaceClass => {
                        frame_state.phase8_fallback_missing_current_surface_class += 1;
                    }
                    RenderFallbackReason::IncompatibleCurrentSurfaceClass(_) => {
                        frame_state.phase8_fallback_incompatible_current_surface_class += 1;
                    }
                    RenderFallbackReason::NoCompatiblePooledSurface => {
                        frame_state.phase8_fallback_no_compatible_pooled_surface += 1;
                    }
                }
                RenderLifecycleCommand::ColdCreate(reason.clone())
            }
        };

        let upload_bytes = packed_regions
            .as_ref()
            .map(|regions| {
                regions.vertex_region.len()
                    + regions.attribute_region.len()
                    + regions.index_region.len()
            })
            .unwrap_or(0);
        let mut staging = None;
        if self.config.enable_godot_staging && packed_regions.is_some() {
            let mut staged =
                self.stage_payload_bytes(key, &surface_class, packed_regions.as_ref(), &warm_path);
            staged
                .copy_from_regions(
                    packed_regions
                        .as_ref()
                        .expect("staging requires packed regions"),
                    &surface_class,
                )
                .expect("staging capacity must match the selected surface class");
            frame_state.phase7_staged_chunks += 1;
            staging = Some(staged);
        }

        let payload = ChunkPayload {
            payload_epoch: epoch,
            surface_class: surface_class.clone(),
            stitch_mask: surface_class.stitch_mask,
            sample_count,
            chunk_origin_planet,
            mesh,
            packed_regions,
            gd_staging: staging,
            pooled_render_entry: match warm_path {
                RenderWarmPath::ReusePooledSurface(entry) => Some(entry),
                _ => None,
            },
            render_tile,
            render_tile_handle: None,
            gpu_custom_aabb,
            assets,
            collision: ChunkCollisionPayload::default(),
            render_lifecycle,
        };
        self.insert_payload(key, payload);
        if let Some(entry) = self.render_residency.get_mut(&key) {
            entry.resident_surface_class = Some(surface_class);
        }
        if self.active_render.contains(&key) {
            self.asset_groups_dirty = true;
        }

        upload_bytes
    }

    fn stage_payload_bytes(
        &mut self,
        key: ChunkKey,
        surface_class: &SurfaceClassKey,
        _packed_regions: Option<&PackedMeshRegions>,
        warm_path: &RenderWarmPath,
    ) -> GdPackedStaging {
        let policy = self.config.staging_policy;
        policy.acquire_staging(self, key, surface_class, warm_path)
    }

    pub(crate) fn ensure_collision_payload(&mut self, key: ChunkKey) {
        let Some(payload) = self.resident_payloads.get_mut(&key) else {
            return;
        };

        if payload.collision.collider_vertices.is_none() {
            payload.collision.collider_vertices = Some(payload.mesh.positions.clone());
        }
        if payload.collision.collider_indices.is_none() {
            payload.collision.collider_indices = Some(payload.mesh.indices.clone());
        }
        if payload.collision.collider_faces.is_none() {
            let Some(vertices) = payload.collision.collider_vertices.as_deref() else {
                return;
            };
            let Some(indices) = payload.collision.collider_indices.as_deref() else {
                return;
            };
            payload.collision.collider_faces =
                Some(collider_face_vertices_from_indices(vertices, indices));
        }
    }
}
