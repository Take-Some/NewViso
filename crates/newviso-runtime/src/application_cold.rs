//! Engine snow/thermal services. No project names, illness rules or Frostpunk tuning.
use super::*;
mod weather;
mod expansion;
use weather::SnowMassBalanceReport;
use newviso_environment::{
    snow::{SnowField, SnowFieldSpec, SnowPolicy, SnowHeatPolicy, SnowMicroprint,
        SnowDriftPolicy, SnowWindObstacle, SnowDynamicsPolicy, SnowDynamicsReport, SnowLayerPolicy, SnowPrecipitationPlan},
    thermal::{HeatSource, ThermalBody, ThermalEnvironment},
    surface_snow::{SnowSurfaceSystem,SnowSurfacePatch,SnowSurfacePolicy},
};
use serde::{Deserialize, Serialize};
use newviso_scene::{SceneSnowCap,SceneParticleSpawnDesc,SceneParticleBlend};

fn cover_descriptors(surface:&SnowSurfaceSystem)->Vec<SceneSnowCap>{
    surface.entries.iter().map(|entry|SceneSnowCap{
        id:entry.patch.id.clone(),
        kind:match entry.patch.kind{
            newviso_environment::surface_snow::SnowSurfaceKind::Roof=>"roof",
            newviso_environment::surface_snow::SnowSurfaceKind::Branch=>"branch",
            newviso_environment::surface_snow::SnowSurfaceKind::Rock=>"rock",
        }.into(),
        position:entry.patch.position,
        area:entry.patch.area,
        slope_degrees:entry.patch.slope_degrees,
        snow_depth:entry.state.snow,
    }).collect()
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SnowActor {
    entity: u64,
    feet_offset: f32,
    contact_radius: f32,
    contact_spacing: f32,
    contact_strength: f32,
    max_contact_gap: f32,
    max_travel: f32,
    #[serde(default)]
    microprint_enabled: bool,
    #[serde(skip)]
    step_index: u32,
    #[serde(skip)]
    previous: Option<[f32; 3]>,
    #[serde(skip)]
    distance: f32,
}
impl SnowActor {
    fn validate(&self) -> Result<(), String> {
        if self.entity == 0
            || ![
                self.feet_offset,
                self.contact_radius,
                self.contact_spacing,
                self.contact_strength,
                self.max_contact_gap,
                self.max_travel,
            ]
            .iter()
            .all(|v| v.is_finite())
            || self.feet_offset < 0.0
            || self.contact_radius <= 0.0
            || self.contact_spacing <= 0.0
            || !(0.0..=1.0).contains(&self.contact_strength)
            || self.max_contact_gap <= 0.0
            || self.max_travel <= self.contact_spacing
        {
            return Err("invalid snow actor binding".into());
        }
        Ok(())
    }
}
#[derive(Default, Serialize)]
struct ColdPerformance {
    last_weather_ms: f64,
    last_dynamics_ms: f64,
    last_surface_sync_ms: f64,
    last_geometry_ms: f64,
    last_collision_sync_ms: f64,
    last_collision_patches: usize,
    last_snow_vertices: usize,
    last_liquid_vertices: usize,
    sync_count: u64,
    weather_count: u64,
}
#[derive(Default, Serialize, Deserialize)]
pub(super) struct ColdRuntime {
    field: Option<SnowField>,
    field_asset: Option<String>,
    field_digest: Option<String>,
    actors: BTreeMap<u64, SnowActor>,
    bodies: BTreeMap<String, ThermalBody>,
    sources: BTreeMap<String, HeatSource>,
    environment: Option<ThermalEnvironment>,
    #[serde(default)]
    samples: BTreeMap<u64, Value>,
    sync_interval: f32,
    #[serde(default)]
    heat_policy: Option<SnowHeatPolicy>,
    #[serde(default)]
    drift_policy: Option<SnowDriftPolicy>,
    #[serde(default)]
    snow_surfaces: Option<SnowSurfaceSystem>,
    #[serde(default)]
    dynamics_policy: Option<SnowDynamicsPolicy>,
    #[serde(skip)]
    dynamics_report: SnowDynamicsReport,
    #[serde(skip)]
    precipitation_plan: Option<SnowPrecipitationPlan>,
    #[serde(skip)]
    snow_budget: SnowMassBalanceReport,
    #[serde(skip)]
    performance: ColdPerformance,
    #[serde(skip)]
    wind_obstacles: Vec<SnowWindObstacle>,
    #[serde(skip)]
    shelter_direction: Option<[f32; 2]>,
    #[serde(skip)]
    wind_shelter: Vec<f32>,
    #[serde(skip)]
    heat_elapsed: f32,
    #[serde(skip)]
    strata_elapsed: f32,
    #[serde(skip)]
    sync_elapsed: f32,
    #[serde(skip)]
    pending: Vec<[f32; 6]>,
    #[serde(skip)]
    pending_liquid: Vec<[f32; 5]>,
    #[serde(skip)]
    micro_dirty: bool,
}
impl ColdRuntime {
    pub(super) fn restore(value: &Value) -> Result<Self, String> {
        if value.is_null() {
            return Ok(Self::default());
        }
        if value["schema"] != "newviso.cold.save.v1" {
            return Err("cold save schema mismatch".into());
        }
        let mut result: Self = serde_json::from_value(value["state"].clone())
            .map_err(|e| format!("cold save: {e}"))?;
        if let Some(f) = &result.field {
            f.validate()?;
            result.pending = f.all_render_vertices();
            result.pending_liquid=f.all_liquid_vertices();
        }
        for (key, a) in &result.actors {
            a.validate()?;
            if *key != a.entity {
                return Err("saved snow actor identity mismatch".into());
            }
        }
        for (key, b) in &result.bodies {
            b.validate()?;
            if key != &b.id {
                return Err("saved thermal identity mismatch".into());
            }
        }
        for (key, s) in &result.sources {
            s.validate()?;
            if key != &s.id {
                return Err("saved heat source identity mismatch".into());
            }
        }
        if let Some(e) = &result.environment {
            e.validate()?;
        }
        if result.field.is_some()
            && (!result.sync_interval.is_finite() || result.sync_interval <= 0.0)
        {
            return Err("invalid cold sync interval".into());
        }
        if let Some(surfaces)=&result.snow_surfaces{surfaces.validate()?;}
        if let Some(policy)=result.heat_policy {policy.validate()?;}
        if let Some(policy)=result.drift_policy {policy.validate()?;}
        // The project reconfigures source obstruction after startup. A saved
        // cache is deliberately not trusted across maps or geometry versions.
        if let Some(policy)=result.dynamics_policy {policy.validate()?;}
        result.dynamics_policy=None;
        result.drift_policy=None;
        result.wind_shelter.clear();
        result.heat_elapsed = 0.0;
        result.strata_elapsed = 0.0;
        result.sync_elapsed = result.sync_interval;
        result.samples.clear();
        result.micro_dirty=true; // replay persisted footprints on reload
        Ok(result)
    }
    fn cold_wind_direction(&self) -> Option<[f32; 2]> {
        self.drift_policy.map(|p|p.wind_direction)
    }
    pub(super) fn checkpoint(&self) -> Value {
        json!({"schema":"newviso.cold.save.v1","state":self})
    }
    pub(super) fn state(&self) -> Value {
        json!({"schema":"newviso.cold.runtime.v1","enabled":self.field.is_some()||self.environment.is_some(),
            "field":self.field.as_ref().map(|f|json!({"id":f.spec.id,"seed":f.spec.seed,"cell_size":f.spec.cell_size,
                "columns":f.spec.columns,"rows":f.spec.rows,"layered":f.layers_enabled()})),"samples":self.samples.values().collect::<Vec<_>>(),
            "environment":self.environment,"bodies":self.bodies.values().collect::<Vec<_>>(),
            "sources":self.sources.values().collect::<Vec<_>>(),
            "snow_dynamics":self.dynamics_report,
            "snow_budget":self.snow_budget,
            "performance":self.performance,
            "wind_direction":self.cold_wind_direction(),
            "surface_snow":self.snow_surfaces.as_ref().map(|surface|json!({
                "count":surface.entries.len(),
                "snow_volume_m3":surface.entries.iter().map(|e|e.patch.area*e.state.snow).sum::<f32>(),
                "water_volume_m3":surface.entries.iter().map(|e|e.patch.area*e.state.water).sum::<f32>(),
                "ice_volume_m3":surface.entries.iter().map(|e|e.patch.area*e.state.ice).sum::<f32>()
            })),"pending_vertices":self.pending.len()+self.field.as_ref().map_or(0,|f|f.pending_visual_cells())})
    }
}

fn cold_string<'a>(c: &'a Value, key: &str, index: usize) -> Result<&'a str, String> {
    c.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("script command[{index}] requires string '{key}'"))
}
fn decode<T: serde::de::DeserializeOwned>(c: &Value, key: &str) -> Result<T, String> {
    serde_json::from_value(
        c.get(key)
            .ok_or_else(|| format!("cold command requires '{key}'"))?
            .clone(),
    )
    .map_err(|e| e.to_string())
}

