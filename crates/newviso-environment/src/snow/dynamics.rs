//! Conservative, terrain-aware snow transport and repose relaxation.
//! This is a bounded heightfield approximation, not a CFD/MPM solver.
use super::{SnowDriftPolicy, SnowField, SnowStrata};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnowDynamicsPolicy {
    pub dry_repose_degrees: f32,
    pub wet_repose_degrees: f32,
    pub crust_cohesion_metres: f32,
    pub relaxation_per_second: f32,
    pub saltation_length_metres: f32,
    pub terrain_shadow_distance: f32,
    pub wind_packing_per_second: f32,
    pub substep_seconds: f32,
}
impl SnowDynamicsPolicy {
    pub fn validate(&self) -> Result<(), String> {
        let checks = [
            (self.dry_repose_degrees, 15.0, 60.0),
            (self.wet_repose_degrees, 10.0, 60.0),
            (self.crust_cohesion_metres, 0.0, 2.0),
            (self.relaxation_per_second, 0.0, 5.0),
            (self.saltation_length_metres, 0.1, 12.0),
            (self.terrain_shadow_distance, 1.0, 40.0),
            (self.wind_packing_per_second, 0.0, 0.05),
            (self.substep_seconds, 0.5, 2.0),
        ];
        if checks.iter().any(|(v, lo, hi)| !v.is_finite() || v < lo || v > hi) {
            return Err("invalid snow dynamics policy".into());
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct SnowDynamicsReport {
    pub wind_volume_m3: f64,
    pub slide_volume_m3: f64,
    pub changed_cells: usize,
    pub substeps: usize,
}
#[derive(Clone, Copy, Debug)]
struct Transfer {
    from: usize,
    to: usize,
    depth: f32,
    // Extensive material channels: powder, crust, moisture, compaction.
    material: [f32; 4],
}
#[derive(Clone, Debug)]
pub(super) struct Scratch {
    transfers: Vec<Transfer>,
    outgoing: Vec<f64>,
    incoming: Vec<f64>,
    accepted: Vec<f64>,
    delta: Vec<[f64; 5]>,
    heights: Vec<f32>,
    previous: Vec<f32>,
    wind_kernels: Option<WindKernels>,
}
impl Scratch {
    fn new(n: usize) -> Self {
        Self { transfers: Vec::new(), outgoing: vec![0.0; n],
            incoming: vec![0.0; n], accepted: vec![0.0; n], delta: vec![[0.0; 5]; n], heights: vec![0.0; n], previous:vec![0.0;n],wind_kernels:None }
    }
    fn clear(&mut self) {
        self.transfers.clear();
        self.outgoing.fill(0.0);
        self.incoming.fill(0.0);
        self.accepted.fill(0.0);
        self.delta.fill([0.0; 5]);
    }
}
/// Sampling coordinates are separable on a regular grid. Compute the exact
/// clamped f32 coordinates once per row/column, instead of per cell and probe.
#[derive(Clone,Debug)]
struct HeightSampleKernel {
    columns: Vec<(usize,f32)>,
    rows: Vec<(usize,f32)>,
}
impl HeightSampleKernel {
    fn new(columns:usize,rows:usize,shift:[f32;2])->Self {
        let coordinates=|count:usize,offset:f32| (0..count).map(|i| {
            let p=(i as f32-offset).clamp(0.0,(count-1) as f32);
            let base=(p.floor() as usize).min(count-2);
            (base,p-base as f32)
        }).collect();
        Self {columns:coordinates(columns,shift[0]),rows:coordinates(rows,shift[1])}
    }
    fn sample(&self,heights:&[f32],col:usize,row:usize,n:usize)->f32 {
        let (x,u)=self.columns[col];let (z,v)=self.rows[row];let a=z*n+x;
        heights[a]*(1.0-u)*(1.0-v)+heights[a+1]*u*(1.0-v)+
            heights[a+n]*(1.0-u)*v+heights[a+n+1]*u*v
    }
}

#[derive(Clone,Debug)]
struct WindKernels {
    key:[u32;6],
    upwind:HeightSampleKernel,
    probes:[HeightSampleKernel;3],
    distances:[f32;3],
}
impl WindKernels {
    fn new(n:usize,rows:usize,direction:[f32;2],distance:f32,cell:f32)->Self{
        let distances=[distance*0.25,distance*0.5,distance];
        Self{key:[n as u32,rows as u32,direction[0].to_bits(),direction[1].to_bits(),distance.to_bits(),cell.to_bits()],
            upwind:HeightSampleKernel::new(n,rows,direction),
            probes:distances.map(|d|HeightSampleKernel::new(n,rows,[direction[0]*d/cell,direction[1]*d/cell])),
            distances}
    }
}

impl SnowField {
    pub(super) fn transport_height(&self, i: usize) -> f32 {
        self.spec.ground[i] + self.spec.depth[i] *
            (1.0 - self.compaction[i] * (1.0 - self.policy.compacted_fraction))
    }
    // Bilinear sampling of the SAME surface used by render/collision.
    #[cfg(test)]
    fn transport_height_at(&self, heights: &[f32], x: f32, z: f32) -> f32 {
        let n = self.spec.columns;
        let x = x.clamp(0.0, (n - 1) as f32);
        let z = z.clamp(0.0, (self.spec.rows - 1) as f32);
        let col = (x.floor() as usize).min(n - 2);
        let row = (z.floor() as usize).min(self.spec.rows - 2);
        let u = x - col as f32;
        let v = z - row as f32;
        let a = row * n + col;
        heights[a] * (1.0-u) * (1.0-v) +
            heights[a+1] * u * (1.0-v) +
            heights[a+n] * (1.0-u) * v +
            heights[a+n+1] * u * v
    }
    // All proposals are computed before commit. Aggregate receiver budgets
    // prevent several diagonal/uphill flows from overfilling a single cell.
    fn commit_transport(&mut self, scratch: &mut Scratch) -> f64 {
        for t in &scratch.transfers {
            scratch.outgoing[t.from] += t.depth as f64;
            scratch.incoming[t.to] += t.depth as f64;
        }
        // Resolve each cell budget once, rather than two divisions for
        // every edge. The common unsaturated case needs no division.
        for i in 0..scratch.outgoing.len() {
            let depth=self.spec.depth[i] as f64;
            scratch.outgoing[i]=if scratch.outgoing[i]<=depth {1.0}
                else {depth/scratch.outgoing[i]};
            let capacity=(20.0-depth).max(0.0);
            scratch.incoming[i]=if scratch.incoming[i]<=capacity {1.0}
                else {capacity/scratch.incoming[i]};
        }
        let mut moved = 0.0;
        for t in &scratch.transfers {
            let donor = scratch.outgoing[t.from];
            let receiver = scratch.incoming[t.to];
            let amount = t.depth as f64 * donor.min(receiver);
            if amount <= 1e-10 { continue; }
            moved += amount;
            scratch.accepted[t.to] += amount;
            scratch.delta[t.from][0] -= amount;
            scratch.delta[t.to][0] += amount;
            if t.material==[1.0,0.0,0.0,0.0] {
                scratch.delta[t.from][1]-=amount;
                scratch.delta[t.to][1]+=amount;
            } else {
                for k in 0..4 {
                    let channel = amount * t.material[k] as f64;
                    scratch.delta[t.from][k+1] -= channel;
                    scratch.delta[t.to][k+1] += channel;
                }
            }
        }
        for (i, delta) in scratch.delta.iter().enumerate() {
            if delta.iter().all(|x| x.abs() < 1e-12) { continue; }
            let old = self.spec.depth[i] as f64;
            let next = (old+delta[0]).clamp(0.0, 20.0);
            if self.layers_enabled(){
                let incoming=scratch.accepted[i];
                let outgoing=(incoming-delta[0]).max(0.0);
                let top=self.column_top(i).material();
                let material=if incoming>1e-10 {std::array::from_fn(|k|
                    ((delta[k+1]+outgoing*top[k] as f64)/incoming).clamp(0.0,1.0) as f32)}else{[0.0;4]};
                self.column_transport(i,outgoing as f32,incoming as f32,material);
                self.dirty.insert(i);continue;
            }
            let layer = self.strata[i];
            let before = [layer.powder, layer.crust, layer.moisture, self.compaction[i]];
            let mut after = [0.0; 4];
            if next > 1e-10 {
                for k in 0..4 {
                    after[k] = ((old*before[k] as f64+delta[k+1])/next)
                        .clamp(0.0, 1.0) as f32;
                }
            }
            self.spec.depth[i] = next as f32;
            self.strata[i] = SnowStrata {
                powder: after[0], crust: after[1], moisture: after[2]
            };
            self.compaction[i] = after[3];
            self.dirty.insert(i);
        }
        moved * (self.spec.cell_size as f64).powi(2)
    }
    fn wind_transport_step(&mut self, h: f32, wind: f32,
        drift: SnowDriftPolicy, shelter: &[f32], policy: SnowDynamicsPolicy,
        scratch: &mut Scratch) -> f64 {
        scratch.clear();
        if wind <= drift.minimum_wind || drift.transport_per_second == 0.0 { return 0.0; }
        let n = self.spec.columns;
        let rows = self.spec.rows;
        let cell = self.spec.cell_size;
        let norm = drift.wind_direction[0].hypot(drift.wind_direction[1]);
        let direction = drift.wind_direction.map(|v| v/norm);
        let key=[n as u32,rows as u32,direction[0].to_bits(),direction[1].to_bits(),
            policy.terrain_shadow_distance.to_bits(),cell.to_bits()];
        if scratch.wind_kernels.as_ref().is_none_or(|k|k.key!=key){
            scratch.wind_kernels=Some(WindKernels::new(n,rows,direction,policy.terrain_shadow_distance,cell));
        }
        let kernels=scratch.wind_kernels.as_ref().unwrap();
        for (i,height) in scratch.heights.iter_mut().enumerate() {*height=self.transport_height(i);}
        for i in 0..self.spec.depth.len() {
            let top=if self.layers_enabled(){self.column_top(i)}else{
                let l=self.strata[i];super::layers::Layer::new(self.spec.depth[i],
                    [l.powder,l.crust,l.moisture,self.compaction[i]])};
            let m=top.material();
            let mobile = top.depth() * m[0] * (1.0-m[3]) * (1.0-m[1]).powi(2) * (1.0-m[2]);
            if mobile < 1e-6 { continue; }
            let col = (i % n) as f32;
            let row = (i / n) as f32;
            let height = scratch.heights[i];
            // Crest exposure and lee separation. No terrain cache is reused
            // after the snow surface itself has changed.
            let up = kernels.upwind.sample(&scratch.heights,i%n,i/n,n);
            let slope = ((height-up)/cell).clamp(-0.5, 0.5);
            let mut shadow = 0.0_f32;
            for (probe,distance) in kernels.probes.iter().zip(kernels.distances) {
                let sample = probe.sample(&scratch.heights,i%n,i/n,n);
                shadow = shadow.max(((sample-height)/distance-0.08).max(0.0));
            }
            let local_wind = wind * (1.0-shelter[i]) *
                (1.0+0.65*slope) * (1.0/(1.0+8.0*shadow));
            if local_wind <= drift.minimum_wind { continue; }
            // Thresholded quadratic stress response; rates remain authored,
            // rather than pretending empirical coefficients are universal.
            let rate = drift.transport_per_second * local_wind *
                (1.0-(drift.minimum_wind/local_wind).powi(2));
            let fraction = (1.0-(-rate*h).exp())
                .min(drift.max_fraction_per_step*h).min(0.25);
            let amount = mobile*fraction;
            let hop = (policy.saltation_length_metres *
                (local_wind/drift.minimum_wind.max(1.0)).clamp(1.0, 3.0)/cell)
                .clamp(0.25, 16.0);
            let x = col+direction[0]*hop;
            let z = row+direction[1]*hop;
            let x0 = x.floor() as isize;
            let z0 = z.floor() as isize;
            let u = x-x.floor();
            let v = z-z.floor();
            for (dx, dz, w) in [(0,0,(1.0-u)*(1.0-v)),
                (1,0,u*(1.0-v)), (0,1,(1.0-u)*v), (1,1,u*v)] {
                let c = x0+dx;
                let r = z0+dz;
                // Closed boundaries retain the rejected share at its donor.
                if c < 0 || c >= n as isize || r < 0 || r >= rows as isize { continue; }
                let to = r as usize*n+c as usize;
                if to == i || amount*w < 1e-9 { continue; }
                scratch.transfers.push(Transfer {
                    from:i, to, depth:amount*w, material:if self.layers_enabled(){m}else{[1.0,0.0,0.0,0.0]}
                });
            }
        }
        let moved = self.commit_transport(scratch);
        // Wind deposits evolve into firmer wind-packed snow. This changes
        // porosity/height, not the stored snow-volume budget.
        for i in 0..self.spec.depth.len() {
            let arriving = (scratch.accepted[i] as f32).min(self.spec.depth[i]);
            if arriving <= 0.0 || self.spec.depth[i] <= 0.0 { continue; }
            let packing = (policy.wind_packing_per_second*h*(wind/15.0).min(3.0) *
                arriving/self.spec.depth[i]).min(0.05);
            if packing > 0.0 {
                if self.layers_enabled(){self.column_pack(i,packing);}
                else{
                    self.compaction[i] = self.compaction[i].max((self.compaction[i]+packing).min(0.85));
                    self.strata[i].powder = (self.strata[i].powder-packing).max(0.0);
                }
                self.dirty.insert(i);
            }
        }
        moved
    }
    fn gravity_transport_step(&mut self, h: f32, policy: SnowDynamicsPolicy,
        scratch: &mut Scratch) -> f64 {
        scratch.clear();
        if policy.relaxation_per_second == 0.0 { return 0.0; }
        let n = self.spec.columns;
        let rows = self.spec.rows;
        let response = 1.0-(-policy.relaxation_per_second*h).exp();
        let dry_limit=policy.dry_repose_degrees.to_radians().tan();
        let neighbors=[(-1_isize,-1_isize,std::f32::consts::SQRT_2),
            (0,-1,1.0),(1,-1,std::f32::consts::SQRT_2),(-1,0,1.0),
            (1,0,1.0),(-1,1,std::f32::consts::SQRT_2),(0,1,1.0),
            (1,1,std::f32::consts::SQRT_2)];
        for (i,height) in scratch.heights.iter_mut().enumerate() {*height=self.transport_height(i);}
        for i in 0..self.spec.depth.len() {
            let depth = self.spec.depth[i];
            if depth < 1e-6 { continue; }
            let top=if self.layers_enabled(){self.column_top(i)}else{
                let l=self.strata[i];super::layers::Layer::new(depth,[l.powder,l.crust,l.moisture,self.compaction[i]])};
            let m=top.material();
            let layer=SnowStrata{powder:m[0],crust:m[1],moisture:m[2]};
            let angle = policy.dry_repose_degrees +
                (policy.wet_repose_degrees-policy.dry_repose_degrees)*layer.moisture;
            let limit = if layer.moisture==0.0 {dry_limit} else {angle.to_radians().tan()};
            let source_scale = 1.0-m[3]*(1.0-self.policy.compacted_fraction);
            let mut best: Option<(usize, f32)> = None;
            let col = (i%n) as isize;
            let row = (i/n) as isize;
            for (dx,dz,distance_scale) in neighbors {
                let c = col+dx;
                let r = row+dz;
                if c < 0 || c >= n as isize || r < 0 || r >= rows as isize { continue; }
                let j = r as usize*n+c as usize;
                let distance = self.spec.cell_size*distance_scale;
                let excess = scratch.heights[i]-scratch.heights[j] -
                    limit*distance-policy.crust_cohesion_metres*layer.crust;
                if excess<=0.0 {continue;}
                let dest_scale = if self.layers_enabled(){source_scale}
                    else{1.0-self.compaction[j]*(1.0-self.policy.compacted_fraction)};
                let amount = (excess/(source_scale+dest_scale)).max(0.0)*response;
                if best.is_none_or(|(_,a)| amount > a) { best = Some((j,amount)); }
            }
            if let Some((to, amount)) = best {
                let mobile = top.depth()*(1.0-0.85*layer.crust)*(1.0-0.7*m[3]);
                let amount = amount.min(mobile).min(depth*0.25);
                if amount > 1e-9 {
                    scratch.transfers.push(Transfer { from:i, to, depth:amount,
                        material:m });
                }
            }
        }
        self.commit_transport(scratch)
    }
    /// Updates ground snow using fixed bounded substeps. Reject long pauses
    /// before mutation; callers can explicitly chunk offline simulation.
    pub fn advance_dynamics(&mut self, dt: f64, wind: f32,
        drift: SnowDriftPolicy, shelter: &[f32], policy: SnowDynamicsPolicy)
        -> Result<SnowDynamicsReport, String> {
        policy.validate()?;
        drift.validate()?;
        if !dt.is_finite() || !(0.0..=60.0).contains(&dt) ||
            !wind.is_finite() || !(0.0..=200.0).contains(&wind) ||
            shelter.len()!=self.spec.depth.len() ||
            shelter.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)) {
            return Err("invalid snow dynamics step, wind or shelter".into());
        }
        let mut report = SnowDynamicsReport::default();
        if dt == 0.0 { return Ok(report); }
        self.ensure_strata();
        let steps = (dt/policy.substep_seconds as f64).ceil() as usize;
        let h = (dt/steps as f64) as f32;
        let mut scratch=self.dynamics_scratch.take()
            .filter(|s|s.heights.len()==self.spec.depth.len())
            .unwrap_or_else(||Box::new(Scratch::new(self.spec.depth.len())));
        // Count cells altered by this invocation, without draining render dirt.
        scratch.previous.copy_from_slice(&self.spec.depth);
        for _ in 0..steps {
            report.wind_volume_m3 += self.wind_transport_step(h,wind,drift,shelter,policy,&mut scratch);
            report.slide_volume_m3 += self.gravity_transport_step(h,policy,&mut scratch);
        }
        report.changed_cells = self.spec.depth.iter().zip(scratch.previous.iter())
            .filter(|(a,b)| a!=b).count();
        report.substeps = steps;
        // Local erosion/deposition must age fine footprints as well as mesh.
        let marks = std::mem::take(&mut self.microprints);
        self.microprints = marks.into_iter().filter_map(|mut p| {
            let x = ((p.position[0]-self.spec.origin[0])/self.spec.cell_size).round() as usize;
            let z = ((p.position[1]-self.spec.origin[1])/self.spec.cell_size).round() as usize;
            let i = z*self.spec.columns+x;
            let buried = (self.spec.depth[i]-scratch.previous[i]).abs();
            p.depression = (p.depression-buried).max(0.0);
            let s = self.sample(p.position[0],p.position[1])?;
            (s.depth > 0.055 && p.depression > 0.002).then_some(p)
        }).collect();
        self.dynamics_scratch=Some(scratch);
        Ok(report)
    }
}

#[cfg(test)]
mod tests;
