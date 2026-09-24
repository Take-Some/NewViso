use super::*;

impl Scene3dRuntime {
    pub fn set_runtime_entity_transform(
        &mut self,
        key: &str,
        position: Option<[f32; 3]>,
        rotation_degrees: Option<[f32; 3]>,
        scale: Option<[f32; 3]>,
    ) -> Result<(), String> {
        let key = key.trim();
        let id = self
            .runtime_entity_ids
            .get(key)
            .copied()
            .map(SceneEntityId)
            .or_else(|| self.world.entity_id_by_name(key))
            .ok_or_else(|| format!("runtime entity '{}' does not exist", key))?;
        let current = self
            .world
            .entity(id)
            .ok_or_else(|| format!("runtime entity '{}' disappeared", key))?
            .transform;

        let p = position.unwrap_or([current.position.x, current.position.y, current.position.z]);
        let r = rotation_degrees.unwrap_or([
            current.rotation_degrees.x,
            current.rotation_degrees.y,
            current.rotation_degrees.z,
        ]);
        let s = scale.unwrap_or([current.scale.x, current.scale.y, current.scale.z]);
        self.set_entity_transform(id.0, p, r, s)
    }
    pub fn remove_runtime_entity(&mut self, key: &str) -> Result<(), String> {
        let key = key.trim();
        let id = self
            .runtime_entity_ids
            .get(key)
            .copied()
            .map(SceneEntityId)
            .or_else(|| self.world.entity_id_by_name(key))
            .ok_or_else(|| format!("runtime entity '{}' does not exist", key))?;
        self.world.request_remove(id)?;
        self.sky_visuals.remove(key);
        self.lens_flares.retain(|_, flare| flare.source != key);
        self.runtime_entity_ids.remove(key);
        Ok(())
    }
    pub fn set_scene_focus_camera(&mut self) {
        self.world.set_focus_camera();
    }
    pub fn set_scene_focus_entity(&mut self, stable_id: u64) -> Result<(), String> {
        self.world.set_focus_entity(SceneEntityId(stable_id))
    }
    pub fn set_scene_focus_override(&mut self, position: [f32; 3], velocity: [f32; 3]) {
        self.world.set_focus_override(
            Vec3::new(position[0], position[1], position[2]),
            Vec3::new(velocity[0], velocity[1], velocity[2]),
        );
    }
    pub fn set_entity_visibility(
        &mut self,
        stable_id: u64,
        module: &str,
        visible: bool,
    ) -> Result<(), String> {
        let module = visibility_module_from_name(module)
            .ok_or_else(|| format!("unknown scene visibility module '{module}'"))?;
        self.world
            .set_visibility(SceneEntityId(stable_id), module, visible)
    }
    pub fn request_remove_entity(&mut self, stable_id: u64) -> Result<(), String> {
        self.world.request_remove(SceneEntityId(stable_id))
    }
    pub fn pending_stream_requests(&self) -> Vec<SceneStreamRequest> {
        self.frame_plan
            .requested_entities
            .iter()
            .filter_map(|id| {
                let entity = self.world.entity(*id)?;
                Some(SceneStreamRequest {
                    stable_id: id.0,
                    asset_ref: entity.asset_ref.clone()?,
                    priority: entity.priority_score,
                })
            })
            .collect()
    }
    pub fn streaming_interests(&self) -> Vec<SceneStreamRequest> {
        self.frame_plan
            .streaming_entities
            .iter()
            .filter_map(|id| {
                let entity = self.world.entity(*id)?;
                Some(SceneStreamRequest {
                    stable_id: id.0,
                    asset_ref: entity.asset_ref.clone()?,
                    priority: entity.priority_score,
                })
            })
            .collect()
    }
    pub fn mark_entity_resident(&mut self, stable_id: u64) -> Result<(), String> {
        self.world
            .set_residency(SceneEntityId(stable_id), SceneResidency::Resident)
    }
    pub fn mark_entity_unloaded(&mut self, stable_id: u64) -> Result<(), String> {
        self.world
            .set_residency(SceneEntityId(stable_id), SceneResidency::Unloaded)
    }
    pub fn set_entity_parent(
        &mut self,
        child_stable_id: u64,
        parent_stable_id: Option<u64>,
    ) -> Result<(), String> {
        self.world.set_parent(
            SceneEntityId(child_stable_id),
            parent_stable_id.map(SceneEntityId),
        )
    }
    pub fn set_entity_transform(
        &mut self,
        stable_id: u64,
        position: [f32; 3],
        rotation_degrees: [f32; 3],
        scale: [f32; 3],
    ) -> Result<(), String> {
        let id = SceneEntityId(stable_id);
        let transform = SceneTransform {
            position: Vec3::new(position[0], position[1], position[2]),
            rotation_degrees: Vec3::new(
                rotation_degrees[0],
                rotation_degrees[1],
                rotation_degrees[2],
            ),
            scale: Vec3::new(scale[0], scale[1], scale[2]),
        };
        if [
            position[0],
            position[1],
            position[2],
            rotation_degrees[0],
            rotation_degrees[1],
            rotation_degrees[2],
            scale[0],
            scale[1],
            scale[2],
        ]
        .iter()
        .any(|value| !value.is_finite())
        {
            return Err("scene transform values must be finite".to_owned());
        }

        let (render_slot, previous_bounds) = {
            let entity = self
                .world
                .entity(id)
                .ok_or_else(|| format!("scene entity {} does not exist", stable_id))?;
            (entity.render_slot, entity.bounds)
        };

        let bounds = if let Some(render_slot) = render_slot {
            let cube = self
                .cubes
                .get_mut(render_slot)
                .ok_or_else(|| format!("scene render slot {} is invalid", render_slot))?;
            cube.position = transform.position;
            cube.rotation_degrees = transform.rotation_degrees;
            cube.scale = transform.scale;
            let bounds = cube.bounds();
            SceneBounds {
                min: Vec3::new(bounds.min[0], bounds.min[1], bounds.min[2]),
                max: Vec3::new(bounds.max[0], bounds.max[1], bounds.max[2]),
            }
        } else {
            let old_center = previous_bounds.center();
            let old_half = previous_bounds.max.sub(old_center);
            SceneBounds::from_center_half_extent(transform.position, old_half)
        };

        self.world.update_spatial(id, transform, bounds)
    }
    pub fn entity_state(&self, stable_id: u64) -> Option<Value> {
        let entity = self.world.entity(SceneEntityId(stable_id))?;
        let kind = match entity.kind {
            SceneEntityKind::Camera => "camera",
            SceneEntityKind::StaticMesh => "static_mesh",
            SceneEntityKind::DynamicMesh => "dynamic_mesh",
            SceneEntityKind::Light => "light",
            SceneEntityKind::SkyVisual => "sky_visual",
            SceneEntityKind::Trigger => "trigger",
            SceneEntityKind::Portal => "portal",
            SceneEntityKind::Unknown => "unknown",
        };
        let mobility = match entity.mobility {
            SceneMobility::Static => "static",
            SceneMobility::Dynamic => "dynamic",
        };
        let lifecycle = match entity.lifecycle {
            SceneLifecycle::Constructed => "constructed",
            SceneLifecycle::Added => "added",
            SceneLifecycle::Active => "active",
            SceneLifecycle::Dormant => "dormant",
            SceneLifecycle::PendingRemove => "pending_remove",
            SceneLifecycle::Removed => "removed",
        };
        let residency = match entity.residency {
            SceneResidency::Unloaded => "unloaded",
            SceneResidency::Requested => "requested",
            SceneResidency::Resident => "resident",
        };

        Some(json!({
            "id": entity.id.0,
            "name": entity.name,
            "kind": kind,
            "mobility": mobility,
            "lifecycle": lifecycle,
            "residency": residency,
            "asset_ref": entity.asset_ref,
            "visibility_mask": entity.visibility.raw(),
            "priority_score": entity.priority_score,
            "lod_alpha": entity.lod_alpha,
            "transform": {
                "position": [
                    entity.transform.position.x,
                    entity.transform.position.y,
                    entity.transform.position.z
                ],
                "rotation_degrees": [
                    entity.transform.rotation_degrees.x,
                    entity.transform.rotation_degrees.y,
                    entity.transform.rotation_degrees.z
                ],
                "scale": [
                    entity.transform.scale.x,
                    entity.transform.scale.y,
                    entity.transform.scale.z
                ]
            }
        }))
    }
}
