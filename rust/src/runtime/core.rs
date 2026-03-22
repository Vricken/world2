use super::*;

impl Default for PlanetRuntime {
    fn default() -> Self {
        Self::new(RuntimeConfig::default(), Rid::Invalid, Rid::Invalid)
    }
}

impl PlanetRuntime {
    pub fn new(config: RuntimeConfig, scenario_rid: Rid, physics_space_rid: Rid) -> Self {
        let config = config.normalized();
        let metadata_max_lod = config.max_lod;
        let mut runtime = Self {
            threaded_payload_generator: ThreadedPayloadGenerator::new(config.worker_thread_count),
            threaded_metadata_generator: ThreadedMetadataGenerator::new(config.worker_thread_count),
            threaded_asset_group_generator: ThreadedAssetGroupGenerator::new(1),
            origin_snapshot: OriginSnapshot::for_config(&config, DVec3::ZERO),
            config,
            scenario_rid,
            physics_space_rid,
            meta: MetadataStore::new(metadata_max_lod),
            active_render: HashSet::new(),
            active_physics: HashSet::new(),
            resident_payloads: HashMap::new(),
            rid_state: HashMap::new(),
            render_pool: HashMap::new(),
            physics_pool: VecDeque::new(),
            asset_groups: HashMap::new(),
            asset_family_meshes: HashMap::new(),
            frame_state: SelectionFrameState::default(),
            pending_meta_requests: HashMap::new(),
            pending_payload_requests: HashMap::new(),
            pending_asset_group_epoch: None,
            asset_groups_dirty: true,
            next_meta_request_epoch: 1,
            next_payload_request_epoch: 1,
            next_asset_group_epoch: 1,
            deferred_starvation: HashMap::new(),
            origin_shift_pending_rebind: false,
            pending_origin_rebases: 0,
            pending_render_transform_rebinds: 0,
            pending_physics_transform_rebinds: 0,
        };
        runtime
            .build_metadata_tree_through_lod(
                runtime
                    .config
                    .metadata_precompute_max_lod
                    .min(runtime.config.max_lod),
            )
            .expect("phase 7 metadata prebuild keys must always be valid");
        runtime
    }

    pub fn metadata_precompute_max_lod(&self) -> u8 {
        self.config.metadata_precompute_max_lod
    }

    pub fn payload_precompute_max_lod(&self) -> u8 {
        self.config.payload_precompute_max_lod
    }

    pub fn worker_thread_count(&self) -> usize {
        self.threaded_payload_generator.worker_count()
    }

    pub fn origin_snapshot(&self) -> OriginSnapshot {
        self.origin_snapshot
    }

