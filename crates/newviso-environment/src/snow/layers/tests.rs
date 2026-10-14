use super::*;
use crate::snow::{SnowFieldSpec,SnowPolicy,SnowDynamicsPolicy,SnowDriftPolicy,SnowMicroprint};
fn field()->SnowField {
    SnowField::new(SnowFieldSpec{id:"layers".into(),seed:42,origin:[-4.0,-4.0],
        cell_size:1.0,columns:9,rows:9,ground:vec![0.0;81],depth:vec![1.0;81]},
        SnowPolicy{compacted_fraction:0.47,contact_compaction:0.5,
            max_contact_radius:2.0,ridge_displacement_fraction:0.28}).unwrap()
}
fn policy()->SnowLayerPolicy{SnowLayerPolicy{contact_depth_metres:0.32,surface_blend_metres:0.025}}
fn icy()->SnowField {
    let mut f=field();
    f.compaction.fill(0.45);
    f.strata.fill(SnowStrata{powder:0.1,crust:0.95,moisture:0.0});
    f.configure_layers(policy()).unwrap();f.take_visual_changes();f
}
fn drift()->SnowDriftPolicy {
    SnowDriftPolicy{wind_direction:[1.0,0.0],minimum_wind:8.0,
        transport_per_second:0.005,max_fraction_per_step:0.15,
        lee_distance:8.0,lee_width:1.4,lee_shelter:0.88}
}
fn dynamics()->SnowDynamicsPolicy {
    SnowDynamicsPolicy{dry_repose_degrees:34.0,wet_repose_degrees:24.0,
        crust_cohesion_metres:0.2,relaxation_per_second:0.6,
        saltation_length_metres:1.2,terrain_shadow_distance:12.0,
        wind_packing_per_second:0.006,substep_seconds:1.0}
}
fn total(f:&SnowField)->f64{f.spec.depth.iter().map(|d|*d as f64).sum()}
#[test]
fn fresh_snow_buries_intact_crust_and_erosion_exposes_it_again(){
    let mut f=icy();let old=f.layer_profile(0.0,0.0).unwrap()[0];
    f.advance(4.0,0.05,0.0).unwrap();
    let profile=f.layer_profile(0.0,0.0).unwrap();
    assert_eq!(profile.len(),2);assert_eq!(profile[1],old);
    assert!(f.sample(0.0,0.0).unwrap().powder>0.99);
    f.layer_columns[40].remove_top(0.2);f.sync_column(40);
    assert!(f.sample(0.0,0.0).unwrap().crust>0.94);
    assert!((f.spec.depth[40]-1.0).abs()<1e-6);f.validate().unwrap();
}
#[test]
fn a_thin_dusting_blends_visually_without_diluting_the_buried_layer(){
    let mut f=icy();let crust=f.layer_profile(0.0,0.0).unwrap()[0];
    f.advance(1.0,0.002,0.0).unwrap();
    let s=f.sample(0.0,0.0).unwrap();
    assert!(s.powder>0.1&&s.powder<0.5);
    assert!(s.crust>0.8);assert_eq!(f.layer_profile(0.0,0.0).unwrap()[1],crust);
}
#[test]
fn repeated_snowfall_merges_one_deposition_event_without_growing_a_stack(){
    let mut f=icy();
    for _ in 0..300{f.advance(2.0,0.0001,0.0).unwrap();}
    assert_eq!(f.layer_profile(0.0,0.0).unwrap().len(),2);
    assert!((f.spec.depth[40]-1.06).abs()<0.0001);
}
#[test]
fn one_contact_never_compacts_the_entire_metre_deep_column(){
    let mut f=field();f.configure_layers(policy()).unwrap();
    f.contact(0.0,0.0,0.75,1.0).unwrap();
    let p=f.layer_profile(0.0,0.0).unwrap();
    assert!(p.len()>=2);assert!(p[0][1]>0.1);
    assert!(p.last().unwrap()[1]<1e-6,"basal snow is below penetration depth");
    assert!(f.sample(0.0,0.0).unwrap().surface_height>0.80);
    f.validate().unwrap();
}
#[test]
fn a_light_step_stops_at_crust_and_a_heavy_step_fractures_it(){
    let mut light=icy();let mut heavy=light.clone();
    light.contact(0.0,0.0,0.75,0.2).unwrap();
    heavy.contact(0.0,0.0,0.75,1.0).unwrap();
    assert!(heavy.sample(0.0,0.0).unwrap().crust<light.sample(0.0,0.0).unwrap().crust-0.2);
    assert!(heavy.sample(0.0,0.0).unwrap().surface_height<light.sample(0.0,0.0).unwrap().surface_height);
}
#[test]
fn layered_ridge_displacement_conserves_snow_and_preserves_deep_layers(){
    let mut f=icy();f.advance(4.0,0.05,0.0).unwrap();let before=total(&f);
    let bottom=f.layer_profile(0.0,0.0).unwrap().last().copied().unwrap();
    for _ in 0..10{f.contact(0.0,0.0,0.75,0.7).unwrap();}
    assert!((before-total(&f)).abs()<0.00002);
    let basal=f.layer_profile(0.0,0.0).unwrap().last().copied().unwrap();
    assert!(basal[0]>0.4,"deep material remains outside the contact zone");
    assert_eq!(&basal[1..],&bottom[1..]);
    f.validate().unwrap();
}
#[test]
fn thermal_freeze_builds_a_surface_crust_without_freezing_the_whole_column(){
    let mut f=field();f.strata.fill(SnowStrata{powder:0.7,crust:0.0,moisture:0.3});
    f.configure_layers(policy()).unwrap();
    f.weather_strata(20.0,-25.0).unwrap();
    let p=f.layer_profile(0.0,0.0).unwrap();
    assert_eq!(p.len(),2);assert!(p[0][3]>0.0);assert_eq!(p[1][3],0.0);
    assert_eq!(p[1][4],0.3);assert_eq!(f.spec.depth[40],1.0);
    f.validate().unwrap();
}
#[test]
fn equal_snowfall_and_melt_still_replace_surface_material_and_create_runoff(){
    let mut f=icy();let old=total(&f);
    f.advance(2.0,0.01,0.01).unwrap();
    assert!((old-total(&f)).abs()<0.00002);
    assert!(f.surface_water[40]>0.0);assert!(f.sample(0.0,0.0).unwrap().moisture>0.0);
    assert!(f.has_visual_changes());f.validate().unwrap();
}
#[test]
fn column_capacity_merges_deepest_layers_preserving_height_and_channels(){
    let mut c=Column::default();
    let mut expected_height=0.0;let mut expected_compaction=0.0;
    for i in 0..20 {
        let m=[if i%2==0{1.0}else{0.0},if i%2==0{0.0}else{0.9},0.0,i as f32/25.0];
        let l=Layer::new(0.1,m);expected_height+=l.height(0.47);
        expected_compaction+=l.depth()*m[3];c.deposit(l);
    }
    assert_eq!(c.len as usize,MAX_LAYERS);
    assert!((c.depth()-2.0).abs()<1e-6);
    assert!((c.layers[..4].iter().map(|l|l.height(0.47)).sum::<f32>()-expected_height).abs()<1e-6);
    assert!((c.layers[..4].iter().map(|l|l.depth()*l.0[1]).sum::<f32>()-expected_compaction).abs()<1e-6);
    assert_eq!(c.top().material()[1],0.9);
}
#[test]
fn transport_moves_only_exposed_snow_and_keeps_columns_consistent(){
    let mut f=icy();f.column_deposit(40,0.2,[1.0,0.0,0.0,0.0]);
    let old=total(&f);let mut p=dynamics();p.relaxation_per_second=0.0;
    let r=f.advance_dynamics(20.0,28.0,drift(),&vec![0.0;81],p).unwrap();
    assert!(r.wind_volume_m3>0.0);assert!((old-total(&f)).abs()<0.0001);
    assert_eq!(f.layer_profile(0.0,0.0).unwrap().last().unwrap()[0],1.0);
    for _ in 0..10{f.advance_dynamics(2.0,28.0,drift(),&vec![0.0;81],dynamics()).unwrap();}
    f.validate().unwrap();
}
#[test]
fn legacy_wind_service_also_preserves_layered_columns(){
    let mut f=icy();f.advance(4.0,0.05,0.0).unwrap();let old=total(&f);
    assert!(f.advance_wind(10.0,28.0,drift(),&vec![0.0;81]).unwrap()>0);
    assert!((old-total(&f)).abs()<0.0001);f.validate().unwrap();
}
#[test]
fn layers_and_fine_tracks_survive_a_checkpoint_exactly(){
    let mut f=icy();f.advance(4.0,0.05,0.0).unwrap();f.contact(0.0,0.0,0.75,0.8).unwrap();
    f.stamp_microprint(SnowMicroprint{position:[0.0,0.0],forward:[1.0,0.0],
        half_width:0.11,half_length:0.2,depression:0.07}).unwrap();
    let saved=serde_json::to_string(&f).unwrap();
    assert!(!saved.contains("layer_contact_scratch"));
    let restored:SnowField=serde_json::from_str(&saved).unwrap();restored.validate().unwrap();
    assert_eq!(restored.layer_columns,f.layer_columns);
    assert_eq!(restored.microprints(),f.microprints());
    assert_eq!(restored.all_render_vertices(),f.all_render_vertices());
}
#[test]
fn legacy_checkpoint_migration_preserves_existing_heights_and_tracks(){
    let mut f=field();f.contact(0.0,0.0,0.75,0.8).unwrap();
    let encoded=serde_json::to_string(&f).unwrap();
    assert!(!encoded.contains("layer_columns"));
    let mut old:SnowField=serde_json::from_str(&encoded).unwrap();
    let vertices=old.all_vertices();old.configure_layers(policy()).unwrap();
    assert_eq!(vertices,old.all_vertices());old.validate().unwrap();
}
#[test]
fn invalid_policy_or_saved_layer_budget_is_rejected(){
    let mut f=field();let encoded=serde_json::to_string(&f).unwrap();
    assert!(f.configure_layers(SnowLayerPolicy{contact_depth_metres:f32::NAN,..policy()}).is_err());
    assert_eq!(encoded,serde_json::to_string(&f).unwrap());
    f.configure_layers(policy()).unwrap();
    f.layer_columns[40].layers[0].0[0]=2.0;assert!(f.validate().is_err());
    assert!(serde_json::from_str::<Column>("[[1,0,1,0,0],[1,0,1,0,0],[1,0,1,0,0],[1,0,1,0,0],[1,0,1,0,0]]").is_err());
    assert!(serde_json::from_str::<Column>("[[1,0,1,0,2]]").is_err());
}
#[test]
fn geometry_payload_and_gameplay_sample_share_the_same_surface(){
    let mut f=icy();f.advance(4.0,0.05,0.0).unwrap();f.contact(0.0,0.0,0.75,0.8).unwrap();
    for v in f.all_render_vertices(){
        let s=f.sample(v[0],v[2]).unwrap();
        assert_eq!(v[1],s.surface_height);assert_eq!(v[3],s.powder);
        assert_eq!(v[4],s.crust);assert_eq!(v[5],s.moisture);
    }
}

#[test]
fn wind_packing_never_loosens_already_heavily_compacted_snow(){
    let mut f=icy();f.layer_columns[40].layers[0].0[1]=0.96;f.sync_column(40);
    let before=f.sample(0.0,0.0).unwrap().surface_height;
    f.column_pack(40,0.01);
    assert_eq!(f.compaction[40],0.96);
    assert_eq!(f.sample(0.0,0.0).unwrap().surface_height,before);
    f.validate().unwrap();
}
#[test]
fn saturated_layered_transport_and_contacts_never_overfill_or_lose_snow(){
    let mut f=field();
    for (i,d) in f.spec.depth.iter_mut().enumerate(){*d=if i%3==0{20.0}else{19.999};}
    f.configure_layers(policy()).unwrap();let before=total(&f);
    for _ in 0..5 {
        f.advance_dynamics(2.0,28.0,drift(),&vec![0.0;81],dynamics()).unwrap();
        f.contact(0.0,0.0,0.75,0.8).unwrap();f.validate().unwrap();
    }
    assert!((before-total(&f)).abs()<0.001);
}
