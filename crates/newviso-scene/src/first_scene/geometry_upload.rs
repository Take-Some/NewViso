//! Retain unchanged packed geometry across the acquired frame-buffer ring.
//! Dirty chunks include transient changes; no scene mutation can be missed by
//! relying on a static-world epoch or camera-only invalidation.
use super::*;
use std::sync::Arc;

const CHUNK_FLOATS: usize = 64 * FLOATS_PER_VERTEX;

#[derive(Debug, Default)]
pub(super) struct GeometryUploadCache {
    data: Vec<f32>,
    shared_source: Option<Arc<[f32]>>,
    block_sources: [Option<(u32, Vec<Arc<[f32]>>)>; SCENE_FRAME_SLOTS],
    revisions: Vec<u64>,
    generation: u64,
    buffers: [u32; SCENE_FRAME_SLOTS],
    uploaded: [Vec<u64>; SCENE_FRAME_SLOTS],
}

fn packed_bytes(values: &[f32]) -> &[u8] {
    // f32 has no padding. An immutable byte view also distinguishes signed
    // zero and NaN payloads, preserving exactly what the renderer receives.
    unsafe {
        std::slice::from_raw_parts(values.as_ptr().cast(), std::mem::size_of_val(values))
    }
}

/// Return merged dirty float ranges within an unchanged-size geometry block.
/// Fixed vertex stride makes comparisons safe for Vulkan draw vertex offsets.
fn dirty_block_ranges(old:&[f32],new:&[f32])->Vec<(usize,usize)>{
    if old.len()!=new.len(){return vec![(0,new.len())];}
    let mut ranges=Vec::<(usize,usize)>::new();
    for start in (0..new.len()).step_by(CHUNK_FLOATS){
        let end=(start+CHUNK_FLOATS).min(new.len());
        if packed_bytes(&old[start..end])==packed_bytes(&new[start..end]){continue;}
        if let Some(last)=ranges.last_mut(){
            // Merge nearby writes to avoid excessive driver submission calls.
            if start-last.1<=CHUNK_FLOATS*2 {
                last.1=end;
                continue;
            }
        }
        ranges.push((start,end));
    }
    ranges
}
impl GeometryUploadCache {
    fn pending_blocks(&self, slot: usize, buffer: u32, blocks: &[Arc<[f32]>]) -> Vec<(usize,usize)> {
        let old=self.block_sources[slot].as_ref().filter(|(id,_)| *id==buffer).map(|(_,blocks)| blocks);
        let mut offset=0;
        let mut old_offset=0;
        let mut pending=Vec::new();
        for (index, block) in blocks.iter().enumerate() {
            let previous=old.and_then(|blocks| blocks.get(index));
            let unchanged=previous.is_some_and(|previous|
                offset==old_offset && Arc::ptr_eq(previous,block));
            if !unchanged && !block.is_empty() { pending.push((index,offset)); }
            offset+=block.len();
            old_offset+=previous.map_or(0,|block| block.len());
        }
        pending
    }

    pub(super) fn sync_blocks(&mut self, render: &RenderClient, slot: usize,
        buffer: u32, blocks: &[Arc<[f32]>]) -> Result<usize,String>
    {
        if slot>=SCENE_FRAME_SLOTS { return Err("geometry upload exceeds frame ring".into()); }
        let pending=self.pending_blocks(slot,buffer,blocks);
        let previous=self.block_sources[slot].as_ref()
            .filter(|(id,_)|*id==buffer)
            .map(|(_,blocks)|blocks.as_slice());
        let mut bytes=0;
        let mut old_offset=0_usize;
        let mut old_positions=Vec::new();
        if let Some(old)=previous {
            for block in old{old_positions.push(old_offset);old_offset+=block.len();}
        }
        for (index,offset) in pending {
            let block=&blocks[index];
            let old=previous.and_then(|a|a.get(index));
            let safe_same_offset=old_positions.get(index).copied()==Some(offset)
                &&old.is_some_and(|a|a.len()==block.len());
            let ranges=if safe_same_offset {
                dirty_block_ranges(old.unwrap(),block)
            }else{vec![(0,block.len())]};
            for (start,end) in ranges{
                if start==end{continue;}
                let byte_offset=offset.checked_add(start)
                    .and_then(|v|u64::try_from(v).ok())
                    .and_then(|v|v.checked_mul(4))
                    .ok_or_else(||"geometry block byte offset overflow".to_owned())?;
                render.write_buffer_f32(buffer,byte_offset,&block[start..end])?;
                bytes+=(end-start)*4;
            }
        }
        // Commit only after every write succeeded, keeping retries complete.
        self.block_sources[slot]=Some((buffer,blocks.to_vec()));
        self.shared_source=None;
        self.data.clear();
        self.revisions.clear();
        for uploaded in &mut self.uploaded { uploaded.clear(); }
        Ok(bytes)
    }

