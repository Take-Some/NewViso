//! Persistent snow on authored roofs, branches and rocks.
//! Geometry-independent physical service: no project-specific models or shaders.
use crate::thermal::{HeatSource,ThermalEnvironment};
use serde::{Serialize,Deserialize};
use std::collections::{BTreeMap,BTreeSet};

#[derive(Clone,Copy,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="snake_case")]
pub enum SnowSurfaceKind {Roof,Branch,Rock}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnowSurfacePatch {
    pub id:String,
    pub kind:SnowSurfaceKind,
    pub position:[f32;3],
    /// Horizontal m² of surface catching snow.
    pub area:f32,
    /// Inclination in degrees; 0 is horizontal.
    pub slope_degrees:f32,
    pub catch_fraction:f32,
    /// Stable point on the snow field below the drip/slip edge.
    pub discharge:[f32;2],
}
impl SnowSurfacePatch {
    pub fn validate(&self)->Result<(),String>{
        if self.id.trim().is_empty()||self.id.len()>128
           || !self.position.iter().chain(self.discharge.iter()).all(|v|v.is_finite())
           || !self.area.is_finite()||!(0.025..=250.0).contains(&self.area)
           || !self.slope_degrees.is_finite()||!(0.0..=89.0).contains(&self.slope_degrees)
           || !self.catch_fraction.is_finite()||!(0.0..=1.0).contains(&self.catch_fraction){
            return Err("invalid snow surface patch".into())
        }
        Ok(())
    }
}
#[derive(Clone,Copy,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnowSurfacePolicy {
    pub maximum_depth:f32,
    pub melt_metres_per_second:f32,
    pub drainage_fraction_per_second:f32,
    pub slide_fraction_per_second:f32,
    pub wind_loss:f32,
}
impl SnowSurfacePolicy {
    pub fn validate(&self)->Result<(),String>{
        if !self.maximum_depth.is_finite()||!(0.05..=3.0).contains(&self.maximum_depth)
           || !self.melt_metres_per_second.is_finite()
           || !(0.0..=0.05).contains(&self.melt_metres_per_second)
           || !self.drainage_fraction_per_second.is_finite()
           || !(0.0..=0.2).contains(&self.drainage_fraction_per_second)
           || !self.slide_fraction_per_second.is_finite()
           || !(0.0..=0.25).contains(&self.slide_fraction_per_second)
           || !self.wind_loss.is_finite()||!(0.0..=5.0).contains(&self.wind_loss){
            return Err("invalid snow surface policy".into())
        }
        Ok(())
    }
}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
pub struct SnowSurfaceState {
    /// Conserved snow-volume thickness in metres, using the ground field's
    /// volume proxy. Physical density/porosity is visualised separately.
    pub snow:f32,
    pub water:f32,
    pub ice:f32,
}
impl Default for SnowSurfaceState {
    fn default()->Self{Self{snow:0.0,water:0.0,ice:0.0}}
}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnowSurfaceEntry {
    pub patch:SnowSurfacePatch,
    pub state:SnowSurfaceState,
}
#[derive(Clone,Copy,Debug,PartialEq)]
pub struct SnowSurfaceRunoff {
    pub origin:[f32;3],
    pub discharge:[f32;2],
    /// Volume in m³, not metres of grid depth.
    pub snow_volume:f32,
    pub liquid_volume:f32,
    pub ice_volume:f32,
}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnowSurfaceSystem {
    pub policy:SnowSurfacePolicy,
    pub entries:Vec<SnowSurfaceEntry>,
}
impl SnowSurfaceSystem {
    pub fn new(policy:SnowSurfacePolicy,patches:Vec<SnowSurfacePatch>)
        ->Result<Self,String>{
        policy.validate()?;
        if patches.len()>512{return Err("too many snow surface patches".into());}
        let mut ids=BTreeSet::new();
        for patch in &patches{
            patch.validate()?;
            if !ids.insert(patch.id.as_str()){return Err("duplicate snow surface patch".into());}
        }
        Ok(Self{policy,entries:patches.into_iter().map(|patch|SnowSurfaceEntry{
            patch,state:SnowSurfaceState::default()}).collect()})
    }
    /// Preserve volumes when roofs change area. Removed or over-cap deposits
    /// fall to their old discharge point instead of being copied or erased.
    pub fn reconcile(&mut self,policy:SnowSurfacePolicy,patches:Vec<SnowSurfacePatch>)
        ->Result<Vec<SnowSurfaceRunoff>,String>{
        self.validate()?;
        let mut next=Self::new(policy,patches)?;
        let mut old:BTreeMap<&str,&SnowSurfaceEntry>=self.entries.iter()
            .map(|entry|(entry.patch.id.as_str(),entry)).collect();
        let mut out=Vec::new();
        for entry in &mut next.entries{
            if let Some(previous)=old.remove(entry.patch.id.as_str()){
                let scale=previous.patch.area as f64/entry.patch.area as f64;
                let values=[previous.state.snow,previous.state.water,previous.state.ice];
                let mut retained=[0.0;3];let mut shed=[0.0;3];
                for k in 0..3 {
                    let volume=values[k] as f64*previous.patch.area as f64;
                    retained[k]=(values[k] as f64*scale).min(policy.maximum_depth as f64) as f32;
                    // Rounding a resized thickness upward must not create mass.
                    if retained[k] as f64*entry.patch.area as f64>volume{
                        retained[k]=f32::from_bits(retained[k].to_bits()-1);
                    }
                    shed[k]=(volume-retained[k] as f64*entry.patch.area as f64).max(0.0) as f32;
                }
                entry.state=SnowSurfaceState{snow:retained[0],water:retained[1],ice:retained[2]};
                if shed.iter().any(|v|*v>0.0){out.push(SnowSurfaceRunoff{
                    origin:previous.patch.position,discharge:previous.patch.discharge,
                    snow_volume:shed[0],liquid_volume:shed[1],ice_volume:shed[2]});}
            }
        }
        for previous in old.values(){
            let p=&previous.patch;let s=&previous.state;
            if s.snow>0.0||s.water>0.0||s.ice>0.0{out.push(SnowSurfaceRunoff{
                origin:p.position,discharge:p.discharge,snow_volume:s.snow*p.area,
                liquid_volume:s.water*p.area,ice_volume:s.ice*p.area});}
        }
        next.validate()?;*self=next;Ok(out)
    }
    pub fn stored_volume_m3(&self)->f64{
        self.entries.iter().map(|e|e.patch.area as f64*
            (e.state.snow as f64+e.state.water as f64+e.state.ice as f64)).sum()
    }
    pub fn validate(&self)->Result<(),String>{
        self.policy.validate()?;
        if self.entries.len()>512{return Err("too many saved snow surfaces".into());}
        let mut ids=BTreeSet::new();
        for e in &self.entries{
            e.patch.validate()?;
            if !ids.insert(e.patch.id.as_str()) ||
                [e.state.snow,e.state.water,e.state.ice].iter().any(|v|
                  !v.is_finite()||*v<0.0||*v>self.policy.maximum_depth+0.0001){
                return Err("invalid saved snow surface".into())
            }
        }
        Ok(())
    }
    pub fn advance(&mut self,dt:f64,snowfall:f32,environment:&ThermalEnvironment,
        heat_sources:&[HeatSource])->Result<Vec<SnowSurfaceRunoff>,String>{
        self.advance_caught(dt,snowfall,environment,heat_sources,None)
    }
    /// Per-patch catch fractions come from the same precipitation plan as
    /// ground exposure. No intercepted flake is counted a second time below.
    pub fn advance_caught(&mut self,dt:f64,snowfall:f32,environment:&ThermalEnvironment,
        heat_sources:&[HeatSource],catch:Option<&[f32]>)->Result<Vec<SnowSurfaceRunoff>,String>{
        crate::valid_dt(dt)?;
        self.validate()?;environment.validate()?;
        if dt>60.0||!snowfall.is_finite()||!(0.0..=0.05).contains(&snowfall)||
            catch.is_some_and(|v|v.len()!=self.entries.len()||
                v.iter().any(|x|!x.is_finite()||!(0.0..=1.0).contains(x))){
            return Err("invalid surface snow precipitation or catchment".into());
        }
        for source in heat_sources{source.validate()?;}
        let mut out=Vec::new();let mut remaining=dt;
        while remaining>0.0{
            let elapsed=remaining.min(30.0) as f32;remaining-=elapsed as f64;
            out.extend(self.advance_step(elapsed,snowfall,environment,heat_sources,catch));
        }
        Ok(out)
    }
    fn advance_step(&mut self,elapsed:f32,snowfall:f32,environment:&ThermalEnvironment,
        heat_sources:&[HeatSource],catch:Option<&[f32]>)->Vec<SnowSurfaceRunoff>{
        let mut out=Vec::new();
        for (k,e) in self.entries.iter_mut().enumerate(){
            let p=&e.patch;
            let s=&mut e.state;
            let mut loss=SnowSurfaceRunoff{origin:p.position,discharge:p.discharge,
                snow_volume:0.0,liquid_volume:0.0,ice_volume:0.0};
            // Area is already horizontal: do not project it a second time.
            let added=snowfall*elapsed*catch.map_or(p.catch_fraction,|v|v[k]);
            let capacity=(self.policy.maximum_depth-s.snow).max(0.0);
            s.snow+=added.min(capacity);
            loss.snow_volume+=(added-capacity).max(0.0)*p.area;
            // Local heater contribution is distance-dependent and nonstacking.
            let mut heat=0.0_f32;
            for source in heat_sources{
                if !source.enabled{continue;}
                let x=(p.position[0]-source.position[0])/source.radius;
                let z=(p.position[2]-source.position[2])/source.radius;
                let y=(p.position[1]-source.position[1])/source.radius;
                let dist=x*x+y*y+z*z;
                if dist<1.0{heat=heat.max(source.delta_temperature*(1.0-dist).powi(2));}
            }
            let temp=environment.temperature-
                environment.wind*self.policy.wind_loss+heat;
            if temp>0.0 {
                let melted=(self.policy.melt_metres_per_second*elapsed*
                    (temp/15.0).clamp(0.0,1.0)).min(s.snow);
                s.snow-=melted;
                let room=(self.policy.maximum_depth-s.water).max(0.0);
                s.water+=melted.min(room);
                loss.liquid_volume+=(melted-room).max(0.0)*p.area;
                let thaw=(self.policy.melt_metres_per_second*elapsed*
                    (temp/12.0).clamp(0.0,1.0)).min(s.ice);
                let accepted=thaw.min((self.policy.maximum_depth-s.water).max(0.0));
                s.ice-=thaw;
                s.water+=accepted;
                loss.liquid_volume+=(thaw-accepted)*p.area;
            }else if temp< -2.0{
                let frozen=(((-temp)/30.0).min(1.0)*0.006*elapsed).min(s.water)
                    .min((self.policy.maximum_depth-s.ice).max(0.0));
                s.water-=frozen;
                s.ice+=frozen;
            }
            // Ice bonds material in place. Wet, steep, warm surfaces shed
            // accumulated snow; branch loads can collapse during gusts.
            let slope=p.slope_degrees/89.0;
            let wet=(s.water/(s.snow+s.water+0.0001)).clamp(0.0,1.0);
            let steep=(slope*1.4+wet*0.6-
                (s.ice/(s.snow+s.ice+0.001))*0.8).max(0.0);
            let gust=if p.kind==SnowSurfaceKind::Branch {
                (environment.wind/32.0).clamp(0.0,0.55)
            }else{0.0};
            let release=(self.policy.slide_fraction_per_second*elapsed*
                (steep+gust-0.5).max(0.0)).clamp(0.0,0.9);
            let slide=s.snow*release;
            s.snow-=slide;
            loss.snow_volume+=slide*p.area;
            let drain=s.water*(self.policy.drainage_fraction_per_second*elapsed*
                (0.35+0.65*slope)).clamp(0.0,1.0);
            s.water-=drain;
            loss.liquid_volume+=drain*p.area;
            if loss.snow_volume>0.0||loss.liquid_volume>0.0{
                out.push(loss);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy()->SnowSurfacePolicy{SnowSurfacePolicy{maximum_depth:1.5,
       melt_metres_per_second:0.01,drainage_fraction_per_second:0.02,
       slide_fraction_per_second:0.08,wind_loss:0.0}}
    fn patch(kind:SnowSurfaceKind)->SnowSurfacePatch{
        SnowSurfacePatch{id:"roof-1".into(),kind,
            position:[0.0,3.0,0.0],area:12.0,slope_degrees:45.0,
            catch_fraction:1.0,discharge:[2.0,0.0]}
    }
    fn env(temp:f32,wind:f32)->ThermalEnvironment{
        ThermalEnvironment{temperature:temp,wind,wind_loss:0.0}
    }
    #[test]
    fn accumulation_is_persistent_and_conserves_sliding_snow(){
        let mut s=SnowSurfaceSystem::new(policy(),vec![patch(SnowSurfaceKind::Roof)]).unwrap();
        s.advance(10.0,0.01,&env(-20.0,0.0),&[]).unwrap();
        let old=s.entries[0].state.snow;
        assert!(old>0.015);
        let mass_before=old*12.0;
        let result=s.advance(15.0,0.0,&env(-20.0,12.0),&[]).unwrap();
        let mass_after=s.entries[0].state.snow*12.0+
            result.iter().map(|x|x.snow_volume).sum::<f32>();
        assert!((mass_before-mass_after).abs()<1e-4);
        let saved=serde_json::to_string(&s).unwrap();
        let reopened:SnowSurfaceSystem=serde_json::from_str(&saved).unwrap();
        assert_eq!(reopened.entries[0].state,s.entries[0].state);
    }
    #[test]
    fn warming_roof_produces_runoff_and_refreeze_holds_ice(){
        let mut s=SnowSurfaceSystem::new(policy(),vec![patch(SnowSurfaceKind::Roof)]).unwrap();
        s.advance(25.0,0.03,&env(-20.0,0.0),&[]).unwrap();
        let heater=HeatSource{id:"heat".into(),enabled:true,position:[0.0,3.0,0.0],
            radius:9.0,delta_temperature:60.0};
        let run=s.advance(25.0,0.0,&env(-20.0,0.0),&[heater]).unwrap();
        assert!(run.iter().any(|r|r.liquid_volume>0.0));
        assert!(s.entries[0].state.snow<0.6);
        let _=s.advance(25.0,0.0,&env(-20.0,0.0),&[]).unwrap();
        assert!(s.entries[0].state.ice>0.0);
    }
    #[test]
    fn loading_new_building_preserves_existing_accumulation(){
        let mut s=SnowSurfaceSystem::new(policy(),vec![patch(SnowSurfaceKind::Branch)]).unwrap();
        s.advance(20.0,0.01,&env(-22.0,1.0),&[]).unwrap();
        let old=s.entries[0].state.clone();
        let mut p=patch(SnowSurfaceKind::Roof);
        p.id="hut-2".into();
        s.reconcile(policy(),vec![patch(SnowSurfaceKind::Branch),p]).unwrap();
        assert_eq!(s.entries[0].state,old);
        assert_eq!(s.entries.len(),2);
    }
}
