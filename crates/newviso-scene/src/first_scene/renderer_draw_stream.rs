//! Compact opaque indexed draws after semantic visibility/material resolution.
//! Coalesce only adjacent index spans with identical GPU and instance state.
use super::*;
pub(super) fn compact_direct_draws(draws:&mut Vec<DirectAssetDraw>){
    let mut used=0;
    for read in 0..draws.len(){
        let next=draws[read];
        if used>0 {
            let prior=&mut draws[used-1];
            if prior.material_group==next.material_group &&
                prior.vertex_buffer==next.vertex_buffer &&
                prior.vertex_offset==next.vertex_offset &&
                prior.instance_count==next.instance_count &&
                prior.first_instance==next.first_instance &&
                prior.first_index.checked_add(prior.index_count)==Some(next.first_index) {
                if let Some(count)=prior.index_count.checked_add(next.index_count){
                    prior.index_count=count;continue;
                }
            }
        }
        draws[used]=next;used+=1;
    }
    draws.truncate(used);
}
pub(super) fn draw_direct_draws(render:&RenderClient,draws:&[DirectAssetDraw],
    bound_material:Option<u32>,bound_vertex:Option<u32>)->Result<(),String>{
    let mut bindings=MaterialVertexBindings::new(bound_material,bound_vertex);
    for draw in draws {
        bindings.bind(render,draw.material_group,draw.vertex_buffer)?;
        render.draw_indexed_range_instanced(draw.index_count,draw.first_index,
            draw.vertex_offset,draw.instance_count,draw.first_instance)?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn draw(first:u32,count:u32)->DirectAssetDraw{
        DirectAssetDraw{material_group:5,vertex_buffer:7,vertex_offset:-300,
            index_count:count,first_index:first,instance_count:2,first_instance:4}
    }
    fn stream(draws:&[DirectAssetDraw])->Vec<(u32,u32,i32,u32,u32)>{
        draws.iter().flat_map(|d|(d.first_index..d.first_index+d.index_count)
            .flat_map(move |i|(d.first_instance..d.first_instance+d.instance_count)
                .map(move |instance|(d.material_group,d.vertex_buffer,d.vertex_offset,i,instance)))).collect()
    }
    #[test]
    fn adjacency_keeps_every_index_instance_and_signed_skin_offset(){
        let mut draws=vec![draw(10,3),draw(13,6),draw(19,3)];
        let old=stream(&draws);compact_direct_draws(&mut draws);
        assert_eq!(draws.len(),1);assert_eq!(draws[0].index_count,12);
        assert_eq!(stream(&draws),old);
    }
    #[test]
    fn hidden_gaps_overlaps_and_all_state_boundaries_remain_separate(){
        for changed in 0..7 {
            let a=draw(10,3);let mut b=draw(13,3);
            match changed {
                0=>b.material_group+=1,1=>b.vertex_buffer+=1,2=>b.vertex_offset+=1,
                3=>b.instance_count+=1,4=>b.first_instance+=1,
                5=>b.first_index+=3,_=>b.first_index-=3,
            }
            let mut draws=vec![a,b];let old=stream(&draws);
            compact_direct_draws(&mut draws);assert_eq!(draws.len(),2);
            assert_eq!(stream(&draws),old);
        }
    }
    #[test]
    fn arithmetic_overflow_never_merges_spans(){
        let mut draws=vec![draw(u32::MAX-2,6),draw(3,3)];
        compact_direct_draws(&mut draws);assert_eq!(draws.len(),2);
    }
}
