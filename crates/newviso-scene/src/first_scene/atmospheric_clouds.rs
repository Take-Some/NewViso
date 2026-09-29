use super::*;
use std::collections::BTreeMap;

pub(super) const ATMOSPHERIC_CLOUD_VERTEX_SHADER: &[u8] =
    include_bytes!("../assets/atmospheric_cloud.vert.spv");
pub(super) const ATMOSPHERIC_CLOUD_FRAGMENT_SHADER: &[u8] =
    include_bytes!("../assets/atmospheric_cloud.frag.spv");
pub(super) const ATMOSPHERIC_CLOUD_VERTEX_FLOATS: usize = 16;
pub(super) const ATMOSPHERIC_CLOUD_VERTEX_STRIDE: u64 =
    (ATMOSPHERIC_CLOUD_VERTEX_FLOATS * std::mem::size_of::<f32>()) as u64;
pub(super) const ATMOSPHERIC_CLOUD_UNIFORM_FLOATS: usize = 204;
pub(super) const ATMOSPHERIC_SOFT_DEPTH_UNIFORM_FLOATS: usize = 20;
pub(super) const ATMOSPHERIC_DEPTH_VERTEX_SHADER: &[u8] =
    include_bytes!("../assets/atmospheric_depth.vert.spv");
pub(super) const ATMOSPHERIC_DEPTH_INSTANCED_VERTEX_SHADER: &[u8] =
    include_bytes!("../assets/atmospheric_depth_instanced.vert.spv");
pub(super) const ATMOSPHERIC_DEPTH_INSTANCED_FRAGMENT_SHADER: &[u8] =
    include_bytes!("../assets/atmospheric_depth_instanced.frag.spv");
pub(super) const ATMOSPHERIC_DEPTH_FRAGMENT_SHADER: &[u8] =
    include_bytes!("../assets/atmospheric_depth.frag.spv");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AtmosphericCloudAnimMode {
    Combine,
    Sculpt,
}

#[derive(Clone, Copy, Debug)]
pub struct AtmosphericCloudUvLayerDesc {
    pub enabled: bool,
    pub mode: AtmosphericCloudAnimMode,
    pub velocity: [f32; 2],
    pub scale: f32,
    pub weight: f32,
}

impl Default for AtmosphericCloudUvLayerDesc {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: AtmosphericCloudAnimMode::Combine,
            velocity: [0.0; 2],
            scale: 1.0,
            weight: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AtmosphericCloudVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 4],
    pub color: [f32; 4],
    pub uv: [f32; 2],
}

#[derive(Clone, Debug)]
pub struct AtmosphericCloudMeshResources {
    pub name: String,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub vertices: Vec<AtmosphericCloudVertex>,
    pub indices: Vec<u32>,
    pub index_format: SkyIndexFormat,
}

#[derive(Clone, Debug)]
pub struct AtmosphericCloudTextureSet {
    /// Canonical semantic addresses in CloudsPS binding order.
    pub refs: [String; 6],
    pub density: SkyTextureResources,
    pub normal: SkyTextureResources,
    pub detail_density: SkyTextureResources,
    pub detail_normal: SkyTextureResources,
    pub detail_density2: SkyTextureResources,
    pub detail_normal2: SkyTextureResources,
    pub detail_present: [bool; 2],
}

#[derive(Clone, Debug)]
pub struct AtmosphericCloudLayerDesc {
    pub id: String,
    pub position: [f32; 3],
    pub rotation_degrees: [f32; 3],
    pub scale: [f32; 3],
    pub angular_velocity_degrees: [f32; 3],
    pub rotation_scale: f32,
    pub camera_position_scale: [f32; 3],
    pub altitude_min: Option<f32>,
    pub altitude_min_fade: f32,
    pub altitude_max: Option<f32>,
    pub altitude_max_fade: f32,
    pub transition_seconds: f32,
    pub transition_in_time_percent: f32,
    pub transition_out_time_percent: f32,
    pub transition_delay_percent: f32,
    pub transition_midpoint: f32,
    pub transition_alpha_range: f32,
    pub cost_factor: f32,
    pub soft_intersection_distance: f32,
    pub density: f32,
    pub softness: f32,
    pub opacity: f32,
    pub color: [f32; 3],
    pub density_shift_scale: [f32; 4],
    pub scatter: [f32; 4],
    pub piercing: [f32; 4],
    pub scale_diffuse_fill_ambient: [f32; 4],
    pub wrap_lighting: [f32; 4],
    pub rescale_uv: [[f32; 2]; 3],
    pub layer_anim_scale: [[f32; 2]; 3],
    pub weather_weights: BTreeMap<String, f32>,
    pub uv_layers: [AtmosphericCloudUvLayerDesc; 3],
}

#[derive(Clone, Debug)]
pub struct AtmosphericCloudLayerResources {
    pub model_name: String,
    pub mesh: AtmosphericCloudMeshResources,
    pub textures: AtmosphericCloudTextureSet,
    pub desc: AtmosphericCloudLayerDesc,
}

