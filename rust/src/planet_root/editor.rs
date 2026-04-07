use super::super::*;
use godot::classes::{DirectionalLight3D, Node, Node3D, SphereMesh};

impl PlanetRoot {
    pub(crate) fn sync_editor_preview(&mut self) {
        self.editor_preview = self.prune_editor_preview_nodes();
        let radius = self.planet_radius.max(1.0);
        if (self.editor_preview_radius_applied - radius).abs() <= f64::EPSILON
            && self.editor_preview.is_some()
        {
            return;
        }

        let mut preview = self.ensure_editor_preview();
        self.assign_editor_preview_owner(&mut preview);
        let mut mesh = SphereMesh::new_gd();
        mesh.set_radius(radius as f32);
        mesh.set_height((radius * 2.0) as f32);
        preview.set_name(EDITOR_PREVIEW_NODE_NAME);
        preview.set("mesh", &mesh.to_variant());
        preview.set_visible(true);
        self.editor_preview_radius_applied = radius;
    }

    pub(crate) fn sync_atmosphere_settings(&mut self) {
        let radius = self.planet_radius.max(1.0);
        let height = self.effective_atmosphere_height();
        let light_transform = self.first_directional_light_transform();
        let atmosphere_node_name = StringName::from(ATMOSPHERE_NODE_NAME);
        let child_count = self.base().get_child_count();

        for child_index in 0..child_count {
            let Some(mut child) = self.base().get_child(child_index) else {
                continue;
            };
            if child.get_name() != atmosphere_node_name {
                continue;
            }

            child.set(ATMOSPHERE_PLANET_RADIUS_PROPERTY, &radius.to_variant());
            child.set(ATMOSPHERE_HEIGHT_PROPERTY, &height.to_variant());

            let Ok(mut atmosphere) = child.try_cast::<Node3D>() else {
                continue;
            };
            if let Some(light_transform) = light_transform {
                let mut atmosphere_transform = atmosphere.get_transform();
                atmosphere_transform.basis = light_transform.basis;
                atmosphere_transform.origin = Vector3::ZERO;
                atmosphere.set_transform(atmosphere_transform);
            }
        }
    }

    pub(crate) fn first_directional_light_transform(&self) -> Option<Transform3D> {
        let child_count = self.base().get_child_count();
        for child_index in 0..child_count {
            let Some(child) = self.base().get_child(child_index) else {
                continue;
            };
            let Ok(light) = child.try_cast::<DirectionalLight3D>() else {
                continue;
            };
            return Some(light.get_transform());
        }
        None
    }

    fn ensure_editor_preview(&mut self) -> Gd<MeshInstance3D> {
        if let Some(preview) = self.editor_preview.as_ref() {
            return preview.clone();
        }

        if let Some(preview) = self.find_editor_preview_node() {
            self.editor_preview = Some(preview.clone());
            return preview;
        }

        let mut preview = MeshInstance3D::new_alloc();
        preview.set_name(EDITOR_PREVIEW_NODE_NAME);
        self.base_mut().add_child(&preview);
        self.assign_editor_preview_owner(&mut preview);
        self.editor_preview = Some(preview.clone());
        preview
    }

    fn assign_editor_preview_owner(&self, preview: &mut Gd<MeshInstance3D>) {
        let owner = self
            .base()
            .get_tree()
            .get_edited_scene_root()
            .or_else(|| Some(self.base().clone().upcast::<Node>()));
        if let Some(owner) = owner {
            preview.set_owner(&owner);
        }
    }

    fn find_editor_preview_node(&self) -> Option<Gd<MeshInstance3D>> {
        let preview_node_name = StringName::from(EDITOR_PREVIEW_NODE_NAME);
        let child_count = self.base().get_child_count();
        for child_index in 0..child_count {
            let Some(child) = self.base().get_child(child_index) else {
                continue;
            };
            if child.get_name() != preview_node_name {
                continue;
            }
            if let Ok(preview) = child.try_cast::<MeshInstance3D>() {
                return Some(preview);
            }
        }
        None
    }

    fn prune_editor_preview_nodes(&mut self) -> Option<Gd<MeshInstance3D>> {
        let preview_node_name = StringName::from(EDITOR_PREVIEW_NODE_NAME);
        let child_count = self.base().get_child_count();
        let mut kept_preview = None;
        let mut duplicate_children = Vec::new();

        for child_index in 0..child_count {
            let Some(child) = self.base().get_child(child_index) else {
                continue;
            };
            if child.get_name() != preview_node_name {
                continue;
            }

            match child.clone().try_cast::<MeshInstance3D>() {
                Ok(preview) if kept_preview.is_none() => {
                    kept_preview = Some(preview);
                }
                Ok(_) | Err(_) => duplicate_children.push(child),
            }
        }

        for mut child in duplicate_children {
            self.base_mut().remove_child(&child);
            child.queue_free();
        }

        kept_preview
    }

    pub(crate) fn remove_runtime_preview_node(&mut self) {
        self.editor_preview = None;
        let preview_node_name = StringName::from(EDITOR_PREVIEW_NODE_NAME);
        let child_count = self.base().get_child_count();
        let mut to_remove = Vec::new();

        for child_index in 0..child_count {
            let Some(child) = self.base().get_child(child_index) else {
                continue;
            };
            if child.get_name() == preview_node_name {
                to_remove.push(child);
            }
        }

        for mut child in to_remove {
            self.base_mut().remove_child(&child);
            child.queue_free();
        }
        self.editor_preview_radius_applied = -1.0;
    }

    pub(crate) fn teardown_editor_preview(&mut self) {
        if let Some(mut preview) = self.editor_preview.take() {
            preview.queue_free();
        }
        self.editor_preview_radius_applied = -1.0;
    }
}
