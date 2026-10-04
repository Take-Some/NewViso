use super::*;

impl Scene3dRuntime {
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn set_clear_color(&mut self, clear_color: [f32; 4]) {
        self.clear_color = clear_color;
    }
    pub fn set_sky_time_seconds(&mut self, seconds: f32) -> Result<(), String> {
        if !seconds.is_finite() {
            return Err("scene sky time must be finite".to_owned());
        }
        self.sky_time_seconds = seconds.rem_euclid(self.timecycle_backend.duration_seconds);
        self.sky_cloud_cycle_time_days =
            self.sky_time_seconds / self.timecycle_backend.duration_seconds;
        Ok(())
    }
    pub fn set_sky_time_scale(&mut self, scale: f32) -> Result<(), String> {
        if !scale.is_finite() || scale.abs() > 100_000.0 {
            return Err("scene sky time scale is invalid".to_owned());
        }
        self.sky_time_scale = scale;
        Ok(())
    }

    pub fn timecycle_backend(&self) -> &TimeCycleBackendState {
        &self.timecycle_backend
    }

    pub fn set_timecycle_backend(
        &mut self,
        cycle_seconds: f32,
        phase: f32,
        rate: f32,
        duration_seconds: f32,
    ) -> Result<(), String> {
        if !cycle_seconds.is_finite()
            || cycle_seconds < 0.0
            || !phase.is_finite()
            || !(0.0..=1.0).contains(&phase)
            || !rate.is_finite()
            || rate.abs() > 100_000.0
            || !duration_seconds.is_finite()
            || duration_seconds <= 0.0
        {
            return Err("invalid generic timecycle backend state".to_owned());
        }

        self.timecycle_backend = TimeCycleBackendState {
            cycle_seconds,
            phase,
            rate,
            duration_seconds,
        };
        Ok(())
    }

    pub fn weather_backend(&self) -> &WeatherBackendState {
        &self.weather_backend
    }

    pub fn weather_effects(&self) -> &WeatherEffectsState {
        &self.weather_effects
    }

    pub fn surface_weather_state(&self) -> (f32, f32) {
        (
            self.weather_wetness
                .max(self.weather_effects.rain.clamp(0.0, 1.0))
                .clamp(0.0, 1.0),
            self.weather_effects
                .snow
                .max(self.weather_effects.snow_mist)
                .clamp(0.0, 1.0),
        )
    }

    pub fn set_weather_effects(&mut self, state: WeatherEffectsState) -> Result<(), String> {
        let strings = [
            &state.current_cloud_settings,
            &state.next_cloud_settings,
            &state.current_timecycle,
            &state.next_timecycle,
            &state.current_drop_setting,
            &state.next_drop_setting,
            &state.current_mist_setting,
            &state.next_mist_setting,
            &state.current_ground_setting,
            &state.next_ground_setting,
            &state.current_cloud_variant,
            &state.next_cloud_variant,
        ];
        if strings
            .iter()
            .any(|value| value.len() > 256 || value.chars().any(char::is_control))
        {
            return Err("invalid generic weather effects string field".to_owned());
        }

        let scalars = [
            state.sun,
            state.cloud,
            state.wind_min,
            state.wind_max,
            state.wind_speed,
            state.rain,
            state.snow,
            state.snow_mist,
            state.fog,
            state.ripple_bumpiness,
            state.ripple_min_bumpiness,
            state.ripple_max_bumpiness,
            state.ripple_bumpiness_wind_scale,
            state.ripple_scale,
            state.ripple_speed,
            state.ripple_velocity_transfer,
            state.ocean_bumpiness,
            state.deep_ocean_scale,
            state.ocean_noise_min_amplitude,
            state.ocean_wave_amplitude,
            state.shore_wave_amplitude,
            state.ocean_wave_wind_scale,
            state.shore_wave_wind_scale,
            state.ocean_wave_min_amplitude,
            state.shore_wave_min_amplitude,
            state.ocean_wave_max_amplitude,
            state.shore_wave_max_amplitude,
            state.ocean_foam_intensity,
            state.ocean_foam_scale,
            state.ripple_disturb,
            state.lightning,
            state.sandstorm,
        ];
        if scalars.iter().any(|value| !value.is_finite())
            || state.wind_direction.iter().any(|value| !value.is_finite())
            || state.wind_min < 0.0
            || state.wind_max < state.wind_min
            || state.wind_speed < 0.0
            || [
                state.sun,
                state.cloud,
                state.rain,
                state.snow,
                state.snow_mist,
                state.fog,
                state.lightning,
                state.sandstorm,
            ]
            .iter()
            .any(|value| *value < 0.0 || *value > 4.0)
        {
            return Err("invalid generic weather effects numeric field".to_owned());
        }

        self.weather_effects = state;
        Ok(())
    }