#[derive(Clone, Debug)]
pub struct AtmosphericCloudResources {
    pub enabled: bool,
    pub cloud_hat_speed: f32,
    pub wind_min_speed: f32,
    pub wind_max_speed: f32,
    pub altitude_scroll_scale: f32,
    pub global_alpha: f32,
    pub transition_midpoint: f32,
    pub transition_alpha_range: f32,
    pub streaming_budget: f32,
    pub soft_depth_resolution: u32,
    pub layers: Vec<AtmosphericCloudLayerResources>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AtmosphericCloudTransitionDirection {
    In,
    Out,
}

#[derive(Clone, Debug)]
pub(super) struct AtmosphericCloudLayerRuntime {
    pub alpha: f32,
    pub angular_accum_degrees: [f32; 3],
    pub uv_accum: [[f32; 2]; 3],
    pub override_target: Option<(f32, f32)>,
    /// Runtime admission state. GPU residency follows this state rather than
    /// keeping every authored cloud layer resident indefinitely.
    pub resident: bool,
    /// Latest target that established the current transition envelope.
    pub last_desired_target: f32,
    pub transition_from: f32,
    pub transition_target: f32,
    /// Container/request clock. Delay is evaluated against this clock.
    pub request_elapsed: f32,
    /// Fade clock. This intentionally does not advance while an incoming layer
    /// is waiting for residency, mirroring GTA's "streaming wait does not eat
    /// transition time" behavior.
    pub fade_elapsed: f32,
    pub transition_duration: f32,
    pub transition_delay: f32,
    pub transition_direction: AtmosphericCloudTransitionDirection,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct GpuAtmosphericCloudLayer {
    pub vertex_buffer: u32,
    pub index_buffer: u32,
    pub uniform_buffers: [u32; SCENE_FRAME_SLOTS],
    pub bind_groups: [u32; SCENE_FRAME_SLOTS],
    pub index_count: u32,
    pub index_format: &'static str,
}

#[derive(Clone, Debug)]
pub(super) struct GpuAtmosphericClouds {
    pub sampler: u32,
    pub bind_group_layout: u32,
    pub vertex_shader: u32,
    pub fragment_shader: u32,
    pub pipeline: u32,
    pub layers: Vec<Option<GpuAtmosphericCloudLayer>>,
    pub soft_depth_render_target: u32,
    pub soft_depth_uniforms: [u32; SCENE_FRAME_SLOTS],
    pub soft_depth_bind_group_layout: u32,
    pub soft_depth_bind_groups: [u32; SCENE_FRAME_SLOTS],
    pub soft_sample_bind_group_layout: u32,
    pub soft_sample_bind_group: u32,
    pub texture_cache: BTreeMap<String, u32>,
    pub soft_depth_vertex_shader: u32,
    pub soft_depth_instanced_vertex_shader: u32,
    pub soft_depth_fragment_shader: u32,
    pub soft_depth_instanced_fragment_shader: u32,
    pub soft_depth_pipeline: u32,
    pub soft_depth_instanced_pipeline: u32,
    pub soft_depth_resolution: u32,
}

impl Scene3dRuntime {
    pub fn set_atmospheric_clouds(
        &mut self,
        resources: AtmosphericCloudResources,
    ) -> Result<(), String> {
        if self.gpu.is_some() || self.gpu_atmospheric_clouds.is_some() {
            return Err(
                "atmospheric cloud resources must be assigned before renderer initialization"
                    .to_owned(),
            );
        }
        if !resources.cloud_hat_speed.is_finite()
            || resources.cloud_hat_speed.abs() > 64.0
            || !resources.wind_min_speed.is_finite()
            || resources.wind_min_speed < 0.0
            || !resources.wind_max_speed.is_finite()
            || resources.wind_max_speed <= resources.wind_min_speed
            || !resources.altitude_scroll_scale.is_finite()
            || resources.altitude_scroll_scale.abs() > 16.0
            || !resources.global_alpha.is_finite()
            || !(0.0..=1.0).contains(&resources.global_alpha)
            || !resources.transition_midpoint.is_finite()
            || !(0.0..=1.0).contains(&resources.transition_midpoint)
            || !resources.transition_alpha_range.is_finite()
            || !(0.0..=1.0).contains(&resources.transition_alpha_range)
            || !resources.streaming_budget.is_finite()
            || !(0.001..=16.0).contains(&resources.streaming_budget)
            || !(64..=2048).contains(&resources.soft_depth_resolution)
            || !resources.soft_depth_resolution.is_power_of_two()
            || resources.layers.len() > 256
        {
            return Err("invalid atmospheric cloud global parameters".to_owned());
        }

        let mut seen = std::collections::BTreeSet::new();
        for layer in &resources.layers {
            let desc = &layer.desc;
            if desc.id.trim().is_empty()
                || !seen.insert(desc.id.clone())
                || layer.mesh.vertices.is_empty()
                || layer.mesh.indices.is_empty()
                || !desc.position.iter().all(|v| v.is_finite())
                || !desc.rotation_degrees.iter().all(|v| v.is_finite())
                || !desc.scale.iter().all(|v| v.is_finite() && v.abs() > 1.0e-4)
                || !desc.angular_velocity_degrees.iter().all(|v| v.is_finite())
                || !desc.camera_position_scale.iter().all(|v| v.is_finite())
                || !desc.rotation_scale.is_finite()
                || !desc.transition_seconds.is_finite()
                || desc.transition_seconds < 0.0
                || !desc.transition_in_time_percent.is_finite()
                || !(0.0..=1.0).contains(&desc.transition_in_time_percent)
                || !desc.transition_out_time_percent.is_finite()
                || !(0.0..=1.0).contains(&desc.transition_out_time_percent)
                || !desc.transition_delay_percent.is_finite()
                || !(0.0..=1.0).contains(&desc.transition_delay_percent)
                || !desc.transition_midpoint.is_finite()
                || !(0.0..=1.0).contains(&desc.transition_midpoint)
                || !desc.transition_alpha_range.is_finite()
                || !(0.0..=1.0).contains(&desc.transition_alpha_range)
                || !desc.cost_factor.is_finite()
                || !(0.0..=16.0).contains(&desc.cost_factor)
                || !desc.soft_intersection_distance.is_finite()
                || !(0.0..=10000.0).contains(&desc.soft_intersection_distance)
                || !desc.density.is_finite()
                || !(0.0..=2.0).contains(&desc.density)
                || !desc.softness.is_finite()
                || !(0.001..=1.0).contains(&desc.softness)
                || !desc.opacity.is_finite()
                || !(0.0..=1.0).contains(&desc.opacity)
                || desc
                    .density_shift_scale
                    .iter()
                    .chain(desc.scatter.iter())
                    .chain(desc.piercing.iter())
                    .chain(desc.scale_diffuse_fill_ambient.iter())
                    .chain(desc.wrap_lighting.iter())
                    .chain(desc.rescale_uv.iter().flatten())
                    .chain(desc.layer_anim_scale.iter().flatten())
                    .any(|value| !value.is_finite() || value.abs() > 65536.0)
                || desc.uv_layers.iter().any(|uv| {
                    !uv.velocity
                        .iter()
                        .all(|value| value.is_finite() && value.abs() <= 64.0)
                        || !uv.scale.is_finite()
                        || !(0.001..=1024.0).contains(&uv.scale)
                        || !uv.weight.is_finite()
                        || !(0.0..=32.0).contains(&uv.weight)
                })
            {
                return Err(format!("invalid atmospheric cloud layer '{}'", desc.id));
            }
            let vertex_count = layer.mesh.vertices.len() as u32;
            if layer
                .mesh
                .indices
                .iter()
                .any(|index| *index >= vertex_count)
            {
                return Err(format!(
                    "atmospheric cloud layer '{}' contains an out-of-range index",
                    desc.id
                ));
            }
        }

        self.atmospheric_cloud_runtime = resources
            .layers
            .iter()
            .map(|_| AtmosphericCloudLayerRuntime {
                alpha: 0.0,
                angular_accum_degrees: [0.0; 3],
                uv_accum: [[0.0; 2]; 3],
                override_target: None,
                resident: false,
                last_desired_target: 0.0,
                transition_from: 0.0,
                transition_target: 0.0,
                request_elapsed: 0.0,
                fade_elapsed: 0.0,
                transition_duration: 0.0,
                transition_delay: 0.0,
                transition_direction: AtmosphericCloudTransitionDirection::Out,
            })
            .collect();
        self.atmospheric_clouds = Some(resources);
        Ok(())
    }

