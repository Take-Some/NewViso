//! Non-colliding snow cover caps generated from persistent object-surface snow.
//! Uses the existing heightfield vertex path and snow material; no cube proxy
//! is ever drawn for the cap despite reusing a runtime render slot.
use super::*;
use std::collections::BTreeSet;
use std::sync::Arc;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneSnowCap {
    pub id: String,
    pub kind: String,
    pub position: [f32; 3],
    pub area: f32,
    pub slope_degrees: f32,
    pub snow_depth: f32,
}
impl SceneSnowCap {
    fn validate(&self)->Result<(),String>{
        if self.id.is_empty()||self.id.len()>128
            || !matches!(self.kind.as_str(),"roof"|"branch"|"rock")
            || !self.position.iter().all(|v|v.is_finite())
            || !self.area.is_finite()||!(0.025..=250.0).contains(&self.area)
            || !self.slope_degrees.is_finite()||!(0.0..=89.0).contains(&self.slope_degrees)
            || !self.snow_depth.is_finite()||!(0.0..=3.0).contains(&self.snow_depth){
            return Err("invalid scene snow cap".into());
        }
        Ok(())
    }
    fn half_span(&self)->f32{
        match self.kind.as_str(){
            "roof" => (self.area.sqrt()*0.5).clamp(0.3,8.0),
            _ => (self.area/std::f32::consts::PI).sqrt().clamp(0.25,4.0),
        }
    }
}

impl heightfield::HeightfieldPatch {
    /// Mesh-shaped shallow snow cap. Polygonal roof silhouette (superellipse)
    /// or irregular branch/rock dome, without white rectangle base geometry.
    /// A radial fan tapers to zero along the cap perimeter.
    pub(super) fn update_surface_cap_mesh(&mut self,cap:&SceneSnowCap){
        let mut vertices=Vec::new();
        let sections=16_usize;
        let levels=[0.0_f32,0.38,0.7,0.9,1.0];
        let span=cap.half_span();
        let mound=(cap.snow_depth*2.25).clamp(0.002,0.75);
        let seed=cap.position[0]*3.73+cap.position[2]*5.19;
        let eval=|radius:f32,angle:f32|->(Vec3,Vec3){
            let co=angle.cos();let si=angle.sin();
            let (ux,uz)=if cap.kind=="roof" {
                let degree=4.2;
                (co.signum()*co.abs().powf(2.0/degree),
                 si.signum()*si.abs().powf(2.0/degree))
            }else{(co,si)};
            let irregular=if cap.kind=="roof" {1.0}
                else {1.0+0.085*(3.0*angle+seed).sin()+
                                0.04*(5.0*angle-seed).cos()};
            // Roof lips extend a few centimetres past the footprint and
            // droop under load; boughs/rocks keep their irregular ellipse.
            let lip=if cap.kind=="roof"{
                1.0+(cap.snow_depth*0.18).clamp(0.0,0.13)
            }else{1.0};
            let px=span*radius*ux*irregular*lip;
            let pz=span*radius*uz*irregular*lip;
            let taper=(1.0-radius*radius).max(0.0).powf(1.4);
            let grain=(0.015*(px*8.3+pz*5.1+seed).sin()*
                             (pz*7.5-px*3.8).cos())*taper*
                             (mound/0.1).clamp(0.0,1.0);
            let roof_tilt=if cap.kind=="roof"{
                px*cap.slope_degrees.to_radians().tan().min(0.5)*0.18
            }else{0.0};
            let overhang=if cap.kind=="roof"{
                ((radius-0.86)/0.14).clamp(0.0,1.0)*
                (cap.snow_depth*0.16).clamp(0.0,0.11)
            }else{0.0};
            let y=cap.position[1]+roof_tilt+mound*taper+grain-overhang;
            let slope=-2.8*radius*(1.0-radius*radius).max(0.0).powf(0.4)
                *mound/span;
            let tilt=if cap.kind=="roof"{
                cap.slope_degrees.to_radians().tan().min(0.5)*0.18
            }else{0.0};
            let normal=Vec3::new(-(slope*co+tilt),1.0,-slope*si).normalized();
            (Vec3::new(cap.position[0]+px,y,cap.position[2]+pz),normal)
        };
        let mut push=|r:f32,a:f32|{
            let (point,normal)=eval(r,a);
            heightfield::append_snow_vertex(
                &mut vertices,point,normal,[0.93,0.96,1.0,1.125],
                [1.0,0.0,0.0],true);
        };
        for ring in 0..levels.len()-1 {
            let inner=levels[ring];let outer=levels[ring+1];
            for i in 0..sections {
                let a=i as f32*std::f32::consts::TAU/sections as f32;
                let b=(i+1) as f32*std::f32::consts::TAU/sections as f32;
                for (r,t) in [(inner,a),(outer,b),(outer,a)]{push(r,t);}
                if ring!=0 {
                    for (r,t) in [(inner,a),(inner,b),(outer,b)]{push(r,t);}
                }
            }
        }
        self.vertices=Arc::from(vertices);
    }
}

