use super::super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CommitOp {
    kind: CommitOpKind,
    key: ChunkKey,
    upload_bytes: usize,
    priority_group: u8,
    distance_key_mm: u64,
}

#[derive(Default)]
struct CommitBudgetUsage {
    render_activations: usize,
    render_updates: usize,
    render_deactivations: usize,
    physics_activations: usize,
    physics_deactivations: usize,
}

struct DeactivationCoverageTracker {
    candidates: Vec<ChunkKey>,
    blocking_counts: HashMap<ChunkKey, usize>,
}

impl DeactivationCoverageTracker {
    fn new(
        candidates: Vec<ChunkKey>,
        desired: &HashSet<ChunkKey>,
        ready: &HashSet<ChunkKey>,
    ) -> Self {
        let mut blocking_counts = candidates
            .iter()
            .copied()
            .map(|key| (key, 0usize))
            .collect::<HashMap<_, _>>();

        if !candidates.is_empty() {
            for desired_key in desired.iter().copied().filter(|key| !ready.contains(key)) {
                for candidate in &candidates {
                    if keys_intersect_hierarchically(*candidate, desired_key) {
                        *blocking_counts
                            .get_mut(candidate)
                            .expect("coverage tracker candidate must exist") += 1;
                    }
                }
            }
        }

        Self {
            candidates,
            blocking_counts,
        }
    }

    fn would_open_hole(&self, key: ChunkKey) -> bool {
        self.blocking_counts.get(&key).copied().unwrap_or(0) > 0
    }

    fn mark_ready(&mut self, ready_key: ChunkKey) {
        for candidate in &self.candidates {
            if !keys_intersect_hierarchically(*candidate, ready_key) {
                continue;
            }

            if let Some(count) = self.blocking_counts.get_mut(candidate) {
                *count = count.saturating_sub(1);
            }
        }
    }
}

