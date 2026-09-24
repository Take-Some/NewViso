use super::*;

fn quaternion_to_euler_degrees(rotation: [f32; 4]) -> Result<Vec3, String> {
    let [mut x, mut y, mut z, mut w] = rotation;
    let length_sq = x * x + y * y + z * z + w * w;
    if !length_sq.is_finite() || length_sq <= 1.0e-12 {
        return Err("physics pose quaternion must be finite and non-zero".to_owned());
    }
    let inv_length = length_sq.sqrt().recip();
    x *= inv_length;
    y *= inv_length;
    z *= inv_length;
    w *= inv_length;

    let roll_x = (2.0 * (w * x + y * z)).atan2(1.0 - 2.0 * (x * x + y * y));
    let sin_pitch = (2.0 * (w * y - z * x)).clamp(-1.0, 1.0);
    let pitch_y = sin_pitch.asin();
    let yaw_z = (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z));

    Ok(Vec3::new(
        roll_x.to_degrees(),
        pitch_y.to_degrees(),
        yaw_z.to_degrees(),
    ))
}

impl Scene3dRuntime {
    pub fn upsert_runtime_dynamic_entity(
        &mut self,
        key: &str,
        desc: SceneRuntimeEntityDesc,
    ) -> Result<u64, String> {
        let key = key.trim();
        if key.is_empty() {
            return Err("runtime dynamic entity id must not be empty".to_owned());
        }
        let values = desc
            .position
            .iter()
            .chain(desc.rotation_degrees.iter())
            .chain(desc.scale.iter())
            .chain(desc.bounds_half_extent.iter())
            .chain(desc.base_color.iter());
        if values.clone().any(|value| !value.is_finite())
            || desc.scale.iter().any(|value| value.abs() <= 1.0e-6)
            || desc
                .bounds_half_extent
                .iter()
                .any(|value| *value <= 0.0 || *value > 1.0e6)
            || desc
                .base_color
                .iter()
                .any(|value| *value < 0.0 || *value > 64.0)
            || desc.visible_distance.is_nan()
            || desc.visible_distance <= 0.0
            || desc.stream_distance.is_nan()
            || desc.stream_distance < desc.visible_distance
            || !desc.fade_range.is_finite()
            || desc.fade_range < 0.0
        {
            return Err("invalid generic SceneRuntimeEntityDesc parameters".to_owned());
        }
        if desc
            .asset_ref
            .as_deref()
            .is_some_and(|value| value.trim().is_empty() || value.len() > 2048)
        {
            return Err("runtime dynamic entity asset_ref is invalid".to_owned());
        }
        if desc.visual == SceneRuntimeVisualKind::Cube && desc.asset_ref.is_some() {
            return Err(
                "runtime dynamic entity cannot combine cube visual with asset_ref; use one representation source"
                    .to_owned(),
            );
        }

        let existing = self
            .runtime_entity_ids
            .get(key)
            .copied()
            .map(SceneEntityId)
            .or_else(|| self.world.entity_id_by_name(key));

        let position = Vec3::new(desc.position[0], desc.position[1], desc.position[2]);
        let rotation_degrees = Vec3::new(
            desc.rotation_degrees[0],
            desc.rotation_degrees[1],
            desc.rotation_degrees[2],
        );
        let scale = Vec3::new(desc.scale[0], desc.scale[1], desc.scale[2]);
        let lod = SceneLodPolicy {
            visible_distance: desc.visible_distance,
            stream_distance: desc.stream_distance,
            fade_range: desc.fade_range,
        };

        if let Some(id) = existing {
            let current = self
                .world
                .entity(id)
                .ok_or_else(|| format!("runtime entity '{}' disappeared", key))?;
            if !matches!(
                current.kind,
                SceneEntityKind::DynamicMesh | SceneEntityKind::Unknown
            ) {
                return Err(format!(
                    "runtime entity '{}' already exists and is not a dynamic entity",
                    key
                ));
            }

            let render_slot = match desc.visual {
                SceneRuntimeVisualKind::None => current.render_slot,
                SceneRuntimeVisualKind::Cube => {
                    if let Some(slot) = current.render_slot {
                        Some(slot)
                    } else {
                        if let Some(gpu) = self.gpu.as_ref() {
                            if self.cubes.len() >= gpu.cube_capacity {
                                return Err(format!(
                                    "runtime cube capacity exceeded ({})",
                                    gpu.cube_capacity
                                ));
                            }
                        }
                        let slot = self.cubes.len();
                        self.cubes.push(Cube {
                            position,
                            rotation_degrees,
                            scale,
                            base_color: desc.base_color,
                        });
                        Some(slot)
                    }
                }
            };

            if let Some(slot) = render_slot {
                if desc.visual == SceneRuntimeVisualKind::Cube {
                    let cube = self
                        .cubes
                        .get_mut(slot)
                        .ok_or_else(|| format!("runtime cube slot {slot} is invalid"))?;
                    cube.position = position;
                    cube.rotation_degrees = rotation_degrees;
                    cube.scale = scale;
                    cube.base_color = desc.base_color;
                }
            }

            let previous_asset_ref = current.asset_ref.clone();
            {
                let entity = self
                    .world
                    .entity_mut(id)
                    .ok_or_else(|| format!("runtime entity '{}' disappeared", key))?;
                entity.kind = SceneEntityKind::DynamicMesh;
                entity.mobility = SceneMobility::Dynamic;
                entity.solid = desc.solid;
                entity.asset_ref = desc.asset_ref.clone();
                entity.render_slot = render_slot;
                entity.lod = lod;
                if desc.visual == SceneRuntimeVisualKind::Cube || entity.asset_ref.is_none() {
                    entity.residency = SceneResidency::Resident;
                } else if previous_asset_ref != entity.asset_ref {
                    entity.residency = SceneResidency::Unloaded;
                }
            }

            let bounds = if let Some(slot) = render_slot {
                if desc.visual == SceneRuntimeVisualKind::Cube {
                    let bounds = self
                        .cubes
                        .get(slot)
                        .ok_or_else(|| format!("runtime cube slot {slot} is invalid"))?
                        .bounds();
                    SceneBounds {
                        min: Vec3::new(bounds.min[0], bounds.min[1], bounds.min[2]),
                        max: Vec3::new(bounds.max[0], bounds.max[1], bounds.max[2]),
                    }
                } else {
                    SceneBounds::from_center_half_extent(
                        position,
                        Vec3::new(
                            desc.bounds_half_extent[0],
                            desc.bounds_half_extent[1],
                            desc.bounds_half_extent[2],
                        ),
                    )
                }
            } else {
                SceneBounds::from_center_half_extent(
                    position,
                    Vec3::new(
                        desc.bounds_half_extent[0],
                        desc.bounds_half_extent[1],
                        desc.bounds_half_extent[2],
                    ),
                )
            };
            self.world.update_spatial_from(
                id,
                SceneTransform {
                    position,
                    rotation_degrees,
                    scale,
                },
                bounds,
                SceneMutationSource::Engine,
            )?;
            self.world.activate_entity(id)?;
            self.runtime_entity_ids.insert(key.to_owned(), id.0);
            return Ok(id.0);
        }

        let (render_slot, bounds, residency) = match desc.visual {
            SceneRuntimeVisualKind::Cube => {
                if let Some(gpu) = self.gpu.as_ref() {
                    if self.cubes.len() >= gpu.cube_capacity {
                        return Err(format!(
                            "runtime cube capacity exceeded ({})",
                            gpu.cube_capacity
                        ));
                    }
                }
                let cube = Cube {
                    position,
                    rotation_degrees,
                    scale,
                    base_color: desc.base_color,
                };
                let cube_bounds = cube.bounds();
                let slot = self.cubes.len();
                self.cubes.push(cube);
                (
                    Some(slot),
                    SceneBounds {
                        min: Vec3::new(cube_bounds.min[0], cube_bounds.min[1], cube_bounds.min[2]),
                        max: Vec3::new(cube_bounds.max[0], cube_bounds.max[1], cube_bounds.max[2]),
                    },
                    SceneResidency::Resident,
                )
            }
            SceneRuntimeVisualKind::None => (
                None,
                SceneBounds::from_center_half_extent(
                    position,
                    Vec3::new(
                        desc.bounds_half_extent[0],
                        desc.bounds_half_extent[1],
                        desc.bounds_half_extent[2],
                    ),
                ),
                if desc.asset_ref.is_some() {
                    SceneResidency::Unloaded
                } else {
                    SceneResidency::Resident
                },
            ),
        };

        let id = SceneEntityId(self.next_runtime_entity_id);
        self.next_runtime_entity_id = self.next_runtime_entity_id.wrapping_add(1);
        self.world.add_entity(SceneEntity {
            id,
            name: key.to_owned(),
            kind: SceneEntityKind::DynamicMesh,
            mobility: SceneMobility::Dynamic,
            lifecycle: SceneLifecycle::Constructed,
            transform: SceneTransform {
                position,
                rotation_degrees,
                scale,
            },
            light: None,
            bounds,
            parent: None,
            children: Vec::new(),
            visibility: VisibilityMask::default(),
            lod,
            solid: desc.solid,
            asset_ref: desc.asset_ref,
            render_slot,
            residency,
            priority_score: 0.0,
            lod_alpha: 1.0,
            last_visible_frame: None,
            revision: 0,
            last_mutation_frame: 0,
            process_claims: SceneProcessClaims::default(),
            last_process_frame: None,
        })?;
        self.world.activate_entity(id)?;
        self.runtime_entity_ids.insert(key.to_owned(), id.0);
        Ok(id.0)
    }