impl Scene3dRuntime {
    /// Refreshes only significantly changed caps and hides melted/removed caps.
    /// Inactive caps allocate no entity or GPU triangles.
    pub fn sync_surface_snow_caps(&mut self,caps:&[SceneSnowCap])->Result<usize,String>{
        if caps.len()>512{return Err("too many scene snow caps".into());}
        let mut ids=BTreeSet::<&str>::new();
        for cap in caps{
            cap.validate()?;
            if !ids.insert(&cap.id){return Err("duplicate scene snow cap ID".into());}
        }
        let mut updated=0;
        for cap in caps{
            let key=format!("northstar.snowcap.{}",cap.id);
            let existing=self.runtime_entity_ids.get(&key).copied();
            if cap.snow_depth<0.008{
                if let Some(id)=existing{
                    self.set_entity_visibility(id,"surface_snow",false)?;
                }
                self.snow_cap_depths.remove(&key);
                continue;
            }
            if let Some(previous)=self.snow_cap_depths.get(&key){
                if (*previous-cap.snow_depth).abs()<0.004{continue;}
            }
            let span=cap.half_span();
            let id=if let Some(id)=existing{id}else{
                self.upsert_runtime_dynamic_entity(&key,SceneRuntimeEntityDesc{
                    visual:SceneRuntimeVisualKind::Cube,
                    asset_ref:None,texture_dictionary:None,
                    position:cap.position,rotation_degrees:[0.0;3],
                    scale:[span*2.0,1.0,span*2.0],
                    bounds_half_extent:[span,0.9,span],
                    base_color:[0.93,0.96,1.0,1.0],
                    solid:false,visible_distance:85.0,stream_distance:110.0,
                    fade_range:15.0,
                })?
            };
            let slot=self.world.entity(SceneEntityId(id))
                .and_then(|entity|entity.render_slot)
                .ok_or("snow cap has no runtime render slot")?;
            if !self.terrain_meshes.contains_key(&slot){
                let record=json!({"collider":{"solid":false},
                   "mesh":{"heightfield":{"columns":3,"rows":3,
                       "heights":[0,0,0,0,0,0,0,0,0],
                       "ground_heights":vec![cap.position[1];9],
                       "surface_kind":"snow","render_subdivisions":1,
                       "deformable":false}}});
                // The scene heightfield parser reads 3x3 mesh position
                // and snow material semantics, then the radial cap replaces
                // its placeholder render triangles.
                let patch=heightfield::HeightfieldPatch::from_record(
                    id,&record,Vec3::new(cap.position[0],cap.position[1],cap.position[2]),
                    Vec3::new(span*2.0,1.0,span*2.0),[0.93,0.96,1.0,1.0])?;
                self.terrain_meshes.insert(slot,patch);
            }
            self.terrain_meshes.get_mut(&slot).unwrap().update_surface_cap_mesh(cap);
            self.set_entity_visibility(id,"surface_snow",true)?;
            self.snow_cap_depths.insert(key,cap.snow_depth);
            updated+=1;
        }
        // A destroyed or absent building cannot retain a detached snow roof.
        let missing=self.snow_cap_depths.keys()
            .filter(|key|!ids.contains(key.trim_start_matches("northstar.snowcap.")))
            .cloned().collect::<Vec<_>>();
        for key in missing{
            if let Some(id)=self.runtime_entity_ids.get(&key).copied(){
                self.set_entity_visibility(id,"surface_snow",false)?;
            }
            self.snow_cap_depths.remove(&key);
        }
        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snow_cap_geometry_has_no_rectangular_base(){
        let desc=SceneSnowCap{id:"branch.7".into(),kind:"branch".into(),
            position:[3.0,6.0,2.0],area:5.0,slope_degrees:35.0,
            snow_depth:0.08};
        desc.validate().unwrap();
        let h=json!({"collider":{"solid":false},
            "mesh":{"heightfield":{"columns":3,"rows":3,
                "heights":[0,0,0,0,0,0,0,0,0],
                "ground_heights":[6,6,6,6,6,6,6,6,6],
                "surface_kind":"snow"}}});
        let mut cap=heightfield::HeightfieldPatch::from_record(4,&h,
            Vec3::new(3.0,6.0,2.0),Vec3::new(3.0,1.0,3.0),[1.0;4]).unwrap();
        cap.update_surface_cap_mesh(&desc);
        let triangles=cap.vertices.len()/FLOATS_PER_VERTEX/3;
        assert_eq!(triangles,16*(1+3*2));
        assert!(!cap.collides);
        assert!(cap.vertices.chunks_exact(FLOATS_PER_VERTEX).any(|v|v[1]>6.01));
        assert!(cap.vertices.chunks_exact(FLOATS_PER_VERTEX).all(|v|
            (v[10]-1.125).abs()<0.0001));
        let roof=SceneSnowCap{id:"roof.test".into(),kind:"roof".into(),
            position:[0.0,5.0,0.0],area:9.0,
            slope_degrees:27.0,snow_depth:0.5};
        cap.update_surface_cap_mesh(&roof);
        assert!(cap.vertices.chunks_exact(FLOATS_PER_VERTEX)
          .any(|v|v[0].abs()>roof.half_span()),
          "loaded roof snow should overhang its edge, not form a cuboid");
        let mut bad=desc;
        bad.snow_depth=-0.5;
        assert!(bad.validate().is_err());
    }
}