// Measure upward separation from the terrain, not upward world velocity.
// Walking uphill must remain a snow contact; jumping away from it must not.
// Contact spacing decides when a foot lands. Per-frame displacement is used
// only to obtain its direction, never as a frame-rate-dependent step threshold.
fn snow_step_direction(previous:[f32;3],current:[f32;3],travel:f32)->Option<[f32;2]>{
    if !travel.is_finite()||travel<=f32::EPSILON{return None;}
    Some([(current[0]-previous[0])/travel,(current[2]-previous[2])/travel])
}

fn follows_snow_surface(
    feet: f32,
    height: f32,
    gap: f32,
    vertical_velocity: f32,
    previous_height: Option<f32>,
    dt: f32,
) -> bool {
    let surface_velocity = previous_height
        .filter(|_| dt > 0.0)
        .map(|previous| (height - previous) / dt)
        .unwrap_or(0.0);
    (feet - height).abs() <= gap && vertical_velocity - surface_velocity <= 0.25
}
impl EngineApplication {
    pub(super) fn apply_cold_command(
        &mut self,
        c: &Value,
        index: usize,
        op: &str,
    ) -> Result<(), String> {
        match op {
            "snow.field.extend" => self.extend_snow_terrain(c)?,
            "snow.field.load" => {
                let asset = cold_string(c, "asset", index)?;
                let value = AssetClient::new().json(asset)?;
                let spec: SnowFieldSpec = serde_json::from_value(value.clone())
                    .map_err(|e| format!("snow asset: {e}"))?;
                spec.validate()?;
                let digest = blake3::hash(
                    serde_json::to_string(&value)
                        .map_err(|e| e.to_string())?
                        .as_bytes(),
                )
                .to_hex()
                .to_string();
                let policy: SnowPolicy = decode(c, "policy")?;
                policy.validate()?;
                let interval = command_number(c, "sync_interval", index)?;
                if !(0.01..=10.0).contains(&interval) {
                    return Err("snow sync interval must be in 0.01..=10".into());
                }
                if self.cold.field.is_some() && self.cold.field_digest.as_ref() != Some(&digest) {
                    return Err("saved snow terrain differs from regenerated map; use a new save for a different seed/config".into());
                }
                if self.cold.field.is_none() {
                    self.cold.field = Some(SnowField::new(spec, policy.clone())?);
                }
                if let Some(f) = &mut self.cold.field {
                    f.policy = policy;
                    self.cold.pending = f.all_render_vertices();
                    self.cold.pending_liquid=f.all_liquid_vertices();
                    self.cold.micro_dirty = true;
                }
                self.cold.field_asset = Some(asset.into());
                self.cold.field_digest = Some(digest);
                self.cold.sync_interval = interval;
                self.cold.precipitation_plan=None;
            }
            "snow.actor.bind" => {
                let a: SnowActor = decode(c, "actor")?;
                a.validate()?;
                if self.cold.actors.len() >= 256 && !self.cold.actors.contains_key(&a.entity) {
                    return Err("snow actor limit reached".into());
                }
                if self
                    .cold
                    .field
                    .as_ref()
                    .is_some_and(|f| a.contact_radius > f.policy.max_contact_radius)
                {
                    return Err("snow actor radius exceeds policy".into());
                }
                self.cold.actors.insert(a.entity, a);
            }
            "snow.actor.unbind" => {
                let e = command_u64(c, "entity", index)?;
                self.cold.actors.remove(&e);
                self.cold.samples.remove(&e);
            }
            "snow.contact" => {
                let p = command_vec3(c, "position", index)?;
                let radius = command_number(c, "radius", index)?;
                let strength = command_number(c, "strength", index)?;
                self.cold
                    .field
                    .as_mut()
                    .ok_or("snow field not loaded")?
                    .contact(p[0], p[2], radius, strength)?;
            }
            "snow.layers.configure" => {
                let policy:SnowLayerPolicy=decode(c,"policy")?;
                self.cold.field.as_mut().ok_or("snow field not loaded")?.configure_layers(policy)?;
            }
            "snow.heat.configure" => {
                let policy: SnowHeatPolicy = decode(c, "policy")?;
                policy.validate()?;
                self.cold.heat_policy=Some(policy);
                self.cold.heat_elapsed=0.0;
            }
            "snow.drift.configure" => {
                let policy: SnowDriftPolicy=decode(c,"policy")?;
                let obstacles:Vec<SnowWindObstacle>=decode(c,"obstacles")?;
                let field=self.cold.field.as_ref().ok_or("snow field not loaded")?;
                let shelter=field.wind_shelter_map(policy,&obstacles)?;
                self.cold.wind_shelter=shelter;
                self.cold.wind_obstacles=obstacles;
                self.cold.shelter_direction=Some(policy.wind_direction);
                self.cold.drift_policy=Some(policy);
            }
            "snow.dynamics.configure" => {
                let policy:SnowDynamicsPolicy=decode(c,"policy")?;
                policy.validate()?;
                self.cold.dynamics_policy=Some(policy);
            }
            "snow.surfaces.configure" => {
                let policy:SnowSurfacePolicy=decode(c,"policy")?;
                let patches:Vec<SnowSurfacePatch>=decode(c,"patches")?;
                self.configure_surface_snow(policy,patches)?;
            }
            "snow.weather.advance" => self.apply_snow_weather_command(c,index)?,
            "thermal.environment.set" => {
                let e: ThermalEnvironment = decode(c, "environment")?;
                e.validate()?;
                self.cold.environment = Some(e);
            }
            "thermal.source.upsert" => {
                let s: HeatSource = decode(c, "source")?;
                s.validate()?;
                if self.cold.sources.len() >= 256 && !self.cold.sources.contains_key(&s.id) {
                    return Err("heat source limit reached".into());
                }
                self.cold.sources.insert(s.id.clone(), s);
            }
            "thermal.source.remove" => {
                self.cold.sources.remove(cold_string(c, "id", index)?);
            }
            "thermal.body.upsert" => {
                let mut b: ThermalBody = decode(c, "body")?;
                b.validate()?;
                if self.cold.bodies.len() >= 256 && !self.cold.bodies.contains_key(&b.id) {
                    return Err("thermal body limit reached".into());
                }
                if c.get("preserve_temperature").and_then(Value::as_bool) == Some(true) {
                    if let Some(old) = self.cold.bodies.get(&b.id) {
                        b.temperature = old.temperature;
                        if b.entity != 0 && b.entity == old.entity { b.position = old.position; }
                    }
                }
                if b.entity != 0 {
                    if let Some(pose) = self.physics.as_ref().and_then(|physics| physics.pickup_body_pose(b.entity)) {
                        b.position = pose.position;
                    }
                }
                self.cold.bodies.insert(b.id.clone(), b);
            }
            "thermal.body.remove" => {
                self.cold.bodies.remove(cold_string(c, "id", index)?);
            }
            _ => return Err(format!("unsupported cold command {op} at {index}")),
        }
        Ok(())
    }
    pub(super) fn tick_cold(&mut self, dt: f32) -> Result<(), String> {
        if self.cold.field.is_none() && self.cold.environment.is_none() {
            return Ok(());
        }
        let ids = self
            .cold
            .actors
            .keys()
            .copied()
            .chain(self.cold.bodies.values().map(|b| b.entity))
            .collect::<BTreeSet<_>>();
        let positions = self
            .physics
            .as_ref()
            .map(|p| {
                ids.iter()
                    .filter_map(|id| p.pickup_body_pose(*id).map(|pose| (*id, pose.position)))
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();
        // Heat-based thaw is evaluated at project-authored cadence, not 120Hz.
        if let (Some(policy),Some(env),Some(field)) =
            (self.cold.heat_policy,self.cold.environment.as_ref(),self.cold.field.as_mut()){
            self.cold.heat_elapsed+=dt;
            if self.cold.heat_elapsed>=policy.active_interval {
                let elapsed=self.cold.heat_elapsed.min(5.0);
                self.cold.heat_elapsed=0.0;
                let sources=self.cold.sources.values().cloned().collect::<Vec<_>>();
                let before=field.microprints().len();
                field.advance_heat(elapsed as f64,env,&sources,policy)?;
                if field.microprints().len()!=before {self.cold.micro_dirty=true;}
            }
        }
        // Run strata aging well below frame rate. Only cells whose material
        // composition changes enter the normal dirty render queue.
        self.cold.strata_elapsed += dt;
        if self.cold.strata_elapsed >= 5.0 {
            let elapsed = self.cold.strata_elapsed.min(30.0);
            self.cold.strata_elapsed = 0.0;
            if let (Some(field),Some(env)) =
                (self.cold.field.as_mut(),self.cold.environment.as_ref()){
                if let Some(heat)=self.cold.heat_policy {
                    let sources=self.cold.sources.values().cloned().collect::<Vec<_>>();
                    field.weather_strata_with_heat(elapsed as f64,env,&sources,heat)?;
                }else{
                    field.weather_strata(elapsed as f64,env.temperature)?;
                }
            }
        }
        if let Some(field) = &mut self.cold.field {
            for a in self.cold.actors.values_mut() {
                let Some(p) = positions.get(&a.entity).copied() else {
                    a.previous = None;
                    continue;
                };
                let feet = p[1] - a.feet_offset;
                let Some(sample) = field.sample(p[0], p[2]) else {
                    self.cold.samples.remove(&a.entity);
                    a.previous = Some(p);
                    continue;
                };
                let previous_height = a
                    .previous
                    .and_then(|old| field.sample(old[0], old[2]))
                    .map(|s| s.surface_height);
                let on_snow = self.physics.as_ref().is_some_and(|physics| {
                    follows_snow_surface(
                        feet,
                        sample.surface_height,
                        a.max_contact_gap,
                        physics.body_linear_velocity(a.entity)[1],
                        previous_height,
                        dt,
                    )
                });
                if on_snow {
                    if let Some(old) = a.previous {
                        let travel = ((p[0] - old[0]).powi(2) + (p[2] - old[2]).powi(2)).sqrt();
                        if travel <= a.max_travel {
                            a.distance += travel;
                            if a.distance >= a.contact_spacing {
                                // Stamp the travelled segment: fast movement must not leave disconnected dots.
                                let count = (travel / a.contact_spacing).ceil().max(1.0) as usize;
                                for step in 1..=count {
                                    let t = step as f32 / count as f32;
                                    let x=old[0]+(p[0]-old[0])*t;
                                    let z=old[2]+(p[2]-old[2])*t;
                                    field.contact(x,z,a.contact_radius,a.contact_strength)?;
                                    if let Some(forward)=a.microprint_enabled
                                        .then(||snow_step_direction(old,p,travel)).flatten() {
                                        let side=if a.step_index%2==0{1.0}else{-1.0};
                                        let print=SnowMicroprint{
                                            position:[x-forward[1]*side*0.145,z+forward[0]*side*0.145],
                                            forward,
                                            half_width:0.112,
                                            half_length:0.18,
                                            depression:0.075*a.contact_strength,
                                        };
                                        if field.stamp_microprint(print)? {
                                            self.cold.micro_dirty=true;
                                        }
                                        a.step_index=a.step_index.wrapping_add(1);
                                    }
                                }
                                a.distance = 0.0;
                            }
                        } else {
                            a.distance = 0.0;
                        }
                    }
                } else {
                    a.distance = 0.0;
                }
                a.previous = Some(p);
                let sample = field.sample(p[0], p[2]).unwrap();
                self.cold.samples.insert(a.entity,json!({"entity":a.entity,"position":p,"on_snow":on_snow,
                    "surface_height":sample.surface_height,"depth":sample.depth,"loose_depth":sample.loose_depth,"compaction":sample.compaction,
                    "powder":sample.powder,"crust":sample.crust,"moisture":sample.moisture,
                    "slush":sample.slush_fraction,"standing_water":sample.standing_water_depth,
                    "ground_ice":sample.ground_ice_depth,
                    "layers":field.layer_profile(p[0],p[2])}));
            }
            // Keep dirty membership in the field until the sync boundary.
            // Repeated footsteps then send each vertex only once.
        }
        if let Some(e) = &self.cold.environment {
            for b in self.cold.bodies.values_mut() {
                if b.entity != 0 {
                    if let Some(p) = positions.get(&b.entity) {
                        b.position = *p;
                    } else {
                        continue;
                    }
                }
                // Non-stacking zones are a generic service; additive/insulated-building rules live in scripts.
                let heat = self
                    .cold
                    .sources
                    .values()
                    .map(|s| s.contribution(b.position))
                    .fold(0.0, f32::max);
                b.advance(dt as f64, e, heat)?;
            }
        }
        self.cold.sync_elapsed += dt;
        if self.cold.sync_elapsed >= self.cold.sync_interval
            && (!self.cold.pending.is_empty()||
                !self.cold.pending_liquid.is_empty()||self.cold.micro_dirty||
                self.cold.field.as_ref().is_some_and(|f|f.has_visual_changes())) {
            let sync_start=std::time::Instant::now();
            self.cold.sync_elapsed = 0.0;
            if let Some(field)=self.cold.field.as_mut() {
                let (snow,liquid)=field.take_visual_changes();
                self.cold.pending.extend(snow);
                self.cold.pending_liquid.extend(liquid);
            }
            let geometry_start=std::time::Instant::now();
            let pending = std::mem::take(&mut self.cold.pending);
            self.cold.performance.last_snow_vertices=pending.len();
            let collisions = self.scene.deform_heightfields(&pending)?;
            self.cold.performance.last_collision_patches=collisions.len();
            let liquid=std::mem::take(&mut self.cold.pending_liquid);
            self.cold.performance.last_liquid_vertices=liquid.len();
            if !liquid.is_empty(){
                self.scene.update_ground_liquids(&liquid)?;
            }
            if self.cold.micro_dirty {
                let prints=self.cold.field.as_ref().map(|field|{
                    field.microprints().iter().map(|p|[
                        p.position[0],p.position[1],p.forward[0],p.forward[1],
                        p.half_width,p.half_length,p.depression
                    ]).collect::<Vec<_>>()
                }).unwrap_or_default();
                self.scene.update_snow_microprints(&prints)?;
                self.cold.micro_dirty=false;
            }
            self.cold.performance.last_geometry_ms=geometry_start.elapsed().as_secs_f64()*1000.0;
            let collision_start=std::time::Instant::now();
            if let Some(physics) = &mut self.physics {
                for (entity, vertices, triangles) in collisions {
                    let surface=self.scene.heightfield_surface_id(entity).unwrap_or(0);
                    physics.update_streamed_heightfield(entity,vertices,triangles,surface)?;
                }
            }
            self.cold.performance.last_collision_sync_ms=collision_start.elapsed().as_secs_f64()*1000.0;
            self.cold.performance.last_surface_sync_ms=sync_start.elapsed().as_secs_f64()*1000.0;
            self.cold.performance.sync_count+=1;
        }
        Ok(())
    }
}

#[cfg(test)]
mod contact_tests {
    use super::*;
    #[test]
    fn uphill_walking_compacts_snow_but_jumps_and_roofs_do_not() {
        assert!(follows_snow_surface(
            1.02,
            1.0,
            0.08,
            0.8,
            Some(0.98),
            0.025
        ));
        assert!(!follows_snow_surface(
            1.02,
            1.0,
            0.08,
            5.4,
            Some(0.98),
            0.025
        ));
        assert!(!follows_snow_surface(1.3, 1.0, 0.08, 0.0, Some(1.0), 0.025));
        assert!(follows_snow_surface(1.02, 1.0, 0.08, 0.0, None, 0.0));
    }

    #[test]
    fn walking_step_direction_survives_high_frame_rates(){
        for fps in [24.0,60.0,120.0,240.0]{
            let distance=1.8/fps;
            assert_eq!(snow_step_direction([0.0;3],[distance,0.0,0.0],distance),Some([1.0,0.0]));
        }
        assert_eq!(snow_step_direction([0.0;3],[0.0;3],0.0),None);
    }
}