    pub fn set_atmospheric_cloud_layer_target(
        &mut self,
        id: &str,
        alpha: f32,
        transition_seconds: f32,
    ) -> Result<(), String> {
        if !alpha.is_finite()
            || !(0.0..=1.0).contains(&alpha)
            || !transition_seconds.is_finite()
            || !(0.0..=3600.0).contains(&transition_seconds)
        {
            return Err("invalid atmospheric cloud layer target".to_owned());
        }
        let resources = self
            .atmospheric_clouds
            .as_ref()
            .ok_or_else(|| "atmospheric clouds are not configured".to_owned())?;
        let index = resources
            .layers
            .iter()
            .position(|layer| layer.desc.id == id)
            .ok_or_else(|| format!("unknown atmospheric cloud layer '{id}'"))?;
        self.atmospheric_cloud_runtime[index].override_target = Some((alpha, transition_seconds));
        Ok(())
    }

    pub fn clear_atmospheric_cloud_layer_target(&mut self, id: &str) -> Result<(), String> {
        let resources = self
            .atmospheric_clouds
            .as_ref()
            .ok_or_else(|| "atmospheric clouds are not configured".to_owned())?;
        let index = resources
            .layers
            .iter()
            .position(|layer| layer.desc.id == id)
            .ok_or_else(|| format!("unknown atmospheric cloud layer '{id}'"))?;
        self.atmospheric_cloud_runtime[index].override_target = None;
        Ok(())
    }

    pub(super) fn update_atmospheric_clouds(&mut self, dt: f32) {
        let Some(resources) = self.atmospheric_clouds.as_ref() else {
            return;
        };
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }

        // GTA CloudHat uses the resolved weather wind magnitude to scale the
        // authored UV velocities. Procedural sky-cloud speed is a compatibility
        // fallback only; using it for GTA layers makes them almost stationary.
        let procedural_wind = self.sky_clouds.speed;
        let procedural_wind_speed = (procedural_wind[0] * procedural_wind[0]
            + procedural_wind[1] * procedural_wind[1])
            .sqrt();
        let weather_wind_resolved = self.weather_effects.wind_max > 0.0
            || self.weather_effects.wind_min > 0.0
            || self.weather_effects.wind_speed > 0.0;
        let wind_speed = if weather_wind_resolved {
            self.weather_effects.wind_speed.max(0.0)
        } else {
            procedural_wind_speed
        };
        let wind_mult = ((wind_speed - resources.wind_min_speed)
            / (resources.wind_max_speed - resources.wind_min_speed))
            .clamp(0.0, 1.0);

        // Animation is simulation state, not residency state. GTA keeps these
        // accumulators independently from render-buffer availability.
        for (layer, runtime) in resources
            .layers
            .iter()
            .zip(self.atmospheric_cloud_runtime.iter_mut())
        {
            for axis in 0..3 {
                runtime.angular_accum_degrees[axis] = (runtime.angular_accum_degrees[axis]
                    + layer.desc.angular_velocity_degrees[axis] * dt * resources.cloud_hat_speed)
                    .rem_euclid(360.0);
            }

            for (index, uv) in layer.desc.uv_layers.iter().enumerate() {
                if !uv.enabled {
                    continue;
                }
                runtime.uv_accum[index][0] =
                    (runtime.uv_accum[index][0] + uv.velocity[0] * dt * wind_mult).rem_euclid(1.0);
                runtime.uv_accum[index][1] =
                    (runtime.uv_accum[index][1] + uv.velocity[1] * dt * wind_mult).rem_euclid(1.0);
            }
        }

        let desired = resources
            .layers
            .iter()
            .zip(self.atmospheric_cloud_runtime.iter())
            .map(|(layer, runtime)| {
                if !resources.enabled {
                    (0.0, layer.desc.transition_seconds)
                } else {
                    runtime.override_target.unwrap_or_else(|| {
                        (
                            weather_target_alpha(
                                &layer.desc,
                                &self.weather_backend,
                                &self.weather_effects,
                            ),
                            layer.desc.transition_seconds,
                        )
                    })
                }
            })
            .collect::<Vec<_>>();

