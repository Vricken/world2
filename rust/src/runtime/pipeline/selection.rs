use super::super::*;

const METADATA_SAMPLE_GRID_EDGE: u32 = 5;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectionFrameState {
    pub tick: u64,
    pub desired_render_count: usize,
    pub desired_physics_count: usize,
    pub horizon_survivor_count: usize,
    pub frustum_survivor_count: usize,
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
    pub phase9_generation_jobs: usize,
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
    pub render_pool_entries: usize,
    pub physics_pool_entries: usize,
}

impl PlanetRuntime {
    pub fn step_visibility_selection(&mut self, camera: &CameraState) -> Result<(), TopologyError> {
        let mut frame_state = SelectionFrameState {
            tick: self.frame_state.tick.saturating_add(1),
            phase9_worker_threads: self.threaded_payload_generator.worker_count(),
            ..SelectionFrameState::default()
        };
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
        self.frame_state = frame_state;

        Ok(())
    }

    pub(crate) fn build_chunk_meta(&self, key: ChunkKey) -> Result<ChunkMeta, TopologyError> {
        let sample_dirs = self.chunk_sample_directions(key, METADATA_SAMPLE_GRID_EDGE)?;
        let center_dir = sample_dirs
            .iter()
            .copied()
            .fold(DVec3::ZERO, |sum, dir| sum + dir)
            .normalize_or_zero();
        let terrain = TerrainFieldSettings {
            planet_radius: self.config.planet_radius,
            height_amplitude: self.config.height_amplitude,
            ..TerrainFieldSettings::default()
        };
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
                let height = self
                    .terrain_settings()
                    .sample_height(unit_dir)
                    .clamp(-self.config.height_amplitude, self.config.height_amplitude)
                    as f32;
                let temperature = (1.0 - unit_dir.y.abs()) as f32;
                let moisture_signal =
                    (unit_dir.dot(DVec3::new(1.731, -0.613, 0.947)).sin() * 0.5 + 0.5) as f32;
                let biome0 = moisture_signal.clamp(0.0, 1.0);
                let biome1 = ((temperature * 0.75)
                    + ((height / self.config.height_amplitude as f32) * 0.25 + 0.25))
                    .clamp(0.0, 1.0);

                samples.push(ChunkSample {
                    unit_dir,
                    height,
                    biome0,
                    biome1,
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
                mesh.colors
                    .push([sample.biome0, sample.biome1, sample.slope_hint, 1.0]);
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

    fn select_render_set(
        &mut self,
        camera: &CameraState,
        frame_state: &mut SelectionFrameState,
    ) -> Result<HashSet<ChunkKey>, TopologyError> {
        let mut selected = HashSet::new();
        let split_ancestors = self.build_current_split_ancestors();

        for face in Face::ALL {
            self.select_render_chunk(
                ChunkKey::new(face, 0, 0, 0),
                camera,
                &mut selected,
                &split_ancestors,
                frame_state,
            )?;
        }

        frame_state.neighbor_split_count = self.normalize_neighbor_lod_delta(&mut selected)?;
        frame_state.selected_leaf_count = selected.len();

        Ok(selected)
    }

    fn select_render_chunk(
        &mut self,
        key: ChunkKey,
        camera: &CameraState,
        selected: &mut HashSet<ChunkKey>,
        split_ancestors: &HashSet<ChunkKey>,
        frame_state: &mut SelectionFrameState,
    ) -> Result<(), TopologyError> {
        let meta = self.ensure_chunk_meta(key)?.clone();

        if !self.horizon_visible(camera, &meta) {
            return Ok(());
        }
        frame_state.horizon_survivor_count += 1;

        if !self.frustum_visible(camera, &meta) {
            return Ok(());
        }
        frame_state.frustum_survivor_count += 1;

        let error_px = self.projected_error_px(camera, &meta);
        let should_split = key.lod < self.config.max_lod
            && self.should_split_chunk(key, error_px, split_ancestors);

        if should_split {
            for child in key
                .children()
                .expect("child keys must exist while below configured max lod")
            {
                self.select_render_chunk(child, camera, selected, split_ancestors, frame_state)?;
            }
        } else {
            selected.insert(key);
        }

        Ok(())
    }

    fn build_current_split_ancestors(&self) -> HashSet<ChunkKey> {
        let mut split_ancestors = HashSet::new();

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

    fn normalize_neighbor_lod_delta(
        &mut self,
        selected: &mut HashSet<ChunkKey>,
    ) -> Result<usize, TopologyError> {
        let mut splits_applied = 0usize;

        loop {
            let mut split_targets = HashSet::new();

            for key in selected.iter().copied().collect::<Vec<_>>() {
                for edge in Edge::ALL {
                    let neighbor_same_lod = topology::same_lod_neighbor(key, edge)?;

                    if let Some(active_ancestor) =
                        Self::find_active_ancestor_covering(neighbor_same_lod, selected)
                    {
                        if key.lod > active_ancestor.lod + 1 {
                            split_targets.insert(active_ancestor);
                        }
                    }
                }
            }

            if split_targets.is_empty() {
                break;
            }

            for coarse_key in split_targets {
                if !selected.remove(&coarse_key) {
                    continue;
                }

                for child in coarse_key
                    .children()
                    .expect("normalization only splits non-leaf chunks")
                {
                    self.ensure_chunk_meta(child)?;
                    selected.insert(child);
                }

                splits_applied += 1;
            }
        }

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

    pub(crate) fn select_physics_set(
        &mut self,
        camera: &CameraState,
        desired_render: &HashSet<ChunkKey>,
    ) -> Result<HashSet<ChunkKey>, TopologyError> {
        let render_keys = desired_render.iter().copied().collect::<Vec<_>>();
        let mut candidates = Vec::with_capacity(render_keys.len());

        for key in render_keys {
            let meta = self.ensure_chunk_meta(key)?.clone();
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

    #[cfg(test)]
    fn terrain_settings(&self) -> TerrainFieldSettings {
        TerrainFieldSettings {
            planet_radius: self.config.planet_radius,
            height_amplitude: self.config.height_amplitude,
            ..TerrainFieldSettings::default()
        }
    }

    pub(crate) fn required_surface_class_for_selection(
        &mut self,
        key: ChunkKey,
        desired_render: &HashSet<ChunkKey>,
    ) -> Result<SurfaceClassKey, TopologyError> {
        let meta = self.ensure_chunk_meta(key)?.clone();
        let mut neighbor_lods = [key.lod; Edge::ALL.len()];

        for (index, edge) in Edge::ALL.into_iter().enumerate() {
            let neighbor_same_lod = meta.neighbors.get(edge);
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
        let Some(request) = self.prepare_render_payload_request(0, key, desired_render)? else {
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
            key,
            surface_class: request.surface_class,
            sample_count: samples.len(),
            asset_candidate_count: placement.candidate_count,
            asset_rejected_count: placement.rejected_count,
            chunk_origin_planet: request.chunk_origin_planet,
            mesh,
            assets: placement.assets,
            packed_regions,
            scratch_metrics: super::super::workers::payloads::WorkerScratchJobMetrics::default(),
        };
        Ok(self.install_prepared_render_payload(prepared, frame_state))
    }

    pub(crate) fn prepare_render_payload_request(
        &mut self,
        sequence: usize,
        key: ChunkKey,
        desired_render: &HashSet<ChunkKey>,
    ) -> Result<Option<RenderPayloadRequest>, TopologyError> {
        let surface_class = self.required_surface_class_for_selection(key, desired_render)?;
        let existing_matches = self
            .resident_payloads
            .get(&key)
            .map(|payload| {
                payload.surface_class == surface_class && payload.packed_regions.is_some()
            })
            .unwrap_or(false);
        if existing_matches {
            return Ok(None);
        }

        Ok(Some(RenderPayloadRequest {
            sequence,
            key,
            surface_class,
            chunk_origin_planet: self.ensure_chunk_meta(key)?.bounds.center_planet,
            config: self.config.clone(),
        }))
    }

    pub(crate) fn prepare_render_payloads_for_selection(
        &mut self,
        keys: &[ChunkKey],
        desired_render: &HashSet<ChunkKey>,
        frame_state: &mut SelectionFrameState,
    ) -> Result<(), TopologyError> {
        let mut sorted_keys = keys.to_vec();
        sorted_keys.sort_unstable();
        sorted_keys.dedup();

        let mut requests = Vec::new();
        for key in sorted_keys {
            if let Some(request) =
                self.prepare_render_payload_request(requests.len(), key, desired_render)?
            {
                requests.push(request);
            }
        }

        let batch = self.threaded_payload_generator.generate(requests);
        frame_state.phase9_worker_threads = self.threaded_payload_generator.worker_count();
        frame_state.phase9_generation_jobs += batch.results.len();
        frame_state.phase9_queue_peak = frame_state.phase9_queue_peak.max(batch.queue_peak);
        frame_state.phase9_result_wait_count += batch.result_wait_count;

        for prepared in batch.results {
            frame_state.phase9_sample_scratch_reuse_hits +=
                usize::from(prepared.scratch_metrics.sample_reuse);
            frame_state.phase9_mesh_scratch_reuse_hits +=
                usize::from(prepared.scratch_metrics.mesh_reuse);
            frame_state.phase9_pack_scratch_reuse_hits +=
                usize::from(prepared.scratch_metrics.pack_reuse);
            frame_state.phase9_scratch_growth_events += prepared.scratch_metrics.growth_events;
            self.install_prepared_render_payload(prepared, frame_state);
        }

        Ok(())
    }

    fn install_prepared_render_payload(
        &mut self,
        prepared: PreparedRenderPayload,
        frame_state: &mut SelectionFrameState,
    ) -> usize {
        let PreparedRenderPayload {
            key,
            surface_class,
            sample_count,
            asset_candidate_count,
            asset_rejected_count,
            chunk_origin_planet,
            mesh,
            assets,
            packed_regions,
            ..
        } = prepared;

        frame_state.phase7_sampled_chunks += 1;
        frame_state.phase7_meshed_chunks += 1;
        frame_state.phase7_packed_chunks += 1;
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

        let upload_bytes = packed_regions.vertex_region.len()
            + packed_regions.attribute_region.len()
            + packed_regions.index_region.len();
        let mut staging = None;
        if self.config.enable_godot_staging {
            let mut staged =
                self.stage_payload_bytes(key, &surface_class, &packed_regions, &warm_path);
            staged
                .copy_from_regions(&packed_regions, &surface_class)
                .expect("staging capacity must match the selected surface class");
            frame_state.phase7_staged_chunks += 1;
            staging = Some(staged);
        }

        let payload = ChunkPayload {
            surface_class: surface_class.clone(),
            stitch_mask: surface_class.stitch_mask,
            sample_count,
            chunk_origin_planet,
            mesh,
            packed_regions: Some(packed_regions),
            gd_staging: staging,
            pooled_render_entry: match warm_path {
                RenderWarmPath::ReusePooledSurface(entry) => Some(entry),
                _ => None,
            },
            assets,
            collider_vertices: None,
            collider_indices: None,
            render_lifecycle,
        };
        if let Some(previous_payload) = self.resident_payloads.insert(key, payload) {
            self.reclaim_payload_resources(previous_payload);
        }

        upload_bytes
    }

    fn stage_payload_bytes(
        &mut self,
        key: ChunkKey,
        surface_class: &SurfaceClassKey,
        _packed_regions: &PackedMeshRegions,
        warm_path: &RenderWarmPath,
    ) -> GdPackedStaging {
        let policy = self.config.staging_policy;
        policy.acquire_staging(self, key, surface_class, warm_path)
    }

    pub(crate) fn ensure_collision_payload(&mut self, key: ChunkKey) {
        let Some(payload) = self.resident_payloads.get_mut(&key) else {
            return;
        };

        if payload.collider_vertices.is_none() {
            payload.collider_vertices = Some(payload.mesh.positions.clone());
            payload.collider_indices = Some(payload.mesh.indices.clone());
        }
    }
}
