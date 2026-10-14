//! Exact indexed terrain shared by the main, shadow and cloud-depth passes.
//! CPU heightfields/collision stay expanded; GPU corners are interned by all
//! 17 float bit patterns. No LOD, normal, material or footprint is approximated.
use super::*;
use std::{sync::Arc,collections::HashMap,hash::{Hash,Hasher}};

#[derive(Clone,Copy,PartialEq,Eq)]
struct VertexKey([u32;FLOATS_PER_VERTEX]);
impl Hash for VertexKey {
    fn hash<H:Hasher>(&self,state:&mut H){
        // Position distributes terrain corners; full bitwise Eq retains all
        // normal/material variants at a shared position.
        self.0[..3].hash(state);
    }
}
#[derive(Clone,Debug)]
struct IndexedPatch {
    source:Arc<[f32]>,
    vertices:Vec<f32>,
    indices:Vec<u32>,
}
fn index_patch(source:Arc<[f32]>)->Result<IndexedPatch,String>{
    if source.len()%FLOATS_PER_VERTEX!=0{return Err("unaligned terrain vertex stream".into());}
    let count=source.len()/FLOATS_PER_VERTEX;
    let mut vertices=Vec::with_capacity(source.len()/3);
    let mut indices=Vec::with_capacity(count);
    let mut intern=HashMap::<VertexKey,u32>::with_capacity(count/3);
    for vertex in source.chunks_exact(FLOATS_PER_VERTEX){
        let key=VertexKey(std::array::from_fn(|i|vertex[i].to_bits()));
        let id=match intern.entry(key){
            std::collections::hash_map::Entry::Occupied(entry)=>*entry.get(),
            std::collections::hash_map::Entry::Vacant(entry)=>{
                let id=u32::try_from(vertices.len()/FLOATS_PER_VERTEX)
                    .map_err(|_|"indexed terrain vertex count exceeds u32")?;
                vertices.extend_from_slice(vertex);entry.insert(id);id
            }
        };
        indices.push(id);
    }
    Ok(IndexedPatch{source,vertices,indices})
}
impl IndexedPatch {
    /// Most deformation changes attributes, not corner sharing. Reuse indices
    /// only after checking every shared corner's complete float bit pattern.
    fn update(&self,source:Arc<[f32]>)->Result<Self,String>{
        if source.len()==self.source.len(){
            let count=self.vertices.len()/FLOATS_PER_VERTEX;
            let mut vertices=vec![0.0;self.vertices.len()];let mut seen=vec![false;count];
            let mut compatible=true;
            for (vertex,id) in source.chunks_exact(FLOATS_PER_VERTEX).zip(&self.indices){
                let id=*id as usize;
                let target=&mut vertices[id*FLOATS_PER_VERTEX..(id+1)*FLOATS_PER_VERTEX];
                if !seen[id]{target.copy_from_slice(vertex);seen[id]=true;}
                else if !target.iter().zip(vertex).all(|(a,b)|a.to_bits()==b.to_bits()){
                    compatible=false;break;
                }
            }
            if compatible{return Ok(Self{source,vertices,indices:self.indices.clone()});}
        }
        index_patch(source)
    }
}

