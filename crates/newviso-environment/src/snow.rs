//! Persistent deformable snow. Authored terrain and all tuning are caller data.
mod dynamics;
mod layers;
mod weather;
mod precipitation;
mod expansion;
pub use expansion::SnowTerrainProfile;
pub use weather::SnowWeatherReport;
pub use precipitation::SnowPrecipitationPlan;
pub use layers::SnowLayerPolicy;
pub use dynamics::{SnowDynamicsPolicy, SnowDynamicsReport};
use serde::{Deserialize, Serialize};
/// Sparse dirty membership in a dense bitset. Ordered extraction is O(n/64+k)
/// and insertion is O(1), with no allocation per changed snow cell.
#[derive(Clone, Debug, Default)]
struct SnowDirtyCells {
    words: Vec<u64>,
    count: usize,
}
impl SnowDirtyCells {
    fn new() -> Self {Self::default()}
    fn insert(&mut self, i:usize) {
        let word=i/64;
        if word>=self.words.len(){self.words.resize(word+1,0);}
        let bit=1_u64<<(i%64);
        if self.words[word]&bit==0 {self.count+=1;self.words[word]|=bit;}
    }
    fn is_empty(&self)->bool {self.count==0}
    fn len(&self)->usize {self.count}
    fn take_indices(&mut self)->Vec<usize> {
        let mut result=Vec::with_capacity(self.count);
        for (word_index,word) in self.words.iter_mut().enumerate() {
            let mut bits=*word;
            while bits!=0 {
                result.push(word_index*64+bits.trailing_zeros() as usize);
                bits&=bits-1;
            }
            *word=0;
        }
        self.count=0;
        result
    }
}


use crate::thermal::{HeatSource, ThermalEnvironment};
/// Project-authored surface response to native heat sources. The body temperature
/// model remains independent; heat enters the snow solver only when configured.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnowHeatPolicy {
    pub surface_heat_multiplier: f32,
    pub melt_temperature: f32,
    pub melt_metres_per_second: f32,
    pub temperature_range: f32,
    pub wind_cooling: f32,
    pub active_interval: f32,
}
impl SnowHeatPolicy {
    pub fn validate(&self) -> Result<(), String> {
        if ![
            self.surface_heat_multiplier,self.melt_temperature,
            self.melt_metres_per_second,self.temperature_range,
            self.wind_cooling,self.active_interval
        ].iter().all(|v|v.is_finite())
            || !(0.0..=30.0).contains(&self.surface_heat_multiplier)
            || !(-150.0..=100.0).contains(&self.melt_temperature)
            || !(0.0..=0.02).contains(&self.melt_metres_per_second)
            || !(0.01..=300.0).contains(&self.temperature_range)
            || !(0.0..=5.0).contains(&self.wind_cooling)
            || !(0.05..=5.0).contains(&self.active_interval) {
            return Err("invalid thermal snow policy".into());
        }
        Ok(())
    }
}


