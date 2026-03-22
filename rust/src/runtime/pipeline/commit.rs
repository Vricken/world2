use super::super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CommitOp {
    kind: CommitOpKind,
    key: ChunkKey,
    upload_bytes: usize,
    priority_group: u8,
    distance_key_mm: u64,
}

impl PlanetRuntime {
    pub(crate) fn apply_budgeted_diffs(
        &mut self,
        desired_render: &HashSet<ChunkKey>,
        desired_physics: &HashSet<ChunkKey>,
        camera: &CameraState,
        frame_state: &mut SelectionFrameState,
    ) -> Result<(), TopologyError> {
        let mut ops =
            self.build_commit_ops(desired_render, desired_physics, camera, frame_state)?;
        ops.sort_by(|a, b| {
            a.priority_group
                .cmp(&b.priority_group)
                .then(a.distance_key_mm.cmp(&b.distance_key_mm))
                .then(a.key.cmp(&b.key))
        });

        frame_state.queued_commit_ops = ops.len();

        let mut committed = 0usize;
        let mut upload_bytes_committed = 0usize;
        let mut deferred_upload_bytes = 0usize;
        let mut deferred_now = HashSet::new();

        for op in ops {
            let over_commit_budget = committed >= self.config.commit_budget_per_frame;
            let over_upload_budget = op.upload_bytes > 0
                && upload_bytes_committed + op.upload_bytes
                    > self.config.upload_budget_bytes_per_frame;

            if over_commit_budget || over_upload_budget {
                deferred_upload_bytes += op.upload_bytes;
                deferred_now.insert(DeferredOpKey::new(op.kind, op.key));
                continue;
            }

            self.apply_commit_op(op, frame_state);
            committed += 1;
            upload_bytes_committed += op.upload_bytes;
        }

        for key in &deferred_now {
            let starvation = self.deferred_starvation.entry(*key).or_insert(0);
            *starvation = starvation.saturating_add(1);
        }
        self.deferred_starvation
            .retain(|key, _| deferred_now.contains(key));

        frame_state.committed_commit_ops = committed;
        frame_state.deferred_commit_ops = deferred_now.len();
        frame_state.upload_bytes_committed = upload_bytes_committed;
        frame_state.upload_bytes_deferred = deferred_upload_bytes;
        frame_state.max_deferred_starvation_frames = self
            .deferred_starvation
            .values()
            .copied()
            .max()
            .unwrap_or(0);
        frame_state.render_pool_entries = self.render_pool_entry_count();
        frame_state.physics_pool_entries = self.physics_pool.len();

        Ok(())
    }

