use super::*;
#[test]
fn separable_sampling_preserves_f32_clamping_and_interpolation_exactly() {
    let mut f=field(17);
    for (i,height) in f.spec.ground.iter_mut().enumerate() {*height=(i as f32*0.37).sin();}
    let heights=(0..289).map(|i|f.transport_height(i)).collect::<Vec<_>>();
    for shift in [[0.6,0.8],[-0.6,-0.8],[12.345,-5.678],[24.0,0.0]] {
        let kernel=HeightSampleKernel::new(17,17,shift);
        for row in 0..17 {for col in 0..17 {
            assert_eq!(kernel.sample(&heights,col,row,17),
                f.transport_height_at(&heights,col as f32-shift[0],row as f32-shift[1]));
        }}
    }
}
#[test]
fn dry_transport_does_not_emit_unchanged_liquid_payloads() {
    let mut f=field(17);f.spec.depth.fill(0.5);
    f.advance_dynamics(2.0,28.0,drift([0.6,0.8]),&vec![0.0;289],policy()).unwrap();
    assert!(f.has_visual_changes());
    let (snow,liquid)=f.take_visual_changes();
    assert!(!snow.is_empty());assert!(liquid.is_empty());
    assert!(!f.has_visual_changes());
    f.deposit_runoff([8.0,8.0],0.0,1.0).unwrap();
    assert!(!f.take_visual_changes().1.is_empty());
}
#[test]
fn dirty_membership_deduplicates_and_orders_cells_across_drains() {
    let mut dirty=crate::snow::SnowDirtyCells::new();
    for i in [130,1,64,63,130,1] {dirty.insert(i);}
    assert_eq!(dirty.len(),4);assert_eq!(dirty.take_indices(),vec![1,63,64,130]);
    assert!(dirty.is_empty());
    dirty.insert(65);assert_eq!(dirty.take_indices(),vec![65]);
}