        // Establish/reverse transition envelopes first. A reversal always starts
        // from the currently visible alpha so there is no pop.
        for ((layer, runtime), (target, requested_seconds)) in resources
            .layers
            .iter()
            .zip(self.atmospheric_cloud_runtime.iter_mut())
            .zip(desired.iter().copied())
        {
            let target = target.clamp(0.0, 1.0);
            if (target - runtime.last_desired_target).abs() > 1.0e-5 {
                begin_transition(runtime, &layer.desc, target, requested_seconds);
            }
        }

        // GTA updates outgoing content first and its resident drawables keep
        // occupying cost while they fade. In NewViso the same invariant is
        // expressed directly as a residency budget: every currently resident
        // layer occupies cost; only the residual can admit a new incoming layer.
        let occupied_cost: f32 = resources
            .layers
            .iter()
            .zip(self.atmospheric_cloud_runtime.iter())
            .filter(|(_, runtime)| runtime.resident)
            .map(|(layer, _)| layer.desc.cost_factor)
            .sum();
        let mut available_cost = (resources.streaming_budget - occupied_cost).max(0.0);

        for ((layer, runtime), (target, _)) in resources
            .layers
            .iter()
            .zip(self.atmospheric_cloud_runtime.iter_mut())
            .zip(desired.iter().copied())
        {
            let target = target.clamp(0.0, 1.0);

            if runtime.transition_direction == AtmosphericCloudTransitionDirection::In
                && target > runtime.alpha + 1.0e-5
            {
                // CloudHat freezes the incoming container clock when there is no
                // cost at all. If some cost exists but not enough for this layer,
                // the request/delay clock may continue while fade time remains held.
                if available_cost > 0.0 || runtime.resident {
                    runtime.request_elapsed += dt;
                }

                if !runtime.resident
                    && can_admit_cloud_layer(
                        available_cost,
                        layer.desc.cost_factor,
                        runtime.request_elapsed,
                        runtime.transition_delay,
                    )
                {
                    runtime.resident = true;
                    runtime.fade_elapsed = 0.0;
                    available_cost = (available_cost - layer.desc.cost_factor).max(0.0);
                }

                if runtime.resident && runtime.request_elapsed + 1.0e-6 >= runtime.transition_delay
                {
                    advance_transition(runtime, layer.desc.transition_midpoint, dt);
                }
            } else if runtime.transition_direction == AtmosphericCloudTransitionDirection::Out
                && runtime.resident
            {
                runtime.request_elapsed += dt;
                if runtime.request_elapsed + 1.0e-6 >= runtime.transition_delay {
                    advance_transition(runtime, layer.desc.transition_midpoint, dt);
                }
                if runtime.transition_target <= 1.0e-5
                    && runtime.alpha <= 1.0e-5
                    && transition_complete(runtime)
                {
                    runtime.alpha = 0.0;
                    runtime.resident = false;
                }
            } else if runtime.resident && (runtime.alpha - target).abs() <= 1.0e-5 {
                runtime.alpha = target;
            }
        }
    }