#[derive(Clone,Copy,Debug)]
struct TerrainRange {first_index:u32,index_count:u32}
#[derive(Debug)]
pub(super) struct IndexedTerrain {
    patches:BTreeMap<usize,IndexedPatch>,
    ranges:BTreeMap<usize,TerrainRange>,
    vertices:Vec<f32>,
    indices:Vec<u32>,
    vertex_buffers:[u32;SCENE_FRAME_SLOTS],
    index_buffers:[u32;SCENE_FRAME_SLOTS],
    vertex_capacity:usize,
    index_capacity:usize,
    generation:u64,
    topology_generation:u64,
    uploaded_vertices:[u64;SCENE_FRAME_SLOTS],
    uploaded_indices:[u64;SCENE_FRAME_SLOTS],
}
impl Default for IndexedTerrain {
    fn default()->Self{
        Self{patches:BTreeMap::new(),ranges:BTreeMap::new(),vertices:Vec::new(),indices:Vec::new(),
            vertex_buffers:[0;SCENE_FRAME_SLOTS],index_buffers:[0;SCENE_FRAME_SLOTS],
            vertex_capacity:0,index_capacity:0,generation:1,topology_generation:1,
            uploaded_vertices:[0;SCENE_FRAME_SLOTS],uploaded_indices:[0;SCENE_FRAME_SLOTS]}
    }
}
impl IndexedTerrain {
    fn prepare(&mut self,sources:&[(usize,Arc<[f32]>)])->Result<(),String>{
        let mut changed=self.patches.len()!=sources.len();
        self.patches.retain(|slot,_|sources.iter().any(|(s,_)|s==slot));
        for (slot,source) in sources{
            if self.patches.get(slot).is_some_and(|p|Arc::ptr_eq(&p.source,source)){continue;}
            let patch=match self.patches.get(slot){
                Some(prior)=>prior.update(Arc::clone(source))?,None=>index_patch(Arc::clone(source))?,
            };
            self.patches.insert(*slot,patch);changed=true;
        }
        if !changed{return Ok(());}
        let mut vertices=Vec::new();let mut indices=Vec::new();let mut ranges=BTreeMap::new();
        vertices.reserve(self.patches.values().map(|p|p.vertices.len()).sum::<usize>());
        indices.reserve(self.patches.values().map(|p|p.indices.len()).sum::<usize>());
        for (slot,patch) in &self.patches{
            let base=u32::try_from(vertices.len()/FLOATS_PER_VERTEX).map_err(|_|"terrain base exceeds u32")?;
            let first_index=u32::try_from(indices.len()).map_err(|_|"terrain index base exceeds u32")?;
            let index_count=u32::try_from(patch.indices.len()).map_err(|_|"terrain index count exceeds u32")?;
            vertices.extend_from_slice(&patch.vertices);
            for i in &patch.indices{indices.push(base.checked_add(*i).ok_or("terrain index overflow")?);}
            ranges.insert(*slot,TerrainRange{first_index,index_count});
        }
        self.generation=self.generation.wrapping_add(1);
        if self.indices!=indices{
            self.topology_generation=self.topology_generation.wrapping_add(1);
            self.indices=indices;
        }
        self.vertices=vertices;self.ranges=ranges;Ok(())
    }
    fn sync(&mut self,render:&RenderClient,slot:usize)->Result<u64,String>{
        if self.indices.is_empty(){return Ok(0);}
        if self.vertices.len()>self.vertex_capacity{
            let capacity=self.vertices.len().checked_next_power_of_two().ok_or("terrain capacity overflow")?;
            let next=create_frame_buffer_ring(render,"newviso.terrain.indexed.vertices",
                capacity as u64*4,"Vertex")?;
            for buffer in self.vertex_buffers{if buffer!=0{render.destroy_buffer(buffer);}}
            self.vertex_buffers=next;self.vertex_capacity=capacity;self.uploaded_vertices.fill(0);
        }
        if self.indices.len()>self.index_capacity{
            let capacity=self.indices.len().checked_next_power_of_two().ok_or("terrain capacity overflow")?;
            let next=create_frame_buffer_ring(render,"newviso.terrain.indexed.indices",
                capacity as u64*4,"Index")?;
            for buffer in self.index_buffers{if buffer!=0{render.destroy_buffer(buffer);}}
            self.index_buffers=next;self.index_capacity=capacity;self.uploaded_indices.fill(0);
        }
        let mut bytes=0;
        if self.uploaded_vertices[slot]!=self.generation{
            render.write_buffer_f32(self.vertex_buffers[slot],0,&self.vertices)?;
            self.uploaded_vertices[slot]=self.generation;bytes+=self.vertices.len() as u64*4;
        }
        if self.uploaded_indices[slot]!=self.topology_generation{
            let data=self.indices.iter().flat_map(|i|i.to_le_bytes()).collect::<Vec<_>>();
            render.write_buffer(self.index_buffers[slot],0,&data)?;
            self.uploaded_indices[slot]=self.topology_generation;bytes+=data.len() as u64;
        }
        Ok(bytes)
    }
    pub(super) fn stats(&self)->[usize;3]{[self.patches.len(),self.vertices.len()/FLOATS_PER_VERTEX,self.indices.len()]}
    fn shutdown(&mut self,render:&RenderClient){
        for buffer in self.vertex_buffers.into_iter().chain(self.index_buffers){
            if buffer!=0{render.destroy_buffer(buffer);}
        }
        *self=Self::default();
    }
    fn draw(&self,render:&RenderClient,frame_slot:usize,slots:impl IntoIterator<Item=usize>)->Result<usize,String>{
        if self.indices.is_empty(){return Ok(0);}
        let mut draws=Vec::new();
        for slot in slots {
            if let Some(range)=self.ranges.get(&slot){
                draws.push(DirectAssetDraw{material_group:0,vertex_buffer:self.vertex_buffers[frame_slot],
                    vertex_offset:0,index_count:range.index_count,first_index:range.first_index,
                    instance_count:1,first_instance:0});
            }
        }
        compact_direct_draws(&mut draws);
        if draws.is_empty(){return Ok(0);}
        render.set_vertex_buffer(0,self.vertex_buffers[frame_slot],0)?;
        render.set_index_buffer(self.index_buffers[frame_slot],0,"U32")?;
        for draw in &draws{
            render.draw_indexed_range_instanced(draw.index_count,draw.first_index,0,1,0)?;
        }
        Ok(draws.len())
    }
}
impl Scene3dRuntime {
    pub(super) fn prepare_indexed_terrain(&mut self)->Result<(),String>{
        let sources=self.world.active_render_slots().into_iter()
            .filter(|slot|!self.water_render_slots.contains(slot))
            .filter_map(|slot|self.terrain_meshes.get(&slot).map(|mesh|(slot,Arc::clone(&mesh.vertices))))
            .collect::<Vec<_>>();
        self.indexed_terrain.prepare(&sources)
    }
    pub(super) fn sync_indexed_terrain(&mut self,render:&RenderClient,slot:usize)->Result<u64,String>{
        self.indexed_terrain.sync(render,slot)
    }
    pub(super) fn has_indexed_terrain(&self)->bool{!self.indexed_terrain.indices.is_empty()}
    pub(super) fn draw_indexed_terrain_main(&self,render:&RenderClient,slot:usize)->Result<usize,String>{
        self.indexed_terrain.draw(render,slot,self.primitive_main_render_slots())
    }
    pub(super) fn draw_indexed_terrain_depth(&self,render:&RenderClient,slot:usize)->Result<usize,String>{
        self.indexed_terrain.draw(render,slot,self.primitive_shadow_render_slots())
    }
    pub(super) fn shutdown_indexed_terrain(&mut self,render:&RenderClient){self.indexed_terrain.shutdown(render);}
}
#[cfg(test)]
mod tests{
    use super::*;
    fn vertex(x:f32,mask:f32)->[f32;FLOATS_PER_VERTEX]{
        let mut v=[0.0;FLOATS_PER_VERTEX];v[0]=x;v[1]=0.4;v[6]=1.0;v[11]=mask;v
    }
    fn expand(p:&IndexedPatch)->Vec<f32>{
        p.indices.iter().flat_map(|i|p.vertices[*i as usize*FLOATS_PER_VERTEX..(*i as usize+1)*FLOATS_PER_VERTEX].iter().copied()).collect()
    }
    fn bits(v:&[f32])->Vec<u32>{v.iter().map(|f|f.to_bits()).collect()}
    #[test]
    fn exact_indexing_preserves_normals_materials_and_signed_zero(){
        let a=vertex(0.0,0.3);let b=vertex(1.0,0.4);let c=vertex(2.0,0.5);
        let mut seam=a;seam[6]=0.9;let mut material=a;material[11]=0.7;let mut zero=a;zero[0]=-0.0;
        let source=[a,c,b,b,c,a,seam,material,zero].concat();
        let indexed=index_patch(Arc::from(source.clone())).unwrap();
        assert_eq!(indexed.vertices.len()/FLOATS_PER_VERTEX,6);
        assert_eq!(bits(&expand(&indexed)),bits(&source));
    }
    #[test]
    fn unchanged_arcs_reuse_layout_and_deformation_does_not_reupload_topology(){
        let source:Arc<[f32]>=Arc::from([vertex(0.0,1.0),vertex(1.0,1.0),vertex(2.0,1.0)].concat());
        let mut cache=IndexedTerrain::default();cache.prepare(&[(4,Arc::clone(&source))]).unwrap();
        let g=cache.generation;let topology=cache.topology_generation;
        cache.prepare(&[(4,Arc::clone(&source))]).unwrap();assert_eq!(cache.generation,g);
        let mut changed=source.to_vec();changed[1]+=0.01;
        cache.prepare(&[(4,Arc::from(changed))]).unwrap();
        assert!(cache.generation>g);assert_eq!(cache.topology_generation,topology);
        cache.prepare(&[]).unwrap();assert!(cache.ranges.is_empty());assert!(cache.indices.is_empty());
    }
    #[test]
    fn packed_ranges_keep_patch_order_and_every_triangle(){
        let a:Arc<[f32]>=Arc::from([vertex(0.0,1.0),vertex(1.0,1.0),vertex(2.0,1.0)].concat());
        let b:Arc<[f32]>=Arc::from([vertex(10.0,0.2),vertex(11.0,0.2),vertex(12.0,0.2)].concat());
        let mut cache=IndexedTerrain::default();cache.prepare(&[(8,Arc::clone(&b)),(4,Arc::clone(&a))]).unwrap();
        for (slot,source) in [(4,a),(8,b)]{
            let r=cache.ranges[&slot];let reconstructed=cache.indices[r.first_index as usize..(r.first_index+r.index_count) as usize]
                .iter().flat_map(|i|cache.vertices[*i as usize*FLOATS_PER_VERTEX..(*i as usize+1)*FLOATS_PER_VERTEX].iter().copied()).collect::<Vec<_>>();
            assert_eq!(bits(&reconstructed),bits(&source));
        }
    }

    #[test]
    fn reused_corner_classes_fall_back_when_a_shared_normal_or_material_splits(){
        let a=vertex(0.0,1.0);let b=vertex(1.0,1.0);let c=vertex(2.0,1.0);
        let source:Arc<[f32]>=Arc::from([a,c,b,b,c,a].concat());
        let before=index_patch(Arc::clone(&source)).unwrap();
        for changed in [1,6,11,15]{
            let mut after=source.to_vec();after[5*FLOATS_PER_VERTEX+changed]+=0.02;
            let updated=before.update(Arc::from(after.clone())).unwrap();
            assert_eq!(bits(&expand(&updated)),bits(&after));
            assert_eq!(updated.vertices.len()/FLOATS_PER_VERTEX,4);
        }
    }
}