/// World-authored wind transport. Wind sweeps *loose* snow only.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnowDriftPolicy {
    pub wind_direction: [f32;2],
    pub minimum_wind: f32,
    pub transport_per_second: f32,
    pub max_fraction_per_step: f32,
    pub lee_distance: f32,
    pub lee_width: f32,
    pub lee_shelter: f32,
}
impl SnowDriftPolicy {
    pub fn validate(&self)->Result<(),String>{
        let norm=self.wind_direction[0].hypot(self.wind_direction[1]);
        if !self.wind_direction.iter().all(|v|v.is_finite())
           || norm<0.95||norm>1.05
           || !self.minimum_wind.is_finite()||!(0.0..=50.0).contains(&self.minimum_wind)
           || !self.transport_per_second.is_finite()||!(0.0..=0.01).contains(&self.transport_per_second)
           || !self.max_fraction_per_step.is_finite()||!(0.0..=0.25).contains(&self.max_fraction_per_step)
           || !self.lee_distance.is_finite()||!(0.5..=40.0).contains(&self.lee_distance)
           || !self.lee_width.is_finite()||!(0.2..=8.0).contains(&self.lee_width)
           || !self.lee_shelter.is_finite()||!(0.0..=0.98).contains(&self.lee_shelter)
        {return Err("invalid snow drift policy".into());}
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnowWindObstacle {
    pub position: [f32;2],
    pub radius: f32,
}
impl SnowWindObstacle {
    pub fn validate(&self)->Result<(),String>{
        if !self.position.iter().all(|v|v.is_finite())
           || !self.radius.is_finite()||!(0.15..=12.0).contains(&self.radius)
        {return Err("invalid wind obstacle".into());}
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnowPolicy {
    pub compacted_fraction: f32,
    pub contact_compaction: f32,
    pub max_contact_radius: f32,
    #[serde(default)]
    pub ridge_displacement_fraction: f32,
}
impl SnowPolicy {
    pub fn validate(&self) -> Result<(), String> {
        if !self.compacted_fraction.is_finite()
            || !(0.0..=1.0).contains(&self.compacted_fraction)
            || !self.contact_compaction.is_finite()
            || !(0.0..=1.0).contains(&self.contact_compaction)
            || !self.max_contact_radius.is_finite()
            || !(0.01..=20.0).contains(&self.max_contact_radius)
            || !self.ridge_displacement_fraction.is_finite()
            || !(0.0..=0.6).contains(&self.ridge_displacement_fraction)
        {
            return Err("invalid snow compaction policy".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnowFieldSpec {
    pub id: String,
    pub seed: u64,
    pub origin: [f32; 2],
    pub cell_size: f32,
    pub columns: usize,
    pub rows: usize,
    pub ground: Vec<f32>,
    pub depth: Vec<f32>,
}
impl SnowFieldSpec {
    pub fn validate(&self) -> Result<(), String> {
        let count = self
            .columns
            .checked_mul(self.rows)
            .ok_or("snow grid size overflow")?;
        if self.id.is_empty()
            || self.id.len() > 128
            || !(3..=1025).contains(&self.columns)
            || !(3..=1025).contains(&self.rows)
            || count > 1_050_625
            || !self.origin.iter().all(|x| x.is_finite())
            || !self.cell_size.is_finite()
            || !(0.05..=100.0).contains(&self.cell_size)
            || self.ground.len() != count
            || self.depth.len() != count
            || !self.ground.iter().all(|v| v.is_finite())
            || !self
                .depth
                .iter()
                .all(|v| v.is_finite() && (0.0..=20.0).contains(v))
        {
            return Err("invalid snow field dimensions, heights or depths".into());
        }
        Ok(())
    }
}
/// Sparse render-only footprints. Physics still uses the coarse heightfield.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SnowMicroprint {
    pub position:[f32;2],
    pub forward:[f32;2],
    pub half_width:f32,
    pub half_length:f32,
    pub depression:f32,
}
impl SnowMicroprint {
    pub fn validate(&self)->Result<(),String>{
        if !self.position.iter().chain(self.forward.iter()).all(|n|n.is_finite())
            || !(0.95..=1.05).contains(&self.forward[0].hypot(self.forward[1]))
            || !self.half_width.is_finite()||!(0.055..=0.23).contains(&self.half_width)
            || !self.half_length.is_finite()||!(0.12..=0.43).contains(&self.half_length)
            || !self.depression.is_finite()||!(0.0..=0.16).contains(&self.depression) {
            return Err("invalid snow microprint".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SnowField {
    pub spec: SnowFieldSpec,
    pub policy: SnowPolicy,
    compaction: Vec<f32>,
    /// Local snow strata. Missing in pre-v2 saves: reconstructed from depth
    /// and compaction on first mutation, without invalidating existing saves.
    #[serde(default)]
    strata: Vec<SnowStrata>,
    #[serde(default)]
    microprints: Vec<SnowMicroprint>,
    /// Accumulated liquid runoff, and ice after freezing. Old saves have none.
    #[serde(default)]
    surface_water: Vec<f32>,
    #[serde(default)]
    surface_ice: Vec<f32>,
    #[serde(skip)]
    dirty: SnowDirtyCells,
    #[serde(skip)]
    liquid_dirty: SnowDirtyCells,
    #[serde(skip)]
    dynamics_scratch: Option<Box<dynamics::Scratch>>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    layer_policy: Option<SnowLayerPolicy>,
    #[serde(default,skip_serializing_if="Vec::is_empty")]
    layer_columns: Vec<layers::Column>,
    #[serde(skip)]
    layer_contact_scratch: Option<Box<layers::ContactScratch>>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct SnowStrata {
    /// Fraction of the remaining snow which is loose, crystalline powder.
    pub powder: f32,
    /// Bonded frozen surface crust (0: none, 1: hard icy crust).
    pub crust: f32,
    /// Surface liquid-water fraction (0: dry, 1: saturated).
    pub moisture: f32,
}
impl Default for SnowStrata {
    fn default()->Self {Self{powder:1.0,crust:0.0,moisture:0.0}}
}
/// Evolve one surface layer without changing snow mass or geometry.
fn age_snow_strata(layer:&mut SnowStrata,dt:f64,temperature:f32)->bool{
    let old=*layer;
    let time=(dt as f32).min(30.0);
    if temperature< -2.0 && layer.moisture>0.001 {
        let frozen=(time*0.004*((-temperature)/15.0).clamp(0.2,2.0))
            .min(layer.moisture);
        layer.moisture-=frozen;
        layer.crust=(layer.crust+frozen*1.5).min(1.0);
        layer.powder=(layer.powder-frozen).max(0.0);
    }else if temperature>0.0 {
        layer.crust=(layer.crust-time*0.001*temperature.min(10.0)).max(0.0);
    }
    *layer!=old
}
impl SnowStrata {
    fn valid(self)->bool {[self.powder,self.crust,self.moisture].iter()
        .all(|v|v.is_finite()&&(0.0..=1.0).contains(v))}
}
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct SnowSample {
    pub surface_height: f32,
    pub depth: f32,
    pub loose_depth: f32,
    pub compaction: f32,
    pub powder: f32,
    pub crust: f32,
    pub moisture: f32,
    pub standing_water_depth: f32,
    pub ground_ice_depth: f32,
    pub slush_fraction: f32,
}
impl SnowField {
    pub fn new(spec: SnowFieldSpec, policy: SnowPolicy) -> Result<Self, String> {
        spec.validate()?;
        policy.validate()?;
        Ok(Self {
            compaction: vec![0.0; spec.ground.len()],
            strata: vec![SnowStrata::default();spec.ground.len()],
            microprints: Vec::new(),
            surface_water: vec![0.0;spec.ground.len()],
            surface_ice: vec![0.0;spec.ground.len()],
            spec,
            policy,
            dirty: SnowDirtyCells::new(),
            liquid_dirty: SnowDirtyCells::new(),
            dynamics_scratch: None,
            layer_policy: None,
            layer_columns: Vec::new(),
            layer_contact_scratch: None,
        })
    }
    pub fn validate(&self) -> Result<(), String> {
        self.spec.validate()?;
        self.policy.validate()?;
        if self.compaction.len() != self.spec.ground.len()
            || !self
                .compaction
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            || (!self.strata.is_empty() &&
                (self.strata.len()!=self.spec.ground.len() ||
                 self.strata.iter().any(|l|!l.valid())))
            || ((!self.surface_water.is_empty()||!self.surface_ice.is_empty())&&
                (self.surface_water.len()!=self.spec.ground.len()||
                 self.surface_ice.len()!=self.spec.ground.len()||
                 self.surface_water.iter().chain(self.surface_ice.iter()).any(|v|
                    !v.is_finite()||!(0.0..=2.0).contains(v))))
            || self.microprints.len()>256
            || self.microprints.iter().any(|p|p.validate().is_err() ||
               self.sample(p.position[0],p.position[1]).is_none())
        {
            return Err("invalid saved snow compaction or strata".into());
        }
        self.validate_columns()?;
        Ok(())
    }
    fn ensure_strata(&mut self) {
        if self.strata.is_empty(){
            self.strata=self.spec.depth.iter().zip(self.compaction.iter())
                .map(|(d,c)| SnowStrata{powder:if *d>0.0{1.0-*c}else{0.0},
                     crust:0.0,moisture:0.0}).collect();
        }
    }
    fn ensure_liquids(&mut self){
        if self.surface_water.is_empty()&&self.surface_ice.is_empty(){
            self.surface_water=vec![0.0;self.spec.ground.len()];
            self.surface_ice=vec![0.0;self.spec.ground.len()];
        }
    }
    fn exchange_water_ice(&mut self,i:usize,dt:f32,temp:f32)->bool{
        let prior_water=self.surface_water[i];
        let prior_ice=self.surface_ice[i];
        if temp< -1.0 {
            let frozen=(dt*0.002*(-temp/20.0).min(2.0)).min(prior_water)
                .min((2.0-prior_ice).max(0.0));
            self.surface_water[i]-=frozen;
            self.surface_ice[i]+=frozen;
        }else if temp>0.0{
            let thaw=(dt*0.0025*(temp/15.0).min(2.0)).min(prior_ice)
                .min((2.0-prior_water).max(0.0));
            self.surface_ice[i]-=thaw;
            self.surface_water[i]+=thaw;
        }
        let changed=self.surface_water[i]!=prior_water||self.surface_ice[i]!=prior_ice;
        if changed{self.dirty.insert(i);self.liquid_dirty.insert(i);}
        changed
    }
    fn vertex(&self, i: usize) -> SnowSample {
        let c = self.compaction[i];
        let d = self.spec.depth[i];
        let layer=self.strata.get(i).copied().unwrap_or(SnowStrata{
            powder:if d>0.0{1.0-c}else{0.0},crust:0.0,moisture:0.0});
        SnowSample {
            powder:if d>0.0 {layer.powder}else{0.0},
            crust:if d>0.0 {layer.crust}else{0.0},
            moisture:if d>0.0 {layer.moisture}else{0.0},
            standing_water_depth:self.surface_water.get(i).copied().unwrap_or(0.0),
            ground_ice_depth:self.surface_ice.get(i).copied().unwrap_or(0.0),
            slush_fraction:if d>0.0{layer.moisture}else{0.0},
            surface_height: self.spec.ground[i]
                + d * (1.0 - c * (1.0 - self.policy.compacted_fraction)),
            depth: d,
            // Only dry, unbonded powder creates deep-snow walking resistance.
            loose_depth: if self.layers_enabled(){self.column_loose_depth(i)}
                else{d * (1.0-c)*layer.powder*(1.0-0.75*layer.crust)},
            compaction: c,
        }
    }
    pub fn sample(&self, x: f32, z: f32) -> Option<SnowSample> {
        let u = (x - self.spec.origin[0]) / self.spec.cell_size;
        let v = (z - self.spec.origin[1]) / self.spec.cell_size;
        if !u.is_finite()
            || !v.is_finite()
            || u < 0.0
            || v < 0.0
            || u > (self.spec.columns - 1) as f32
            || v > (self.spec.rows - 1) as f32
        {
            return None;
        }
        let col = (u.floor() as usize).min(self.spec.columns - 2);
        let row = (v.floor() as usize).min(self.spec.rows - 2);
        let u = u - col as f32;
        let v = v - row as f32;
        let a = row * self.spec.columns + col;
        let b = a + 1;
        let c = a + self.spec.columns;
        let d = c + 1;
        let vertices = if u + v <= 1.0 {
            [(a, 1.0 - u - v), (b, u), (c, v)]
        } else {
            [(b, 1.0 - v), (c, 1.0 - u), (d, u + v - 1.0)]
        };
        let mut result = SnowSample::default();
        for (i, w) in vertices {
            let s = self.vertex(i);
            result.surface_height += s.surface_height * w;
            result.depth += s.depth * w;
            result.loose_depth += s.loose_depth * w;
            result.compaction += s.compaction * w;
            result.powder += s.powder * w;
            result.crust += s.crust * w;
            result.moisture += s.moisture * w;
            result.standing_water_depth+=s.standing_water_depth*w;
            result.ground_ice_depth+=s.ground_ice_depth*w;
            result.slush_fraction+=s.slush_fraction*w;
        }
        Some(result)
    }
    /// High-resolution print independent of 1m collision mesh.
    pub fn stamp_microprint(&mut self,print:SnowMicroprint)->Result<bool,String>{
        print.validate()?;
        let sample=self.sample(print.position[0],print.position[1])
            .ok_or("snow microprint outside field")?;
        if sample.depth<0.06||sample.powder<0.06||print.depression==0.0{return Ok(false);}
        let mut p=print;
        p.depression=p.depression.min(sample.depth*0.42)
            *(1.0-0.65*sample.crust)*(1.0-0.45*sample.moisture);
        if p.depression<0.002{return Ok(false);}
        if self.microprints.iter().rev().take(8).any(|old|{
            let dx=old.position[0]-p.position[0];
            let dz=old.position[1]-p.position[1];
            dx*dx+dz*dz<0.09*0.09
        }) {return Ok(false);}
        if self.microprints.len()>=256{self.microprints.remove(0);}
        self.microprints.push(p);
        Ok(true)
    }
    pub fn microprints(&self)->&[SnowMicroprint]{&self.microprints}
    pub fn contact(&mut self, x: f32, z: f32, radius: f32, strength: f32) -> Result<(), String> {
        if ![x, z, radius, strength].iter().all(|v| v.is_finite())
            || radius <= 0.0
            || radius > self.policy.max_contact_radius
            || !(0.0..=1.0).contains(&strength)
        {
            return Err("invalid snow contact".into());
        }
        self.ensure_strata();
        if self.layers_enabled(){self.contact_layers(x,z,radius,strength);return Ok(());}
        let s=&self.spec;
        // Track centres compact the porous layer and push a bounded quantity
        // of loose snow to the outside. No affected grid cell is fabricated.
        let rim_fraction=self.policy.ridge_displacement_fraction;
        let rim_width=if rim_fraction>0.0 {
            (radius*0.9).max(s.cell_size*0.85)
        }else{0.0};
        let reach=radius+rim_width;
        let c0=(((x-reach-s.origin[0])/s.cell_size).floor() as isize)
            .clamp(0,s.columns as isize-1) as usize;
        let r0=(((z-reach-s.origin[1])/s.cell_size).floor() as isize)
            .clamp(0,s.rows as isize-1) as usize;
        let c1=(((x+reach-s.origin[0])/s.cell_size).ceil() as isize)
            .clamp(0,s.columns as isize-1) as usize;
        let r1=(((z+reach-s.origin[1])/s.cell_size).ceil() as isize)
            .clamp(0,s.rows as isize-1) as usize;
        let mut pressed=Vec::<(usize,f32,f32,f32)>::new();
        let mut rim=Vec::<(usize,f32)>::new();
        for row in r0..=r1 {
            for col in c0..=c1 {
                let dx=s.origin[0]+col as f32*s.cell_size-x;
                let dz=s.origin[1]+row as f32*s.cell_size-z;
                let d=(dx*dx+dz*dz).sqrt();
                let index=row*s.columns+col;
                if d<radius {
                    let weight=(1.0-d*d/(radius*radius)).max(0.0);
                    let old=self.compaction[index];
                    let shell=self.strata[index].crust*
                        (1.0-0.45*self.strata[index].moisture);
                    let load=weight*strength;
                    let breakthrough=(load-(0.25+0.4*shell))
                        .max(0.0)/(0.75-0.4*shell).max(0.1);
                    let compliance=(1.0-0.85*shell)+breakthrough*0.85*shell;
                    let moisture_resistance=1.0-0.35*self.strata[index].moisture;
                    let next=(old+load*self.policy.contact_compaction*
                        compliance*moisture_resistance).min(1.0);
                    if next!=old {
                        let shifted=(self.spec.depth[index]*rim_fraction*(next-old))
                            .min(self.spec.depth[index]*0.20);
                        pressed.push((index,next,shifted,breakthrough));
                    }
                }else if d<reach&&rim_fraction>0.0 {
                    let t=(d-radius)/rim_width;
                    let weight=4.0*t*(1.0-t);
                    if weight>0.0&&self.spec.depth[index]<20.0 {
                        rim.push((index,weight));
                    }
                }
            }
        }
        // Bound the displaced mass by the room actually available in the
        // surrounding snow. Saturated drifts must never exceed 20 m depth.
        let requested:f32=pressed.iter().map(|(_,_,shift,_)|*shift).sum();
        let capacity:f32=rim.iter().map(|(i,_)|(20.0-self.spec.depth[*i]).max(0.0)).sum();
        let volume=requested.min(capacity);
        let scale=if requested>0.0{volume/requested}else{0.0};
        for (idx,next,shifted,breakthrough) in pressed {
            let previous=self.compaction[idx];
            self.compaction[idx]=next;
            // Crushing breaks crust locally and reduces the loose surface.
            let layer=&mut self.strata[idx];
            layer.powder=(layer.powder-(next-previous)*0.9).max(0.0);
            layer.crust=(layer.crust-(next-previous)*0.12-
                breakthrough*0.5).max(0.0);
            if volume>0.0{self.spec.depth[idx]-=shifted*scale;}
            self.dirty.insert(idx);
        }
        // Weighted capacity-aware filling conserves snow depth even next to
        // an almost-full rim cell, where a single weighted pass overflows.
        let mut remaining=volume;
        for _ in 0..=rim.len(){
            if remaining<=1e-6{break;}
            let total_weight:f32=rim.iter()
                .filter(|(i,_)|self.spec.depth[*i]<20.0-1e-6)
                .map(|(_,w)|*w).sum();
            if total_weight<=0.0{break;}
            let current=remaining;
            let mut distributed=0.0;
            for (idx,weight) in &rim {
                let space=(20.0-self.spec.depth[*idx]).max(0.0);
                let delta=(current*weight/total_weight).min(space);
                if delta>0.0{
                    let old_depth=self.spec.depth[*idx];
                    self.spec.depth[*idx]+=delta;
                    let layer=&mut self.strata[*idx];
                    layer.powder=((layer.powder*old_depth+delta)/
                        self.spec.depth[*idx]).clamp(0.0,1.0);
                    self.dirty.insert(*idx);
                    distributed+=delta;
                }
            }
            remaining=(remaining-distributed).max(0.0);
            if distributed<=1e-7{break;}
        }
        Ok(())
    }
    /// Locally melt only vertices influenced by active heat sources. Bounded
    /// per-source rectangles avoid full-map work on every simulation tick.
    /// The warmest source wins: overlapping emitters never multiply free heat.
    pub fn advance_heat(&mut self, dt: f64, environment: &ThermalEnvironment,
        sources: &[HeatSource], policy: SnowHeatPolicy) -> Result<usize, String> {
        super::valid_dt(dt)?;
        environment.validate()?;
        policy.validate()?;
        if dt==0.0||sources.is_empty()||policy.melt_metres_per_second==0.0{return Ok(0);}
        self.ensure_strata();self.ensure_liquids();
        let rows=self.spec.rows;let columns=self.spec.columns;
        // Scratch only for affected vertices: sparse mask with deterministic max.
        let mut heat=std::collections::BTreeMap::<usize,f32>::new();
        let grid=&self.spec;
        for src in sources{
            src.validate()?;
            if !src.enabled||src.delta_temperature<=0.0{continue;}
            let radius=src.radius;
            let c0=(((src.position[0]-radius-grid.origin[0])/grid.cell_size).floor() as isize)
                .clamp(0,columns as isize-1) as usize;
            let c1=(((src.position[0]+radius-grid.origin[0])/grid.cell_size).ceil() as isize)
                .clamp(0,columns as isize-1) as usize;
            let r0=(((src.position[2]-radius-grid.origin[1])/grid.cell_size).floor() as isize)
                .clamp(0,rows as isize-1) as usize;
            let r1=(((src.position[2]+radius-grid.origin[1])/grid.cell_size).ceil() as isize)
                .clamp(0,rows as isize-1) as usize;
            for row in r0..=r1 {
                let z=grid.origin[1]+row as f32*grid.cell_size;
                let dz=(z-src.position[2])/radius;
                for col in c0..=c1{
                    let x=grid.origin[0]+col as f32*grid.cell_size;
                    let dx=(x-src.position[0])/radius;
                    let d2=dx*dx+dz*dz;
                    if d2>=1.0{continue;}
                    // Smooth thermal plume without a hard circular edge.
                    let falloff=(1.0-d2).powi(2);
                    let influence=src.delta_temperature*falloff;
                    let idx=row*columns+col;
                    heat.entry(idx).and_modify(|h|*h=h.max(influence))
                        .or_insert(influence);
                }
            }
        }
        let wind_loss=environment.wind*policy.wind_cooling;
        let base=environment.temperature-wind_loss;
        let mut changed=0;
        for (idx,influence) in heat{
            let effective=base+influence*policy.surface_heat_multiplier;
            let warm=(effective-policy.melt_temperature).max(0.0);
            if warm<=0.0{continue;}
            let fraction=(warm/policy.temperature_range).clamp(0.0,1.0);
            let old=self.spec.depth[idx];
            if old<=0.0{continue;}
            let loss=(policy.melt_metres_per_second as f64*dt*fraction as f64) as f32;
            let next=(old-loss).max(0.0);
            if next!=old{
                let melted=if self.layers_enabled(){self.column_melt(idx,old-next)}
                    else{self.spec.depth[idx]=next;old-next};
                let water=(self.surface_water[idx]+melted).min(2.0);
                if self.surface_water[idx]!=water {self.liquid_dirty.insert(idx);}
                self.surface_water[idx]=water;
                if self.layers_enabled(){
                    // column_melt already removes top layers and wets the exposed surface.
                }else if next<=0.0{
                    self.compaction[idx]=0.0;
                    self.strata[idx]=SnowStrata{powder:0.0,crust:0.0,moisture:0.0};
                }else{
                    let layer=&mut self.strata[idx];
                    layer.powder=(layer.powder-loss/(old+0.0001)).max(0.0);
                    layer.crust=(layer.crust-loss/(old+0.0001)*0.8).max(0.0);
                    layer.moisture=(layer.moisture+loss/(old+0.0001)*0.7).min(1.0);
                }
                self.dirty.insert(idx);
                changed+=1;
            }
        }
        // A fully melted footprint cannot re-emerge after new snowfall.
        // A sparse pass over at most 256 marks is cheap compared with the
        // native snow grid/heightfield update.
        let old_marks=std::mem::take(&mut self.microprints);
        self.microprints=old_marks.into_iter().filter(|p|{
            self.sample(p.position[0],p.position[1])
                .is_some_and(|s|s.depth>=0.055)
        }).collect();
        Ok(changed)
    }
    /// Surface state aging, sampled by the runtime on a bounded cadence.
    /// Warmth softens frozen crust, cold refreezes wet top snow. This is a
    /// snow-surface approximation, not a full thermodynamic conduction solver.
    /// Cached static lee map, recomputed only when wind obstruction changes.
    /// Obstacles are authored per project, not embedded in generic weather.
    pub fn wind_shelter_map(&self, policy:SnowDriftPolicy,
                            obstacles:&[SnowWindObstacle])->Result<Vec<f32>,String>{
        policy.validate()?;
        if obstacles.len()>1024{return Err("too many snow wind obstacles".into());}
        let grid=&self.spec;
        let mut shelter=vec![0.0;grid.depth.len()];
        let dx=policy.wind_direction[0];
        let dz=policy.wind_direction[1];
        for obs in obstacles {
            obs.validate()?;
            let reach=policy.lee_distance+obs.radius;
            let low_x=((obs.position[0]-reach-grid.origin[0])/grid.cell_size).floor()
                .max(0.0) as usize;
            let high_x=((obs.position[0]+reach-grid.origin[0])/grid.cell_size).ceil()
                .min((grid.columns-1) as f32) as usize;
            let low_z=((obs.position[1]-reach-grid.origin[1])/grid.cell_size).floor()
                .max(0.0) as usize;
            let high_z=((obs.position[1]+reach-grid.origin[1])/grid.cell_size).ceil()
                .min((grid.rows-1) as f32) as usize;
            for row in low_z..=high_z {
                for col in low_x..=high_x {
                    let x=grid.origin[0]+col as f32*grid.cell_size-obs.position[0];
                    let z=grid.origin[1]+row as f32*grid.cell_size-obs.position[1];
                    let downstream=x*dx+z*dz;
                    if downstream<0.0||downstream>policy.lee_distance+obs.radius{continue;}
                    let side=(-x*dz+z*dx).abs();
                    let width=obs.radius+policy.lee_width;
                    if side>=width{continue;}
                    let along=(1.0-downstream/(policy.lee_distance+obs.radius)).max(0.0);
                    let cross=(1.0-(side/width).powi(2)).max(0.0);
                    let score=policy.lee_shelter*along*cross;
                    let idx=row*grid.columns+col;
                    shelter[idx]=1.0-(1.0-shelter[idx])*(1.0-score);
                }
            }
        }
        Ok(shelter)
    }
    /// Conservative single-hop erosion and deposition. The source powder
    /// fraction is bounded by available snow; destination snow cannot exceed
    /// the native 20 m limit, and one pass never reallocates its own output.
    pub fn advance_wind(&mut self,dt:f64,wind:f32,policy:SnowDriftPolicy,
                        shelter:&[f32])->Result<usize,String>{
        super::valid_dt(dt)?;
        policy.validate()?;
        if !wind.is_finite()||wind<0.0||wind>200.0{return Err("invalid drift wind".into());}
        if shelter.len()!=self.spec.depth.len() ||
            shelter.iter().any(|v|!v.is_finite()||!(0.0..=1.0).contains(v)){
            return Err("invalid wind shelter map".into());
        }
        if dt==0.0||wind<=policy.minimum_wind||policy.transport_per_second==0.0{
            return Ok(0);
        }
        self.ensure_strata();
        let direction=policy.wind_direction;
        let (dc,dr)=if direction[0].abs()>=direction[1].abs(){
            (if direction[0]>=0.0{1_isize}else{-1},0_isize)
        }else{(0_isize,if direction[1]>=0.0{1_isize}else{-1})};
        let n=self.spec.columns;
        let m=self.spec.rows;
        let factor=((wind-policy.minimum_wind)*policy.transport_per_second*
            (dt as f32).min(30.0)).min(policy.max_fraction_per_step);
        // Calculate transfers from one immutable snapshot, then commit.
        let mut transfers=Vec::<(usize,usize,f32)>::new();
        for row in 0..m{
            for col in 0..n{
                let to_c=col as isize+dc;let to_r=row as isize+dr;
                if to_c<0||to_c>=n as isize||to_r<0||to_r>=m as isize{continue;}
                let src=row*n+col;let dst=to_r as usize*n+to_c as usize;
                if self.spec.depth[src]<=0.0||self.spec.depth[dst]>=20.0{continue;}
                // Bonded crust and wet/slushy snow are not free dry powder.
                let layer=if self.layers_enabled(){let m=self.column_top(src).material();
                    SnowStrata{powder:m[0],crust:m[1],moisture:m[2]}}else{self.strata[src]};
                let compact=if self.layers_enabled(){self.column_top(src).material()[3]}else{self.compaction[src]};
                let powder=layer.powder*(1.0-compact)*
                    (1.0-layer.crust).powi(2)*(1.0-layer.moisture);
                if powder<0.001{continue;}
                let rise=self.spec.ground[dst]-self.spec.ground[src];
                let slope=(1.0-rise.max(0.0)/self.spec.cell_size).clamp(0.1,1.0);
                let exposure=(1.0-shelter[src]).clamp(0.0,1.0);
                let fraction=factor*exposure*slope;
                let available=if self.layers_enabled(){self.column_top(src).depth()*powder}
                    else{self.spec.depth[src]*powder};
                let volume=(available*fraction).min(self.spec.depth[src]*0.12)
                    .min(20.0-self.spec.depth[dst]);
                if volume>1e-7{transfers.push((src,dst,volume));}
            }
        }
        if self.layers_enabled(){
            // Remove all donors before depositing anything: incoming snow must
            // never be re-eroded in the same immutable-snapshot step.
            let materials=transfers.iter().map(|(src,_,_)|self.column_top(*src).material()).collect::<Vec<_>>();
            for (src,_,volume) in &transfers{
                self.layer_columns[*src].remove_top(*volume);self.sync_column(*src);self.dirty.insert(*src);
            }
            for ((_,dst,volume),material) in transfers.iter().zip(materials){
                self.column_deposit(*dst,*volume,material);self.dirty.insert(*dst);
            }
            return Ok(transfers.len());
        }
        for (src,dst,volume) in &transfers {
            let orig_src=self.spec.depth[*src];
            let orig_dst=self.spec.depth[*dst];
            let retained=(self.strata[*src].powder*orig_src-volume).max(0.0);
            self.spec.depth[*src]=(orig_src-volume).max(0.0);
            self.strata[*src].powder=if self.spec.depth[*src]>0.0{
                (retained/self.spec.depth[*src]).clamp(0.0,1.0)
            }else{0.0};
            if self.spec.depth[*src]>0.0 {
                let concentration=orig_src/self.spec.depth[*src];
                self.strata[*src].crust=(self.strata[*src].crust*concentration).min(1.0);
                self.strata[*src].moisture=(self.strata[*src].moisture*concentration).min(1.0);
            }else{
                self.strata[*src]=SnowStrata{powder:0.0,crust:0.0,moisture:0.0};
            }
            self.spec.depth[*dst]=orig_dst+volume;
            self.strata[*dst].powder=((self.strata[*dst].powder*orig_dst+volume)/
                self.spec.depth[*dst]).clamp(0.0,1.0);
            self.strata[*dst].crust*=orig_dst/self.spec.depth[*dst];
            self.strata[*dst].moisture*=orig_dst/self.spec.depth[*dst];
            self.compaction[*dst]*=orig_dst/self.spec.depth[*dst];
            self.dirty.insert(*src);
            self.dirty.insert(*dst);
        }
        Ok(transfers.len())
    }
    pub fn weather_strata(&mut self, dt:f64, air_temperature:f32)->Result<usize,String>{
        super::valid_dt(dt)?;
        if !air_temperature.is_finite(){return Err("invalid strata temperature".into());}
        self.ensure_strata();self.ensure_liquids();
        let mut changed=0;
        for i in 0..self.strata.len(){
            if self.exchange_water_ice(i,(dt as f32).min(30.0),air_temperature) {
                changed+=1;
            }
            if self.spec.depth[i]<=0.0{continue;}
            let aged=if self.layers_enabled(){self.column_age(i,dt,air_temperature)}
                else{age_snow_strata(&mut self.strata[i],dt,air_temperature)};
            if aged{
                self.dirty.insert(i);changed+=1;
            }
        }
        Ok(changed)
    }
    /// Freeze/refreeze uses the snow surface's local temperature. A running
    /// generator prevents its own wet snow from instantly becoming icy crust
    /// while the surrounding winter air remains far below zero.
    pub fn weather_strata_with_heat(&mut self,dt:f64,
        environment:&ThermalEnvironment,sources:&[HeatSource],
        heat:SnowHeatPolicy)->Result<usize,String>{
        super::valid_dt(dt)?;
        environment.validate()?;heat.validate()?;
        for source in sources{source.validate()?;}
        self.ensure_strata();self.ensure_liquids();
        let ambient=environment.temperature-environment.wind*heat.wind_cooling;
        let mut changed=0;
        for i in 0..self.strata.len(){
            let layer=self.strata[i];
            if layer.moisture<=0.001 && layer.crust<=0.0
               && self.surface_water[i]<=0.0 && self.surface_ice[i]<=0.0 {continue;}
            let x=self.spec.origin[0]+(i%self.spec.columns) as f32*self.spec.cell_size;
            let z=self.spec.origin[1]+(i/self.spec.columns) as f32*self.spec.cell_size;
            let mut local=ambient;
            for src in sources {
                if !src.enabled||src.delta_temperature<=0.0{continue;}
                let dx=(x-src.position[0])/src.radius;
                let dz=(z-src.position[2])/src.radius;
                let d2=dx*dx+dz*dz;
                if d2>=1.0{continue;}
                let influence=src.delta_temperature*(1.0-d2).powi(2);
                local=local.max(ambient+influence*heat.surface_heat_multiplier);
            }
            if self.exchange_water_ice(i,(dt as f32).min(30.0),local){
                changed+=1;
            }
            let aged=if self.spec.depth[i]<=0.0{false}
                else if self.layers_enabled(){self.column_age(i,dt,local)}
                else{age_snow_strata(&mut self.strata[i],dt,local)};
            if aged {
                self.dirty.insert(i);changed+=1;
            }
        }
        Ok(changed)
    }
    /// Render and collider sync payload. Only height is authoritative for
    /// collision; remaining channels are snow material strata (0..1).
    pub fn take_render_changes(&mut self)->Vec<[f32;6]> {
        self.take_visual_changes().0
    }
    /// Single dirty-set drain supplies snow geometry and wet soil material,
    /// avoiding mismatched frames or losing soil-only water/ice transitions.
    pub fn take_visual_changes(&mut self)->(Vec<[f32;6]>,Vec<[f32;5]>){
        let changed=self.dirty.take_indices();
        let liquid=self.liquid_dirty.take_indices();
        let snow=changed.iter().map(|i|self.render_vertex(*i)).collect();
        let ground=liquid.iter().map(|i|self.liquid_vertex(*i)).collect();
        (snow,ground)
    }
    pub fn has_visual_changes(&self)->bool {
        !self.dirty.is_empty()||!self.liquid_dirty.is_empty()
    }
    pub fn pending_visual_cells(&self)->usize {self.dirty.len()}
    pub fn all_liquid_vertices(&self)->Vec<[f32;5]> {
        (0..self.spec.depth.len()).map(|i|self.liquid_vertex(i)).collect()
    }
    fn liquid_vertex(&self,i:usize)->[f32;5]{
        [self.spec.origin[0]+(i%self.spec.columns) as f32*self.spec.cell_size,
         self.spec.ground[i],
         self.spec.origin[1]+(i/self.spec.columns) as f32*self.spec.cell_size,
         self.surface_water.get(i).copied().unwrap_or(0.0),
         self.surface_ice.get(i).copied().unwrap_or(0.0)]
    }
    pub fn all_render_vertices(&self)->Vec<[f32;6]> {
        (0..self.spec.depth.len()).map(|i|self.render_vertex(i)).collect()
    }
    fn render_vertex(&self,i:usize)->[f32;6]{
        // Geometry extraction needs height/material only, not a walking
        // resistance scan through every layer of the column.
        let d=self.spec.depth[i];let c=self.compaction[i];
        let layer=self.strata.get(i).copied().unwrap_or(SnowStrata{
            powder:if d>0.0{1.0-c}else{0.0},crust:0.0,moisture:0.0});
        [self.spec.origin[0]+(i%self.spec.columns) as f32*self.spec.cell_size,
         self.transport_height(i),
         self.spec.origin[1]+(i/self.spec.columns) as f32*self.spec.cell_size,
         if d>0.0{layer.powder}else{0.0},if d>0.0{layer.crust}else{0.0},
         if d>0.0{layer.moisture}else{0.0}]
    }
    pub fn take_changes(&mut self) -> Vec<[f32; 3]> {
        let changed = self.dirty.take_indices();
        changed
            .into_iter()
            .map(|i| {
                [
                    self.spec.origin[0] + (i % self.spec.columns) as f32 * self.spec.cell_size,
                    self.vertex(i).surface_height,
                    self.spec.origin[1] + (i / self.spec.columns) as f32 * self.spec.cell_size,
                ]
            })
            .collect()
    }
    pub fn all_vertices(&self) -> Vec<[f32; 3]> {
        (0..self.spec.ground.len())
            .map(|i| {
                [
                    self.spec.origin[0] + (i % self.spec.columns) as f32 * self.spec.cell_size,
                    self.vertex(i).surface_height,
                    self.spec.origin[1] + (i / self.spec.columns) as f32 * self.spec.cell_size,
                ]
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn field() -> SnowField {
        SnowField::new(
            SnowFieldSpec {
                id: "test".into(),
                seed: 4,
                origin: [-2.0, -2.0],
                cell_size: 1.0,
                columns: 5,
                rows: 5,
                ground: vec![0.0; 25],
                depth: vec![1.0; 25],
            },
            SnowPolicy {
                compacted_fraction: 0.2,
                contact_compaction: 0.5,
                max_contact_radius: 3.0,
                ridge_displacement_fraction: 0.38,
            },
        )
        .unwrap()
    }
    #[test]
    fn passage_compacts_and_survives_restart() {
        let mut f = field();
        let a = f.sample(0.0, 0.0).unwrap();
        f.contact(0.0, 0.0, 1.2, 1.0).unwrap();
        let b = f.sample(0.0, 0.0).unwrap();
        assert!(b.surface_height < a.surface_height && b.loose_depth < a.loose_depth);
        let saved = serde_json::to_string(&f).unwrap();
        let resumed: SnowField = serde_json::from_str(&saved).unwrap();
        resumed.validate().unwrap();
        assert_eq!(
            b.surface_height,
            resumed.sample(0.0, 0.0).unwrap().surface_height
        );
        assert_eq!(f.sample(-2.0, -2.0).unwrap().compaction, 0.0);
    }
    #[test]
    fn stationary_time_never_erases_a_trail_but_new_snow_refills_it() {
        let mut f = field();
        f.contact(0.0, 0.0, 1.2, 1.0).unwrap();
        let h = f.sample(0.0, 0.0).unwrap();
        f.advance(100.0, 0.0, 0.0).unwrap();
        assert_eq!(h.compaction, f.sample(0.0, 0.0).unwrap().compaction);
        f.advance(100.0, 0.01, 0.0).unwrap();
        assert!(f.sample(0.0, 0.0).unwrap().surface_height > h.surface_height);
    }
    fn warm_source(id: &str,x:f32,enabled:bool)->HeatSource{
        HeatSource{id:id.into(),position:[x,0.0,0.0],radius:2.5,
            delta_temperature:15.0,enabled}
    }
    fn heat_policy()->SnowHeatPolicy{
        SnowHeatPolicy{surface_heat_multiplier:6.0,melt_temperature:0.0,
            melt_metres_per_second:0.01,temperature_range:15.0,wind_cooling:0.08,
            active_interval:0.5}
    }
    #[test]
    fn localized_heat_exposes_soil_but_leaves_distant_snow_unchanged(){
        let mut f=field();
        let cold=ThermalEnvironment{temperature:-35.0,wind:4.0,wind_loss:0.5};
        let before=f.sample(0.0,0.0).unwrap();
        let affected=f.advance_heat(120.0,&cold,&[warm_source("generator",0.0,true)],heat_policy()).unwrap();
        assert!(affected>0);
        assert_eq!(f.sample(0.0,0.0).unwrap().depth,0.0);
        assert_eq!(f.sample(-2.0,-2.0).unwrap().depth,1.0);
        assert!(before.surface_height>f.sample(0.0,0.0).unwrap().surface_height);
        assert!(f.take_changes().iter().any(|p|p[0]==0.0&&p[2]==0.0));
        let snapshot=serde_json::to_string(&f).unwrap();
        let restored:SnowField=serde_json::from_str(&snapshot).unwrap();
        restored.validate().unwrap();
        assert_eq!(restored.sample(0.0,0.0).unwrap().depth,0.0);
        let mut again=restored;
        again.advance(60.0,0.002,0.0).unwrap();
        assert!(again.sample(0.0,0.0).unwrap().depth>0.0,
            "cooling and fresh snowfall can re-cover exposed earth");
    }
    #[test]
    fn inactive_sources_and_cold_surfaces_do_not_remove_snow(){
        let mut f=field();
        let cold=ThermalEnvironment{temperature:-90.0,wind:20.0,wind_loss:0.5};
        let policy=heat_policy();
        assert_eq!(f.advance_heat(10.0,&cold,&[warm_source("off",0.0,false)],policy).unwrap(),0);
        assert_eq!(f.advance_heat(10.0,&cold,&[warm_source("cold",0.0,true)],policy).unwrap(),0);
        assert_eq!(f.sample(0.0,0.0).unwrap().depth,1.0);
        assert!(f.advance_heat(1.0,&cold,&[warm_source("cold",0.0,true)],
            SnowHeatPolicy{melt_metres_per_second:-1.0,..policy}).is_err());
    }
    #[test]
    fn overlapping_sources_do_not_artificially_double_heat(){
        let cold=ThermalEnvironment{temperature:-35.0,wind:4.0,wind_loss:0.5};
        let mut single=field();let mut double=field();
        let src=warm_source("a",0.0,true);let mut src2=src.clone();src2.id="b".into();
        single.advance_heat(1.0,&cold,&[src.clone()],heat_policy()).unwrap();
        double.advance_heat(1.0,&cold,&[src,src2],heat_policy()).unwrap();
        assert_eq!(single.spec.depth,double.spec.depth);
    }
    #[test]
    fn footfall_sculpts_an_outer_ridge_and_conserves_snow_mass(){
        let mut f=field();
        let before:f32=f.spec.depth.iter().sum();
        f.contact(0.0,0.0,1.15,1.0).unwrap();
        assert!(f.sample(0.0,0.0).unwrap().surface_height<1.0);
        let rim=f.sample(2.0,0.0).unwrap();
        assert!(rim.depth>1.0,"displaced powder forms a shallow flank");
        let after:f32=f.spec.depth.iter().sum();
        assert!((before-after).abs()<1e-4,"snow cannot be created by feet");
        let saved=serde_json::to_string(&f).unwrap();
        let restored:SnowField=serde_json::from_str(&saved).unwrap();
        assert_eq!(restored.spec.depth,f.spec.depth);
        assert!(restored.spec.depth.iter().all(|v|*v>=0.0&&*v<=20.0));
    }
    #[test]
    fn ridge_displacement_does_not_overfill_saturated_cells(){
        let mut f=field();
        f.spec.depth.fill(20.0);
        let total:f32=f.spec.depth.iter().sum();
        f.contact(0.0,0.0,1.15,1.0).unwrap();
        assert!(f.spec.depth.iter().all(|d|*d<=20.0));
        assert!((total-f.spec.depth.iter().sum::<f32>()).abs()<1e-4);
        f.spec.depth.fill(19.999);
        let before:f32=f.spec.depth.iter().sum();
        f.contact(0.0,0.0,1.15,1.0).unwrap();
        assert!(f.spec.depth.iter().all(|d|*d<=20.0));
        assert!((before-f.spec.depth.iter().sum::<f32>()).abs()<0.002);
    }
    #[test]
    fn fresh_snow_is_powder_and_footprints_pack_and_break_crust(){
        let mut f=field();
        assert!(f.sample(0.0,0.0).unwrap().powder>0.95);
        f.contact(0.0,0.0,1.2,1.0).unwrap();
        let compact=f.sample(0.0,0.0).unwrap();
        assert!(compact.powder<1.0);
        f.advance(30.0,0.01,0.0).unwrap();
        assert!(f.sample(0.0,0.0).unwrap().powder>compact.powder);
        let encoded=serde_json::to_string(&f).unwrap();
        let decoded:SnowField=serde_json::from_str(&encoded).unwrap();
        decoded.validate().unwrap();
        assert_eq!(decoded.sample(0.0,0.0).unwrap().powder,
                   f.sample(0.0,0.0).unwrap().powder);
    }
    #[test]
    fn thaw_and_freeze_build_a_crust_without_rebuilding_the_grid(){
        let mut f=field();
        let cold=ThermalEnvironment{temperature:-35.0,wind:4.0,wind_loss:0.5};
        f.advance_heat(0.9,&cold,&[warm_source("g",0.0,true)],heat_policy()).unwrap();
        let damp=f.sample(0.0,0.0).unwrap();
        assert!(damp.moisture>0.0);
        f.weather_strata(20.0,-24.0).unwrap();
        let frozen=f.sample(0.0,0.0).unwrap();
        let sample=f.all_render_vertices().into_iter().find(|s|s[0]==0.0&&s[2]==0.0).unwrap();
        assert_eq!(sample[4],frozen.crust);
        assert_eq!(sample[5],frozen.moisture);
        assert!(frozen.crust>damp.crust);
        assert!(frozen.moisture<damp.moisture);
        let old=serde_json::to_value(&f).unwrap();
        let mut obj=old.as_object().unwrap().clone();
        obj.remove("strata");
        let mut old_save:SnowField=serde_json::from_value(serde_json::Value::Object(obj)).unwrap();
        old_save.validate().unwrap();
        old_save.weather_strata(1.0,-15.0).unwrap();
        assert_eq!(old_save.strata.len(),old_save.spec.depth.len());
    }
    fn drift_policy()->SnowDriftPolicy {
        SnowDriftPolicy{wind_direction:[1.0,0.0],minimum_wind:2.0,
          transport_per_second:0.001,max_fraction_per_step:0.18,
          lee_distance:7.0,lee_width:2.0,lee_shelter:0.9}
    }
    #[test]
    fn operating_generator_keeps_surface_slush_wet_until_heating_stops(){
        let mut f=field();
        let env=ThermalEnvironment{temperature:-36.0,wind:6.0,wind_loss:0.5};
        let source=warm_source("generator",0.0,true);
        let hp=heat_policy();
        f.advance_heat(1.0,&env,&[source.clone()],hp).unwrap();
        let first=f.sample(0.0,0.0).unwrap();
        assert!(first.moisture>0.0);
        f.weather_strata_with_heat(20.0,&env,&[source],hp).unwrap();
        let warm=f.sample(0.0,0.0).unwrap();
        assert_eq!(warm.crust,first.crust);
        f.weather_strata_with_heat(20.0,&env,&[],hp).unwrap();
        let frozen=f.sample(0.0,0.0).unwrap();
        assert!(frozen.crust>warm.crust);
        assert!(frozen.moisture<warm.moisture);
        assert_eq!(frozen.depth,first.depth,"refreezing does not create snow");
    }
    #[test]
    fn hardened_crust_resists_light_steps_but_cracks_under_heavy_load(){
        let mut loose=field();
        let mut hardened=field();
        let idx=2*5+2;
        hardened.strata[idx].powder=0.1;
        hardened.strata[idx].crust=0.94;
        loose.contact(0.0,0.0,1.2,0.25).unwrap();
        hardened.contact(0.0,0.0,1.2,0.25).unwrap();
        assert!(hardened.compaction[idx]<loose.compaction[idx]*0.45);
        let before=hardened.strata[idx].crust;
        hardened.contact(0.0,0.0,1.2,1.0).unwrap();
        assert!(hardened.compaction[idx]>0.1);
        assert!(hardened.strata[idx].crust<before-0.3);
        assert!(hardened.sample(0.0,0.0).unwrap().loose_depth<
            loose.sample(0.0,0.0).unwrap().loose_depth);
    }
    #[test]
    fn heat_melting_removes_fine_footprint_permanently(){
        let mut f=field();
        let stamp=SnowMicroprint{position:[0.0,0.0],forward:[0.0,1.0],
            half_width:0.11,half_length:0.22,depression:0.07};
        f.stamp_microprint(stamp).unwrap();
        let cold=ThermalEnvironment{temperature:-35.0,wind:4.0,wind_loss:0.5};
        f.advance_heat(150.0,&cold,&[warm_source("generator",0.0,true)],heat_policy()).unwrap();
        assert!(f.microprints().is_empty());
        f.advance(150.0,0.015,0.0).unwrap();
        assert!(f.microprints().is_empty(),"new snowfall never revives old tracks");
    }
    #[test]
    fn roof_runoff_creates_ground_slush_then_ice_without_changing_ground(){
        let mut f=field();
        let old_ground=f.spec.ground.clone();
        let water=f.deposit_runoff([0.0,0.0],0.6,0.18).unwrap();
        assert!(water.iter().all(|v|v.abs()<1e-6));
        let center=f.sample(0.0,0.0).unwrap();
        assert!(center.depth>1.0);
        assert!(center.standing_water_depth>0.0);
        assert!(center.slush_fraction>0.0);
        f.weather_strata(30.0,-25.0).unwrap();
        let ice=f.sample(0.0,0.0).unwrap();
        assert!(ice.ground_ice_depth>0.0);
        assert!(ice.standing_water_depth<center.standing_water_depth);
        assert_eq!(f.spec.ground,old_ground);
        let restored:SnowField=serde_json::from_str(&serde_json::to_string(&f).unwrap()).unwrap();
        restored.validate().unwrap();
        assert_eq!(restored.sample(0.0,0.0).unwrap().ground_ice_depth,ice.ground_ice_depth);
    }
    #[test]
    fn runoff_boundary_returns_unaccepted_mass(){
        let mut f=field();
        assert_eq!(f.deposit_runoff([1000.0,1000.0],4.0,2.0).unwrap(),[4.0,2.0]);
        assert!(f.deposit_runoff([0.0,0.0],f32::NAN,0.0).is_err());
    }
    #[test]
    fn fine_footprints_persist_but_do_not_change_coarse_collision(){
        let mut f=field();
        let old=f.spec.depth.clone();
        let imprint=SnowMicroprint{position:[0.13,-0.04],forward:[0.0,1.0],
          half_width:0.11,half_length:0.24,depression:0.085};
        assert!(f.stamp_microprint(imprint).unwrap());
        assert!(!f.stamp_microprint(imprint).unwrap());
        assert_eq!(f.spec.depth,old);
        let saved=serde_json::to_string(&f).unwrap();
        let mut recovered:SnowField=serde_json::from_str(&saved).unwrap();
        recovered.validate().unwrap();
        assert_eq!(recovered.microprints(),f.microprints());
        recovered.advance(12.0,0.02,0.0).unwrap();
        assert!(recovered.microprints().is_empty());
        let mut legacy=serde_json::to_value(&f).unwrap();
        legacy.as_object_mut().unwrap().remove("microprints");
        let old:SnowField=serde_json::from_value(legacy).unwrap();
        assert!(old.microprints().is_empty());
    }
    #[test]
    fn high_resolution_footprints_are_bounded_and_validated(){
        let mut f=field();
        for i in 0..300 {
            let p=SnowMicroprint{position:[-1.8+(i%20) as f32*0.17,
               -1.8+(i/20) as f32*0.24],forward:[1.0,0.0],
               half_width:0.1,half_length:0.2,depression:0.06};
            f.stamp_microprint(p).unwrap();
        }
        assert!(f.microprints().len()<=256);
        f.validate().unwrap();
        assert!(f.stamp_microprint(SnowMicroprint{position:[f32::NAN,0.0],
            forward:[1.0,0.0],half_width:0.1,half_length:0.2,
            depression:0.05}).is_err());
    }
    #[test]
    fn wind_moves_powder_without_creating_snow_or_moving_ground(){
        let mut f=field();
        let orig_ground=f.spec.ground.clone();
        let before:f64=f.spec.depth.iter().map(|x|*x as f64).sum();
        let map=f.wind_shelter_map(drift_policy(),&[]).unwrap();
        let shifted=f.advance_wind(15.0,12.0,drift_policy(),&map).unwrap();
        let after:f64=f.spec.depth.iter().map(|x|*x as f64).sum();
        assert!(shifted>0);
        assert!((before-after).abs()<0.00001,"wind must conserve total snow mass");
        assert_eq!(f.spec.ground,orig_ground);
        assert!(f.spec.depth.iter().all(|d|*d>=0.0&&*d<=20.0));
        let saved=serde_json::to_string(&f).unwrap();
        let resumed:SnowField=serde_json::from_str(&saved).unwrap();
        resumed.validate().unwrap();
        assert_eq!(resumed.spec.depth,f.spec.depth);
    }
    #[test]
    fn wind_shadow_protects_downwind_surface_and_crust_blocks_transport(){
        let mut f=field();
        f.spec.depth.fill(0.0);
        f.spec.depth[2*5+3]=1.0; // isolate outgoing wind erosion from incoming deposition
        let p=drift_policy();
        let shaded=f.wind_shelter_map(p,&[SnowWindObstacle{
            position:[-1.0,0.0],radius:0.8
        }]).unwrap();
        let unshaded=f.wind_shelter_map(p,&[]).unwrap();
        assert!(shaded[2*5+3]>unshaded[2*5+3]);
        let mut exposed=f.clone();
        let mut sheltered=f.clone();
        exposed.advance_wind(10.0,15.0,p,&unshaded).unwrap();
        sheltered.advance_wind(10.0,15.0,p,&shaded).unwrap();
        // More protection means less removal from the downwind cell.
        assert!(sheltered.spec.depth[2*5+3]>=exposed.spec.depth[2*5+3]);
        let mut icy=f;
        icy.compaction.fill(1.0);
        assert_eq!(icy.advance_wind(10.0,15.0,p,&unshaded).unwrap(),0);
        icy.compaction.fill(0.0);
        for layer in &mut icy.strata{layer.crust=1.0;}
        assert_eq!(icy.advance_wind(10.0,15.0,p,&unshaded).unwrap(),0);
        for layer in &mut icy.strata{layer.crust=0.0;layer.moisture=1.0;}
        assert_eq!(icy.advance_wind(10.0,15.0,p,&unshaded).unwrap(),0);
        assert!(icy.wind_shelter_map(p,&[SnowWindObstacle{
            position:[0.0,0.0],radius:-1.0}]).is_err());
    }
    #[test]
    fn old_snow_policies_load_with_ridge_disabled(){
        let p:SnowPolicy=serde_json::from_str(
          r#"{"compacted_fraction":0.3,"contact_compaction":0.5,"max_contact_radius":2.0}"#)
          .unwrap();
        assert_eq!(p.ridge_displacement_fraction,0.0);
        p.validate().unwrap();
    }
    #[test]
    fn malformed_grids_and_contacts_fail_without_mutation() {
        let mut f = field();
        assert!(f.contact(f32::NAN, 0.0, 1.0, 1.0).is_err());
        assert!(f.sample(3.0, 0.0).is_none());
        f.spec.depth.pop();
        assert!(f.validate().is_err());
    }
}