    pub fn set_runtime_entity_materialized(
        &mut self,
        key: &str,
        materialized: bool,
    ) -> Result<bool, String> {
        let key = key.trim();
        let Some(id) = self
            .runtime_entity_ids
            .get(key)
            .copied()
            .map(SceneEntityId)
            .or_else(|| self.world.entity_id_by_name(key))
        else {
            return Ok(false);
        };
        self.world.set_dormant(id, !materialized)?;
        Ok(true)
    }

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
    pub fn runtime_entity_exists(&self, key: &str) -> bool {
        let key = key.trim();
        self.runtime_entity_ids.contains_key(key) || self.world.entity_id_by_name(key).is_some()
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
        let channel = module.trim();
        if channel.is_empty() {
            return Err("scene visibility channel must not be empty".to_owned());
        }
        self.world
            .set_visibility(SceneEntityId(stable_id), channel, visible)
    }
    pub fn request_remove_entity(&mut self, stable_id: u64) -> Result<(), String> {
        self.world.request_remove(SceneEntityId(stable_id))
    }

    pub fn set_entity_process_claim(
        &mut self,
        stable_id: u64,
        owner: &str,
        reason: &str,
        active: bool,
    ) -> Result<bool, String> {
        self.world.set_process_claim(
            SceneEntityId(stable_id),
            owner,
            reason,
            active,
            SceneMutationSource::Script,
        )
    }