    fn update(&mut self, values: &[f32]) {
        let old_len = self.data.len();
        self.data.resize(values.len(), 0.0);
        self.revisions.resize(values.len().div_ceil(CHUNK_FLOATS), 0);
        for (chunk, incoming) in values.chunks(CHUNK_FLOATS).enumerate() {
            let start = chunk * CHUNK_FLOATS;
            let end = start + incoming.len();
            if self.revisions[chunk] == 0
                || end > old_len
                || packed_bytes(&self.data[start..end]) != packed_bytes(incoming)
            {
                self.generation = self.generation.wrapping_add(1).max(1);
                self.data[start..end].copy_from_slice(incoming);
                self.revisions[chunk] = self.generation;
            }
        }
    }

    fn pending_ranges(&mut self, slot: usize, buffer: u32) -> Vec<(usize, usize)> {
        if self.buffers[slot] != buffer {
            self.buffers[slot] = buffer;
            self.uploaded[slot].clear();
        }
        self.uploaded[slot].resize(self.revisions.len(), 0);
        let mut ranges: Vec<(usize, usize)> = Vec::new();
        for (chunk, revision) in self.revisions.iter().enumerate() {
            if self.uploaded[slot][chunk] == *revision {
                continue;
            }
            let start = chunk * CHUNK_FLOATS;
            let end = (start + CHUNK_FLOATS).min(self.data.len());
            if let Some(last) = ranges.last_mut() {
                if last.1 == start {
                    last.1 = end;
                    continue;
                }
            }
            ranges.push((start, end));
        }
        ranges
    }

    #[allow(dead_code)]
    pub(super) fn sync_shared(&mut self, render: &RenderClient, slot: usize,
        buffer: u32, values: &Arc<[f32]>) -> Result<usize,String>
    {
        if slot >= SCENE_FRAME_SLOTS { return Err("geometry upload exceeds frame ring".into()); }
        self.block_sources=Default::default();
        if !self.shared_source.as_ref().is_some_and(|old| Arc::ptr_eq(old,values)) {
            self.update(values);
            self.shared_source = Some(Arc::clone(values));
        }
        self.sync_pending(render,slot,buffer)
    }

    fn sync_pending(&mut self, render: &RenderClient, slot: usize,
        buffer: u32) -> Result<usize,String>
    {
        let ranges = self.pending_ranges(slot,buffer);
        let bytes = ranges.iter().map(|(start,end)| (end-start)*4).sum();
        if !ranges.is_empty() {
            render.write_buffer_f32_ranges(buffer,&self.data,&ranges)?;
            self.uploaded[slot].clone_from(&self.revisions);
        }
        Ok(bytes)
    }