    fn build_commit_ops(
        &mut self,
        desired_render: &HashSet<ChunkKey>,
        desired_physics: &HashSet<ChunkKey>,
        camera: &CameraState,
        frame_state: &mut SelectionFrameState,
    ) -> Result<Vec<CommitOp>, TopologyError> {
        let mut ops = Vec::new();

        let mut render_activations = desired_render
            .difference(&self.active_render)
            .copied()
            .collect::<Vec<_>>();
        render_activations.sort_unstable();

        let mut render_updates = desired_render
            .intersection(&self.active_render)
            .copied()
            .collect::<Vec<_>>();
        render_updates.sort_unstable();

        let mut refresh_keys = render_activations.clone();
        for key in render_updates.iter().copied() {
            let required_surface_class =
                self.required_surface_class_for_selection(key, desired_render)?;
            let needs_refresh = self
                .resident_payloads
                .get(&key)
                .map(|payload| payload.surface_class != required_surface_class)
                .unwrap_or(true);
            if needs_refresh {
                refresh_keys.push(key);
            }
        }
        self.prepare_render_payloads_for_selection(&refresh_keys, desired_render, frame_state)?;

        for key in render_activations {
            let meta = self.ensure_chunk_meta(key)?.clone();
            ops.push(CommitOp {
                kind: CommitOpKind::ActivateRender,
                key,
                upload_bytes: self.payload_upload_bytes(key),
                priority_group: 0,
                distance_key_mm: distance_sort_key(self.chunk_camera_distance(camera, &meta)),
            });
        }

        for key in render_updates {
            let required_surface_class =
                self.required_surface_class_for_selection(key, desired_render)?;
            let needs_refresh = self
                .resident_payloads
                .get(&key)
                .map(|payload| payload.surface_class != required_surface_class)
                .unwrap_or(true);
            if !needs_refresh {
                continue;
            }

            let meta = self.ensure_chunk_meta(key)?.clone();
            ops.push(CommitOp {
                kind: CommitOpKind::UpdateRender,
                key,
                upload_bytes: self.payload_upload_bytes(key),
                priority_group: 1,
                distance_key_mm: distance_sort_key(self.chunk_camera_distance(camera, &meta)),
            });
        }

        let mut physics_activations = desired_physics
            .difference(&self.active_physics)
            .copied()
            .collect::<Vec<_>>();
        physics_activations.sort_unstable();
        for key in physics_activations {
            let meta = self.ensure_chunk_meta(key)?.clone();
            let distance = self.chunk_camera_distance(camera, &meta);
            self.ensure_collision_payload(key);
            ops.push(CommitOp {
                kind: CommitOpKind::ActivatePhysics,
                key,
                upload_bytes: 0,
                priority_group: 2,
                distance_key_mm: distance_sort_key(distance),
            });
        }

        let mut render_deactivations = self
            .active_render
            .difference(desired_render)
            .copied()
            .collect::<Vec<_>>();
        render_deactivations.sort_unstable();
        for key in render_deactivations {
            let meta = self.ensure_chunk_meta(key)?.clone();
            let distance = self.chunk_camera_distance(camera, &meta);
            ops.push(CommitOp {
                kind: CommitOpKind::DeactivateRender,
                key,
                upload_bytes: 0,
                priority_group: 3,
                distance_key_mm: distance_sort_key(distance),
            });
        }

        let mut physics_deactivations = self
            .active_physics
            .difference(desired_physics)
            .copied()
            .collect::<Vec<_>>();
        physics_deactivations.sort_unstable();
        for key in physics_deactivations {
            let meta = self.ensure_chunk_meta(key)?.clone();
            let distance = self.chunk_camera_distance(camera, &meta);
            ops.push(CommitOp {
                kind: CommitOpKind::DeactivatePhysics,
                key,
                upload_bytes: 0,
                priority_group: 4,
                distance_key_mm: distance_sort_key(distance),
            });
        }

        Ok(ops)
    }

    pub(crate) fn payload_upload_bytes(&self, key: ChunkKey) -> usize {
        self.resident_payloads
            .get(&key)
            .map(ChunkPayload::upload_bytes)
            .unwrap_or(0)
    }

    fn apply_commit_op(&mut self, op: CommitOp, frame_state: &mut SelectionFrameState) {
        match op.kind {
            CommitOpKind::ActivateRender => {
                self.commit_render_payload(op.key, frame_state);
                self.active_render.insert(op.key);
                self.ensure_rid_state(op.key).render_resident = true;
            }
            CommitOpKind::UpdateRender => {
                self.commit_render_payload(op.key, frame_state);
                self.active_render.insert(op.key);
                self.ensure_rid_state(op.key).render_resident = true;
            }
            CommitOpKind::DeactivateRender => {
                self.deactivate_render_commit(op.key);
                self.active_render.remove(&op.key);
                self.ensure_rid_state(op.key).render_resident = false;
            }
            CommitOpKind::ActivatePhysics => {
                self.commit_physics_payload(op.key, frame_state);
                self.active_physics.insert(op.key);
                self.ensure_rid_state(op.key).physics_resident = true;
            }
            CommitOpKind::DeactivatePhysics => {
                self.deactivate_physics_commit(op.key);
                self.active_physics.remove(&op.key);
                self.ensure_rid_state(op.key).physics_resident = false;
            }
        }
    }

    fn should_commit_to_servers(&self) -> bool {
        self.has_valid_world_rids()
    }

    pub(crate) fn render_pool_entry_count(&self) -> usize {
        self.render_pool.values().map(VecDeque::len).sum()
    }