    pub fn origin_mode_label(&self) -> &'static str {
        self.origin_snapshot.mode.label()
    }

    pub fn root_scene_position(&self) -> Vector3 {
        dvec3_to_vector3(-self.origin_snapshot.render_origin_planet)
    }

    pub fn camera_planet_position_from_render(&self, position_render: Vector3) -> DVec3 {
        vector3_to_dvec3(position_render) + self.origin_snapshot.render_origin_planet
    }

    pub fn update_origin_from_camera(&mut self, camera_position_planet: DVec3) -> bool {
        let desired = if self.config.use_large_world_coordinates {
            OriginSnapshot::for_config(&self.config, DVec3::ZERO)
        } else {
            let current = self.origin_snapshot.render_origin_planet;
            let delta = (camera_position_planet - current).length();
            if delta < self.config.origin_recenter_distance {
                return false;
            }
            OriginSnapshot::for_config(&self.config, camera_position_planet)
        };

        if desired == self.origin_snapshot {
            return false;
        }

        self.origin_snapshot = desired;
        self.origin_shift_pending_rebind = true;
        true
    }

    pub fn should_precompute_payload_for_lod(&self, lod: u8) -> bool {
        lod <= self.payload_precompute_max_lod()
    }

    pub fn has_valid_world_rids(&self) -> bool {
        self.scenario_rid != Rid::Invalid && self.physics_space_rid != Rid::Invalid
    }

    pub fn set_world_rids(&mut self, scenario_rid: Rid, physics_space_rid: Rid) {
        self.scenario_rid = scenario_rid;
        self.physics_space_rid = physics_space_rid;
    }

    pub fn frame_state(&self) -> &SelectionFrameState {
        &self.frame_state
    }

    pub fn build_metadata_tree_through_lod(&mut self, max_lod: u8) -> Result<usize, TopologyError> {
        let mut inserted = 0usize;

        for lod in 0..=max_lod.min(self.config.max_lod) {
            if self.meta.level_is_built(lod) {
                continue;
            }
            for face in Face::ALL {
                let resolution = ChunkKey::resolution_for_lod(lod);
                let mut entries = Vec::with_capacity((resolution * resolution) as usize);
                for y in 0..resolution {
                    for x in 0..resolution {
                        let key = ChunkKey::new(face, lod, x, y);
                        let meta = self.build_chunk_meta(key)?;
                        entries.push(StoredChunkMeta::from_chunk_meta(&meta));
                        inserted += 1;
                    }
                }
                self.meta.set_face_level(face, lod, entries)?;
            }
        }

        Ok(inserted)
    }

    pub fn ensure_chunk_meta(&mut self, key: ChunkKey) -> Result<ChunkMeta, TopologyError> {
        if !self.meta.contains_key(&key) {
            let meta = self.build_chunk_meta(key)?;
            self.meta.insert_chunk_meta(meta, true)?;
        }

        self.meta
            .get_chunk_meta(key, self.base_chunk_surface_class()?)?
            .ok_or(TopologyError::InvalidChunkKey)
    }

    pub fn register_chunk_meta(
        &mut self,
        meta: ChunkMeta,
    ) -> Result<Option<ChunkMeta>, TopologyError> {
        self.meta.insert_chunk_meta(meta, false)
    }

    pub fn base_chunk_surface_class(&self) -> Result<SurfaceClassKey, TopologyError> {
        SurfaceClassKey::canonical_chunk(
            mesh_topology::BASE_STITCH_MASK,
            self.config.render_material_class,
            self.config.render_format_mask,
            self.config.render_vertex_stride,
            self.config.render_attribute_stride,
            self.config.render_index_stride,
        )
        .map_err(|_| TopologyError::InvalidChunkKey)
    }

    pub fn insert_payload(&mut self, key: ChunkKey, payload: ChunkPayload) -> Option<ChunkPayload> {
        let previous = self.resident_payloads.insert(key, payload);
        if let Some(payload) = previous.as_ref().cloned() {
            self.reclaim_payload_resources(payload);
        }
        previous
    }

    pub fn remove_payload(&mut self, key: &ChunkKey) -> Option<ChunkPayload> {
        let removed = self.resident_payloads.remove(key);
        if let Some(payload) = removed.as_ref().cloned() {
            self.reclaim_payload_resources(payload);
        }
        removed
    }

    pub fn enforce_payload_residency_budget(
        &mut self,
        retained_keys: &HashSet<ChunkKey>,
        max_payloads: usize,
    ) -> Vec<ChunkKey> {
        if self.resident_payloads.len() <= max_payloads {
            return Vec::new();
        }

        let mut eviction_candidates = self
            .resident_payloads
            .keys()
            .copied()
            .filter(|key| !retained_keys.contains(key))
            .collect::<Vec<_>>();
        eviction_candidates.sort_unstable();

        let mut evicted = Vec::new();
        let mut payload_count = self.resident_payloads.len();

        for key in eviction_candidates {
            if payload_count <= max_payloads {
                break;
            }

            if let Some(payload) = self.resident_payloads.remove(&key) {
                self.reclaim_payload_resources(payload);
                evicted.push(key);
                payload_count -= 1;
            }
        }

        evicted
    }

    pub fn ensure_rid_state(&mut self, key: ChunkKey) -> &mut ChunkRidState {
        self.rid_state.entry(key).or_default()
    }

    pub fn activate_render(&mut self, key: ChunkKey) -> bool {
        self.active_render.insert(key)
    }

    pub fn deactivate_render(&mut self, key: &ChunkKey) -> bool {
        self.active_render.remove(key)
    }

    pub fn activate_physics(&mut self, key: ChunkKey) -> bool {
        self.active_physics.insert(key)
    }

    pub fn deactivate_physics(&mut self, key: &ChunkKey) -> bool {
        self.active_physics.remove(key)
    }

    pub fn push_render_pool_entry(&mut self, entry: RenderPoolEntry) {
        self.render_pool
            .entry(entry.surface_class.clone())
            .or_default()
            .push_back(entry);
    }

    pub fn pop_render_pool_entry(
        &mut self,
        surface_class: &SurfaceClassKey,
    ) -> Option<RenderPoolEntry> {
        let entries = self.render_pool.get_mut(surface_class)?;
        let entry = entries.pop_front();

        if entries.is_empty() {
            self.render_pool.remove(surface_class);
        }

        entry
    }

    pub fn choose_render_warm_path(
        &mut self,
        current_surface_class: Option<&SurfaceClassKey>,
        required_surface_class: &SurfaceClassKey,
    ) -> RenderWarmPath {
        if let Some(current_surface_class) = current_surface_class {
            let issues = current_surface_class.compatibility_issues(required_surface_class);
            if issues.is_empty() {
                return RenderWarmPath::ReuseCurrentSurface;
            }

            if let Some(entry) = self.pop_render_pool_entry(required_surface_class) {
                return RenderWarmPath::ReusePooledSurface(entry);
            }

            return RenderWarmPath::ColdPath(
                RenderFallbackReason::IncompatibleCurrentSurfaceClass(issues),
            );
        }

        if let Some(entry) = self.pop_render_pool_entry(required_surface_class) {
            return RenderWarmPath::ReusePooledSurface(entry);
        }

        RenderWarmPath::ColdPath(RenderFallbackReason::MissingCurrentSurfaceClass)
    }

    pub fn push_physics_pool_entry(&mut self, entry: PhysicsPoolEntry) {
        self.physics_pool.push_back(entry);
    }

    pub fn pop_physics_pool_entry(&mut self) -> Option<PhysicsPoolEntry> {
        self.physics_pool.pop_front()
    }

    pub fn meta_count(&self) -> usize {
        self.meta.len()
    }

    pub fn active_render_count(&self) -> usize {
        self.active_render.len()
    }

    pub fn active_physics_count(&self) -> usize {
        self.active_physics.len()
    }

    pub fn desired_render_count(&self) -> usize {
        self.frame_state.desired_render_count
    }

    pub fn desired_physics_count(&self) -> usize {
        self.frame_state.desired_physics_count
    }

    pub fn deferred_commit_count(&self) -> usize {
        self.frame_state.deferred_commit_ops
    }

    pub fn deferred_upload_bytes(&self) -> usize {
        self.frame_state.upload_bytes_deferred
    }

    pub fn resident_payload_count(&self) -> usize {
        self.resident_payloads.len()
    }

    pub fn rid_state_count(&self) -> usize {
        self.rid_state.len()
    }

    pub fn active_asset_group_count(&self) -> usize {
        self.asset_groups.len()
    }

    pub fn active_asset_instance_count(&self) -> usize {
        self.asset_groups
            .values()
            .map(|group| group.instance_count)
            .sum()
    }

    pub fn asset_family_mesh_count(&self) -> usize {
        self.asset_family_meshes.len()
    }

    pub fn asset_debug_snapshot(&self) -> AssetDebugSnapshot {
        asset_debug_snapshot(&self.asset_groups, &self.asset_family_meshes)
    }

    pub fn seam_debug_snapshot(&self) -> SeamDebugSnapshot {
        let mut snapshot = SeamDebugSnapshot {
            active_render_chunks: self.active_render.len(),
            ..SeamDebugSnapshot::default()
        };

        for key in &self.active_render {
            let active_surface_class = self.rid_state.get(key).and_then(|state| {
                state
                    .render_resident
                    .then_some(state.active_surface_class.as_ref())
                    .flatten()
            });
            let payload_surface_class = self
                .resident_payloads
                .get(key)
                .map(|payload| &payload.surface_class);

            if let (Some(active_surface_class), Some(payload_surface_class)) =
                (active_surface_class, payload_surface_class)
            {
                if active_surface_class != payload_surface_class {
                    snapshot.pending_surface_class_mismatch_chunks += 1;
                }
            }

            if let Some(surface_class) = active_surface_class.or(payload_surface_class) {
                snapshot.record_active_mask(surface_class.stitch_mask);
            } else {
                snapshot.active_chunks_missing_surface_class += 1;
            }
        }

        for entries in self.render_pool.values() {
            for entry in entries {
                snapshot.record_pooled_mask(entry.surface_class.stitch_mask);
            }
        }

        snapshot
    }
}