    #[allow(dead_code)]
    pub(super) fn sync(
        &mut self,
        render: &RenderClient,
        slot: usize,
        buffer: u32,
        values: &[f32],
    ) -> Result<usize, String> {
        if slot >= SCENE_FRAME_SLOTS {
            return Err("geometry upload exceeds frame ring".into());
        }
        self.block_sources=Default::default();
        self.shared_source = None;
        self.update(values);
        let ranges = self.pending_ranges(slot, buffer);
        let bytes = ranges.iter().map(|(start, end)| (end - start) * 4).sum();
        if !ranges.is_empty() {
            render.write_buffer_f32_ranges(buffer, &self.data, &ranges)?;
            // An unsuccessful upload leaves every dirty revision pending.
            self.uploaded[slot].clone_from(&self.revisions);
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accept(cache: &mut GeometryUploadCache, slot: usize) {
        cache.uploaded[slot].clone_from(&cache.revisions);
    }

    #[test]
    fn sparse_snow_mutations_upload_only_changed_vertex_chunks(){
        let old=vec![1.0;CHUNK_FLOATS*32];
        let mut new=old.clone();
        new[CHUNK_FLOATS*5+11]=2.0;
        new[CHUNK_FLOATS*24+3]=4.0;
        let regions=dirty_block_ranges(&old,&new);
        assert_eq!(regions,vec![
          (CHUNK_FLOATS*5,CHUNK_FLOATS*6),
          (CHUNK_FLOATS*24,CHUNK_FLOATS*25)
        ]);
        assert_eq!(regions.iter().map(|(a,b)|b-a).sum::<usize>(),
                   CHUNK_FLOATS*2);
        assert!(dirty_block_ranges(&new,&new).is_empty());
        assert_eq!(dirty_block_ranges(&[0.0;1],&[2.0;3]),[(0,3)]);
    }
    #[test]
    fn changing_visibility_tail_keeps_immutable_prefix_in_every_frame_slot() {
        let prefix: Arc<[f32]>=Arc::from(vec![1.;CHUNK_FLOATS*8]);
        let tail: Arc<[f32]>=Arc::from([2.;17]);
        let initial=vec![Arc::clone(&prefix),Arc::clone(&tail)];
        let mut cache=GeometryUploadCache::default();
        for slot in 0..SCENE_FRAME_SLOTS {
            assert_eq!(cache.pending_blocks(slot,100+slot as u32,&initial),
                [(0,0),(1,prefix.len())]);
            cache.block_sources[slot]=Some((100+slot as u32,initial.clone()));
        }
        let changed=vec![Arc::clone(&prefix),Arc::from([3.;34])];
        for slot in 0..SCENE_FRAME_SLOTS {
            assert_eq!(cache.pending_blocks(slot,100+slot as u32,&changed),[(1,prefix.len())]);
        }
        // Uncommitted writes remain pending after failure.
        assert_eq!(cache.pending_blocks(0,100,&changed),[(1,prefix.len())]);
        assert_eq!(cache.pending_blocks(0,200,&changed),[(0,0),(1,prefix.len())]);
        let shifted=vec![Arc::from([1.;17]),Arc::clone(&tail)];
        assert_eq!(cache.pending_blocks(0,100,&shifted),[(0,0),(1,17)]);
        // Retiring the tail also retires its cached contents.
        cache.block_sources[0]=Some((100,vec![prefix]));
        assert_eq!(cache.pending_blocks(0,100,&initial),[(1,initial[0].len())]);
    }

    #[test]
    fn unchanged_geometry_is_uploaded_once_per_acquired_slot() {
        let mut cache = GeometryUploadCache::default();
        let values = vec![1.0; CHUNK_FLOATS * 3];
        cache.update(&values);
        for slot in 0..SCENE_FRAME_SLOTS {
            assert_eq!(cache.pending_ranges(slot, 100 + slot as u32), [(0, values.len())]);
            accept(&mut cache, slot);
        }
        cache.update(&values);
        for slot in 0..SCENE_FRAME_SLOTS {
            assert!(cache.pending_ranges(slot, 100 + slot as u32).is_empty());
        }
        assert_eq!(cache.pending_ranges(0, 200), [(0, values.len())]);
    }

    #[test]
    fn transient_changes_update_only_the_affected_chunks_in_all_slots() {
        let mut cache = GeometryUploadCache::default();
        let mut values = vec![1.0; CHUNK_FLOATS * 4];
        cache.update(&values);
        for slot in 0..SCENE_FRAME_SLOTS {
            cache.pending_ranges(slot, 100 + slot as u32);
            accept(&mut cache, slot);
        }
        values[CHUNK_FLOATS + 7] = 2.0;
        cache.update(&values);
        for slot in 0..SCENE_FRAME_SLOTS {
            assert_eq!(cache.pending_ranges(slot, 100 + slot as u32),
                [(CHUNK_FLOATS, CHUNK_FLOATS * 2)]);
        }
        // A failed/uncommitted write must still be requested next time.
        assert_eq!(cache.pending_ranges(0, 100), [(CHUNK_FLOATS, CHUNK_FLOATS * 2)]);
    }

    #[test]
    fn shrink_regrow_and_bit_changes_cannot_reuse_stale_gpu_data() {
        let mut cache = GeometryUploadCache::default();
        cache.update(&vec![7.0; CHUNK_FLOATS * 2]);
        cache.pending_ranges(0, 100);
        accept(&mut cache, 0);
        cache.update(&[7.0; 4]);
        cache.pending_ranges(0, 100);
        accept(&mut cache, 0);
        let mut values = vec![0.0; CHUNK_FLOATS * 2];
        values[..4].fill(7.0);
        cache.update(&values);
        assert_eq!(cache.pending_ranges(0, 100), [(0, values.len())]);
        accept(&mut cache, 0);
        values[5] = -0.0;
        cache.update(&values);
        assert_eq!(cache.pending_ranges(0, 100), [(0, CHUNK_FLOATS)]);
        accept(&mut cache, 0);
        values[5] = f32::from_bits(0x7fc0_0001);
        cache.update(&values);
        assert_eq!(cache.pending_ranges(0, 100), [(0, CHUNK_FLOATS)]);
        accept(&mut cache, 0);
        cache.update(&values);
        assert!(cache.pending_ranges(0, 100).is_empty());
    }
}