    pub fn cloudhat_keyframe(&self) -> CloudHatKeyframeState {
        self.cloudhat_keyframe
    }

    pub fn set_cloudhat_keyframe(&mut self, state: CloudHatKeyframeState) -> Result<(), String> {
        let vectors = [
            state.cloud_color,
            state.cloud_light_color,
            state.cloud_ambient_color,
            state.cloud_sky_color,
            state.cloud_bounce_color,
            state.cloud_east_color,
            state.cloud_west_color,
            state.scale_fill_colors,
            state.density_shift_scale_scattering,
            state.piercing_light,
            state.scale_diffuse_fill_ambient_wrap,
        ];
        if vectors
            .iter()
            .flatten()
            .any(|value| !value.is_finite() || value.abs() > 256.0)
        {
            return Err("invalid CloudHat keyframe value".to_owned());
        }
        self.cloudhat_keyframe = state;
        Ok(())
    }

    pub fn set_weather_backend(
        &mut self,
        current: &str,
        next: &str,
        blend: f32,
    ) -> Result<(), String> {
        let current = current.trim();
        let next = next.trim();
        if current.is_empty()
            || next.is_empty()
            || current.len() > 128
            || next.len() > 128
            || !blend.is_finite()
            || !(0.0..=1.0).contains(&blend)
        {
            return Err("invalid generic weather backend state".to_owned());
        }

        self.weather_backend = WeatherBackendState {
            current: current.to_owned(),
            next: next.to_owned(),
            blend,
        };
        Ok(())
    }

    pub fn focus_position(&self) -> [f32; 3] {
        let focus = self.world.focus().position;
        [focus.x, focus.y, focus.z]
    }

    pub fn solid_aabbs(&self) -> Vec<([f32; 3], [f32; 3])> {
        self.world
            .solid_bounds()
            .map(|bounds| {
                (
                    [bounds.min.x, bounds.min.y, bounds.min.z],
                    [bounds.max.x, bounds.max.y, bounds.max.z],
                )
            })
            .collect()
    }
    pub fn physics_static_solid_colliders_near(
        &self,
        interests: &[([f32; 3], [f32; 3])],
    ) -> Vec<(u64, [f32; 3], [f32; 3])> {
        let interests = interests
            .iter()
            .map(|(min, max)| SceneBounds {
                min: Vec3::new(min[0], min[1], min[2]),
                max: Vec3::new(max[0], max[1], max[2]),
            })
            .collect::<Vec<_>>();
        self.world
            .static_solid_colliders_near(&interests)
            .into_iter()
            .map(|(id, bounds)| {
                (
                    id.0,
                    [bounds.min.x, bounds.min.y, bounds.min.z],
                    [bounds.max.x, bounds.max.y, bounds.max.z],
                )
            })
            .collect()
    }

    pub fn physics_static_solid_aabbs_near(
        &self,
        interests: &[([f32; 3], [f32; 3])],
    ) -> Vec<([f32; 3], [f32; 3])> {
        self.physics_static_solid_colliders_near(interests)
            .into_iter()
            .map(|(_, min, max)| (min, max))
            .collect()
    }

    /// Compact hot-path snapshot for gameplay scripting.
    /// Full diagnostic collections stay in runtime_state().
    pub fn script_frame_state(&self) -> Value {
        let focus = self.world.focus();
        json!({
            "scene": {
                "particles": self.particle_runtime_state(),
                "world": {
                    "frame": self.frame_plan.frame,
                    "process_active": self.world.process_active_count(),
                    "process_due": self.world.process_due_count(),
                    "process_budget": self.world.process_effective_budget(),
                    "process_scanned": self.world.process_scanned_count(),
                    "process_work": self.process_work_state(),
                    "resident": self.frame_plan.resident_count,
                    "stream_requests": self.frame_plan.requested_entities.len(),
                    "focus": {
                        "position": [focus.position.x, focus.position.y, focus.position.z],
                        "velocity": [focus.velocity.x, focus.velocity.y, focus.velocity.z]
                    }
                },
                "camera": {
                    "position": {
                        "x": self.camera.position.x,
                        "y": self.camera.position.y,
                        "z": self.camera.position.z
                    },
                    "target": {
                        "x": self.camera.target.x,
                        "y": self.camera.target.y,
                        "z": self.camera.target.z
                    },
                    "up": {
                        "x": self.camera.up.x,
                        "y": self.camera.up.y,
                        "z": self.camera.up.z
                    },
                    "fov_y_degrees": self.camera.fov_y_degrees,
                    "near": self.camera.near,
                    "far": self.camera.far,
                    "orbit": {
                        "yaw_radians": self.orbit.yaw,
                        "pitch_radians": self.orbit.pitch,
                        "distance": self.orbit.distance
                    }
                },
                "timecycle": {
                    "cycle_seconds": self.timecycle_backend.cycle_seconds,
                    "phase": self.timecycle_backend.phase,
                    "rate": self.timecycle_backend.rate,
                    "duration_seconds": self.timecycle_backend.duration_seconds
                },
                "weather": {
                    "current": self.weather_backend.current,
                    "next": self.weather_backend.next,
                    "blend": self.weather_backend.blend,
                    "effects": self.weather_effects_json()
                }
            }
        })
    }