use crate::snow::{SnowFieldSpec, SnowPolicy, SnowWindObstacle};
fn field(n:usize) -> SnowField {
    SnowField::new(SnowFieldSpec {id:"transport-test".into(),seed:1,
        origin:[0.0,0.0],cell_size:1.0,columns:n,rows:n,
        ground:vec![0.0;n*n],depth:vec![0.0;n*n]},
        SnowPolicy {compacted_fraction:0.47,contact_compaction:0.32,
            max_contact_radius:2.0,ridge_displacement_fraction:0.28}).unwrap()
}
fn policy() -> SnowDynamicsPolicy {
    SnowDynamicsPolicy {dry_repose_degrees:34.0,wet_repose_degrees:24.0,
        crust_cohesion_metres:0.2,relaxation_per_second:0.6,
        saltation_length_metres:1.0,terrain_shadow_distance:8.0,
        wind_packing_per_second:0.006,substep_seconds:1.0}
}
fn drift(direction:[f32;2]) -> SnowDriftPolicy {
    SnowDriftPolicy {wind_direction:direction,minimum_wind:8.0,
        transport_per_second:0.003,max_fraction_per_step:0.2,
        lee_distance:8.0,lee_width:1.5,lee_shelter:0.9}
}
fn total(f:&SnowField)->f64 {f.spec.depth.iter().map(|x|*x as f64).sum()}
#[test]
fn diagonal_wind_moves_in_both_axes_and_conserves_volume() {
    let mut f=field(17);f.spec.depth[8*17+8]=0.5;
    let ground=f.spec.ground.clone();let before=total(&f);
    let mut p=policy();p.relaxation_per_second=0.0;
    let report=f.advance_dynamics(12.0,24.0,drift([0.6,0.8]),&vec![0.0;289],p).unwrap();
    let mut x=0.0;let mut z=0.0;
    for (i,d) in f.spec.depth.iter().enumerate(){x+=(i%17) as f64*(*d as f64);z+=(i/17) as f64*(*d as f64);}
    assert!(x/total(&f)>8.1 && z/total(&f)>8.1);
    assert!(report.wind_volume_m3>0.0);
    assert!((before-total(&f)).abs()<2e-6);
    assert_eq!(ground,f.spec.ground);f.validate().unwrap();
}
#[test]
fn receiver_budget_rejects_converging_overflow_without_losing_snow() {
    let mut f=field(5);f.spec.depth.fill(20.0);f.spec.depth[12]=19.9;
    let mut scratch=Scratch::new(25);
    for from in [6,7,8,11,13,16,17,18] {
        scratch.transfers.push(Transfer{from,to:12,depth:2.0,material:[1.0,0.0,0.0,0.0]});
    }
    let before=total(&f);f.commit_transport(&mut scratch);
    assert!(f.spec.depth.iter().all(|v|*v<=20.0&&*v>=0.0));
    assert!((before-total(&f)).abs()<2e-5);
    assert!((f.spec.depth[12]-20.0).abs()<1e-6);
}
#[test]
fn unstable_mound_slides_and_stable_plane_does_not() {
    let mut f=field(13);f.spec.depth[6*13+6]=4.0;
    let before=total(&f);
    let r=f.advance_dynamics(20.0,0.0,drift([1.0,0.0]),&vec![0.0;169],policy()).unwrap();
    assert!(r.slide_volume_m3>0.1 && f.spec.depth[6*13+6]<2.0);
    assert!((before-total(&f)).abs()<2e-5);
    let mut flat=field(9);flat.spec.depth.fill(0.5);
    assert_eq!(flat.advance_dynamics(10.0,0.0,drift([1.0,0.0]),&vec![0.0;81],policy()).unwrap().slide_volume_m3,0.0);
}
#[test]
fn wet_slopes_release_before_dry_slopes_and_crust_resists() {
    let mut dry=field(7);dry.spec.depth[24]=0.6;
    let mut wet=dry.clone();wet.strata[24].moisture=1.0;
    let mut crust=dry.clone();crust.strata[24].crust=1.0;
    let map=vec![0.0;49];
    let a=dry.advance_dynamics(2.0,0.0,drift([1.0,0.0]),&map,policy()).unwrap();
    let b=wet.advance_dynamics(2.0,0.0,drift([1.0,0.0]),&map,policy()).unwrap();
    let c=crust.advance_dynamics(2.0,0.0,drift([1.0,0.0]),&map,policy()).unwrap();
    assert_eq!(a.slide_volume_m3,0.0);assert!(b.slide_volume_m3>0.0);assert_eq!(c.slide_volume_m3,0.0);
}
#[test]
fn shelter_builds_a_lee_drift_instead_of_uniform_translation() {
    let mut f=field(25);f.spec.depth.fill(0.5);
    let p=drift([1.0,0.0]);
    let map=f.wind_shelter_map(p,&[SnowWindObstacle {position:[8.0,12.0],radius:1.5}]).unwrap();
    let before=total(&f);f.advance_dynamics(30.0,24.0,p,&map,policy()).unwrap();
    assert!(f.spec.depth[12*25+8]>0.5,"snow must accumulate where transport slows");
    assert!((before-total(&f)).abs()<0.0005);f.validate().unwrap();
}
#[test]
fn terrain_lee_suppresses_erosion_and_hardened_snow_does_not_blow() {
    let mut exposed=field(25);exposed.spec.depth[12*25+12]=0.3;
    let mut lee=exposed.clone();for row in 0..25 {lee.spec.ground[row*25+10]=3.0;}
    let mut packed=exposed.clone();packed.compaction.fill(1.0);
    let mut p=policy();p.relaxation_per_second=0.0;
    let map=vec![0.0;625];let d=drift([1.0,0.0]);
    exposed.advance_dynamics(8.0,24.0,d,&map,p).unwrap();
    lee.advance_dynamics(8.0,24.0,d,&map,p).unwrap();
    assert!(lee.spec.depth[12*25+12]>exposed.spec.depth[12*25+12]);
    assert_eq!(packed.advance_dynamics(8.0,24.0,d,&map,p).unwrap().wind_volume_m3,0.0);
}
#[test]
fn fixed_substeps_match_chunked_steps_and_restart_preserves_deformation() {
    let mut a=field(13);a.spec.depth[84]=3.0;
    let mut b=a.clone();let map=vec![0.0;169];let d=drift([0.6,0.8]);
    a.advance_dynamics(10.0,16.0,d,&map,policy()).unwrap();
    for _ in 0..10 {b.advance_dynamics(1.0,16.0,d,&map,policy()).unwrap();}
    assert_eq!(a.spec.depth,b.spec.depth);assert_eq!(a.strata,b.strata);
    let saved=serde_json::to_string(&a).unwrap();
    let mut resumed:SnowField=serde_json::from_str(&saved).unwrap();resumed.validate().unwrap();
    resumed.advance_dynamics(1.0,16.0,d,&map,policy()).unwrap();
    a.advance_dynamics(1.0,16.0,d,&map,policy()).unwrap();
    assert_eq!(resumed.spec.depth,a.spec.depth);
}
#[test]
fn invalid_inputs_fail_before_mutating_field() {
    let mut f=field(5);f.spec.depth.fill(0.5);
    let saved=serde_json::to_string(&f).unwrap();
    assert!(f.advance_dynamics(61.0,24.0,drift([1.0,0.0]),&vec![0.0;25],policy()).is_err());
    assert!(f.advance_dynamics(1.0,f32::NAN,drift([1.0,0.0]),&vec![0.0;25],policy()).is_err());
    assert!(f.advance_dynamics(1.0,24.0,drift([0.0,0.0]),&vec![0.0;25],policy()).is_err());
    assert_eq!(saved,serde_json::to_string(&f).unwrap());
}
#[test]
fn full_frostline_grid_has_bounded_step_cost() {
    let mut f=field(257);f.spec.depth.fill(0.5);
    let start=std::time::Instant::now();
    let report=f.advance_dynamics(2.0,28.0,drift([0.6,0.8]),&vec![0.0;257*257],policy()).unwrap();
    eprintln!("snow 257x257 / 2 seconds: {:?}, cells={}",start.elapsed(),report.changed_cells);
    assert_eq!(report.substeps,2);f.validate().unwrap();
}