    pub(crate) fn commit_render_payload(
        &mut self,
        key: ChunkKey,
        frame_state: &mut SelectionFrameState,
    ) {
        let Some(payload) = self.resident_payloads.get_mut(&key) else {
            return;
        };

        let surface_class = payload.surface_class.clone();
        let chunk_origin_planet = payload.chunk_origin_planet;
        let render_lifecycle = payload.render_lifecycle.clone();
        let mesh = payload.mesh.clone();
        let packed_regions = payload.packed_regions.clone();
        let mut gd_staging = payload.gd_staging.take();
        let pooled_render_entry = payload.pooled_render_entry.take();

        let render_transform = self.render_transform_for_chunk(chunk_origin_planet);

        if self.should_commit_to_servers() {
            let mut rendering_server = RenderingServer::singleton();
            match render_lifecycle {
                RenderLifecycleCommand::WarmReuseCurrent => {
                    let Some((mesh_rid, render_instance_rid)) = self.current_render_rids(key)
                    else {
                        return;
                    };
                    let Some(staging) = self.ensure_commit_staging(
                        gd_staging.take(),
                        packed_regions.as_ref(),
                        &surface_class,
                    ) else {
                        return;
                    };
                    let arrays = cpu_mesh_to_surface_arrays(&mesh);

                    rendering_server.mesh_clear(mesh_rid);
                    rendering_server.mesh_add_surface_from_arrays(
                        mesh_rid,
                        PrimitiveType::TRIANGLES,
                        &arrays,
                    );
                    rendering_server.instance_set_base(render_instance_rid, mesh_rid);
                    rendering_server.instance_set_scenario(render_instance_rid, self.scenario_rid);
                    rendering_server.instance_set_transform(render_instance_rid, render_transform);
                    rendering_server.instance_set_visible(render_instance_rid, true);

                    gd_staging = Some(staging);
                    frame_state.phase8_render_warm_current_commits += 1;
                }
                RenderLifecycleCommand::WarmReusePooled => {
                    let Some(entry) = pooled_render_entry else {
                        return;
                    };
                    if let Some(previous_entry) = self.take_current_render_entry(key) {
                        self.recycle_render_entry(previous_entry);
                    }

                    let Some(staging) = self.ensure_commit_staging(
                        gd_staging.take().or(entry.gd_staging),
                        packed_regions.as_ref(),
                        &surface_class,
                    ) else {
                        return;
                    };
                    let arrays = cpu_mesh_to_surface_arrays(&mesh);

                    rendering_server.mesh_clear(entry.mesh_rid);
                    rendering_server.mesh_add_surface_from_arrays(
                        entry.mesh_rid,
                        PrimitiveType::TRIANGLES,
                        &arrays,
                    );
                    rendering_server.instance_set_base(entry.render_instance_rid, entry.mesh_rid);
                    rendering_server
                        .instance_set_scenario(entry.render_instance_rid, self.scenario_rid);
                    rendering_server
                        .instance_set_transform(entry.render_instance_rid, render_transform);
                    rendering_server.instance_set_visible(entry.render_instance_rid, true);

                    self.install_render_entry(
                        key,
                        entry.mesh_rid,
                        entry.render_instance_rid,
                        surface_class.clone(),
                        Some(staging.clone()),
                    );
                    gd_staging = Some(staging);
                    frame_state.phase8_render_warm_pool_commits += 1;
                }
                RenderLifecycleCommand::ColdCreate(_) => {
                    if let Some(previous_entry) = self.take_current_render_entry(key) {
                        self.recycle_render_entry(previous_entry);
                    }

                    let mesh_rid = rendering_server.mesh_create();
                    let arrays = cpu_mesh_to_surface_arrays(&mesh);
                    rendering_server.mesh_add_surface_from_arrays(
                        mesh_rid,
                        PrimitiveType::TRIANGLES,
                        &arrays,
                    );
                    let render_instance_rid = rendering_server.instance_create();
                    rendering_server.instance_set_base(render_instance_rid, mesh_rid);
                    rendering_server.instance_set_scenario(render_instance_rid, self.scenario_rid);
                    rendering_server.instance_set_transform(render_instance_rid, render_transform);
                    rendering_server.instance_set_visible(render_instance_rid, true);

                    let staging = self.ensure_commit_staging(
                        gd_staging.take(),
                        packed_regions.as_ref(),
                        &surface_class,
                    );
                    self.install_render_entry(
                        key,
                        mesh_rid,
                        render_instance_rid,
                        surface_class.clone(),
                        staging.clone(),
                    );
                    gd_staging = staging;
                    frame_state.phase8_render_cold_commits += 1;
                }
            }
        } else {
            match render_lifecycle {
                RenderLifecycleCommand::WarmReuseCurrent => {
                    frame_state.phase8_render_warm_current_commits += 1;
                }
                RenderLifecycleCommand::WarmReusePooled => {
                    if let Some(previous_entry) = self.take_current_render_entry(key) {
                        self.recycle_render_entry(previous_entry);
                    }
                    if let Some(entry) = pooled_render_entry {
                        self.install_render_entry(
                            key,
                            entry.mesh_rid,
                            entry.render_instance_rid,
                            surface_class.clone(),
                            gd_staging.take().or(entry.gd_staging),
                        );
                    }
                    frame_state.phase8_render_warm_pool_commits += 1;
                }
                RenderLifecycleCommand::ColdCreate(_) => {
                    if let Some(previous_entry) = self.take_current_render_entry(key) {
                        self.recycle_render_entry(previous_entry);
                    }
                    frame_state.phase8_render_cold_commits += 1;
                }
            }
        }

        let rid_state = self.ensure_rid_state(key);
        rid_state.render_resident = true;
        rid_state.active_surface_class = Some(surface_class.clone());
        rid_state.pooled_surface_class = Some(surface_class);
        if rid_state.gd_staging.is_none() {
            rid_state.gd_staging = gd_staging;
        }
    }

