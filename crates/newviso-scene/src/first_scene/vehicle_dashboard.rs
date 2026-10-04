use super::*;

/// Live channels for an authored script_rt_dials material, independent of the camera.
#[derive(Clone, Copy, Debug, Default)]
pub struct SceneVehicleDashboard {
    pub speed_mph: f32,
    pub revs: f32,
    pub fuel: f32,
    pub engine_temperature: f32,
    pub oil_pressure: f32,
    pub oil_temperature: f32,
    pub boost: f32,
    pub vacuum: f32,
    pub gear: i8,
    pub odometer_miles: f32,
    /// Left, right, parking brake, engine, ABS, fuel, oil, dipped beam,
    /// main beam, battery, ignition and illumination, in that order.
    pub lamps: u32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct DashboardGpuMaterial {
    frames: [GpuAssetMaterial; SCENE_FRAME_SLOTS],
    model_id: u64,
}

fn dashboard_profile(name: &str) -> Option<u32> {
    let name = name.to_ascii_lowercase();
    if !name.contains("script_rt_dials_") {
        return None;
    }
    // These authored atlases have different needle and lamp coordinates.
    if name.ends_with("bobcat") {
        Some(1)
    } else if name.ends_with("dukes") {
        Some(2)
    } else {
        None
    }
}

impl SceneVehicleDashboard {
    fn uniform_data(self, profile: u32) -> [f32; 12] {
        // Dashboard materials use the existing 48-byte material ABI. The
        // dashboard flag selects this layout before any surface lighting.
        [
            self.speed_mph,
            self.revs,
            self.fuel,
            self.engine_temperature,
            self.oil_pressure,
            self.oil_temperature,
            512.0,
            1.0,
            self.boost,
            self.vacuum,
            (self.gear as f32 + 1.0) + self.odometer_miles.floor() * 16.0,
            (self.lamps | (profile << 16)) as f32,
        ]
    }
}

impl Scene3dRuntime {
    pub fn set_vehicle_dashboard(&mut self, entity: u64, state: SceneVehicleDashboard) {
        self.vehicle_dashboards.insert(entity, state);
    }

    pub fn vehicle_dashboard_state(&self, entity: u64) -> Value {
        let materials = self
            .asset_meshes
            .get(&entity)
            .map(|mesh| {
                mesh.materials
                    .iter()
                    .enumerate()
                    .filter_map(|(slot, material)| {
                        let texture = material.textures.base_color.as_ref()?;
                        let profile = dashboard_profile(&texture.name)?;
                        Some(json!({"slot":slot,"texture":texture.name,"profile":profile,
                    "gpu_ready":self.dashboard_gpu_materials.contains_key(&(entity,slot as u32))}))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        json!({"materials":materials})
    }

    fn release_dashboard_material(&mut self, key: (u64, u32), render: &RenderClient) {
        if let Some(material) = self.dashboard_gpu_materials.remove(&key) {
            for frame in material.frames {
                render.destroy_bind_group(frame.bind_group);
                render.destroy_buffer(frame.uniform_buffer);
            }
        }
    }

    pub(super) fn release_dashboard_materials(&mut self, render: &RenderClient) {
        for key in self
            .dashboard_gpu_materials
            .keys()
            .copied()
            .collect::<Vec<_>>()
        {
            self.release_dashboard_material(key, render);
        }
    }

    pub(super) fn dashboard_material_group(
        &self,
        entity: u64,
        slot: u32,
        frame: usize,
    ) -> Option<u32> {
        self.dashboard_gpu_materials
            .get(&(entity, slot))
            .map(|m| m.frames[frame].bind_group)
    }

    pub(super) fn sync_dashboard_material_gpu(
        &mut self,
        render: &RenderClient,
        frame: usize,
    ) -> Result<(), String> {
        let Some(gpu) = self.gpu else {
            return Ok(());
        };
        let stale = self
            .dashboard_gpu_materials
            .iter()
            .filter_map(|(key, value)| {
                (!self.vehicle_dashboards.contains_key(&key.0)
                    || self
                        .asset_meshes
                        .get(&key.0)
                        .is_none_or(|m| m.model_id.0 != value.model_id))
                .then_some(*key)
            })
            .collect::<Vec<_>>();
        for key in stale {
            self.release_dashboard_material(key, render);
        }
        let mut updates = Vec::new();
        for (&entity, &state) in &self.vehicle_dashboards {
            let Some(mesh) = self.asset_meshes.get(&entity) else {
                continue;
            };
            for (slot, material) in mesh.materials.iter().enumerate() {
                let Some(texture) = material.textures.base_color.as_ref() else {
                    continue;
                };
                let Some(profile) = dashboard_profile(&texture.name) else {
                    continue;
                };
                let Some(base_color) = self.asset_gpu_textures.get(&texture.id.0).copied() else {
                    continue;
                };
                updates.push((
                    (entity, slot as u32),
                    mesh.model_id.0,
                    base_color,
                    state.uniform_data(profile),
                ));
            }
        }
        for (key, model_id, base_color, params) in updates {
            if !self.dashboard_gpu_materials.contains_key(&key) {
                let mut frames: Vec<GpuAssetMaterial> = Vec::new();
                for slot in 0..SCENE_FRAME_SLOTS {
                    let created = (|| {
                        let buffer = render.create_frame_buffer(
                            slot,
                            &format!("newviso.dashboard.{}.{}.slot.{slot}", key.0, key.1),
                            48,
                            "Uniform",
                            "CpuToGpu",
                        )?;
                        let group = match render.create_bind_group6(
                            &format!("newviso.dashboard.bind.{}.{}.slot.{slot}", key.0, key.1),
                            gpu.material_bind_group_layout,
                            [
                                Some(base_color),
                                Some(gpu.default_normal_texture),
                                Some(gpu.default_specular_texture),
                                Some(gpu.default_emissive_texture),
                                Some(gpu.default_environment_texture),
                                None,
                            ],
                            Some(gpu.material_sampler),
                            Some((buffer, 0, 48)),
                        ) {
                            Ok(group) => group,
                            Err(error) => {
                                render.destroy_buffer(buffer);
                                return Err(error);
                            }
                        };
                        Ok(GpuAssetMaterial {
                            uniform_buffer: buffer,
                            bind_group: group,
                        })
                    })();
                    match created {
                        Ok(frame) => frames.push(frame),
                        Err(error) => {
                            for frame in frames {
                                render.destroy_bind_group(frame.bind_group);
                                render.destroy_buffer(frame.uniform_buffer);
                            }
                            return Err(error);
                        }
                    }
                }
                let frames = frames
                    .try_into()
                    .map_err(|_| "dashboard frame count mismatch".to_owned())?;
                self.dashboard_gpu_materials
                    .insert(key, DashboardGpuMaterial { frames, model_id });
            }
            let material = &self.dashboard_gpu_materials[&key];
            render.write_buffer_f32(material.frames[frame].uniform_buffer, 0, &params)?;
        }
        Ok(())
    }
}
