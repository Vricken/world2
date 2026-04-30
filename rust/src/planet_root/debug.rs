use super::super::*;
use super::config::runtime_camera_near_clip_for_far_clip;
use glam::DVec3;

impl PlanetRoot {
    pub(crate) fn ensure_runtime_debug_hud(&mut self) {
        if !self.debug_show_runtime_hud {
            self.teardown_runtime_debug_hud();
            return;
        }
        if self.runtime_debug_label.is_some() {
            return;
        }

        let mut canvas = CanvasLayer::new_alloc();
        canvas.set_name(RUNTIME_DEBUG_HUD_NODE_NAME);
        canvas.set_layer(128);

        let mut label = Label::new_alloc();
        label.set_name(RUNTIME_DEBUG_LABEL_NODE_NAME);
        label.set_position(Vector2::new(12.0, 12.0));
        label.set_text("World2 debug HUD initializing...");

        canvas.add_child(&label);
        self.base_mut().add_child(&canvas);
        self.runtime_debug_label = Some(label);
        self.runtime_debug_canvas = Some(canvas);
    }

    pub(crate) fn update_runtime_debug_hud(&mut self) {
        if !self.debug_show_runtime_hud {
            self.teardown_runtime_debug_hud();
            return;
        }
        self.ensure_runtime_debug_hud();

        let text = self.runtime_debug_hud_text();
        if let Some(label) = self.runtime_debug_label.as_mut() {
            label.set_text(&text);
        }
    }

    pub(crate) fn teardown_runtime_debug_hud(&mut self) {
        self.runtime_debug_label = None;
        if let Some(mut canvas) = self.runtime_debug_canvas.take() {
            canvas.queue_free();
        }
    }

    fn runtime_debug_hud_text(&self) -> String {
        let Some(raw) = self.acquire_raw_camera_state() else {
            return "World2 debug HUD\nactive Camera3D: missing".to_string();
        };
        let Some(camera) = self.active_camera_3d() else {
            return "World2 debug HUD\nactive Camera3D: missing".to_string();
        };

        let origin = self.runtime.origin_snapshot();
        let camera_scene = raw.transform.origin;
        let camera_scene_d = vector3_to_dvec3_local(camera_scene);
        let camera_planet = camera_scene_d + origin.render_origin_planet;
        let planet_center_scene = -origin.render_origin_planet;
        let target_far = self.runtime_camera_far_clip_for_distance(camera_planet.length());
        let target_near = runtime_camera_near_clip_for_far_clip(target_far);
        let current_far = f64::from(camera.get_far());
        let current_near = f64::from(camera.get_near());
        let far_near_ratio = current_far / current_near.max(1.0e-6);
        let terrain = self.runtime.config.terrain_settings();
        let terrain_probe = if camera_planet.length_squared() > f64::EPSILON {
            let unit_dir = camera_planet.normalize();
            let sample = terrain.sample(unit_dir);
            let surface_radius = self.runtime.config.planet_radius + sample.height;
            let camera_altitude = camera_planet.length() - surface_radius;
            format!(
                "terrain radius={:.1} amp={:.1} sea={:.1} water_r={:.1}\n\
                 nadir height={:.1} above_sea={:.1} land={:.2} moisture={:.2} camera_alt={:.1}\n\
                 hills={:.2} mountains={:.2} mountain_freq={:.2}",
                self.runtime.config.planet_radius,
                self.runtime.config.height_amplitude,
                self.runtime.config.sea_level_meters,
                self.runtime.config.planet_radius
                    + self.runtime.config.sea_level_meters
                    + self.runtime.config.height_amplitude.max(1.0) * 0.01,
                sample.height,
                sample.height - self.runtime.config.sea_level_meters,
                sample.land_mask,
                sample.moisture,
                camera_altitude,
                self.runtime.config.hill_strength,
                self.runtime.config.mountain_strength,
                self.runtime.config.mountain_frequency
            )
        } else {
            "terrain probe unavailable: camera at planet center".to_string()
        };
        let viewport_size = self
            .base()
            .get_viewport()
            .map(|viewport| viewport.get_visible_rect().size)
            .unwrap_or(Vector2::ZERO);
        let frame = self.runtime.frame_state();

        format!(
            concat!(
                "World2 debug HUD\n",
                "tick={} viewport={:.0}x{:.0} rebases={} recenter_m={:.1}\n",
                "camera_scene={} dist_scene_origin={:.3}m\n",
                "origin_planet={} planet_center_scene={}\n",
                "camera_planet={} dist_planet_center={:.3}m\n",
                "{}\n",
                "camera near={:.4} target_near={:.4} far={:.3} target_far={:.3} far/near={:.1}\n",
                "render desired={} active={} deferred_ops={} deferred_upload={:.3}MiB\n",
                "origin_mode={} large_world_coordinates={}"
            ),
            self.runtime_tick_count,
            viewport_size.x,
            viewport_size.y,
            self.debug_origin_rebase_count,
            self.runtime.config.origin_recenter_distance,
            format_vector3(camera_scene),
            camera_scene_d.length(),
            format_dvec3(origin.render_origin_planet),
            format_dvec3(planet_center_scene),
            format_dvec3(camera_planet),
            camera_planet.length(),
            terrain_probe,
            current_near,
            target_near,
            current_far,
            target_far,
            far_near_ratio,
            frame.desired_render_count,
            self.runtime.active_render_count(),
            self.runtime.deferred_commit_count(),
            self.runtime.deferred_upload_bytes() as f64 / (1024.0 * 1024.0),
            self.runtime.origin_mode_label(),
            self.runtime.config.use_large_world_coordinates
        )
    }
}

fn vector3_to_dvec3_local(value: Vector3) -> DVec3 {
    DVec3::new(f64::from(value.x), f64::from(value.y), f64::from(value.z))
}

fn format_vector3(value: Vector3) -> String {
    format!("[{:.3}, {:.3}, {:.3}]", value.x, value.y, value.z)
}

fn format_dvec3(value: DVec3) -> String {
    format!("[{:.3}, {:.3}, {:.3}]", value.x, value.y, value.z)
}