    fn commit_physics_payload(&mut self, key: ChunkKey, frame_state: &mut SelectionFrameState) {
        self.ensure_collision_payload(key);

        let Some(payload) = self.resident_payloads.get(&key) else {
            return;
        };
        let Some(collider_vertices) = payload.collider_vertices.clone() else {
            return;
        };
        let Some(collider_indices) = payload.collider_indices.clone() else {
            return;
        };
        let physics_transform = self.physics_transform_for_chunk(payload.chunk_origin_planet);

        let pooled_entry = self.pop_physics_pool_entry();
        let (physics_body_rid, physics_shape_rid) = match pooled_entry {
            Some(entry) => (entry.physics_body_rid, entry.physics_shape_rid),
            None => {
                if self.should_commit_to_servers() {
                    let mut physics_server = PhysicsServer3D::singleton();
                    let body_rid = physics_server.body_create();
                    physics_server.body_set_mode(body_rid, BodyMode::STATIC);
                    let shape_rid = physics_server.concave_polygon_shape_create();
                    (body_rid, shape_rid)
                } else {
                    (Rid::Invalid, Rid::Invalid)
                }
            }
        };

        if self.should_commit_to_servers() {
            let collider_faces = collider_faces_from_indices(&collider_vertices, &collider_indices);
            let mut shape_data = Dictionary::<StringName, Variant>::new();
            shape_data.set("faces", &collider_faces.to_variant());
            shape_data.set("backface_collision", false);
            let mut physics_server = PhysicsServer3D::singleton();
            physics_server.shape_set_data(physics_shape_rid, &shape_data.to_variant());
            physics_server.body_clear_shapes(physics_body_rid);
            physics_server.body_add_shape(physics_body_rid, physics_shape_rid);
            physics_server.body_set_state(
                physics_body_rid,
                BodyState::TRANSFORM,
                &physics_transform.to_variant(),
            );
            physics_server.body_set_space(physics_body_rid, self.physics_space_rid);
        }

        let rid_state = self.ensure_rid_state(key);
        rid_state.physics_body_rid = Some(physics_body_rid);
        rid_state.physics_shape_rid = Some(physics_shape_rid);
        rid_state.physics_resident = true;
        frame_state.phase8_physics_commits += 1;
    }

