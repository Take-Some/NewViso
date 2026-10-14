//! Authoritative snow source and transfer accounting, off the frame hot path.
use super::*;
use newviso_environment::surface_snow::SnowSurfaceRunoff;
#[derive(Default,Serialize)]
pub(super) struct SnowMassBalanceReport {
    delta_seconds:f32,
    snowfall_metres_per_second:f32,
    before_volume_m3:f64,
    after_volume_m3:f64,
    precipitation_volume_m3:f64,
    ground_precipitation_volume_m3:f64,
    surface_precipitation_volume_m3:f64,
    shed_snow_volume_m3:f64,
    outflow_volume_m3:f64,
    imbalance_volume_m3:f64,
}
impl ColdRuntime {
    fn rebuild_precipitation_plan(&mut self)->Result<(),String>{
        self.precipitation_plan=match (&self.field,&self.snow_surfaces){
            (Some(field),Some(surfaces))=>Some(SnowPrecipitationPlan::new(&field.spec,
                &surfaces.entries.iter().map(|e|e.patch.clone()).collect::<Vec<_>>())?),
            _=>None,
        };
        Ok(())
    }
    fn stored_snow_volume(&self)->f64{
        self.field.as_ref().map_or(0.0,|f|f.stored_volume_m3())+
            self.snow_surfaces.as_ref().map_or(0.0,|s|s.stored_volume_m3())
    }
}
impl EngineApplication {
    pub(super) fn configure_surface_snow(&mut self,policy:SnowSurfacePolicy,
        patches:Vec<SnowSurfacePatch>)->Result<(),String>{
        if self.cold.field.is_none()&&self.cold.snow_surfaces.as_ref()
            .is_some_and(|s|s.stored_volume_m3()>0.0){
            return Err("snow field required to receive removed surface deposits".into());
        }
        let flows=if let Some(surfaces)=&mut self.cold.snow_surfaces{
            surfaces.reconcile(policy,patches)?
        }else{
            self.cold.snow_surfaces=Some(SnowSurfaceSystem::new(policy,patches)?);Vec::new()
        };
        if !flows.is_empty(){self.transfer_surface_runoff(flows)?;}
        self.cold.rebuild_precipitation_plan()?;
        if let Some(surface)=self.cold.snow_surfaces.as_ref(){
            self.scene.sync_surface_snow_caps(&cover_descriptors(surface))?;
        }
        Ok(())
    }
    fn transfer_surface_runoff(&mut self,flows:Vec<SnowSurfaceRunoff>)->Result<f64,String>{
        let field=self.cold.field.as_mut().ok_or("snow field not loaded")?;
        let had_marks=!field.microprints().is_empty();
        let mut deposited_snow=false;
        let mut falling=Vec::new();let mut outside=0.0;
        for runoff in flows {
            let ground=field.sample(runoff.discharge[0],runoff.discharge[1])
                .map(|s|s.surface_height).unwrap_or(runoff.origin[1]);
            if runoff.snow_volume>0.003&&runoff.origin[1]-ground>0.9&&falling.len()<24{
                let mass=runoff.snow_volume.sqrt().min(0.5);
                let size=(0.08+mass*0.20).clamp(0.08,0.18);
                for i in 0..2 {
                    let side=if i==0{-1.0_f32}else{1.0_f32};
                    falling.push(SceneParticleSpawnDesc{
                        position:[runoff.origin[0]+side*size,runoff.origin[1],runoff.origin[2]],
                        velocity:[side*0.45,0.1,0.2*side],acceleration:[0.0,-9.81,0.0],
                        size:[size,size],end_size:[size*0.65,size*0.65],
                        color:[0.94,0.96,1.0,0.92],end_color:[0.94,0.96,1.0,0.0],
                        lifetime_seconds:1.2,rotation_degrees:0.0,
                        angular_velocity_degrees:side*75.0,blend:SceneParticleBlend::Alpha,style:None});
                }
            }
            let rejected=field.deposit_surface_runoff(runoff.discharge,
                [runoff.snow_volume,runoff.liquid_volume,runoff.ice_volume])?;
            deposited_snow|=runoff.snow_volume>rejected[0];
            outside+=rejected.iter().map(|v|*v as f64).sum::<f64>();
        }
        if had_marks&&deposited_snow{self.cold.micro_dirty=true;}
        if !falling.is_empty(){let _=self.scene.spawn_particles(falling);}
        Ok(outside)
    }
    pub(super) fn apply_snow_weather_command(&mut self,c:&Value,index:usize)->Result<(),String>{
        let weather_start=std::time::Instant::now();
        let dt=command_number(c,"delta_seconds",index)?;
        let snowfall=command_number(c,"snowfall_metres_per_second",index)?;
        let melt=command_number(c,"melt_metres_per_second",index)?;
        if !(0.0..=60.0).contains(&dt)||![snowfall,melt].iter()
            .all(|v|v.is_finite()&&(0.0..=0.05).contains(v)){
            return Err("snow weather delta or rates outside bounds".into());
        }
        let wind=c.get("wind_speed").map(|_|command_number(c,"wind_speed",index))
            .transpose()?.unwrap_or(self.cold.environment.as_ref().map_or(0.0,|e|e.wind));
        if !(0.0..=200.0).contains(&wind){return Err("invalid snow wind speed".into());}
        if self.cold.snow_surfaces.is_some()&&self.cold.environment.is_none(){
            return Err("surface snow requires a thermal environment".into());
        }
        if c.get("wind_direction").is_some(){
            let mut policy=self.cold.drift_policy.ok_or("snow drift not configured")?;
            policy.wind_direction=command_vector::<2>(c,"wind_direction",index)?;
            policy.validate()?;
            let norm=policy.wind_direction[0].hypot(policy.wind_direction[1]);
            policy.wind_direction=policy.wind_direction.map(|v|v/norm);
            if self.cold.shelter_direction.is_none_or(|old|
                old[0]*policy.wind_direction[0]+old[1]*policy.wind_direction[1]<0.9961947){
                self.cold.wind_shelter=self.cold.field.as_ref().ok_or("snow field not loaded")?
                    .wind_shelter_map(policy,&self.cold.wind_obstacles)?;
                self.cold.shelter_direction=Some(policy.wind_direction);
            }
            self.cold.drift_policy=Some(policy);
        }
        if self.cold.precipitation_plan.is_none()&&self.cold.snow_surfaces.is_some(){
            self.cold.rebuild_precipitation_plan()?;
        }
        let before=self.cold.stored_snow_volume();
        let field=self.cold.field.as_mut().ok_or("snow field not loaded")?;
        let had_marks=!field.microprints().is_empty();
        let ground=field.advance_exposed(dt as f64,snowfall,melt,self.cold.precipitation_plan
            .as_ref().map(|p|p.ground_exposure.as_slice()))?;
        if snowfall>0.0&&had_marks{self.cold.micro_dirty=true;}
        self.cold.dynamics_report=SnowDynamicsReport::default();
        if let Some(policy)=self.cold.drift_policy{
            if let Some(dynamics)=self.cold.dynamics_policy{
                let start=std::time::Instant::now();
                self.cold.dynamics_report=field.advance_dynamics(dt as f64,wind,policy,
                    &self.cold.wind_shelter,dynamics)?;
                self.cold.performance.last_dynamics_ms=start.elapsed().as_secs_f64()*1000.0;
                if had_marks{self.cold.micro_dirty=true;}
            }else{field.advance_wind(dt as f64,wind,policy,&self.cold.wind_shelter)?;}
        }
        let mut surface_precipitation=0.0;let mut shed=0.0;
        let mut outside=ground.outflow_volume_m3;
        if let (Some(surfaces),Some(environment))=
            (self.cold.snow_surfaces.as_mut(),self.cold.environment.as_ref()){
            let catch=self.cold.precipitation_plan.as_ref().map(|p|p.surface_catch.as_slice());
            surface_precipitation=surfaces.entries.iter().enumerate().map(|(k,e)|
                e.patch.area as f64*snowfall as f64*dt as f64*
                    catch.map_or(e.patch.catch_fraction,|v|v[k]) as f64).sum();
            let sources=self.cold.sources.values().cloned().collect::<Vec<_>>();
            let mut environment=environment.clone();environment.wind=wind;
            let flows=surfaces.advance_caught(dt as f64,snowfall,&environment,&sources,catch)?;
            shed=flows.iter().map(|r|r.snow_volume as f64).sum();
            if !flows.is_empty(){outside+=self.transfer_surface_runoff(flows)?;}
            self.scene.sync_surface_snow_caps(&cover_descriptors(self.cold.snow_surfaces.as_ref().unwrap()))?;
        }
        let after=self.cold.stored_snow_volume();
        let precipitation=ground.snowfall_volume_m3+surface_precipitation;
        self.cold.snow_budget=SnowMassBalanceReport{delta_seconds:dt,
            snowfall_metres_per_second:snowfall,before_volume_m3:before,after_volume_m3:after,
            precipitation_volume_m3:precipitation,ground_precipitation_volume_m3:ground.snowfall_volume_m3,
            surface_precipitation_volume_m3:surface_precipitation,shed_snow_volume_m3:shed,
            outflow_volume_m3:outside,imbalance_volume_m3:after+outside-before-precipitation};
        self.cold.performance.last_weather_ms=weather_start.elapsed().as_secs_f64()*1000.0;
        self.cold.performance.weather_count+=1;
        Ok(())
    }
}