    pub fn set_physics_process_active(
        &mut self,
        stable_id: u64,
        active: bool,
    ) -> Result<bool, String> {
        let id = SceneEntityId(stable_id);
        if self.world.entity(id).is_none() {
            return Ok(false);
        }
        self.world.set_process_claim(
            id,
            "engine.physics",
            "physics",
            active,
            SceneMutationSource::Physics,
        )
    }

    pub fn set_animation_process_active(
        &mut self,
        stable_id: u64,
        owner: &str,
        active: bool,
    ) -> Result<bool, String> {
        self.world.set_process_claim(
            SceneEntityId(stable_id),
            owner,
            "animation",
            active,
            SceneMutationSource::Animation,
        )
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
        self.set_entity_transform_from(
            stable_id,
            position,
            rotation_degrees,
            scale,
            SceneMutationSource::Script,
        )
    }

    pub fn apply_animation_transform(
        &mut self,
        stable_id: u64,
        position: [f32; 3],
        rotation_degrees: [f32; 3],
        scale: [f32; 3],
    ) -> Result<(), String> {
        self.set_entity_transform_from(
            stable_id,
            position,
            rotation_degrees,
            scale,
            SceneMutationSource::Animation,
        )
    }

    fn set_entity_transform_from(
        &mut self,
        stable_id: u64,
        position: [f32; 3],
        rotation_degrees: [f32; 3],
        scale: [f32; 3],
        source: SceneMutationSource,
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

        self.world
            .update_spatial_from(id, transform, bounds, source)
    }

