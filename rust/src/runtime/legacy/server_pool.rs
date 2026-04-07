use super::super::*;
use godot::classes::rendering_server::PrimitiveType;

impl PlanetRuntime {
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

    pub(crate) fn deactivate_render_commit_with_server_backend(&mut self, key: ChunkKey) {
        if let Some(entry) = self.take_current_render_entry(key) {
            self.recycle_render_entry(entry);
        }

        let rid_state = self.ensure_rid_state(key);
        rid_state.render_resident = false;
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
        } else if let Some(previous_entry) = self.take_current_render_entry(key) {
            self.recycle_render_entry(previous_entry);
        }

        frame_state.phase8_render_cold_commits += 1;
        true
    }
}
