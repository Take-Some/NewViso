//! Depth shadow pass, isolated from main-view and transparent submission.
use super::*;
impl Scene3dRuntime {
    pub(super) fn record_shadow_pass(&self,render:&RenderClient,gpu:GpuScene,
        frame_slot:usize,shadow_enabled:bool,batches:&[GpuResidentInstanceBatch])->Result<usize,String>{
        if !shadow_enabled || (self.shadow_vertex_count()==0&&batches.is_empty()&&!self.has_indexed_terrain()){return Ok(0);}
        render.begin_render_target(gpu.shadow_render_target,Some([1.0;4]),Some(1.0))?;
        render.set_viewport(gpu.shadow_resolution,gpu.shadow_resolution)?;
        render.set_scissor(gpu.shadow_resolution,gpu.shadow_resolution)?;
        render.set_pipeline(gpu.shadow_pipeline)?;
        render.set_bind_group(0,gpu.shadow_bind_groups[frame_slot])?;
        render.set_bind_group(1,gpu.default_material_bind_group)?;
        let terrain_draws=self.draw_indexed_terrain_depth(render,frame_slot)?;
        let vertices=self.shadow_vertex_count();
        if vertices>0 {
            render.set_vertex_buffer(0,gpu.shadow_vertex_buffers[frame_slot],0)?;
            render.draw(vertices)?;
        }
        let mut draws=Vec::new();
        for batch in batches {
            let Some(id)=batch.stable_ids.first() else{continue;};
            let mesh=self.asset_meshes.get(id).ok_or_else(||format!("asset instance {id} disappeared"))?;
            if mesh.main_view_only{continue;}
            let (vertex_buffer,vertex_offset)=asset_mesh_vertex_binding(mesh,gpu,frame_slot)?;
            let mut push=|material_group,first_index,index_count| draws.push(DirectAssetDraw{
                material_group,vertex_buffer,vertex_offset,index_count,first_index,
                instance_count:batch.instance_count,first_instance:batch.first_instance});
            if mesh.local_draw_ranges.is_empty(){
                push(gpu.default_material_bind_group,mesh.first_vertex,mesh.vertex_count);continue;
            }
            for range in mesh.local_draw_ranges.iter().filter(|r|r.render_passes.shadow){
                let group=match range.material_slot {
                    Some(slot)=>match self.asset_gpu_materials.get(&(mesh.model_id.0,slot)){
                        Some(material)=>material.bind_group,None=>continue,
                    },
                    None=>gpu.default_material_bind_group,
                };
                push(group,mesh.first_vertex.saturating_add(range.first_vertex),range.vertex_count);
            }
        }
        // Shadow depth is order-independent, including alpha-tested materials.
        draws.sort_unstable_by_key(|d|(d.material_group,d.vertex_buffer,d.vertex_offset,
            d.first_instance,d.instance_count,d.first_index));
        compact_direct_draws(&mut draws);
        if !draws.is_empty(){
            render.set_pipeline(gpu.asset_shadow_pipeline)?;
            render.set_bind_group(0,gpu.shadow_bind_groups[frame_slot])?;
            render.set_vertex_buffer(1,gpu.asset_instance_buffers[frame_slot],0)?;
            render.set_index_buffer(gpu.asset_index_buffer,0,"U32")?;
            draw_direct_draws(render,&draws,None,None)?;
        }
        render.end_render_target()?;
        Ok(draws.len()+usize::from(vertices>0)+terrain_draws)
    }
}