    pub fn apply_physics_pose(
        &mut self,
        stable_id: u64,
        position: [f32; 3],
        rotation: [f32; 4],
    ) -> Result<bool, String> {
        if position
            .iter()
            .chain(rotation.iter())
            .any(|value| !value.is_finite())
        {
            return Err(format!(
                "physics pose for scene entity {} contains non-finite values",
                stable_id
            ));
        }

        let id = SceneEntityId(stable_id);
        let Some(entity) = self.world.entity(id) else {
            return Ok(false);
        };
        if entity.mobility != SceneMobility::Dynamic
            || entity.lifecycle == SceneLifecycle::Removed
            || entity.lifecycle == SceneLifecycle::PendingRemove
        {
            return Ok(false);
        }

        let scale = entity.transform.scale;
        let transform = SceneTransform {
            position: Vec3::new(position[0], position[1], position[2]),
            rotation_degrees: quaternion_to_euler_degrees(rotation)?,
            scale,
        };

        let (render_slot, previous_bounds) = (entity.render_slot, entity.bounds);
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

        self.world
            .update_spatial_from(id, transform, bounds, SceneMutationSource::Physics)?;
        Ok(true)
    }

    pub fn drain_entity_mutations(&mut self) -> Vec<Value> {
        self.world
            .drain_mutations()
            .into_iter()
            .map(|mutation| {
                json!({
                    "entity": mutation.entity.0,
                    "revision": mutation.revision,
                    "frame": mutation.frame,
                    "source": mutation.source.as_str(),
                    "dirty": mutation.dirty.labels(),
                    "transform": {
                        "position": [
                            mutation.transform.position.x,
                            mutation.transform.position.y,
                            mutation.transform.position.z
                        ],
                        "rotation_degrees": [
                            mutation.transform.rotation_degrees.x,
                            mutation.transform.rotation_degrees.y,
                            mutation.transform.rotation_degrees.z
                        ],
                        "scale": [
                            mutation.transform.scale.x,
                            mutation.transform.scale.y,
                            mutation.transform.scale.z
                        ]
                    },
                    "bounds": {
                        "min": [mutation.bounds.min.x, mutation.bounds.min.y, mutation.bounds.min.z],
                        "max": [mutation.bounds.max.x, mutation.bounds.max.y, mutation.bounds.max.z]
                    },
                    "process_control": {
                        "active": mutation.process_active,
                        "claims": mutation.process_claims
                    }
                })
            })
            .collect()
    }

    pub fn runtime_entity_state(&self, key: &str) -> Option<Value> {
        let id = self
            .runtime_entity_ids
            .get(key.trim())
            .copied()
            .map(SceneEntityId)
            .or_else(|| self.world.entity_id_by_name(key.trim()))?;
        self.entity_state(id.0)
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
            "revision": entity.revision,
            "last_mutation_frame": entity.last_mutation_frame,
            "process_control": {
                "active": self.world.process_is_active(entity.id),
                "requested": entity.process_claims.active(),
                "reasons": entity.process_claims.reasons(),
                "claims": entity.process_claims.snapshot(),
                "last_process_frame": entity.last_process_frame
            },
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