    fn deactivate_render_commit(&mut self, key: ChunkKey) {
        if let Some(entry) = self.take_current_render_entry(key) {
            self.recycle_render_entry(entry);
        }

        let rid_state = self.ensure_rid_state(key);
        rid_state.render_resident = false;
    }

    fn deactivate_physics_commit(&mut self, key: ChunkKey) {
        if let Some(entry) = self.take_current_physics_entry(key) {
            self.recycle_physics_entry(entry);
        }

        let rid_state = self.ensure_rid_state(key);
        rid_state.physics_resident = false;
    }

    fn current_render_rids(&mut self, key: ChunkKey) -> Option<(Rid, Rid)> {
        let rid_state = self.ensure_rid_state(key);
        match (rid_state.mesh_rid, rid_state.render_instance_rid) {
            (Some(mesh_rid), Some(render_instance_rid)) => Some((mesh_rid, render_instance_rid)),
            _ => None,
        }
    }

    fn take_current_render_entry(&mut self, key: ChunkKey) -> Option<RenderPoolEntry> {
        let rid_state = self.ensure_rid_state(key);
        let (Some(mesh_rid), Some(render_instance_rid), Some(surface_class)) = (
            rid_state.mesh_rid,
            rid_state.render_instance_rid,
            rid_state.active_surface_class.clone(),
        ) else {
            return None;
        };

        let gd_staging = rid_state.gd_staging.take();
        rid_state.mesh_rid = None;
        rid_state.render_instance_rid = None;
        rid_state.active_surface_class = None;
        rid_state.pooled_surface_class = None;
        rid_state.render_resident = false;

        Some(RenderPoolEntry {
            mesh_rid,
            render_instance_rid,
            surface_class,
            gd_staging,
        })
    }

    pub(crate) fn install_render_entry(
        &mut self,
        key: ChunkKey,
        mesh_rid: Rid,
        render_instance_rid: Rid,
        surface_class: SurfaceClassKey,
        gd_staging: Option<GdPackedStaging>,
    ) {
        let rid_state = self.ensure_rid_state(key);
        rid_state.mesh_rid = Some(mesh_rid);
        rid_state.render_instance_rid = Some(render_instance_rid);
        rid_state.active_surface_class = Some(surface_class.clone());
        rid_state.pooled_surface_class = Some(surface_class);
        rid_state.gd_staging = gd_staging;
        rid_state.render_resident = true;
    }

    pub(crate) fn recycle_render_entry(&mut self, entry: RenderPoolEntry) {
        if self.should_commit_to_servers() {
            let mut rendering_server = RenderingServer::singleton();
            rendering_server.instance_set_visible(entry.render_instance_rid, false);
        }

        let entries = self
            .render_pool
            .entry(entry.surface_class.clone())
            .or_default();
        if entries.len() < self.config.render_pool_watermark_per_class {
            entries.push_back(entry);
        } else {
            self.free_render_entry(entry);
        }
    }

    fn free_render_entry(&mut self, entry: RenderPoolEntry) {
        if self.should_commit_to_servers() {
            let mut rendering_server = RenderingServer::singleton();
            rendering_server.free_rid(entry.render_instance_rid);
            rendering_server.free_rid(entry.mesh_rid);
        }
    }

    fn take_current_physics_entry(&mut self, key: ChunkKey) -> Option<PhysicsPoolEntry> {
        let rid_state = self.ensure_rid_state(key);
        let (Some(physics_body_rid), Some(physics_shape_rid)) =
            (rid_state.physics_body_rid, rid_state.physics_shape_rid)
        else {
            return None;
        };

        rid_state.physics_body_rid = None;
        rid_state.physics_shape_rid = None;
        rid_state.physics_resident = false;

        Some(PhysicsPoolEntry {
            physics_body_rid,
            physics_shape_rid,
        })
    }