    pub fn runtime_state(&self) -> Value {
        let focus = self.world.focus();
        let focus_source = match focus.source {
            SceneFocusSource::Camera => "camera",
            SceneFocusSource::Entity(_) => "entity",
            SceneFocusSource::Override => "override",
        };
        let solids = self
            .world
            .solid_bounds()
            .map(|bounds| {
                json!({
                    "min": [bounds.min.x, bounds.min.y, bounds.min.z],
                    "max": [bounds.max.x, bounds.max.y, bounds.max.z]
                })
            })
            .collect::<Vec<_>>();
        let lights = self.world.active_lights();
        let shadow_casters = lights
            .iter()
            .filter(|(_, _, light)| light.casts_shadows)
            .count();
        let visible_asset_instances = self
            .frame_plan
            .visible_entities
            .iter()
            .filter(|id| self.asset_meshes.contains_key(&id.0))
            .count();
        let visible_unique_models = self
            .frame_plan
            .visible_entities
            .iter()
            .filter_map(|id| self.asset_meshes.get(&id.0).map(|mesh| mesh.model_id.0))
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let portal_visibility = self
            .portal_visibility
            .as_ref()
            .map(PortalVisibilityGraph::telemetry)
            .unwrap_or_else(|| {
                json!({
                    "rooms": 0,
                    "portals": 0,
                    "mapped_entities": 0,
                    "visible_rooms": []
                })
            });

        let light_state = lights
            .iter()
            .map(|(id, transform, light)| {
                let light_type = match light.light_type {
                    LightType::Directional => "directional",
                    LightType::Point => "point",
                    LightType::Spot => "spot",
                    LightType::Area => "area",
                };
                json!({
                    "entity": id.0,
                    "type": light_type,
                    "rotation_degrees": [
                        transform.rotation_degrees.x,
                        transform.rotation_degrees.y,
                        transform.rotation_degrees.z
                    ],
                    "casts_shadows": light.casts_shadows
                })
            })
            .collect::<Vec<_>>();

        json!({
            "scene": {
                "title": self.title,
                "render_settings": self.render_policy,
                "mesh_count": self.cubes.len(),
                "world": {
                    "frame": self.frame_plan.frame,
                    "entities": self.world.entity_count(),
                    "static_entities": self.world.static_count(),
                    "dynamic_entities": self.world.dynamic_count(),
                    "process_active": self.world.process_active_count(),
                    "process_due": self.world.process_due_count(),
                    "process_budget": self.world.process_effective_budget(),
                    "process_scanned": self.world.process_scanned_count(),
                    "process_work": self.process_work_state(),
                    "spatial_cells": self.world.spatial_cell_count(),
                    "spatial_oversized": self.world.spatial_oversized_count(),
                    "spatial_candidates": self.frame_plan.spatial_candidate_count,
                    "visible": self.frame_plan.visible_count,
                    "culled": self.frame_plan.culled_count,
                    "resident": self.frame_plan.resident_count,
                    "stream_requests": self.frame_plan.requested_entities.len(),
                    "solids": solids,
                    "focus": {
                        "source": focus_source,
                        "position": [focus.position.x, focus.position.y, focus.position.z],
                        "velocity": [focus.velocity.x, focus.velocity.y, focus.velocity.z]
                    }
                },
                "camera": {
                    "position": {
                        "x": self.camera.position.x,
                        "y": self.camera.position.y,
                        "z": self.camera.position.z
                    },
                    "target": {
                        "x": self.camera.target.x,
                        "y": self.camera.target.y,
                        "z": self.camera.target.z
                    },
                    "up": {
                        "x": self.camera.up.x,
                        "y": self.camera.up.y,
                        "z": self.camera.up.z
                    },
                    "fov_y_degrees": self.camera.fov_y_degrees,
                    "near": self.camera.near,
                    "far": self.camera.far,
                    "orbit": {
                        "yaw_radians": self.orbit.yaw,
                        "pitch_radians": self.orbit.pitch,
                        "yaw_degrees": self.orbit.yaw.to_degrees(),
                        "pitch_degrees": self.orbit.pitch.to_degrees(),
                        "distance": self.orbit.distance
                    }
                },
                "main_view_geometry": self.main_view_geometry_state(),
                "visibility": {
                    "portal": portal_visibility,
                    "installed_asset_instances": self.asset_meshes.len(),
                    "unique_asset_models": self.asset_model_gpu_ranges.len(),
                    "visible_asset_instances": visible_asset_instances,
                    "visible_unique_models": visible_unique_models,
                    "instance_cell_size": ASSET_INSTANCE_CELL_SIZE,
                    "gpu_instance_static_count": self.gpu_instance_table.instance_data.len() / INSTANCE_FLOATS,
                    "gpu_instance_resident_batches": self.gpu_instance_table.batches.len(),
                    "gpu_instance_rebuilds": self.gpu_instance_table.rebuild_count,
                    "gpu_instance_uploads": self.gpu_instance_table.upload_count,
                    "gpu_instance_generation": self.gpu_instance_table.generation,
                    "gpu_instance_uploaded_slots": self.gpu_instance_table
                        .uploaded_generation
                        .iter()
                        .filter(|generation| **generation == self.gpu_instance_table.generation)
                        .count(),
                    "gpu_instance_slot_generations": self.gpu_instance_table.uploaded_generation,
                    "hiz_candidate_capacity": MAX_HIZ_DRAW_CANDIDATES,
                    "submission": {
                        "instance_batches": self.last_submission_stats.instance_batches,
                        "instances": self.last_submission_stats.instance_count,
                        "hiz_draws": self.last_submission_stats.hiz_draws,
                        "opaque_indirect_groups": self.last_submission_stats.opaque_indirect_groups,
                        "direct_opaque_draws": self.last_submission_stats.direct_opaque_draws,
                        "alpha_draws": self.last_submission_stats.alpha_draws,
                        "graph_executed_passes": self.last_submission_stats.graph_executed_passes,
                        "graph_skipped_passes": self.last_submission_stats.graph_skipped_passes,
                        "graph_cpu_record_ms": self.last_submission_stats.graph_cpu_record_ms
                    }
                },
                "lighting": {
                    "active_lights": lights.len(),
                    "shadow_casters": shadow_casters,
                    "lights": light_state
                },
                "sky_visuals": {
                    "count": self.sky_visuals.len(),
                    "ids": self.sky_visuals.keys().cloned().collect::<Vec<_>>()
                },
                "lens_flares": {
                    "count": self.lens_flares.len(),
                    "ids": self.lens_flares.keys().cloned().collect::<Vec<_>>()
                },
                "sky_clouds": {
                    "enabled": self.sky_clouds.enabled,
                    "coverage": self.sky_clouds.coverage,
                    "density": self.sky_clouds.density,
                    "softness": self.sky_clouds.softness,
                    "scale": self.sky_clouds.scale,
                    "detail_scale": self.sky_clouds.detail_scale,
                    "speed": self.sky_clouds.speed,
                    "horizon_fade": self.sky_clouds.horizon_fade,
                    "macro_scale": self.sky_clouds.macro_scale,
                    "macro_strength": self.sky_clouds.macro_strength,
                    "detail_strength": self.sky_clouds.detail_strength,
                    "micro_strength": self.sky_clouds.micro_strength,
                    "erosion_strength": self.sky_clouds.erosion_strength,
                    "warp_strength": self.sky_clouds.warp_strength,
                    "shape_contrast": self.sky_clouds.shape_contrast,
                    "shear_speed": self.sky_clouds.shear_speed,
                    "seed_offset": self.sky_clouds.seed_offset,
                    "large_speed": self.sky_clouds.large_speed,
                    "small_speed": self.sky_clouds.small_speed,
                    "overall_detail_speed": self.sky_clouds.overall_detail_speed,
                    "edge_detail_speed": self.sky_clouds.edge_detail_speed,
                    "noise_phase_scale": self.sky_clouds.noise_phase_scale,
                    "noise_phase": self.sky_cloud_noise_phase,
                    "cycle_time_days": self.sky_cloud_cycle_time_days,
                    "time_seconds": self.sky_time_seconds,
                    "time_scale": self.sky_time_scale
                },
                "volumetric_clouds": self.volumetric_cloud_runtime_state(),
                "atmospheric_clouds": self.atmospheric_cloud_runtime_state(),
                "cloudhat_keyframe": {
                    "enabled": self.cloudhat_keyframe.enabled,
                    "cloud_color": self.cloudhat_keyframe.cloud_color,
                    "cloud_light_color": self.cloudhat_keyframe.cloud_light_color,
                    "cloud_ambient_color": self.cloudhat_keyframe.cloud_ambient_color,
                    "cloud_sky_color": self.cloudhat_keyframe.cloud_sky_color,
                    "cloud_bounce_color": self.cloudhat_keyframe.cloud_bounce_color,
                    "cloud_east_color": self.cloudhat_keyframe.cloud_east_color,
                    "cloud_west_color": self.cloudhat_keyframe.cloud_west_color,
                    "scale_fill_colors": self.cloudhat_keyframe.scale_fill_colors,
                    "density_shift_scale_scattering":
                        self.cloudhat_keyframe.density_shift_scale_scattering,
                    "piercing_light": self.cloudhat_keyframe.piercing_light,
                    "scale_diffuse_fill_ambient_wrap":
                        self.cloudhat_keyframe.scale_diffuse_fill_ambient_wrap
                },
                "environment": {
                    "ambient_color": self.scene_environment.ambient_color,
                    "ambient_intensity": self.scene_environment.ambient_intensity,
                    "fog": {
                        "enabled": self.scene_environment.fog_enabled,
                        "color": self.scene_environment.fog_color,
                        "density": self.scene_environment.fog_density,
                        "start_distance": self.scene_environment.fog_start_distance,
                        "height_falloff": self.scene_environment.fog_height_falloff,
                        "base_height": self.scene_environment.fog_base_height,
                        "max_opacity": self.scene_environment.fog_max_opacity
                    },
                    "haze": {
                        "color": self.scene_environment.haze_color,
                        "density": self.scene_environment.haze_density,
                        "start_distance": self.scene_environment.haze_start_distance
                    }
                },
                "atmosphere": {
                    "twilight_altitudes": self.sky_atmosphere.twilight_altitudes,
                    "daylight_altitudes": self.sky_atmosphere.daylight_altitudes,
                    "horizon_power": self.sky_atmosphere.horizon_power,
                    "star_intensity": self.sky_atmosphere.star_intensity,
                    "cloud_alpha_range": self.sky_atmosphere.cloud_alpha_range
                },
                "timecycle": {
                    "cycle_seconds": self.timecycle_backend.cycle_seconds,
                    "phase": self.timecycle_backend.phase,
                    "rate": self.timecycle_backend.rate,
                    "duration_seconds": self.timecycle_backend.duration_seconds
                },
                "weather": {
                    "current": self.weather_backend.current,
                    "next": self.weather_backend.next,
                    "blend": self.weather_backend.blend,
                    "effects": self.weather_effects_json(),
                    "gpu_fx": self.weather_runtime_state()
                },
                "transient": {
                    "spheres": self.transient_spheres.len(),
                    "surface_marks": self.surface_marks.len(),
                    "overlay_quads": self.overlay_quads.len(),
                    "particles": self.particles.len(),
                    "particle_textures": self.particle_textures.len(),
                    "particle_model_count": self.particles.iter().filter(|p| p.desc.style.as_ref().is_some_and(|s| s.model.is_some())).count(),
                    "particle_trail_count": self.particles.iter().filter(|p| p.desc.style.as_ref().is_some_and(|s| s.trail)).count()
                }
            }
        })
    }
    fn weather_effects_json(&self) -> Value {
        let state = &self.weather_effects;
        json!({
            "current_cloud_settings": state.current_cloud_settings,
            "next_cloud_settings": state.next_cloud_settings,
            "current_timecycle": state.current_timecycle,
            "next_timecycle": state.next_timecycle,
            "current_drop_setting": state.current_drop_setting,
            "next_drop_setting": state.next_drop_setting,
            "current_mist_setting": state.current_mist_setting,
            "next_mist_setting": state.next_mist_setting,
            "current_ground_setting": state.current_ground_setting,
            "next_ground_setting": state.next_ground_setting,
            "current_cloud_variant": state.current_cloud_variant,
            "next_cloud_variant": state.next_cloud_variant,
            "sun": state.sun,
            "cloud": state.cloud,
            "wind_min": state.wind_min,
            "wind_max": state.wind_max,
            "wind_speed": state.wind_speed,
            "wind_direction": state.wind_direction,
            "rain": state.rain,
            "snow": state.snow,
            "snow_mist": state.snow_mist,
            "fog": state.fog,
            "water": {
                "ripple_bumpiness": state.ripple_bumpiness,
                "ripple_min_bumpiness": state.ripple_min_bumpiness,
                "ripple_max_bumpiness": state.ripple_max_bumpiness,
                "ripple_bumpiness_wind_scale": state.ripple_bumpiness_wind_scale,
                "ripple_scale": state.ripple_scale,
                "ripple_speed": state.ripple_speed,
                "ripple_velocity_transfer": state.ripple_velocity_transfer,
                "ocean_bumpiness": state.ocean_bumpiness,
                "deep_ocean_scale": state.deep_ocean_scale,
                "ocean_noise_min_amplitude": state.ocean_noise_min_amplitude,
                "ocean_wave_amplitude": state.ocean_wave_amplitude,
                "shore_wave_amplitude": state.shore_wave_amplitude,
                "ocean_wave_wind_scale": state.ocean_wave_wind_scale,
                "shore_wave_wind_scale": state.shore_wave_wind_scale,
                "ocean_wave_min_amplitude": state.ocean_wave_min_amplitude,
                "shore_wave_min_amplitude": state.shore_wave_min_amplitude,
                "ocean_wave_max_amplitude": state.ocean_wave_max_amplitude,
                "shore_wave_max_amplitude": state.shore_wave_max_amplitude,
                "ocean_foam_intensity": state.ocean_foam_intensity,
                "ocean_foam_scale": state.ocean_foam_scale,
                "ripple_disturb": state.ripple_disturb
            },
            "lightning": state.lightning,
            "sandstorm": state.sandstorm
        })
    }