fn keys_intersect_hierarchically(a: ChunkKey, b: ChunkKey) -> bool {
    a.face == b.face && (a.is_descendant_of(&b) || b.is_descendant_of(&a))
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
        let mut ready_render = self
            .active_render
            .intersection(desired_render)
            .copied()
            .collect::<HashSet<_>>();
        let mut ready_physics = self
            .active_physics
            .intersection(desired_physics)
            .copied()
            .collect::<HashSet<_>>();
        let render_deactivation_keys = ops
            .iter()
            .filter_map(|op| match op.kind {
                CommitOpKind::DeactivateRender => Some(op.key),
                _ => None,
            })
            .collect::<Vec<_>>();
        let physics_deactivation_keys = ops
            .iter()
            .filter_map(|op| match op.kind {
                CommitOpKind::DeactivatePhysics => Some(op.key),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut render_deactivation_coverage = DeactivationCoverageTracker::new(
            render_deactivation_keys,
            desired_render,
            &ready_render,
        );
        let mut physics_render_coverage = DeactivationCoverageTracker::new(
            physics_deactivation_keys.clone(),
            desired_render,
            &ready_render,
        );
        let mut physics_deactivation_coverage = DeactivationCoverageTracker::new(
            physics_deactivation_keys,
            desired_physics,
            &ready_physics,
        );

        let mut committed = 0usize;
        let mut upload_bytes_committed = 0usize;
        let mut deferred_upload_bytes = 0usize;
        let mut deferred_now = HashSet::new();
        let mut budget_usage = CommitBudgetUsage::default();

        for op in ops {
            let over_commit_budget = committed >= self.config.commit_budget_per_frame;
            let over_upload_budget = op.upload_bytes > 0
                && upload_bytes_committed + op.upload_bytes
                    > self.config.upload_budget_bytes_per_frame;
            let over_kind_budget = match op.kind {
                CommitOpKind::ActivateRender => {
                    budget_usage.render_activations
                        >= self.config.render_activation_budget_per_frame
                }
                CommitOpKind::UpdateRender => {
                    budget_usage.render_updates >= self.config.render_update_budget_per_frame
                }
                CommitOpKind::DeactivateRender => {
                    budget_usage.render_deactivations
                        >= self.config.render_deactivation_budget_per_frame
                }
                CommitOpKind::ActivatePhysics => {
                    budget_usage.physics_activations
                        >= self.config.physics_activation_budget_per_frame
                }
                CommitOpKind::DeactivatePhysics => {
                    budget_usage.physics_deactivations
                        >= self.config.physics_deactivation_budget_per_frame
                }
            };

            let deactivation_would_open_hole = match op.kind {
                CommitOpKind::DeactivateRender => {
                    render_deactivation_coverage.would_open_hole(op.key)
                }
                CommitOpKind::DeactivatePhysics => {
                    physics_render_coverage.would_open_hole(op.key)
                        || physics_deactivation_coverage.would_open_hole(op.key)
                }
                _ => false,
            };

            if over_commit_budget
                || over_upload_budget
                || over_kind_budget
                || deactivation_would_open_hole
            {
                deferred_upload_bytes += op.upload_bytes;
                deferred_now.insert(DeferredOpKey::new(op.kind, op.key));
                continue;
            }

            let committed_successfully = self.apply_commit_op(op, frame_state);
            committed += 1;
            upload_bytes_committed += op.upload_bytes;
            match op.kind {
                CommitOpKind::ActivateRender => budget_usage.render_activations += 1,
                CommitOpKind::UpdateRender => budget_usage.render_updates += 1,
                CommitOpKind::DeactivateRender => budget_usage.render_deactivations += 1,
                CommitOpKind::ActivatePhysics => budget_usage.physics_activations += 1,
                CommitOpKind::DeactivatePhysics => budget_usage.physics_deactivations += 1,
            }
            if committed_successfully {
                match op.kind {
                    CommitOpKind::ActivateRender => {
                        if desired_render.contains(&op.key) {
                            ready_render.insert(op.key);
                            render_deactivation_coverage.mark_ready(op.key);
                            physics_render_coverage.mark_ready(op.key);
                        }
                    }
                    CommitOpKind::ActivatePhysics => {
                        if desired_physics.contains(&op.key) {
                            ready_physics.insert(op.key);
                            physics_deactivation_coverage.mark_ready(op.key);
                        }
                    }
                    _ => {}
                }
            }
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
        self.sync_asset_groups(frame_state)?;
        frame_state.render_pool_entries = self.render_pool_entry_count();
        frame_state.physics_pool_entries = self.physics_pool.len();
        let asset_debug = self.asset_debug_snapshot();
        frame_state.phase12_active_groups = asset_debug.active_groups;
        frame_state.phase12_active_instances = asset_debug.active_instances;

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
        self.request_render_payloads_for_selection(&refresh_keys, desired_render, frame_state)?;
        self.drain_ready_render_payloads(frame_state);

        for key in render_activations {
            if !self.payload_matches_selection(key, desired_render)? {
                continue;
            }
            ops.push(CommitOp {
                kind: CommitOpKind::ActivateRender,
                key,
                upload_bytes: self.payload_upload_bytes(key),
                priority_group: 0,
                distance_key_mm: distance_sort_key(
                    self.chunk_camera_distance_for_key(camera, key)?,
                ),
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
            if !self.payload_matches_selection(key, desired_render)? {
                continue;
            }

            ops.push(CommitOp {
                kind: CommitOpKind::UpdateRender,
                key,
                upload_bytes: self.payload_upload_bytes(key),
                priority_group: 1,
                distance_key_mm: distance_sort_key(
                    self.chunk_camera_distance_for_key(camera, key)?,
                ),
            });
        }

        let mut physics_activations = desired_physics
            .difference(&self.active_physics)
            .copied()
            .collect::<Vec<_>>();
        physics_activations.sort_unstable();
        for key in physics_activations {
            if !self.payload_matches_selection(key, desired_render)? {
                continue;
            }
            ops.push(CommitOp {
                kind: CommitOpKind::ActivatePhysics,
                key,
                upload_bytes: 0,
                priority_group: 2,
                distance_key_mm: distance_sort_key(
                    self.chunk_camera_distance_for_key(camera, key)?,
                ),
            });
        }

        let mut render_deactivations = self
            .active_render
            .difference(desired_render)
            .copied()
            .collect::<Vec<_>>();
        render_deactivations.sort_unstable();
        for key in render_deactivations {
            ops.push(CommitOp {
                kind: CommitOpKind::DeactivateRender,
                key,
                upload_bytes: 0,
                priority_group: 3,
                distance_key_mm: distance_sort_key(
                    self.chunk_camera_distance_for_key(camera, key)?,
                ),
            });
        }

        let mut physics_deactivations = self
            .active_physics
            .difference(desired_physics)
            .copied()
            .collect::<Vec<_>>();
        physics_deactivations.sort_unstable();
        for key in physics_deactivations {
            ops.push(CommitOp {
                kind: CommitOpKind::DeactivatePhysics,
                key,
                upload_bytes: 0,
                priority_group: 4,
                distance_key_mm: distance_sort_key(
                    self.chunk_camera_distance_for_key(camera, key)?,
                ),
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

    fn apply_commit_op(&mut self, op: CommitOp, frame_state: &mut SelectionFrameState) -> bool {
        match op.kind {
            CommitOpKind::ActivateRender => {
                if self.commit_render_payload(op.key, frame_state) {
                    self.active_render.insert(op.key);
                    self.ensure_rid_state(op.key).render_resident = true;
                    self.asset_groups_dirty = true;
                    true
                } else {
                    self.active_render.remove(&op.key);
                    self.ensure_rid_state(op.key).render_resident = false;
                    self.asset_groups_dirty = true;
                    false
                }
            }
            CommitOpKind::UpdateRender => {
                if self.commit_render_payload(op.key, frame_state) {
                    self.active_render.insert(op.key);
                    self.ensure_rid_state(op.key).render_resident = true;
                    self.asset_groups_dirty = true;
                    true
                } else {
                    let still_has_render_entry = self
                        .rid_state
                        .get(&op.key)
                        .and_then(|state| state.render_instance_rid)
                        .is_some();
                    if still_has_render_entry {
                        self.active_render.insert(op.key);
                        self.ensure_rid_state(op.key).render_resident = true;
                        self.asset_groups_dirty = true;
                        true
                    } else {
                        self.active_render.remove(&op.key);
                        self.ensure_rid_state(op.key).render_resident = false;
                        self.asset_groups_dirty = true;
                        false
                    }
                }
            }
            CommitOpKind::DeactivateRender => {
                self.deactivate_render_commit(op.key);
                self.active_render.remove(&op.key);
                self.ensure_rid_state(op.key).render_resident = false;
                self.asset_groups_dirty = true;
                true
            }
            CommitOpKind::ActivatePhysics => {
                if self.commit_physics_payload(op.key, frame_state) {
                    self.active_physics.insert(op.key);
                    self.ensure_rid_state(op.key).physics_resident = true;
                    true
                } else {
                    self.active_physics.remove(&op.key);
                    self.ensure_rid_state(op.key).physics_resident = false;
                    false
                }
            }
            CommitOpKind::DeactivatePhysics => {
                self.deactivate_physics_commit(op.key);
                self.active_physics.remove(&op.key);
                self.ensure_rid_state(op.key).physics_resident = false;
                true
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
    ) -> bool {
        let backend = self.config.render_backend;
        backend.commit_render_payload(self, key, frame_state)
    }

    pub(crate) fn commit_render_payload_with_server_backend(
        &mut self,
        key: ChunkKey,
        frame_state: &mut SelectionFrameState,
    ) -> bool {
        let (
            surface_class,
            chunk_origin_planet,
            render_lifecycle,
            mut gd_staging,
            pooled_render_entry,
        ) = {
            let Some(payload) = self.resident_payloads.get_mut(&key) else {
                return false;
            };
            (
                payload.surface_class.clone(),
                payload.chunk_origin_planet,
                payload.render_lifecycle.clone(),
                payload.gd_staging.take(),
                payload.pooled_render_entry.take(),
            )
        };

        let render_transform = self.render_transform_for_chunk(chunk_origin_planet);

        if self.should_commit_to_servers() {
            let mut rendering_server = RenderingServer::singleton();
            match render_lifecycle {
                RenderLifecycleCommand::WarmReuseCurrent => {
                    let Some((mesh_rid, render_instance_rid)) = self.current_render_rids(key)
                    else {
                        return self.commit_render_payload_cold(
                            key,
                            surface_class,
                            gd_staging.take(),
                            render_transform,
                            frame_state,
                        );
                    };
                    let arrays = {
                        let Some(payload) = self.resident_payloads.get(&key) else {
                            return false;
                        };
                        cpu_mesh_to_surface_arrays(&payload.mesh)
                    };
                    let Some(staging) = self.ensure_commit_staging(
                        gd_staging.take(),
                        self.resident_payloads
                            .get(&key)
                            .and_then(|payload| payload.packed_regions.as_ref()),
                        &surface_class,
                    ) else {
                        return false;
                    };

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
                        return self.commit_render_payload_cold(
                            key,
                            surface_class,
                            gd_staging.take(),
                            render_transform,
                            frame_state,
                        );
                    };
                    if let Some(previous_entry) = self.take_current_render_entry(key) {
                        self.recycle_render_entry(previous_entry);
                    }

                    let Some(staging) = self.ensure_commit_staging(
                        gd_staging.take().or(entry.gd_staging),
                        self.resident_payloads
                            .get(&key)
                            .and_then(|payload| payload.packed_regions.as_ref()),
                        &surface_class,
                    ) else {
                        return false;
                    };
                    let arrays = {
                        let Some(payload) = self.resident_payloads.get(&key) else {
                            return false;
                        };
                        cpu_mesh_to_surface_arrays(&payload.mesh)
                    };

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
                    return self.commit_render_payload_cold(
                        key,
                        surface_class,
                        gd_staging.take(),
                        render_transform,
                        frame_state,
                    );
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
        true
    }

    fn commit_render_payload_cold(
        &mut self,
        key: ChunkKey,
        surface_class: SurfaceClassKey,
        staging: Option<GdPackedStaging>,
        render_transform: Transform3D,
        frame_state: &mut SelectionFrameState,
    ) -> bool {
        if self.should_commit_to_servers() {
            if let Some(previous_entry) = self.take_current_render_entry(key) {
                self.recycle_render_entry(previous_entry);
            }

            let mut rendering_server = RenderingServer::singleton();
            let mesh_rid = rendering_server.mesh_create();
            let arrays = {
                let Some(payload) = self.resident_payloads.get(&key) else {
                    return false;
                };
                cpu_mesh_to_surface_arrays(&payload.mesh)
            };
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
                staging,
                self.resident_payloads
                    .get(&key)
                    .and_then(|payload| payload.packed_regions.as_ref()),
                &surface_class,
            );
            self.install_render_entry(key, mesh_rid, render_instance_rid, surface_class, staging);
        } else {
            if let Some(previous_entry) = self.take_current_render_entry(key) {
                self.recycle_render_entry(previous_entry);
            }
        }

        frame_state.phase8_render_cold_commits += 1;
        true
    }

    fn commit_physics_payload(
        &mut self,
        key: ChunkKey,
        frame_state: &mut SelectionFrameState,
    ) -> bool {
        self.ensure_collision_payload(key);

        let physics_transform = {
            let Some(payload) = self.resident_payloads.get(&key) else {
                return false;
            };
            self.physics_transform_for_chunk(payload.chunk_origin_planet)
        };

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
            let collider_faces = {
                let Some(payload) = self.resident_payloads.get(&key) else {
                    return false;
                };
                let Some(collider_faces) = payload.collider_faces.as_deref() else {
                    return false;
                };
                PackedVector3Array::from_iter(
                    collider_faces
                        .iter()
                        .copied()
                        .map(|position| Vector3::new(position[0], position[1], position[2])),
                )
            };
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
        true
    }

    fn deactivate_render_commit(&mut self, key: ChunkKey) {
        let backend = self.config.render_backend;
        backend.deactivate_render(self, key);
    }

    pub(crate) fn deactivate_render_commit_with_server_backend(&mut self, key: ChunkKey) {
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
        let policy = self.config.staging_policy;
        policy.fill_staging_from_payload(staging, packed_regions, surface_class)
    }

    pub(crate) fn reclaim_payload_resources(&mut self, mut payload: ChunkPayload) {
        if let Some(entry) = payload.pooled_render_entry.take() {
            self.recycle_render_entry(entry);
        }
    }

    pub(crate) fn sync_asset_groups(
        &mut self,
        frame_state: &mut SelectionFrameState,
    ) -> Result<(), TopologyError> {
        if self.asset_groups_dirty {
            self.submit_desired_asset_groups_request();
            self.asset_groups_dirty = false;
        }

        let mut ready_groups = None;
        for prepared in self.threaded_asset_group_generator.drain_ready() {
            if self.pending_asset_group_epoch == Some(prepared.epoch) {
                ready_groups = Some(prepared.groups);
                self.pending_asset_group_epoch = None;
            }
        }

        let Some(desired) = ready_groups else {
            frame_state.phase12_active_groups = self.asset_groups.len();
            frame_state.phase12_active_instances = self
                .asset_groups
                .values()
                .map(|group| group.instance_count)
                .sum();
            return Ok(());
        };

        let desired_keys = desired.keys().copied().collect::<HashSet<_>>();
        let current_keys = self.asset_groups.keys().copied().collect::<Vec<_>>();

        for key in current_keys {
            if !desired_keys.contains(&key) {
                self.deactivate_asset_group(key);
            }
        }

        let mut desired_groups = desired.into_values().collect::<Vec<_>>();
        desired_groups.sort_by_key(|group| group.key);
        for group in desired_groups {
            self.commit_asset_group(group, frame_state)?;
        }

        self.rebuild_chunk_asset_rid_links();
        Ok(())
    }

    fn submit_desired_asset_groups_request(&mut self) {
        let epoch = self.next_asset_group_epoch;
        self.next_asset_group_epoch = self.next_asset_group_epoch.saturating_add(1);
        self.pending_asset_group_epoch = Some(epoch);

        let mut chunks = Vec::with_capacity(self.active_render.len());
        let mut anchor_origins = HashMap::new();
        for key in self.active_render.iter().copied() {
            let Some(payload) = self.resident_payloads.get(&key) else {
                continue;
            };
            if payload.assets.is_empty() {
                continue;
            }

            chunks.push(super::super::workers::asset_groups::AssetGroupChunkInput {
                key,
                chunk_origin_planet: self
                    .meta
                    .center_planet(&key)
                    .unwrap_or(payload.chunk_origin_planet),
                assets: payload.assets.clone(),
            });

            for family in payload.assets.iter().map(|asset| asset.family_id) {
                let group_key =
                    asset_group_key_for_chunk(key, family, self.config.asset_group_chunk_span);
                let anchor_key =
                    asset_group_anchor_key(group_key, self.config.asset_group_chunk_span);
                if let Some(origin) = self.meta.center_planet(&anchor_key) {
                    anchor_origins.insert(anchor_key, origin);
                }
            }
        }

        self.threaded_asset_group_generator
            .submit_latest(DesiredAssetGroupsBuildRequest {
                epoch,
                config: self.config.clone(),
                chunks,
                anchor_origins,
            });
    }

    fn commit_asset_group(
        &mut self,
        group: DesiredAssetGroup,
        frame_state: &mut SelectionFrameState,
    ) -> Result<(), TopologyError> {
        let DesiredAssetGroup {
            key,
            group_origin_planet,
            source_chunks,
            assets,
            local_bounds,
        } = group;

        if assets.is_empty() {
            self.deactivate_asset_group(key);
            return Ok(());
        }

        let (multimesh_rid, render_instance_rid) = {
            let existing = self.asset_groups.get(&key).cloned();
            match existing {
                Some(existing) => (existing.multimesh_rid, existing.render_instance_rid),
                None => (None, None),
            }
        };

        let mesh_rid = self.ensure_asset_family_mesh_rid(key.family_id);
        if self.should_commit_to_servers() {
            let mut rendering_server = RenderingServer::singleton();
            let multimesh_rid =
                multimesh_rid.unwrap_or_else(|| rendering_server.multimesh_create());
            rendering_server.multimesh_allocate_data(
                multimesh_rid,
                assets.len() as i32,
                MultimeshTransformFormat::TRANSFORM_3D,
            );
            rendering_server.multimesh_set_mesh(multimesh_rid, mesh_rid);
            if let Some(bounds) = local_bounds {
                rendering_server.multimesh_set_custom_aabb(
                    multimesh_rid,
                    Aabb::new(
                        Vector3::new(bounds.min[0], bounds.min[1], bounds.min[2]),
                        Vector3::new(bounds.size[0], bounds.size[1], bounds.size[2]),
                    ),
                );
            }
            for (index, asset) in assets.iter().enumerate() {
                rendering_server.multimesh_instance_set_transform(
                    multimesh_rid,
                    index as i32,
                    self.asset_local_transform(group_origin_planet, asset),
                );
            }
            rendering_server.multimesh_set_visible_instances(multimesh_rid, assets.len() as i32);

            let render_instance_rid =
                render_instance_rid.unwrap_or_else(|| rendering_server.instance_create());
            rendering_server.instance_set_base(render_instance_rid, multimesh_rid);
            rendering_server.instance_set_scenario(render_instance_rid, self.scenario_rid);
            rendering_server.instance_set_transform(
                render_instance_rid,
                self.render_transform_for_chunk(group_origin_planet),
            );
            rendering_server.instance_set_visible(render_instance_rid, true);

            self.asset_groups.insert(
                key,
                AssetGroupState {
                    key,
                    group_origin_planet,
                    source_chunks,
                    instance_count: assets.len(),
                    multimesh_rid: Some(multimesh_rid),
                    render_instance_rid: Some(render_instance_rid),
                },
            );
        } else {
            self.asset_groups.insert(
                key,
                AssetGroupState {
                    key,
                    group_origin_planet,
                    source_chunks,
                    instance_count: assets.len(),
                    multimesh_rid: None,
                    render_instance_rid: None,
                },
            );
        }

        frame_state.phase12_active_groups = self.asset_groups.len();
        frame_state.phase12_active_instances = self.active_asset_instance_count();
        Ok(())
    }

    fn deactivate_asset_group(&mut self, key: AssetGroupKey) {
        let Some(state) = self.asset_groups.remove(&key) else {
            return;
        };

        if self.should_commit_to_servers() {
            let mut rendering_server = RenderingServer::singleton();
            if let Some(render_instance_rid) = state.render_instance_rid {
                rendering_server.instance_set_visible(render_instance_rid, false);
                rendering_server.free_rid(render_instance_rid);
            }
            if let Some(multimesh_rid) = state.multimesh_rid {
                rendering_server.free_rid(multimesh_rid);
            }
        }
    }

    fn ensure_asset_family_mesh_rid(&mut self, family_id: u16) -> Rid {
        if let Some(mesh_rid) = self.asset_family_meshes.get(&family_id).copied() {
            return mesh_rid;
        }

        let family = ASSET_FAMILY_DEFINITIONS
            .iter()
            .copied()
            .find(|family| family.family_id == family_id)
            .unwrap_or(ASSET_FAMILY_DEFINITIONS[0]);

        let mesh_rid = if self.should_commit_to_servers() {
            let mut rendering_server = RenderingServer::singleton();
            let mesh_rid = rendering_server.mesh_create();
            let mesh = build_asset_mesh(family);
            rendering_server.mesh_add_surface_from_arrays(
                mesh_rid,
                PrimitiveType::TRIANGLES,
                &cpu_mesh_to_surface_arrays(&mesh),
            );
            mesh_rid
        } else {
            Rid::Invalid
        };

        self.asset_family_meshes.insert(family_id, mesh_rid);
        mesh_rid
    }

    fn rebuild_chunk_asset_rid_links(&mut self) {
        for rid_state in self.rid_state.values_mut() {
            rid_state.asset_multimesh_rids.clear();
            rid_state.asset_instance_rids.clear();
        }

        let groups = self.asset_groups.values().cloned().collect::<Vec<_>>();
        for state in groups {
            for key in &state.source_chunks {
                let rid_state = self.ensure_rid_state(*key);
                if let Some(multimesh_rid) = state.multimesh_rid {
                    rid_state.asset_multimesh_rids.push(multimesh_rid);
                }
                if let Some(render_instance_rid) = state.render_instance_rid {
                    rid_state.asset_instance_rids.push(render_instance_rid);
                }
            }
        }
    }

    fn asset_local_transform(
        &self,
        group_origin_planet: DVec3,
        asset: &AssetInstance,
    ) -> Transform3D {
        let basis = Basis::from_rows(
            dvec3_to_vector3(asset.basis_x * f64::from(asset.scale)),
            dvec3_to_vector3(asset.basis_y * f64::from(asset.scale)),
            dvec3_to_vector3(asset.basis_z * f64::from(asset.scale)),
        );
        Transform3D::new(basis, dvec3_to_vector3(asset.origin - group_origin_planet))
    }

    pub(crate) fn horizon_visible(&self, camera: &CameraState, meta: &ChunkMeta) -> bool {
        self.config
            .visibility_strategy
            .horizon_visible(&self.config, camera, meta)
    }

    pub(crate) fn frustum_visible(&self, camera: &CameraState, meta: &ChunkMeta) -> bool {
        self.config
            .visibility_strategy
            .frustum_visible(&self.config, camera, meta)
    }

    pub(crate) fn projected_error_px(&self, camera: &CameraState, meta: &ChunkMeta) -> f32 {
        self.config
            .visibility_strategy
            .screen_error_px(camera, meta)
    }

    pub(crate) fn chunk_camera_distance(&self, camera: &CameraState, meta: &ChunkMeta) -> f64 {
        (meta.bounds.center_planet - camera.position_planet).length()
    }

    fn chunk_camera_distance_for_key(
        &mut self,
        camera: &CameraState,
        key: ChunkKey,
    ) -> Result<f64, TopologyError> {
        if let Some(center_planet) = self.meta.center_planet(&key) {
            return Ok((center_planet - camera.position_planet).length());
        }

        let meta = self.ensure_chunk_meta(key)?;
        Ok(self.chunk_camera_distance(camera, &meta))
    }

    fn chunk_origin_planet_for_key(&self, key: ChunkKey) -> Option<DVec3> {
        self.resident_payloads
            .get(&key)
            .map(|payload| payload.chunk_origin_planet)
            .or_else(|| self.meta.center_planet(&key))
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
        let asset_groups = self.asset_groups.values().cloned().collect::<Vec<_>>();

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
            for group in &asset_groups {
                let Some(render_instance_rid) = group.render_instance_rid else {
                    continue;
                };
                rendering_server.instance_set_transform(
                    render_instance_rid,
                    self.render_transform_for_chunk(group.group_origin_planet),
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
            frame_state.phase10_render_transform_rebinds += asset_groups.len();
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
                rid_state.asset_multimesh_rids.clear();
                rid_state.asset_instance_rids.clear();
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
            for (_, state) in self.asset_groups.drain() {
                if let Some(render_instance_rid) = state.render_instance_rid {
                    rendering_server.instance_set_visible(render_instance_rid, false);
                    rendering_server.free_rid(render_instance_rid);
                }
                if let Some(multimesh_rid) = state.multimesh_rid {
                    rendering_server.free_rid(multimesh_rid);
                }
            }
            for (_, mesh_rid) in self.asset_family_meshes.drain() {
                if mesh_rid != Rid::Invalid {
                    rendering_server.free_rid(mesh_rid);
                }
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
                rid_state.asset_multimesh_rids.clear();
                rid_state.asset_instance_rids.clear();
                rid_state.render_resident = false;
                rid_state.physics_resident = false;
            }
            self.render_pool.clear();
            self.physics_pool.clear();
            self.asset_groups.clear();
            self.asset_family_meshes.clear();
        }

        self.active_render.clear();
        self.active_physics.clear();
    }
}
