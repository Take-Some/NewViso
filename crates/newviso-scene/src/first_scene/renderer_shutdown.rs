use super::*;

impl Scene3dRuntime {
    pub fn shutdown_renderer(&mut self) {
        let render = RenderClient::new();
        self.shutdown_mass_instance_gpu(&render);
        self.shutdown_weather_gpu_fx_renderer(&render);
        self.shutdown_volumetric_cloud_renderer(&render);
        if let Some(clouds) = self.gpu_atmospheric_clouds.take() {
            for layer in clouds.layers.into_iter().flatten() {
                for group in layer.bind_groups {
                    render.destroy_bind_group(group);
                }
                for buffer in layer.uniform_buffers {
                    render.destroy_buffer(buffer);
                }
                render.destroy_buffer(layer.index_buffer);
                render.destroy_buffer(layer.vertex_buffer);
            }
            render.destroy_pipeline(clouds.pipeline);
            render.destroy_shader(clouds.fragment_shader);
            render.destroy_shader(clouds.vertex_shader);
            render.destroy_bind_group_layout(clouds.bind_group_layout);
            render.destroy_bind_group(clouds.soft_sample_bind_group);
            render.destroy_bind_group_layout(clouds.soft_sample_bind_group_layout);
            for texture in clouds.texture_cache.into_values() {
                render.destroy_texture(texture);
            }
            render.destroy_pipeline(clouds.soft_depth_instanced_pipeline);
            render.destroy_pipeline(clouds.soft_depth_pipeline);
            render.destroy_shader(clouds.soft_depth_instanced_fragment_shader);
            render.destroy_shader(clouds.soft_depth_fragment_shader);
            render.destroy_shader(clouds.soft_depth_instanced_vertex_shader);
            render.destroy_shader(clouds.soft_depth_vertex_shader);
            for group in clouds.soft_depth_bind_groups {
                render.destroy_bind_group(group);
            }
            render.destroy_bind_group_layout(clouds.soft_depth_bind_group_layout);
            for buffer in clouds.soft_depth_uniforms {
                render.destroy_buffer(buffer);
            }
            render.destroy_render_target(clouds.soft_depth_render_target);
            render.destroy_sampler(clouds.sampler);
        }
        if let Some(sky) = self.gpu_sky.take() {
            render.destroy_pipeline(sky.pipeline);
            render.destroy_shader(sky.fragment_shader);
            render.destroy_shader(sky.vertex_shader);
            for group in sky.bind_groups {
                render.destroy_bind_group(group);
            }
            render.destroy_bind_group_layout(sky.bind_group_layout);
            render.destroy_sampler(sky.sampler);
            for buffer in sky.camera_uniforms {
                render.destroy_buffer(buffer);
            }
            if let Some(texture) = sky.billboard_texture {
                render.destroy_texture(texture);
            }
            render.destroy_texture(sky.detail_noise);
            render.destroy_texture(sky.starfield);
            render.destroy_texture(sky.base_noise);
            render.destroy_buffer(sky.index_buffer);
            render.destroy_buffer(sky.vertex_buffer);
        }

        let Some(gpu) = self.gpu.take() else {
            return;
        };
        for material in std::mem::take(&mut self.particle_gpu_materials).into_values() {
            render.destroy_bind_group(material.bind_group);
            render.destroy_buffer(material.uniform_buffer);
        }
        for texture in std::mem::take(&mut self.particle_gpu_textures).into_values() {
            render.destroy_texture(texture);
        }
        for material in std::mem::take(&mut self.asset_gpu_materials).into_values() {
            render.destroy_bind_group(material.bind_group);
            render.destroy_buffer(material.uniform_buffer);
        }
        self.release_dashboard_materials(&render);
        for texture in std::mem::take(&mut self.asset_gpu_textures).into_values() {
            render.destroy_texture(texture);
        }
        for texture in std::mem::take(&mut self.asset_gpu_pending_textures).into_values() {
            render.destroy_texture(texture);
        }
        render.destroy_pipeline(gpu.flare_pipeline);
        render.destroy_pipeline(gpu.particle_additive_pipeline);
        render.destroy_pipeline(gpu.asset_shadow_pipeline);
        render.destroy_pipeline(gpu.shadow_pipeline);
        render.destroy_pipeline(gpu.asset_alpha_pipeline);
        render.destroy_pipeline(gpu.asset_gbuffer_pipeline);
        render.destroy_pipeline(gpu.asset_pipeline);
        render.destroy_pipeline(gpu.alpha_pipeline);
        render.destroy_pipeline(gpu.gbuffer_pipeline);
        render.destroy_pipeline(gpu.pipeline);
        render.destroy_shader(gpu.flare_fragment_shader);
        render.destroy_shader(gpu.flare_vertex_shader);
        render.destroy_shader(gpu.shadow_fragment_shader);
        render.destroy_shader(gpu.asset_shadow_vertex_shader);
        render.destroy_shader(gpu.shadow_vertex_shader);
        render.destroy_shader(gpu.asset_vertex_shader);
        render.destroy_shader(gpu.gbuffer_fragment_shader);
        render.destroy_shader(gpu.fragment_shader);
        render.destroy_shader(gpu.vertex_shader);
        for group in gpu.shadow_bind_groups {
            render.destroy_bind_group(group);
        }
        for group in gpu.bind_groups {
            render.destroy_bind_group(group);
        }
        render.destroy_bind_group(gpu.default_material_bind_group);
        render.destroy_bind_group_layout(gpu.shadow_bind_group_layout);
        render.destroy_bind_group_layout(gpu.bind_group_layout);
        render.destroy_bind_group_layout(gpu.material_bind_group_layout);
        render.destroy_sampler(gpu.material_sampler);
        render.destroy_buffer(gpu.default_material_uniform);
        render.destroy_texture(gpu.default_environment_texture);
        render.destroy_texture(gpu.default_emissive_texture);
        render.destroy_texture(gpu.default_specular_texture);
        render.destroy_texture(gpu.default_normal_texture);
        render.destroy_texture(gpu.default_base_color_texture);
        render.destroy_sampler(gpu.shadow_sampler);
        render.destroy_render_target(gpu.shadow_render_target);
        for buffer in gpu.frame_uniforms {
            render.destroy_buffer(buffer);
        }
        for buffer in gpu.flare_vertex_buffers {
            render.destroy_buffer(buffer);
        }
        for buffer in gpu.particle_vertex_buffers {
            render.destroy_buffer(buffer);
        }
        for buffer in gpu.visibility_indirect_buffers {
            render.destroy_buffer(buffer);
        }
        for buffer in gpu.visibility_candidate_buffers {
            render.destroy_buffer(buffer);
        }
        for buffer in gpu.asset_instance_buffers {
            render.destroy_buffer(buffer);
        }
        for buffer in gpu.skinned_vertex_buffers {
            render.destroy_buffer(buffer);
        }
        render.destroy_buffer(gpu.asset_index_buffer);
        render.destroy_buffer(gpu.asset_vertex_buffer);
        for buffer in self.retired_asset_vertex_buffers.drain(..) {
            render.destroy_buffer(buffer);
        }
        for buffer in gpu.shadow_vertex_buffers {
            render.destroy_buffer(buffer);
        }
        for buffer in gpu.vertex_buffers {
            render.destroy_buffer(buffer);
        }
    }
}