    pub fn configure_orbit_controls(
        &mut self,
        rotate_button: u64,
        min_pitch: f32,
        max_pitch: f32,
    ) -> Result<(), String> {
        if !min_pitch.is_finite()
            || !max_pitch.is_finite()
            || min_pitch <= -90.0
            || max_pitch >= 90.0
            || min_pitch >= max_pitch
        {
            return Err("invalid orbit pitch limits".into());
        }
        self.orbit.rotate_button = rotate_button;
        self.orbit.min_pitch_degrees = min_pitch;
        self.orbit.max_pitch_degrees = max_pitch;
        Ok(())
    }

    pub fn configure_orbit(
        &mut self,
        rotate_sensitivity: f32,
        zoom_sensitivity: f32,
        min_distance: f32,
        max_distance: f32,
    ) -> Result<(), String> {
        if ![
            rotate_sensitivity,
            zoom_sensitivity,
            min_distance,
            max_distance,
        ]
        .iter()
        .all(|value| value.is_finite())
            || rotate_sensitivity <= 0.0
            || zoom_sensitivity <= 0.0
            || min_distance <= 0.0
            || max_distance < min_distance
        {
            return Err("invalid orbit camera settings".to_owned());
        }

        self.orbit.rotate_sensitivity = rotate_sensitivity;
        self.orbit.zoom_sensitivity = zoom_sensitivity;
        self.orbit.min_distance = min_distance;
        self.orbit.max_distance = max_distance;
        self.orbit.configured = true;
        self.orbit.distance = self.orbit.distance.clamp(min_distance, max_distance);
        self.camera.position = self.orbit.position(self.camera.target);
        Ok(())
    }
    pub fn set_camera_pose(
        &mut self,
        position: [f32; 3],
        target: [f32; 3],
        up: Option<[f32; 3]>,
        fov_y_degrees: Option<f32>,
    ) -> Result<(), String> {
        if position
            .iter()
            .chain(target.iter())
            .any(|value| !value.is_finite())
        {
            return Err("scene.camera.set contains a non-finite position or target".to_owned());
        }
        let position = Vec3::new(position[0], position[1], position[2]);
        let target = Vec3::new(target[0], target[1], target[2]);
        if target.sub(position).length() < 0.0001 {
            return Err("scene.camera.set target must differ from position".to_owned());
        }

        self.camera.position = position;
        self.camera.target = target;
        if let Some(up) = up {
            if up.iter().any(|value| !value.is_finite()) {
                return Err("scene.camera.set contains a non-finite up vector".to_owned());
            }
            let up = Vec3::new(up[0], up[1], up[2]);
            if up.length() < 0.0001 {
                return Err("scene.camera.set up vector must be non-zero".to_owned());
            }
            self.camera.up = up.normalized();
        }
        if let Some(fov) = fov_y_degrees {
            if !fov.is_finite() || !(1.0..179.0).contains(&fov) {
                return Err(
                    "scene.camera.set fov_y_degrees must be finite and in 1..179".to_owned(),
                );
            }
            self.camera.fov_y_degrees = fov;
        }
        self.orbit.sync_pose(&self.camera);
        self.sync_runtime_camera_to_flecs()
    }
    pub fn spawn_particles(
        &mut self,
        particles: Vec<SceneParticleSpawnDesc>,
    ) -> Result<(), String> {
        let transient = particles
            .iter()
            .filter(|p| {
                !self.physical_particle_debris_enabled
                    || p.style
                        .as_ref()
                        .is_none_or(|s| s.physical_debris_density.is_none())
            })
            .count();
        if self.particles.len().saturating_add(transient) > self.render_policy.particle_capacity {
            return Err(format!(
                "scene.particles.spawn exceeds the configured limit of {}",
                self.render_policy.particle_capacity
            ));
        }
        for desc in particles {
            let finite = desc
                .position
                .iter()
                .chain(desc.velocity.iter())
                .chain(desc.acceleration.iter())
                .chain(desc.size.iter())
                .chain(desc.end_size.iter())
                .chain(desc.color.iter())
                .chain(desc.end_color.iter())
                .all(|value| value.is_finite())
                && desc.lifetime_seconds.is_finite()
                && desc.rotation_degrees.is_finite()
                && desc.angular_velocity_degrees.is_finite();
            if !finite
                || desc.lifetime_seconds <= 0.0
                || desc.lifetime_seconds > 120.0
                || desc.size.iter().any(|value| *value <= 0.0)
                || desc.end_size.iter().any(|value| *value <= 0.0)
                || desc.style.as_ref().is_some_and(|style| !style.valid())
            {
                return Err("scene.particles.spawn contains invalid particle data".to_owned());
            }
            let age_seconds = -desc.style.as_ref().map_or(0.0, |style| style.delay_seconds);
            if self.physical_particle_debris_enabled
                && desc
                    .style
                    .as_ref()
                    .is_some_and(|s| s.physical_debris_density.is_some())
            {
                self.spawn_physical_particle(desc)?;
                continue;
            }
            let trail_history = if desc.style.as_ref().is_some_and(|s| s.trail) {
                vec![desc.position]
            } else {
                Vec::new()
            };
            self.particles.push(SceneRuntimeParticle {
                desc,
                age_seconds,
                trail_history,
                physical: None,
            });
        }
        Ok(())
    }