    pub(crate) fn recycle_physics_entry(&mut self, entry: PhysicsPoolEntry) {
        if self.should_commit_to_servers() {
            let mut physics_server = PhysicsServer3D::singleton();
            physics_server.body_set_space(entry.physics_body_rid, Rid::Invalid);
            physics_server.body_clear_shapes(entry.physics_body_rid);
        }

        if self.physics_pool.len() < self.config.physics_pool_watermark {
            self.physics_pool.push_back(entry);
        } else {
            self.free_physics_entry(entry);
        }
    }

    fn free_physics_entry(&mut self, entry: PhysicsPoolEntry) {
        if self.should_commit_to_servers() {
            let mut physics_server = PhysicsServer3D::singleton();
            physics_server.free_rid(entry.physics_body_rid);
            physics_server.free_rid(entry.physics_shape_rid);
        }
    }

    fn ensure_commit_staging(
        &self,
        staging: Option<GdPackedStaging>,
        packed_regions: Option<&PackedMeshRegions>,
        surface_class: &SurfaceClassKey,
    ) -> Option<GdPackedStaging> {
        let mut staging =
            staging.unwrap_or_else(|| GdPackedStaging::new_for_surface_class(surface_class));
        if let Some(packed_regions) = packed_regions {
            staging
                .copy_from_regions(packed_regions, surface_class)
                .ok()?;
        }
        Some(staging)
    }

    pub(crate) fn reclaim_payload_resources(&mut self, mut payload: ChunkPayload) {
        if let Some(entry) = payload.pooled_render_entry.take() {
            self.recycle_render_entry(entry);
        }
    }

    pub(crate) fn horizon_visible(&self, camera: &CameraState, meta: &ChunkMeta) -> bool {
        let camera_distance = camera.position_planet.length();
        let occluder_radius = self.config.planet_radius + self.config.horizon_safety_margin;

        if camera_distance <= occluder_radius {
            return true;
        }

        let beta = (occluder_radius / camera_distance).clamp(-1.0, 1.0).acos();
        let theta = camera
            .position_planet
            .normalize_or_zero()
            .angle_between(meta.bounds.center_planet.normalize_or_zero());

        theta <= beta + f64::from(meta.metrics.angular_radius)
    }

    pub(crate) fn frustum_visible(&self, camera: &CameraState, meta: &ChunkMeta) -> bool {
        let center =
            dvec3_to_vector3(meta.bounds.center_planet - camera.origin.render_origin_planet);
        let radius = meta.bounds.radius as f32;

        camera
            .frustum_planes
            .iter()
            .all(|plane| plane.distance_to(center) <= radius)
    }

    pub(crate) fn projected_error_px(&self, camera: &CameraState, meta: &ChunkMeta) -> f32 {
        let distance = self
            .chunk_camera_distance(camera, meta)
            .max(f64::from(f32::EPSILON));

        (f64::from(meta.metrics.geometric_error) * camera.projection_scale / distance) as f32
    }

    pub(crate) fn chunk_camera_distance(&self, camera: &CameraState, meta: &ChunkMeta) -> f64 {
        (meta.bounds.center_planet - camera.position_planet).length()
    }

    fn chunk_origin_planet_for_key(&self, key: ChunkKey) -> Option<DVec3> {
        self.resident_payloads
            .get(&key)
            .map(|payload| payload.chunk_origin_planet)
            .or_else(|| self.meta.get(&key).map(|meta| meta.bounds.center_planet))
    }

    fn render_transform_for_chunk(&self, chunk_origin_planet: DVec3) -> Transform3D {
        relative_transform(
            chunk_origin_planet,
            self.origin_snapshot.render_origin_planet,
        )
    }

    fn physics_transform_for_chunk(&self, chunk_origin_planet: DVec3) -> Transform3D {
        relative_transform(
            chunk_origin_planet,
            self.origin_snapshot.physics_origin_planet,
        )
    }

