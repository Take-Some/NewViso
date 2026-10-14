//! Opaque terrain and dynamic primitive submissions use independent GPU buffers.
use super::*;
impl Scene3dRuntime {
    pub(super) fn record_primitive_gbuffer(&self,render:&RenderClient,gpu:GpuScene,
        frame_slot:usize,weather_material_group:u32)->Result<(),String>{
        render.set_pipeline(gpu.gbuffer_pipeline)?;
        render.set_bind_group(0,gpu.bind_groups[frame_slot])?;
        render.set_bind_group(1,self.terrain_gpu_material.as_ref()
            .map(|m|m.bind_group).unwrap_or(gpu.default_material_bind_group))?;
        render.set_bind_group(2,weather_material_group)?;
        self.draw_indexed_terrain_main(render,frame_slot)?;
        render.set_bind_group(1,gpu.default_material_bind_group)?;
        let vertices=self.vertex_count();
        if vertices>0{
            render.set_vertex_buffer(0,gpu.vertex_buffers[frame_slot],0)?;
            render.draw(vertices)?;
        }
        Ok(())
    }
}