    pub(super) fn atmospheric_cloud_uniforms(
        &self,
        width: u32,
        height: u32,
    ) -> Vec<[f32; ATMOSPHERIC_CLOUD_UNIFORM_FLOATS]> {
        let aspect = width as f32 / height.max(1) as f32;
        let Some(resources) = self.atmospheric_clouds.as_ref() else {
            return Vec::new();
        };
        let view_proj = camera_view_projection(&self.camera, aspect);
        let (sun_direction, sun_intensity) = self.atmospheric_sun();
        let camera_height = self.camera.position.y;

        resources
            .layers
            .iter()
            .zip(self.atmospheric_cloud_runtime.iter())
            .map(|(layer, runtime)| {
                let mut out = [0.0_f32; ATMOSPHERIC_CLOUD_UNIFORM_FLOATS];
                out[0..16].copy_from_slice(&view_proj);

                let position = Vec3::new(
                    layer.desc.position[0]
                        + self.camera.position.x * layer.desc.camera_position_scale[0],
                    layer.desc.position[1]
                        + self.camera.position.y * layer.desc.camera_position_scale[1],
                    layer.desc.position[2]
                        + self.camera.position.z * layer.desc.camera_position_scale[2],
                );
                let rotation = Vec3::new(
                    layer.desc.rotation_degrees[0]
                        + runtime.angular_accum_degrees[0] * layer.desc.rotation_scale,
                    layer.desc.rotation_degrees[1]
                        + runtime.angular_accum_degrees[1] * layer.desc.rotation_scale,
                    layer.desc.rotation_degrees[2]
                        + runtime.angular_accum_degrees[2] * layer.desc.rotation_scale,
                );
                let scale = Vec3::new(
                    layer.desc.scale[0],
                    layer.desc.scale[1],
                    layer.desc.scale[2],
                );
                let model = geometry::instance_model_matrix(position, rotation, scale);
                out[16..32].copy_from_slice(&model);

                let altitude_alpha = altitude_visibility(&layer.desc, camera_height);
                let transition_alpha =
                    transition_alpha_scale(runtime.alpha, layer.desc.transition_alpha_range);
                let effective_alpha = if resources.enabled && runtime.resident {
                    transition_alpha * altitude_alpha * layer.desc.opacity * resources.global_alpha
                } else {
                    0.0
                };
                out[32] = self.camera.position.x;
                out[33] = self.camera.position.y;
                out[34] = self.camera.position.z;
                out[35] = effective_alpha.clamp(0.0, 1.0);

                out[36] = sun_direction[0];
                out[37] = sun_direction[1];
                out[38] = sun_direction[2];
                out[39] = sun_intensity;

                out[40] = layer.desc.color[0];
                out[41] = layer.desc.color[1];
                out[42] = layer.desc.color[2];
                out[43] = layer.desc.density;
                out[44] = layer.desc.softness;
                out[45] = resources.altitude_scroll_scale;
                out[46] = altitude_alpha;
                // Raw transition envelope remains separate from alpha range so the
                // shader can morph cloud density while early alpha ramps independently.
                out[47] = runtime.alpha;

                let camera_scroll = [
                    self.camera.position.x * resources.altitude_scroll_scale,
                    self.camera.position.z * resources.altitude_scroll_scale,
                ];
                for index in 0..3 {
                    let uv = layer.desc.uv_layers[index];
                    let offset = 48 + index * 4;
                    out[offset] = runtime.uv_accum[index][0] + camera_scroll[0];
                    out[offset + 1] = runtime.uv_accum[index][1] + camera_scroll[1];
                    out[offset + 2] = uv.scale;
                    out[offset + 3] = if !uv.enabled {
                        0.0
                    } else {
                        match uv.mode {
                            AtmosphericCloudAnimMode::Combine => uv.weight,
                            AtmosphericCloudAnimMode::Sculpt => -uv.weight,
                        }
                    };
                }

                out[60] = self.sky_atmosphere.cloud_day_shadow[0];
                out[61] = self.sky_atmosphere.cloud_day_shadow[1];
                out[62] = self.sky_atmosphere.cloud_day_shadow[2];
                out[63] = sun_direction[1];

                out[64] = self.sky_atmosphere.cloud_day_light[0];
                out[65] = self.sky_atmosphere.cloud_day_light[1];
                out[66] = self.sky_atmosphere.cloud_day_light[2];
                out[67] = self.sky_atmosphere.silver_lining_strength;

                out[68] = self.sky_atmosphere.cloud_night[0];
                out[69] = self.sky_atmosphere.cloud_night[1];
                out[70] = self.sky_atmosphere.cloud_night[2];
                out[71] = self.sky_atmosphere.tonemap_shoulder;

                out[72] = width.max(1) as f32;
                out[73] = height.max(1) as f32;
                out[74] = layer.desc.soft_intersection_distance;
                out[75] = if layer.desc.soft_intersection_distance > 0.0 {
                    1.0
                } else {
                    0.0
                };

                out[76..80].copy_from_slice(&layer.desc.density_shift_scale);
                out[80..84].copy_from_slice(&layer.desc.scatter);
                out[84..88].copy_from_slice(&layer.desc.piercing);
                out[88..92].copy_from_slice(&layer.desc.scale_diffuse_fill_ambient);
                out[92..96].copy_from_slice(&layer.desc.wrap_lighting);

                let mut anim_combine = [0.0_f32; 4];
                let mut anim_sculpt = [0.0_f32; 4];
                let mut anim_weights = [0.0_f32; 4];
                for index in 0..3 {
                    let uv = layer.desc.uv_layers[index];
                    let available = index == 0
                        || (index == 1 && layer.textures.detail_present[0])
                        || (index == 2 && layer.textures.detail_present[1]);
                    if !uv.enabled || !available {
                        continue;
                    }
                    anim_weights[index] = uv.weight;
                    match uv.mode {
                        AtmosphericCloudAnimMode::Combine => anim_combine[index] = 1.0,
                        AtmosphericCloudAnimMode::Sculpt => anim_sculpt[index] = 1.0,
                    }
                }
                // Original gAnimCombine/gAnimSculpt are float3 fields.
                // Preserve sculpt.xyz as pure source semantics and use the
                // otherwise-unused combine.w only as the renderer variant bit.
                anim_combine[3] = if layer.textures.detail_present == [true, true] {
                    1.0
                } else {
                    0.0
                };
                out[96..100].copy_from_slice(&anim_combine);
                out[100..104].copy_from_slice(&anim_sculpt);
                out[104..108].copy_from_slice(&anim_weights);

                out[108] = layer.desc.rescale_uv[0][0];
                out[109] = layer.desc.rescale_uv[0][1];
                out[110] = layer.desc.rescale_uv[1][0];
                out[111] = layer.desc.rescale_uv[1][1];
                out[112] = layer.desc.rescale_uv[2][0];
                out[113] = layer.desc.rescale_uv[2][1];
                out[114] = layer.desc.layer_anim_scale[0][0];
                out[115] = layer.desc.layer_anim_scale[0][1];
                out[116] = layer.desc.layer_anim_scale[1][0];
                out[117] = layer.desc.layer_anim_scale[1][1];
                out[118] = layer.desc.layer_anim_scale[2][0];
                out[119] = layer.desc.layer_anim_scale[2][1];

                // Keep CloudHat lighting on the same atmospheric state as the
                // procedural sky. These values deliberately extend the
                // CloudsPS-compatible material closure instead of baking GTA
                // weather decisions into the renderer.
                out[120] = self.sky_atmosphere.cloud_twilight_shadow[0];
                out[121] = self.sky_atmosphere.cloud_twilight_shadow[1];
                out[122] = self.sky_atmosphere.cloud_twilight_shadow[2];
                out[123] = 0.0;

                out[124] = self.sky_atmosphere.cloud_twilight_light[0];
                out[125] = self.sky_atmosphere.cloud_twilight_light[1];
                out[126] = self.sky_atmosphere.cloud_twilight_light[2];
                out[127] = 0.0;

                out[128..132].copy_from_slice(&self.sky_atmosphere.twilight_altitudes);
                out[132] = self.sky_atmosphere.daylight_altitudes[0];
                out[133] = self.sky_atmosphere.daylight_altitudes[1];
                out[134] = self.sky_atmosphere.horizon_power;
                out[135] = 0.0;

                out[136] = self.sky_atmosphere.silver_lining_tint[0];
                out[137] = self.sky_atmosphere.silver_lining_tint[1];
                out[138] = self.sky_atmosphere.silver_lining_tint[2];
                out[139] = self.sky_atmosphere.silver_lining_strength;

                out[140] = self.scene_environment.fog_color[0];
                out[141] = self.scene_environment.fog_color[1];
                out[142] = self.scene_environment.fog_color[2];
                out[143] = if self.scene_environment.fog_enabled {
                    self.scene_environment.fog_density
                } else {
                    0.0
                };
                out[144] = self.scene_environment.fog_start_distance;
                out[145] = self.scene_environment.fog_height_falloff;
                out[146] = self.scene_environment.fog_base_height;
                out[147] = if self.scene_environment.fog_enabled {
                    self.scene_environment.fog_max_opacity
                } else {
                    0.0
                };

                out[148] = self.scene_environment.haze_color[0];
                out[149] = self.scene_environment.haze_color[1];
                out[150] = self.scene_environment.haze_color[2];
                out[151] = self.scene_environment.haze_density;
                out[152] = self.scene_environment.haze_start_distance;
                out[153] = 0.0;
                out[154] = 0.0;
                out[155] = 0.0;

                out[156..160].copy_from_slice(&self.cloudhat_keyframe.cloud_color);
                out[160..164].copy_from_slice(&self.cloudhat_keyframe.cloud_light_color);
                out[164..168].copy_from_slice(&self.cloudhat_keyframe.cloud_ambient_color);
                out[168..172].copy_from_slice(&self.cloudhat_keyframe.cloud_sky_color);
                out[172..176].copy_from_slice(&self.cloudhat_keyframe.cloud_bounce_color);
                out[176..180].copy_from_slice(&self.cloudhat_keyframe.cloud_east_color);
                out[180..184].copy_from_slice(&self.cloudhat_keyframe.cloud_west_color);
                out[184..188].copy_from_slice(&self.cloudhat_keyframe.scale_fill_colors);
                out[188..192]
                    .copy_from_slice(&self.cloudhat_keyframe.density_shift_scale_scattering);
                out[192..196].copy_from_slice(&self.cloudhat_keyframe.piercing_light);
                out[196..200]
                    .copy_from_slice(&self.cloudhat_keyframe.scale_diffuse_fill_ambient_wrap);
                out[200] = if self.cloudhat_keyframe.enabled {
                    1.0
                } else {
                    0.0
                };
                out[201] = 0.0;
                out[202] = 0.0;
                out[203] = 0.0;
                out
            })
            .collect()
    }

