//! Precipitation is the only external snow source; runoff transfers existing volume.
use super::*;
#[derive(Clone,Copy,Debug,Default,Serialize)]
pub struct SnowWeatherReport {
    pub snowfall_volume_m3:f64,
    pub melted_volume_m3:f64,
    pub outflow_volume_m3:f64,
}
impl SnowField {
    /// Stored volume proxy shared by snow, liquid and ice. Compaction changes
    /// porosity and height, never this budget.
    pub fn stored_volume_m3(&self)->f64 {
        let area=(self.spec.cell_size as f64).powi(2);
        self.spec.depth.iter().chain(self.surface_water.iter()).chain(self.surface_ice.iter())
            .map(|v|*v as f64).sum::<f64>()*area
    }
    pub fn advance(&mut self,dt:f64,snowfall:f32,melt:f32)->Result<(),String>{
        self.advance_exposed(dt,snowfall,melt,None).map(|_|())
    }
    /// Exposed fractions already account for snow intercepted by higher
    /// surfaces. Melting remains independent of precipitation interception.
    pub fn advance_exposed(&mut self,dt:f64,snowfall:f32,melt:f32,
        exposure:Option<&[f32]>)->Result<SnowWeatherReport,String>{
        crate::valid_dt(dt)?;
        if ![snowfall,melt].iter().all(|v|v.is_finite()&&(0.0..=0.05).contains(v))
            || exposure.is_some_and(|v|v.len()!=self.spec.depth.len()||
                v.iter().any(|x|!x.is_finite()||!(0.0..=1.0).contains(x))) {
            return Err("invalid snow weather rates or exposure".into());
        }
        let mut report=SnowWeatherReport::default();
        if dt==0.0||(snowfall==0.0&&melt==0.0){return Ok(report);}
        self.ensure_strata();self.ensure_liquids();
        let fresh=(snowfall as f64*dt) as f32;
        let thaw=(melt as f64*dt) as f32;
        let area=(self.spec.cell_size as f64).powi(2);
        let layered=self.layers_enabled();
        for i in 0..self.spec.depth.len() {
            let incoming=fresh*exposure.map_or(1.0,|v|v[i]);
            let old=self.spec.depth[i];
            let added=incoming.min((20.0-old).max(0.0));
            report.snowfall_volume_m3+=incoming as f64*area;
            report.outflow_volume_m3+=(incoming-added) as f64*area;
            if added>0.0 {
                if layered {self.column_deposit(i,added,[1.0,0.0,0.0,0.0]);}
                else {
                    let next=old+added;
                    self.compaction[i]*=old/next;
                    let l=&mut self.strata[i];
                    l.powder=((l.powder*old+added)/next).clamp(0.0,1.0);
                    l.crust=(l.crust*old/next).clamp(0.0,1.0);
                    l.moisture=(l.moisture*old/next).clamp(0.0,1.0);
                    self.spec.depth[i]=next;
                }
            }
            let removed=if thaw>0.0 {
                if layered {self.column_melt(i,thaw)}
                else {
                    let amount=thaw.min(self.spec.depth[i]);
                    self.spec.depth[i]-=amount;
                    if self.spec.depth[i]<=0.0 {
                        self.strata[i]=SnowStrata{powder:0.0,crust:0.0,moisture:0.0};
                    }else if amount>0.0 {
                        self.strata[i].moisture=(self.strata[i].moisture+
                            amount*0.25/self.spec.depth[i]).min(1.0);
                    }
                    amount
                }
            }else{0.0};
            if removed>0.0 {
                let accepted=removed.min((2.0-self.surface_water[i]).max(0.0));
                self.surface_water[i]+=accepted;
                report.melted_volume_m3+=removed as f64*area;
                report.outflow_volume_m3+=(removed-accepted) as f64*area;
                self.liquid_dirty.insert(i);
            }
            if added>0.0||removed>0.0 {self.dirty.insert(i);}
        }
        if fresh>0.0 {
            let spec=&self.spec;
            for p in &mut self.microprints {
                let x=((p.position[0]-spec.origin[0])/spec.cell_size).round() as usize;
                let z=((p.position[1]-spec.origin[1])/spec.cell_size).round() as usize;
                let i=z*spec.columns+x;
                p.depression=(p.depression-fresh*exposure.map_or(1.0,|v|v[i])).max(0.0);
            }
            self.microprints.retain(|p|p.depression>0.002);
        }
        Ok(report)
    }
    pub fn deposit_runoff(&mut self,position:[f32;2],snow:f32,water:f32)
        ->Result<[f32;2],String>{
        self.deposit_surface_runoff(position,[snow,water,0.0]).map(|r|[r[0],r[1]])
    }
    /// A transfer supplied by the donating surface, never a weather source.
    /// Rejected volume (outside the field or at capacity) remains explicit.
    pub fn deposit_surface_runoff(&mut self,position:[f32;2],volumes:[f32;3])
        ->Result<[f32;3],String>{
        if !position.iter().all(|v|v.is_finite())||
            volumes.iter().any(|v|!v.is_finite()||!(0.0..=1000.0).contains(v)){
            return Err("invalid snow runoff".into());
        }
        if volumes==[0.0;3]{return Ok(volumes);}
        let last_x=self.spec.origin[0]+(self.spec.columns-1) as f32*self.spec.cell_size;
        let last_z=self.spec.origin[1]+(self.spec.rows-1) as f32*self.spec.cell_size;
        let margin=self.spec.cell_size*1.5;
        if position[0]<self.spec.origin[0]-margin||position[0]>last_x+margin||
            position[1]<self.spec.origin[1]-margin||position[1]>last_z+margin{return Ok(volumes);}
        self.ensure_strata();self.ensure_liquids();
        let cx=((position[0]-self.spec.origin[0])/self.spec.cell_size).round() as isize;
        let cz=((position[1]-self.spec.origin[1])/self.spec.cell_size).round() as isize;
        let mut targets=[0_usize;9];let mut count=0;
        for dz in -1..=1 {for dx in -1..=1 {
            let x=cx+dx;let z=cz+dz;
            if x>=0&&x<self.spec.columns as isize&&z>=0&&z<self.spec.rows as isize{
                targets[count]=z as usize*self.spec.columns+x as usize;count+=1;
            }
        }}
        let targets=&targets[..count];
        let before=std::array::from_fn::<f32,9,_>(|k|if k<count{self.spec.depth[targets[k]]}else{0.0});
        let area=self.spec.cell_size*self.spec.cell_size;
        let mut remaining=volumes;
        let layered=self.layers_enabled();
        for phase in 0..3 {
            for _ in 0..=count {
                if remaining[phase]<=0.000001{break;}
                let capacity=|f:&Self,i:usize|match phase{
                    0=>20.0-f.spec.depth[i],1=>2.0-f.surface_water[i],_=>2.0-f.surface_ice[i]};
                let available=targets.iter().filter(|i|capacity(self,**i)>0.000001).count();
                if available==0{break;}
                let quota=remaining[phase]/available as f32/area;
                let mut used=0.0;
                for &i in targets {
                    let amount=quota.min(capacity(self,i).max(0.0));
                    if amount<=0.0{continue;}
                    match phase {
                        0=>if layered{self.column_deposit(i,amount,[1.0,0.0,0.0,0.0]);}
                        else{
                            let old=self.spec.depth[i];let next=old+amount;
                            self.compaction[i]*=old/next;
                            self.strata[i].powder=((self.strata[i].powder*old+amount)/next).clamp(0.0,1.0);
                            self.strata[i].crust*=old/next;self.strata[i].moisture*=old/next;
                            self.spec.depth[i]=next;
                        },
                        1=>{
                            self.surface_water[i]+=amount;self.liquid_dirty.insert(i);
                            if self.spec.depth[i]>0.0{
                                if layered{self.column_wet(i,amount);}
                                else{self.strata[i].moisture=(self.strata[i].moisture+
                                    amount/self.spec.depth[i]).min(1.0);}
                            }
                        },
                        _=>{self.surface_ice[i]+=amount;self.liquid_dirty.insert(i);}
                    }
                    self.dirty.insert(i);used+=amount*area;
                }
                remaining[phase]=(remaining[phase]-used).max(0.0);
                if used<=0.0000001{break;}
            }
        }
        if volumes[0]>0.0&&!self.microprints.is_empty(){
            let spec=&self.spec;
            for p in &mut self.microprints{
                let x=((p.position[0]-spec.origin[0])/spec.cell_size).round() as usize;
                let z=((p.position[1]-spec.origin[1])/spec.cell_size).round() as usize;
                if let Some(k)=targets.iter().position(|i|*i==z*spec.columns+x){
                    let deposited=(spec.depth[targets[k]]-before[k]).max(0.0);
                    p.depression=(p.depression-deposited).max(0.0);
                }
            }
            self.microprints.retain(|p|p.depression>0.002);
        }
        Ok(remaining)
    }
}