    pub(crate) fn rebind_active_relative_transforms(
        &mut self,
        frame_state: &mut SelectionFrameState,
    ) {
        let render_keys = self.active_render.iter().copied().collect::<Vec<_>>();
        let physics_keys = self.active_physics.iter().copied().collect::<Vec<_>>();

        if self.should_commit_to_servers() {
            let mut rendering_server = RenderingServer::singleton();
            for key in &render_keys {
                let Some(render_instance_rid) = self
                    .rid_state
                    .get(key)
                    .and_then(|state| state.render_instance_rid)
                else {
                    continue;
                };
                let Some(chunk_origin_planet) = self.chunk_origin_planet_for_key(*key) else {
                    continue;
                };
                rendering_server.instance_set_transform(
                    render_instance_rid,
                    self.render_transform_for_chunk(chunk_origin_planet),
                );
                frame_state.phase10_render_transform_rebinds += 1;
            }

            let mut physics_server = PhysicsServer3D::singleton();
            for key in &physics_keys {
                let Some(physics_body_rid) = self
                    .rid_state
                    .get(key)
                    .and_then(|state| state.physics_body_rid)
                else {
                    continue;
                };
                let Some(chunk_origin_planet) = self.chunk_origin_planet_for_key(*key) else {
                    continue;
                };
                physics_server.body_set_state(
                    physics_body_rid,
                    BodyState::TRANSFORM,
                    &self
                        .physics_transform_for_chunk(chunk_origin_planet)
                        .to_variant(),
                );
                frame_state.phase10_physics_transform_rebinds += 1;
            }
        } else {
            for key in render_keys {
                if self.chunk_origin_planet_for_key(key).is_some() {
                    frame_state.phase10_render_transform_rebinds += 1;
                }
            }
            for key in physics_keys {
                if self.chunk_origin_planet_for_key(key).is_some() {
                    frame_state.phase10_physics_transform_rebinds += 1;
                }
            }
        }
    }

    pub fn release_server_resources(&mut self) {
        if self.should_commit_to_servers() {
            let mut rendering_server = RenderingServer::singleton();
            let mut physics_server = PhysicsServer3D::singleton();

            for payload in self.resident_payloads.values_mut() {
                if let Some(entry) = payload.pooled_render_entry.take() {
                    rendering_server.free_rid(entry.render_instance_rid);
                    rendering_server.free_rid(entry.mesh_rid);
                }
            }

            for rid_state in self.rid_state.values_mut() {
                if let Some(render_instance_rid) = rid_state.render_instance_rid.take() {
                    rendering_server.instance_set_visible(render_instance_rid, false);
                    rendering_server.free_rid(render_instance_rid);
                }
                if let Some(mesh_rid) = rid_state.mesh_rid.take() {
                    rendering_server.free_rid(mesh_rid);
                }
                if let Some(physics_body_rid) = rid_state.physics_body_rid.take() {
                    physics_server.body_set_space(physics_body_rid, Rid::Invalid);
                    physics_server.free_rid(physics_body_rid);
                }
                if let Some(physics_shape_rid) = rid_state.physics_shape_rid.take() {
                    physics_server.free_rid(physics_shape_rid);
                }

                rid_state.active_surface_class = None;
                rid_state.pooled_surface_class = None;
                rid_state.gd_staging = None;
                rid_state.render_resident = false;
                rid_state.physics_resident = false;
            }

            for (_, entries) in self.render_pool.drain() {
                for entry in entries {
                    rendering_server.free_rid(entry.render_instance_rid);
                    rendering_server.free_rid(entry.mesh_rid);
                }
            }
            for entry in self.physics_pool.drain(..) {
                physics_server.body_set_space(entry.physics_body_rid, Rid::Invalid);
                physics_server.free_rid(entry.physics_body_rid);
                physics_server.free_rid(entry.physics_shape_rid);
            }
        } else {
            for payload in self.resident_payloads.values_mut() {
                payload.pooled_render_entry = None;
            }
            for rid_state in self.rid_state.values_mut() {
                rid_state.mesh_rid = None;
                rid_state.render_instance_rid = None;
                rid_state.physics_body_rid = None;
                rid_state.physics_shape_rid = None;
                rid_state.active_surface_class = None;
                rid_state.pooled_surface_class = None;
                rid_state.gd_staging = None;
                rid_state.render_resident = false;
                rid_state.physics_resident = false;
            }
            self.render_pool.clear();
            self.physics_pool.clear();
        }

        self.active_render.clear();
        self.active_physics.clear();
    }
}
