//! snapshot.
use super::*;

impl Scene3dRuntime {
    /// Compact hot-path snapshot for gameplay scripting.
    /// Full diagnostic collections stay in runtime_state().
    pub fn script_frame_state(&self) -> Value {
        let focus = self.world.focus();
        json!({
            "scene": {
                "surface_marks": {
                    "count": self.surface_marks.len(),
                    "attached": self.surface_marks.iter().filter(|mark| mark.owner.is_some()).count(),
                    "world": self.surface_marks.iter().filter(|mark| mark.owner.is_none()).count(),
                    "attached_samples": self.surface_marks.iter().filter(|mark| mark.owner.is_some()).take(16)
                        .filter_map(|mark| self.surface_mark_world(*mark).map(|world| json!({
                            "owner_key": mark.owner.unwrap().to_string(),
                            "local_position": mark.position, "world_position": world.position, "world_normal": world.normal
                        }))).collect::<Vec<_>>()
                },
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
                    "effects": self.weather_effects_json(),
                    "emitter_volumes":self.weather_world_layer_state()
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
                "indexed_terrain":{"patches":self.indexed_terrain.stats()[0],
                    "vertices":self.indexed_terrain.stats()[1],"indices":self.indexed_terrain.stats()[2]},
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
                    "light_direction": self.cloudhat_keyframe.light_direction,
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
                    "attached_surface_marks": self.surface_marks.iter().filter(|mark| mark.owner.is_some()).count(),
                    "overlay_quads": self.overlay_quads.len(),
                    "particles": self.particles.len(),
                    "particle_textures": self.particle_textures.len(),
                    "particle_model_count": self.particles.iter().filter(|p| p.desc.style.as_ref().is_some_and(|s| s.model.is_some())).count(),
                    "particle_trail_count": self.particles.iter().filter(|p| p.desc.style.as_ref().is_some_and(|s| s.trail)).count()
                }
            }
        })
    }
}