    pub(super) fn atmospheric_soft_depth_uniform(
        &self,
        aspect: f32,
    ) -> [f32; ATMOSPHERIC_SOFT_DEPTH_UNIFORM_FLOATS] {
        let mut out = [0.0_f32; ATMOSPHERIC_SOFT_DEPTH_UNIFORM_FLOATS];
        out[0..16].copy_from_slice(&camera_view_projection(&self.camera, aspect));
        out[16] = self.camera.position.x;
        out[17] = self.camera.position.y;
        out[18] = self.camera.position.z;
        out[19] = self.camera.far;
        out
    }

    pub(super) fn atmospheric_sun(&self) -> ([f32; 3], f32) {
        let mut result = ([0.0, 1.0, 0.0], 0.0_f32);
        for (key, visual) in &self.sky_visuals {
            if !visual.atmosphere_driver || visual.intensity < result.1 {
                continue;
            }
            let Some(id) = self
                .runtime_entity_ids
                .get(key)
                .copied()
                .map(SceneEntityId)
                .or_else(|| self.world.entity_id_by_name(key))
            else {
                continue;
            };
            let Some(entity) = self.world.entity(id) else {
                continue;
            };
            if entity.lifecycle != SceneLifecycle::Active {
                continue;
            }
            let direction = sky_visual_direction(entity.transform.rotation_degrees);
            result = ([direction.x, direction.y, direction.z], visual.intensity);
        }
        result
    }

