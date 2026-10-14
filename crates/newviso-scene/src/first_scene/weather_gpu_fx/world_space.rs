//! Weather window follows observer translation, with axes fixed in the world.
//! Imported GTA vectors are Z-up. Their horizontal centre offsets describe a
//! view-ahead culling window, not a world weather source: use those as stable
//! seed phases and centre the horizontal window around the observer.
use super::{Vec3,WeatherGpuFxEmitterDesc};
pub(super) fn emitter_world_box(position:Vec3,emitter:&WeatherGpuFxEmitterDesc)
    ->([f32;3],[f32;3]){
    let offset=emitter.box_centre_offset;
    let size=emitter.box_size;
    ([position.x,position.y+offset[2],position.z],
     [size[0].max(0.01),size[2].max(0.01),size[1].max(0.01)])
}
#[cfg(test)]
mod tests {
    use super::*;
    fn emitter()->WeatherGpuFxEmitterDesc{
        WeatherGpuFxEmitterDesc{box_centre_offset:[2.0,13.0,6.0],box_size:[45.0,35.0,18.0],
            life_min_max:[5.0,8.0],velocity_min:[-1.0,1.0,-5.0],
            velocity_max:[1.0,1.0,-5.0],clamp_to_ground:false}
    }
    #[test]
    fn source_z_up_box_matches_the_y_up_world_used_by_gravity_and_collision(){
        let (centre,size)=emitter_world_box(Vec3::new(-3.0,7.0,9.0),&emitter());
        assert_eq!(centre,[-3.0,13.0,9.0]);assert_eq!(size,[45.0,18.0,35.0]);
    }
    #[test]
    fn observer_translation_moves_only_the_window_and_keeps_its_world_axes(){
        let e=emitter();
        let (a,sa)=emitter_world_box(Vec3::new(3.0,7.0,9.0),&e);
        let (b,sb)=emitter_world_box(Vec3::new(12.0,2.0,-4.0),&e);
        assert_eq!(sa,sb);assert_eq!([b[0]-a[0],b[1]-a[1],b[2]-a[2]],[9.0,-5.0,-13.0]);
    }
}

#[cfg(test)]
mod surrounding_window_tests {
    use super::*;
    #[test]
    fn imported_view_ahead_offset_does_not_put_all_precipitation_behind_a_turn(){
        let emitter=WeatherGpuFxEmitterDesc{box_centre_offset:[0.0,13.0,6.0],
            box_size:[20.0,25.0,18.0],life_min_max:[5.0,8.0],velocity_min:[0.0;3],
            velocity_max:[0.0;3],clamp_to_ground:false};
        let position=Vec3::new(4.0,8.0,-3.0);
        let (centre,size)=emitter_world_box(position,&emitter);
        for [dx,dz] in [[1.0,0.0],[-1.0,0.0],[0.0,1.0],[0.0,-1.0]]{
            let point=[position.x+dx*5.0,position.z+dz*5.0];
            assert!((point[0]-centre[0]).abs()<size[0]*0.5);
            assert!((point[1]-centre[2]).abs()<size[2]*0.5);
        }
    }
}
