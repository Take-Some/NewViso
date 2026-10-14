use super::*;
use crate::snow::{SnowField,SnowPolicy,SnowLayerPolicy,SnowDriftPolicy,SnowDynamicsPolicy};
use crate::surface_snow::{SnowSurfaceSystem,SnowSurfacePolicy,SnowSurfaceKind,SnowSurfaceState};
use crate::thermal::ThermalEnvironment;
fn field(depth:f32,layered:bool)->SnowField{
    let mut f=SnowField::new(SnowFieldSpec{id:"budget".into(),seed:1,origin:[-4.0,-4.0],
        cell_size:1.0,columns:9,rows:9,ground:vec![0.0;81],depth:vec![depth;81]},
        SnowPolicy{compacted_fraction:0.47,contact_compaction:0.32,max_contact_radius:2.0,
            ridge_displacement_fraction:0.28}).unwrap();
    if layered {f.configure_layers(SnowLayerPolicy{contact_depth_metres:0.32,surface_blend_metres:0.025}).unwrap();}
    f
}
fn surface(id:&str,area:f32,height:f32,catch:f32)->SnowSurfacePatch{
    SnowSurfacePatch{id:id.into(),kind:SnowSurfaceKind::Roof,position:[0.0,height,0.0],
        area,slope_degrees:0.0,catch_fraction:catch,discharge:[2.0,0.0]}
}
fn policy()->SnowSurfacePolicy{
    SnowSurfacePolicy{maximum_depth:1.4,melt_metres_per_second:0.001,
        drainage_fraction_per_second:0.006,slide_fraction_per_second:0.025,wind_loss:0.0}
}
fn env()->ThermalEnvironment{ThermalEnvironment{temperature:-25.0,wind:28.0,wind_loss:0.0}}
fn close(a:f64,b:f64){assert!((a-b).abs()<0.00003,"volume {a} != {b}");}
fn drift()->SnowDriftPolicy{
    SnowDriftPolicy{wind_direction:[0.6,0.8],minimum_wind:8.0,transport_per_second:0.005,
        max_fraction_per_step:0.035,lee_distance:8.0,lee_width:1.4,lee_shelter:0.88}
}
fn dynamics()->SnowDynamicsPolicy{
    SnowDynamicsPolicy{dry_repose_degrees:34.0,wet_repose_degrees:24.0,crust_cohesion_metres:0.2,
        relaxation_per_second:0.6,saltation_length_metres:1.2,terrain_shadow_distance:12.0,
        wind_packing_per_second:0.006,substep_seconds:1.0}
}
#[test]
fn wind_and_cold_cannot_create_snow_on_an_empty_field(){
    for layered in [false,true]{
        let mut f=field(0.0,layered);
        let mut s=SnowSurfaceSystem::new(policy(),vec![surface("roof",4.0,3.0,1.0)]).unwrap();
        let p=SnowPrecipitationPlan::new(&f.spec,&[s.entries[0].patch.clone()]).unwrap();
        for _ in 0..30{
            let r=f.advance_exposed(2.0,0.0,0.0,Some(&p.ground_exposure)).unwrap();
            let d=f.advance_dynamics(2.0,80.0,drift(),&vec![0.0;81],dynamics()).unwrap();
            assert_eq!(r.snowfall_volume_m3,0.0);assert_eq!(d.wind_volume_m3,0.0);
            assert!(s.advance_caught(2.0,0.0,&env(),&[],Some(&p.surface_catch)).unwrap().is_empty());
        }
        assert_eq!(f.stored_volume_m3()+s.stored_volume_m3(),0.0);f.validate().unwrap();
    }
}
#[test]
fn dry_still_weather_does_not_refill_or_change_existing_snow(){
    let mut f=field(1.0,true);let before=serde_json::to_string(&f).unwrap();
    for _ in 0..300{f.advance(2.0,0.0,0.0).unwrap();}
    assert_eq!(serde_json::to_string(&f).unwrap(),before);
}
#[test]
fn wind_redistributes_existing_snow_without_a_precipitation_source(){
    let mut f=field(1.0,true);let before=f.stored_volume_m3();let depths=f.spec.depth.clone();
    let r=f.advance_dynamics(10.0,28.0,drift(),&vec![0.0;81],dynamics()).unwrap();
    assert!(r.wind_volume_m3>0.0);close(before,f.stored_volume_m3());
    assert!(f.spec.depth.iter().zip(&depths).any(|(a,b)|a>b));
    assert!(f.spec.depth.iter().zip(&depths).any(|(a,b)|a<b));f.validate().unwrap();
}
#[test]
fn one_sky_flux_is_split_between_overlapping_roofs_and_ground(){
    let mut f=field(0.0,true);
    let patches=vec![surface("lower",4.0,3.0,1.0),surface("upper",4.0,5.0,1.0)];
    let p=SnowPrecipitationPlan::new(&f.spec,&patches).unwrap();
    close(p.source_area_m2,81.0);assert_eq!(p.surface_catch[0],0.0);
    assert_eq!(p.surface_catch[1],1.0);assert_eq!(p.ground_exposure[40],0.0);
    let mut s=SnowSurfaceSystem::new(policy(),patches).unwrap();
    for _ in 0..20{
        f.advance_exposed(2.0,0.001,0.0,Some(&p.ground_exposure)).unwrap();
        assert!(s.advance_caught(2.0,0.001,&env(),&[],Some(&p.surface_catch)).unwrap().is_empty());
    }
    close(f.stored_volume_m3()+s.stored_volume_m3(),81.0*0.001*40.0);
    assert_eq!(f.spec.depth[40],0.0);
}
#[test]
fn fractional_and_partial_catchments_preserve_the_whole_precipitation_budget(){
    let mut f=field(0.0,false);
    let mut edge=surface("edge",6.2,5.0,0.73);edge.position[0]=4.0;edge.position[2]=3.95;
    let mut overlap=surface("overlap",2.3,3.0,0.54);overlap.position[0]=3.7;overlap.position[2]=3.6;
    let patches=vec![edge,overlap];
    let p=SnowPrecipitationPlan::new(&f.spec,&patches).unwrap();
    let area_ground=p.ground_exposure.iter().map(|v|*v as f64).sum::<f64>();
    let area_surface=patches.iter().zip(&p.surface_catch).map(|(e,v)|e.area as f64* *v as f64).sum::<f64>();
    close(area_ground+area_surface,81.0);
    let mut s=SnowSurfaceSystem::new(policy(),patches).unwrap();
    f.advance_exposed(60.0,0.0001,0.0,Some(&p.ground_exposure)).unwrap();
    assert!(s.advance_caught(60.0,0.0001,&env(),&[],Some(&p.surface_catch)).unwrap().is_empty());
    close(f.stored_volume_m3()+s.stored_volume_m3(),81.0*0.0001*60.0);
}
#[test]
fn buried_or_off_map_surfaces_cannot_create_additional_sky_snow(){
    let f=field(0.0,true);
    let mut off=surface("outside",4.0,4.0,1.0);off.position[0]=1000.0;
    let p=SnowPrecipitationPlan::new(&f.spec,&[surface("below",4.0,-1.0,1.0),off]).unwrap();
    assert!(p.ground_exposure.iter().all(|v|*v==1.0));assert_eq!(p.surface_catch,vec![0.0,0.0]);
}
#[test]
fn shedding_a_loaded_branch_only_transfers_its_existing_snow(){
    let mut f=field(0.0,true);
    let mut patch=surface("branch",4.0,4.0,0.58);patch.kind=SnowSurfaceKind::Branch;patch.slope_degrees=65.0;
    let mut s=SnowSurfaceSystem::new(policy(),vec![patch]).unwrap();
    s.entries[0].state.snow=0.2;let before=s.stored_volume_m3();
    let flows=s.advance(20.0,0.0,&env(),&[]).unwrap();assert!(!flows.is_empty());
    let mut outside=0.0;
    for r in flows{outside+=f.deposit_surface_runoff(r.discharge,[r.snow_volume,r.liquid_volume,r.ice_volume])
        .unwrap().iter().map(|v|*v as f64).sum::<f64>();}
    assert!(f.stored_volume_m3()>0.0);
    close(before,s.stored_volume_m3()+f.stored_volume_m3()+outside);
}
#[test]
fn roof_resize_removal_and_capacity_changes_preserve_all_three_phases(){
    let mut s=SnowSurfaceSystem::new(policy(),vec![surface("roof",4.0,3.0,1.0)]).unwrap();
    s.entries[0].state=SnowSurfaceState{snow:0.3,water:0.2,ice:0.1};let before=s.stored_volume_m3();
    let mut flows=s.reconcile(policy(),vec![surface("roof",12.0,3.0,1.0)]).unwrap();
    close(before,s.stored_volume_m3()+flows.iter().map(|r|
        r.snow_volume as f64+r.liquid_volume as f64+r.ice_volume as f64).sum::<f64>());
    assert!(s.stored_volume_m3()<=before);close(s.entries[0].state.snow as f64,0.1);
    flows.extend(s.reconcile(policy(),vec![surface("roof",0.25,3.0,1.0)]).unwrap());
    flows.extend(s.reconcile(policy(),vec![]).unwrap());
    let mut f=field(0.0,true);let mut outside=0.0;
    for r in flows{outside+=f.deposit_surface_runoff(r.discharge,[r.snow_volume,r.liquid_volume,r.ice_volume])
        .unwrap().iter().map(|v|*v as f64).sum::<f64>();}
    close(before,f.stored_volume_m3()+s.stored_volume_m3()+outside);
    let saved=serde_json::to_value(f).unwrap();
    assert!(saved["surface_ice"].as_array().unwrap().iter().any(|v|v.as_f64().unwrap()>0.0));
}
#[test]
fn saturation_and_melting_account_for_rejected_volume_instead_of_creating_it(){
    for layered in [false,true]{
        let mut f=field(20.0,layered);
        let before=f.stored_volume_m3();
        let r=f.advance_exposed(60.0,0.05,0.05,None).unwrap();
        close(before+r.snowfall_volume_m3,f.stored_volume_m3()+r.outflow_volume_m3);
        f.validate().unwrap();
        assert_eq!(f.deposit_surface_runoff([f32::MAX,f32::MAX],[1.0,2.0,3.0]).unwrap(),[1.0,2.0,3.0]);
    }
}
#[test]
fn legacy_equal_snowfall_and_melt_keeps_new_water_and_preserves_the_budget(){
    let mut f=field(1.0,false);let before=f.stored_volume_m3();
    let r=f.advance_exposed(2.0,0.01,0.01,None).unwrap();
    close(before+r.snowfall_volume_m3,f.stored_volume_m3()+r.outflow_volume_m3);
    assert!(r.melted_volume_m3>0.0);f.validate().unwrap();
}
#[test]
fn invalid_interception_factors_are_rejected_before_mutation(){
    let mut f=field(1.0,true);let before=serde_json::to_string(&f).unwrap();
    assert!(f.advance_exposed(2.0,0.001,0.0,Some(&[1.0])).is_err());
    assert!(f.advance_exposed(2.0,0.001,0.0,Some(&vec![f32::NAN;81])).is_err());
    assert_eq!(serde_json::to_string(&f).unwrap(),before);
}

#[test]
fn falling_surface_snow_buries_tracks_without_a_sky_source(){
    let mut f=field(1.0,true);
    f.stamp_microprint(crate::snow::SnowMicroprint{position:[0.0,0.0],forward:[1.0,0.0],
        half_width:0.11,half_length:0.2,depression:0.07}).unwrap();
    let before=f.stored_volume_m3();
    let rejected=f.deposit_surface_runoff([0.0,0.0],[0.9,0.0,0.0]).unwrap();
    close(before+0.9,f.stored_volume_m3()+rejected.iter().map(|v|*v as f64).sum::<f64>());
    assert!(f.microprints().is_empty());f.validate().unwrap();
}