    pub(super) fn atmospheric_cloud_runtime_state(&self) -> Value {
        let Some(resources) = self.atmospheric_clouds.as_ref() else {
            return json!({"configured": false});
        };
        let layers = resources
            .layers
            .iter()
            .zip(self.atmospheric_cloud_runtime.iter())
            .map(|(layer, runtime)| {
                json!({
                    "id": layer.desc.id,
                    "model": layer.model_name,
                    "alpha": runtime.alpha,
                    "resident": runtime.resident,
                    "transition_direction": match runtime.transition_direction {
                        AtmosphericCloudTransitionDirection::In => "in",
                        AtmosphericCloudTransitionDirection::Out => "out",
                    },
                    "transition_target": runtime.transition_target,
                    "request_elapsed": runtime.request_elapsed,
                    "fade_elapsed": runtime.fade_elapsed,
                    "transition_duration": runtime.transition_duration,
                    "transition_delay": runtime.transition_delay,
                    "cost_factor": layer.desc.cost_factor,
                    "soft_intersection_distance": layer.desc.soft_intersection_distance,
                    "soft_depth_domain": "camera_view_depth",
                    "effective_altitude_alpha": altitude_visibility(&layer.desc, self.camera.position.y),
                    "angular_accum_degrees": runtime.angular_accum_degrees,
                    "uv_accum": runtime.uv_accum,
                    "script_override": runtime.override_target.map(|(alpha, transition_seconds)| {
                        json!({"alpha": alpha, "transition_seconds": transition_seconds})
                    })
                })
            })
            .collect::<Vec<_>>();
        json!({
            "configured": true,
            "enabled": resources.enabled,
            "cloud_hat_speed": resources.cloud_hat_speed,
            "wind_min_speed": resources.wind_min_speed,
            "wind_max_speed": resources.wind_max_speed,
            "altitude_scroll_scale": resources.altitude_scroll_scale,
            "global_alpha": resources.global_alpha,
            "transition_midpoint": resources.transition_midpoint,
            "transition_alpha_range": resources.transition_alpha_range,
            "streaming_budget": resources.streaming_budget,
            "soft_depth_resolution": resources.soft_depth_resolution,
            "layers": layers
        })
    }
}

fn can_admit_cloud_layer(
    available_cost: f32,
    cost_factor: f32,
    request_elapsed: f32,
    transition_delay: f32,
) -> bool {
    request_elapsed + 1.0e-6 >= transition_delay && available_cost + 1.0e-6 >= cost_factor
}

fn begin_transition(
    runtime: &mut AtmosphericCloudLayerRuntime,
    desc: &AtmosphericCloudLayerDesc,
    target: f32,
    requested_seconds: f32,
) {
    let direction = if target > runtime.alpha {
        AtmosphericCloudTransitionDirection::In
    } else {
        AtmosphericCloudTransitionDirection::Out
    };
    let duration_percent = match direction {
        AtmosphericCloudTransitionDirection::In => desc.transition_in_time_percent,
        AtmosphericCloudTransitionDirection::Out => desc.transition_out_time_percent,
    };
    runtime.transition_from = runtime.alpha;
    runtime.transition_target = target;
    runtime.request_elapsed = 0.0;
    runtime.fade_elapsed = 0.0;
    runtime.transition_duration = (requested_seconds * duration_percent).max(0.0);
    // GTA deliberately uses the transition-in delay percentage for both the
    // transition-in and transition-out paths.
    runtime.transition_delay = (requested_seconds * desc.transition_delay_percent).max(0.0);
    runtime.transition_direction = direction;
    runtime.last_desired_target = target;
}

fn advance_transition(
    runtime: &mut AtmosphericCloudLayerRuntime,
    transition_midpoint: f32,
    dt: f32,
) {
    if runtime.transition_duration <= 1.0e-6 {
        runtime.fade_elapsed = runtime.transition_duration;
        runtime.alpha = runtime.transition_target;
        return;
    }
    runtime.fade_elapsed = (runtime.fade_elapsed + dt).min(runtime.transition_duration);
    let raw_t = (runtime.fade_elapsed / runtime.transition_duration).clamp(0.0, 1.0);
    let shaped_t = remap_transition_midpoint(raw_t, transition_midpoint);
    runtime.alpha =
        runtime.transition_from + (runtime.transition_target - runtime.transition_from) * shaped_t;
}

fn transition_complete(runtime: &AtmosphericCloudLayerRuntime) -> bool {
    runtime.transition_duration <= 1.0e-6
        || runtime.fade_elapsed + 1.0e-6 >= runtime.transition_duration
}

fn remap_transition_midpoint(t: f32, midpoint: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if !(midpoint > 0.0 && midpoint < 1.0) {
        return t;
    }
    let first_half = (t / midpoint).clamp(0.0, 1.0) * 0.5;
    let second_half = ((t - midpoint) / (1.0 - midpoint)).clamp(0.0, 1.0) * 0.5;
    first_half + second_half
}

fn transition_alpha_scale(alpha: f32, range: f32) -> f32 {
    if range > 0.0 {
        (alpha / range).clamp(0.0, 1.0)
    } else {
        1.0
    }
}

fn weather_target_alpha(
    desc: &AtmosphericCloudLayerDesc,
    weather: &WeatherBackendState,
    effects: &WeatherEffectsState,
) -> f32 {
    if desc.weather_weights.is_empty() {
        return 1.0;
    }
    let weight = |name: &str| {
        desc.weather_weights
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| *value)
            .or_else(|| desc.weather_weights.get("*").copied())
            .unwrap_or(0.0)
    };
    let selected = |variant: &str| {
        let variant = variant.trim();
        if !variant
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("gtav."))
        {
            return 1.0;
        }
        let prefix = format!("{variant}.");
        desc.id
            .get(..prefix.len())
            .is_some_and(|value| value.eq_ignore_ascii_case(&prefix))
            .then_some(1.0)
            .unwrap_or(0.0)
    };

    let current = weight(&weather.current) * selected(&effects.current_cloud_variant);
    let next = if weather.next.trim().is_empty() {
        current
    } else {
        weight(&weather.next) * selected(&effects.next_cloud_variant)
    };
    current + (next - current) * weather.blend.clamp(0.0, 1.0)
}