    pub fn clear_particles(&mut self) {
        self.particles.clear();
        self.particle_interior_stats = ParticleInteriorStats::default();
        self.particle_interior_contacts = 0;
    }

    pub(super) fn update_particles(&mut self, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        self.update_physical_particles(dt);
        let solids = if self
            .particles
            .iter()
            .any(|p| p.desc.style.as_ref().is_some_and(|s| s.collision.is_some()))
        {
            self.world.solid_bounds().collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let interiors = self.particle_interior_volumes();
        for particle in &mut self.particles {
            particle.age_seconds += dt;
            let active_dt = dt.min(particle.age_seconds.max(0.0));
            if active_dt <= 0.0 {
                continue;
            }
            let before = particle.desc.position;
            let style = particle.desc.style.as_ref();
            let t = (particle.age_seconds / particle.desc.lifetime_seconds).clamp(0.0, 1.0);
            let simulation_dt = active_dt * style.and_then(|s| s.motion_rate).unwrap_or(1.0);
            let acceleration = style.map_or(particle.desc.acceleration, |s| {
                sample_particle_curve(&s.acceleration_keys, t, particle.desc.acceleration)
            });
            let drag = style.map_or([0.0; 3], |s| {
                sample_particle_curve(&s.drag_keys, t, [0.0; 3])
            });
            for axis in 0..3 {
                particle.desc.velocity[axis] += acceleration[axis] * simulation_dt;
                particle.desc.velocity[axis] *= (1.0 - drag[axis] * simulation_dt).max(0.0);
                particle.desc.position[axis] += particle.desc.velocity[axis] * simulation_dt;
            }
            if let Some([bounce, radius_multiplier, min_radius, rest_speed]) =
                style.and_then(|s| s.collision)
            {
                let size = sample_particle_curve(&style.unwrap().size_keys, t, particle.desc.size);
                let radius = (size[0].min(size[1]) * 0.5 * radius_multiplier)
                    .max(min_radius)
                    .max(0.001);
                if let Some((position, normal)) =
                    particle_surface_contact(before, particle.desc.position, radius, &solids)
                {
                    particle.desc.position = position;
                    let dot = (0..3)
                        .map(|i| particle.desc.velocity[i] * normal[i])
                        .sum::<f32>();
                    particle.desc.velocity = std::array::from_fn(|i| {
                        (particle.desc.velocity[i] - 2.0 * dot * normal[i]) * bounce
                    });
                    if particle.desc.velocity.iter().map(|v| v * v).sum::<f32>()
                        < rest_speed * rest_speed
                    {
                        particle.desc.velocity = [0.0; 3];
                        particle.desc.angular_velocity_degrees = 0.0;
                        if let Some(style) = particle.desc.style.as_mut() {
                            style.model_rotation = std::array::from_fn(|i| {
                                style.model_rotation[i] + style.model_spin[i] * particle.age_seconds
                            });
                            style.model_spin = [0.0; 3];
                        }
                    }
                }
            }
            for volume in &interiors {
                if let Some((position, normal)) = volume.contact(before, particle.desc.position) {
                    particle.desc.position = position;
                    let inward_speed = (0..3)
                        .map(|i| particle.desc.velocity[i] * normal[i])
                        .sum::<f32>()
                        .min(0.0);
                    for i in 0..3 {
                        particle.desc.velocity[i] -= inward_speed * normal[i];
                    }
                    self.particle_interior_contacts += 1;
                }
            }
            particle.desc.rotation_degrees +=
                particle.desc.angular_velocity_degrees * simulation_dt;
            if particle.desc.style.as_ref().is_some_and(|s| s.trail) {
                particle.trail_history.push(particle.desc.position);
                if particle.trail_history.len() > 16 {
                    particle.trail_history.remove(0);
                }
            }
        }
        self.particles
            .retain(|particle| particle.age_seconds < particle.desc.lifetime_seconds);
    }

    pub fn set_transient_spheres(
        &mut self,
        spheres: Vec<SceneTransientSphere>,
    ) -> Result<(), String> {
        if spheres.len() > self.render_policy.transient_sphere_capacity {
            return Err(format!(
                "scene.transient_spheres.set exceeds the configured limit of {}",
                self.render_policy.transient_sphere_capacity
            ));
        }
        for sphere in &spheres {
            if sphere.position.iter().any(|value| !value.is_finite())
                || sphere
                    .rotation_degrees
                    .iter()
                    .any(|value| !value.is_finite())
                || !sphere.radius.is_finite()
                || sphere.radius <= 0.0
                || sphere.color.iter().any(|value| !value.is_finite())
                || sphere
                    .marker_color
                    .is_some_and(|color| color.iter().any(|value| !value.is_finite()))
                || (sphere.marker_color.is_some()
                    && (sphere
                        .marker_direction
                        .iter()
                        .any(|value| !value.is_finite())
                        || sphere
                            .marker_direction
                            .iter()
                            .map(|value| value * value)
                            .sum::<f32>()
                            <= 1.0e-8
                        || !sphere.marker_threshold.is_finite()
                        || !(-1.0..=1.0).contains(&sphere.marker_threshold)))
            {
                return Err("scene.transient_spheres.set contains invalid sphere data".to_owned());
            }
        }
        self.transient_spheres = spheres;
        Ok(())
    }
    pub fn add_surface_mark(&mut self, mut mark: SceneSurfaceMark) -> Result<(), String> {
        let length_sq = mark.normal.iter().map(|value| value * value).sum::<f32>();
        if mark.position.iter().any(|value| !value.is_finite())
            || mark.normal.iter().any(|value| !value.is_finite())
            || !length_sq.is_finite()
            || length_sq <= 1.0e-8
            || !mark.radius.is_finite()
            || !(0.001..=0.5).contains(&mark.radius)
            || mark.color.iter().any(|value| !value.is_finite())
        {
            return Err("scene.surface_mark.add contains invalid mark data".to_owned());
        }
        let inv_length = length_sq.sqrt().recip();
        mark.normal = mark.normal.map(|value| value * inv_length);
        let capacity = self.render_policy.overlay_quad_capacity;
        if capacity == 0 {
            return Ok(());
        }
        if self.surface_marks.len() >= capacity {
            let overflow = self.surface_marks.len() + 1 - capacity;
            self.surface_marks.drain(0..overflow);
        }
        self.surface_marks.push(mark);
        Ok(())
    }

    pub fn clear_surface_marks(&mut self) {
        self.surface_marks.clear();
    }

    pub fn set_overlay_quads(&mut self, quads: Vec<SceneOverlayQuad>) -> Result<(), String> {
        if quads.len() > self.render_policy.overlay_quad_capacity {
            return Err(format!(
                "scene.overlay_quads.set exceeds the configured limit of {}",
                self.render_policy.overlay_quad_capacity
            ));
        }
        for quad in &quads {
            if quad.rect.iter().any(|value| !value.is_finite())
                || quad.color.iter().any(|value| !value.is_finite())
            {
                return Err("scene.overlay_quads.set contains non-finite data".to_owned());
            }
        }
        self.overlay_quads = quads;
        Ok(())
    }
    pub(super) fn update_scene_world(&mut self, dt: f32) -> Result<(), String> {
        self.world.update_transform(
            SceneEntityId(self.camera_entity_id),
            SceneTransform {
                position: self.camera.position,
                rotation_degrees: Vec3::ZERO,
                scale: Vec3::ONE,
            },
        )?;
        self.world.pre_update(self.camera.position, dt);
        self.world.update();
        Ok(())
    }
    pub(super) fn sync_runtime_camera_to_flecs(&self) -> Result<(), String> {
        let response = host_runtime::call_json(
            ECS_SERVICE,
            "command_json_v1",
            &json!({
                "commands": [{
                    "op": "set_component_json",
                    "entity_id": self.camera_entity_id,
                    "component_type": "newviso.camera.runtime",
                    "payload": {
                        "position": [
                            self.camera.position.x,
                            self.camera.position.y,
                            self.camera.position.z
                        ],
                        "target": [
                            self.camera.target.x,
                            self.camera.target.y,
                            self.camera.target.z
                        ],
                        "orbit": {
                            "yaw_radians": self.orbit.yaw,
                            "pitch_radians": self.orbit.pitch,
                            "distance": self.orbit.distance
                        }
                    }
                }]
            }),
        )?;

        let ok = response
            .get("results")
            .and_then(Value::as_array)
            .and_then(|results| results.first())
            .and_then(|result| result.get("ok"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !ok {
            return Err(format!("Flecs rejected runtime camera update: {response}"));
        }
        Ok(())
    }
}