fn altitude_visibility(desc: &AtmosphericCloudLayerDesc, camera_height: f32) -> f32 {
    let mut alpha = 1.0_f32;
    if let Some(min) = desc.altitude_min {
        if camera_height < min {
            return 0.0;
        }
        if desc.altitude_min_fade > 0.0 {
            alpha *= ((camera_height - min) / desc.altitude_min_fade).clamp(0.0, 1.0);
        }
    }
    if let Some(max) = desc.altitude_max {
        if camera_height > max {
            return 0.0;
        }
        if desc.altitude_max_fade > 0.0 {
            alpha *= ((max - camera_height) / desc.altitude_max_fade).clamp(0.0, 1.0);
        }
    }
    alpha
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer() -> AtmosphericCloudLayerDesc {
        AtmosphericCloudLayerDesc {
            id: "test".to_owned(),
            position: [0.0; 3],
            rotation_degrees: [0.0; 3],
            scale: [1.0; 3],
            angular_velocity_degrees: [0.0; 3],
            rotation_scale: 1.0,
            camera_position_scale: [1.0, 0.0, 1.0],
            altitude_min: Some(100.0),
            altitude_min_fade: 50.0,
            altitude_max: Some(300.0),
            altitude_max_fade: 50.0,
            transition_seconds: 5.0,
            transition_in_time_percent: 1.0,
            transition_out_time_percent: 1.0,
            transition_delay_percent: 0.0,
            transition_midpoint: 0.5,
            transition_alpha_range: 0.0,
            cost_factor: 0.5,
            soft_intersection_distance: 12.0,
            density: 0.5,
            softness: 0.1,
            opacity: 1.0,
            color: [1.0; 3],
            density_shift_scale: [0.0, 1.0, 0.0, 0.0],
            scatter: [-0.75, 0.5625, 2.1, 1.0],
            piercing: [1.0; 4],
            scale_diffuse_fill_ambient: [1.0, 1.0, 1.0, 0.0],
            wrap_lighting: [1.0, 1.0, 1.0, 0.0],
            rescale_uv: [[1.0, 1.0]; 3],
            layer_anim_scale: [[1.0, 1.0]; 3],
            weather_weights: BTreeMap::from([("clear".to_owned(), 0.2), ("rain".to_owned(), 1.0)]),
            uv_layers: [AtmosphericCloudUvLayerDesc::default(); 3],
        }
    }

    #[test]
    fn altitude_gate_fades_at_both_boundaries() {
        let layer = layer();
        assert_eq!(altitude_visibility(&layer, 90.0), 0.0);
        assert!((altitude_visibility(&layer, 125.0) - 0.5).abs() < 1.0e-6);
        assert_eq!(altitude_visibility(&layer, 200.0), 1.0);
        assert!((altitude_visibility(&layer, 275.0) - 0.5).abs() < 1.0e-6);
        assert_eq!(altitude_visibility(&layer, 310.0), 0.0);
    }

    #[test]
    fn weather_weights_crossfade() {
        let layer = layer();
        let weather = WeatherBackendState {
            current: "clear".to_owned(),
            next: "rain".to_owned(),
            blend: 0.5,
        };
        assert!(
            (weather_target_alpha(&layer, &weather, &WeatherEffectsState::default()) - 0.6).abs()
                < 1.0e-6
        );
    }

    #[test]
    fn gta_cloud_variant_filters_fragment_and_crossfades_selection() {
        let mut current_layer = layer();
        current_layer.id = "gtav.02.00.horizonring.m0".to_owned();
        current_layer.weather_weights =
            BTreeMap::from([("clear".to_owned(), 1.0), ("rain".to_owned(), 1.0)]);

        let mut next_layer = current_layer.clone();
        next_layer.id = "gtav.03.00.horizonring.m0".to_owned();

        let weather = WeatherBackendState {
            current: "clear".to_owned(),
            next: "rain".to_owned(),
            blend: 0.25,
        };
        let effects = WeatherEffectsState {
            current_cloud_variant: "gtav.02".to_owned(),
            next_cloud_variant: "gtav.03".to_owned(),
            ..WeatherEffectsState::default()
        };

        assert!((weather_target_alpha(&current_layer, &weather, &effects) - 0.75).abs() < 1.0e-6);
        assert!((weather_target_alpha(&next_layer, &weather, &effects) - 0.25).abs() < 1.0e-6);
    }

    #[test]
    fn transition_midpoint_remap_matches_reference_piecewise_curve() {
        assert!((remap_transition_midpoint(0.25, 0.25) - 0.5).abs() < 1.0e-6);
        assert!((remap_transition_midpoint(0.625, 0.25) - 0.75).abs() < 1.0e-6);
        assert!((remap_transition_midpoint(0.25, 0.5) - 0.25).abs() < 1.0e-6);
        assert_eq!(remap_transition_midpoint(0.75, 0.0), 0.75);
        assert_eq!(remap_transition_midpoint(0.75, 1.0), 0.75);
    }

    #[test]
    fn transition_alpha_range_scales_only_early_alpha() {
        assert!((transition_alpha_scale(0.1, 0.2) - 0.5).abs() < 1.0e-6);
        assert_eq!(transition_alpha_scale(0.3, 0.2), 1.0);
        assert_eq!(transition_alpha_scale(0.1, 0.0), 1.0);
    }

    #[test]
    fn streaming_admission_requires_both_delay_and_cost() {
        assert!(!can_admit_cloud_layer(1.0, 0.4, 1.9, 2.0));
        assert!(!can_admit_cloud_layer(0.39, 0.4, 2.0, 2.0));
        assert!(can_admit_cloud_layer(0.4, 0.4, 2.0, 2.0));
    }

    #[test]
    fn transition_out_keeps_reference_incoming_delay_percentage() {
        let mut desc = layer();
        desc.transition_in_time_percent = 0.75;
        desc.transition_out_time_percent = 0.60;
        desc.transition_delay_percent = 0.20;
        let mut runtime = AtmosphericCloudLayerRuntime {
            alpha: 0.0,
            angular_accum_degrees: [0.0; 3],
            uv_accum: [[0.0; 2]; 3],
            override_target: None,
            resident: false,
            last_desired_target: 0.0,
            transition_from: 0.0,
            transition_target: 0.0,
            request_elapsed: 0.0,
            fade_elapsed: 0.0,
            transition_duration: 0.0,
            transition_delay: 0.0,
            transition_direction: AtmosphericCloudTransitionDirection::Out,
        };

        begin_transition(&mut runtime, &desc, 1.0, 10.0);
        assert_eq!(
            runtime.transition_direction,
            AtmosphericCloudTransitionDirection::In
        );
        assert!((runtime.transition_duration - 7.5).abs() < 1.0e-6);
        assert!((runtime.transition_delay - 2.0).abs() < 1.0e-6);

        runtime.alpha = 1.0;
        runtime.last_desired_target = 1.0;
        begin_transition(&mut runtime, &desc, 0.0, 10.0);
        assert_eq!(
            runtime.transition_direction,
            AtmosphericCloudTransitionDirection::Out
        );
        assert!((runtime.transition_duration - 6.0).abs() < 1.0e-6);
        assert!((runtime.transition_delay - 2.0).abs() < 1.0e-6);
    }
}
