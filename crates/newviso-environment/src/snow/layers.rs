//! Bounded vertical snow columns. Depth is the engine's existing conserved
//! snow-volume proxy, not kilograms; compaction changes porosity, not that budget.
use super::{SnowField, SnowStrata, age_snow_strata};
use serde::{Deserialize, Serialize, Serializer, Deserializer};
use serde::de::{Visitor, SeqAccess, Error};
use std::fmt;
const MAX_LAYERS:usize=4;
#[derive(Clone,Copy,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnowLayerPolicy {
    /// Physical depth compressed by one contact; lower snow remains intact.
    pub contact_depth_metres:f32,
    /// Surface appearance blends through a thin dusting without losing buried crust.
    pub surface_blend_metres:f32,
}
impl SnowLayerPolicy {
    pub fn validate(self)->Result<(),String>{
        if !self.contact_depth_metres.is_finite() ||
            !(0.02..=1.5).contains(&self.contact_depth_metres) ||
            !self.surface_blend_metres.is_finite() ||
            !(0.002..=0.15).contains(&self.surface_blend_metres) {
            return Err("invalid snow layer policy".into());
        }
        Ok(())
    }
}
/// Compact checkpoint tuple: [depth, compaction, powder, crust, moisture].
#[derive(Clone,Copy,Debug,Default,PartialEq,Serialize,Deserialize)]
pub(super) struct Layer(pub [f32;5]);
impl Layer {
    pub fn new(depth:f32,material:[f32;4])->Self {Self([depth,material[3],material[0],material[1],material[2]])}
    pub fn depth(self)->f32{self.0[0]}
    pub fn material(self)->[f32;4]{[self.0[2],self.0[3],self.0[4],self.0[1]]}
    fn height(self,f:f32)->f32{self.depth()*(1.0-self.0[1]*(1.0-f))}
    fn valid(self)->bool{
        self.depth().is_finite() && self.depth()>0.0 && self.depth()<=20.001 &&
            self.0[1..].iter().all(|v|v.is_finite()&&(0.0..=1.0).contains(v))
    }
    fn merge(self,other:Self)->Self{
        let depth=self.depth()+other.depth();
        let a=self.material();let b=other.material();
        Self::new(depth,std::array::from_fn(|k|
            if a[k]==b[k]{a[k]}else{((a[k]*self.depth()+b[k]*other.depth())/depth).clamp(0.0,1.0)}))
    }
}
/// Contiguous, fixed capacity: no per-cell heap allocation. Top is index zero.
#[derive(Clone,Copy,Debug,PartialEq)]
pub(super) struct Column {layers:[Layer;MAX_LAYERS],len:u8}
impl Default for Column {fn default()->Self{Self{layers:[Layer::default();MAX_LAYERS],len:0}}}
impl Serialize for Column {
    fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{
        self.layers[..self.len as usize].serialize(s)
    }
}
impl<'de> Deserialize<'de> for Column {
    fn deserialize<D:Deserializer<'de>>(d:D)->Result<Self,D::Error>{
        struct ColumnVisitor;
        impl<'de> Visitor<'de> for ColumnVisitor {
            type Value=Column;
            fn expecting(&self,f:&mut fmt::Formatter)->fmt::Result{f.write_str("at most four snow layers")}
            fn visit_seq<A:SeqAccess<'de>>(self,mut seq:A)->Result<Column,A::Error>{
                let mut c=Column::default();
                while let Some(layer)=seq.next_element::<Layer>()? {
                    if c.len as usize==MAX_LAYERS{return Err(A::Error::custom("snow column exceeds four layers"));}
                    if !layer.valid(){return Err(A::Error::custom("invalid snow layer"));}
                    c.layers[c.len as usize]=layer;c.len+=1;
                }
                Ok(c)
            }
        }
        d.deserialize_seq(ColumnVisitor)
    }
}
impl Column {
    pub fn top(&self)->Layer{if self.len==0{Layer::default()}else{self.layers[0]}}
    fn depth(&self)->f32{self.layers[..self.len as usize].iter().map(|l|l.depth()).sum()}
    fn merge_pair(&mut self,i:usize){
        let n=self.len as usize;
        self.layers[i]=self.layers[i].merge(self.layers[i+1]);
        for j in i+2..n{self.layers[j-1]=self.layers[j];}
        self.layers[n-1]=Layer::default();self.len-=1;
    }
    fn merge_bottom(&mut self){self.merge_pair(self.len as usize-2);}
    fn make_contact_room(&mut self){
        // Preserve the untouched basal layer when successive footsteps split
        // a contact boundary. Merge the closest material pair above it.
        let mut best=0;let mut score=f32::MAX;
        for i in 0..self.len as usize-2{
            let a=self.layers[i].material();let b=self.layers[i+1].material();
            let difference=(0..4).map(|k|(a[k]-b[k]).abs()).sum::<f32>();
            if difference<score{score=difference;best=i;}
        }
        self.merge_pair(best);
    }
    pub fn deposit(&mut self,layer:Layer){
        if layer.depth()<=0.0{return;}
        if self.len>0 {
            let a=self.top().material();let b=layer.material();
            // Consecutive nearly-identical deposits are one event, not thousands
            // of tiny snowfall layers. A material boundary remains separate.
            if (0..4).all(|k|(a[k]-b[k]).abs()<0.035) {
                self.layers[0]=self.layers[0].merge(layer);return;
            }
        }
        if self.len as usize==MAX_LAYERS{self.merge_bottom();}
        for i in (0..self.len as usize).rev(){self.layers[i+1]=self.layers[i];}
        self.layers[0]=layer;self.len+=1;
    }
    pub fn remove_top(&mut self,mut amount:f32)->Layer{
        let mut removed=Layer::default();
        while amount>0.0 && self.len>0 {
            let top=self.top();let take=amount.min(top.depth());
            let piece=Layer::new(take,top.material());
            removed=if removed.depth()>0.0{removed.merge(piece)}else{piece};
            amount=(amount-take).max(0.0);
            if take>=top.depth(){
                let n=self.len as usize;
                for i in 1..n{self.layers[i-1]=self.layers[i];}
                self.layers[n-1]=Layer::default();self.len-=1;
            }else{self.layers[0].0[0]-=take;}
        }
        removed
    }
    /// Split at a physical depth before contact or surface freeze. Merge only
    /// the deepest pair if needed; no mass/material/height is discarded.
    fn split_at_height(&mut self,height:f32,f:f32)->usize{
        for attempt in 0..2 {
            let mut remaining=height;
            for i in 0..self.len as usize {
                let h=self.layers[i].height(f);
                if remaining>=h-1e-6 {remaining-=h;continue;}
                if remaining<=1e-6{return i;}
                if self.len as usize==MAX_LAYERS {
                    if attempt==0{self.make_contact_room();break;}
                    return i+1;
                }
                let layer=self.layers[i];let upper=layer.depth()*remaining/h;
                for j in (i+1..self.len as usize).rev(){self.layers[j+1]=self.layers[j];}
                self.layers[i]=Layer::new(upper,layer.material());
                self.layers[i+1]=Layer::new(layer.depth()-upper,layer.material());
                self.len+=1;
                return i+1;
            }
        }
        self.len as usize
    }
    fn surface(&self,blend:f32,f:f32)->SnowStrata{
        let mut left=blend;let mut weight=0.0;let mut channels=[0.0;3];
        for layer in &self.layers[..self.len as usize] {
            let h=layer.height(f).min(left);let m=layer.material();
            for k in 0..3{channels[k]+=h*m[k];}
            weight+=h;left-=h;if left<=0.0{break;}
        }
        if weight<=0.0{return SnowStrata{powder:0.0,crust:0.0,moisture:0.0};}
        SnowStrata{powder:channels[0]/weight,crust:channels[1]/weight,moisture:channels[2]/weight}
    }
    fn loose_depth(&self)->f32{
        let mut depth=0.0;let mut transmission=1.0;
        for layer in &self.layers[..self.len as usize] {
            let m=layer.material();
            depth+=transmission*layer.depth()*(1.0-m[3])*m[0]*(1.0-0.75*m[1]);
            // Intact crust supports the load and shields porous snow below.
            transmission*=1.0-m[1]*(1.0-m[2])*0.95;
        }
        depth
    }
    fn compact(&mut self,height:f32,load:f32,contact_compaction:f32,f:f32)->f32{
        let count=self.split_at_height(height,f);
        let mut transmission=1.0;let mut changed_depth=0.0;
        for layer in &mut self.layers[..count] {
            let old=layer.0[1];let shell=layer.0[3]*(1.0-0.45*layer.0[4]);
            let local=load*transmission;
            let breakthrough=(local-(0.25+0.4*shell)).max(0.0)/(0.75-0.4*shell).max(0.1);
            let compliance=(1.0-0.85*shell)+breakthrough*0.85*shell;
            let next=(old+local*contact_compaction*compliance*(1.0-0.35*layer.0[4])).min(1.0);
            layer.0[1]=next;layer.0[2]=(layer.0[2]-(next-old)*0.9).max(0.0);
            layer.0[3]=(layer.0[3]-(next-old)*0.12-breakthrough*0.5).max(0.0);
            changed_depth+=layer.depth()*(next-old);
            transmission*=1.0-shell*(1.0-breakthrough)*0.95;
        }
        changed_depth
    }
}
#[derive(Clone,Debug,Default)]
pub(super) struct ContactScratch {pressed:Vec<(usize,Column,f32)>,rim:Vec<(usize,f32)>}
impl SnowField {
    pub fn configure_layers(&mut self,policy:SnowLayerPolicy)->Result<(),String>{
        policy.validate()?;
        self.ensure_strata();
        if self.layer_columns.is_empty(){
            self.layer_columns=self.spec.depth.iter().enumerate().map(|(i,d)|{
                let mut c=Column::default();let s=self.strata[i];
                c.deposit(Layer::new(*d,[s.powder,s.crust,s.moisture,self.compaction[i]]));c
            }).collect();
        }
        self.layer_policy=Some(policy);
        for i in 0..self.layer_columns.len(){self.sync_column(i);self.dirty.insert(i);}
        Ok(())
    }
    /// Caller-visible profile, top first, without serializing the whole field.
    pub fn layer_profile(&self,x:f32,z:f32)->Option<Vec<[f32;5]>>{
        self.sample(x,z)?;
        if self.layer_columns.is_empty(){return None;}
        let col=((x-self.spec.origin[0])/self.spec.cell_size).round() as usize;
        let row=((z-self.spec.origin[1])/self.spec.cell_size).round() as usize;
        let c=self.layer_columns[row*self.spec.columns+col];
        Some(c.layers[..c.len as usize].iter().map(|l|l.0).collect())
    }
    pub fn layers_enabled(&self)->bool{self.layer_policy.is_some()}
    pub(super) fn validate_columns(&self)->Result<(),String>{
        if let Some(p)=self.layer_policy{p.validate()?;}
        if self.layer_columns.is_empty(){
            if self.layer_policy.is_some(){return Err("missing saved snow columns".into());}
            return Ok(());
        }
        if self.layer_policy.is_none() || self.layer_columns.len()!=self.spec.depth.len(){
            return Err("invalid saved snow layer dimensions".into());
        }
        for (i,c) in self.layer_columns.iter().enumerate(){
            if c.layers[..c.len as usize].iter().any(|l|!l.valid()) ||
                (c.depth()-self.spec.depth[i]).abs()>0.00002*self.spec.depth[i].max(1.0){
                return Err("saved snow column depth differs from field".into());
            }
            let depth=c.depth();
            let compaction=if depth>0.0 {c.layers[..c.len as usize].iter()
                .map(|l|l.depth()*l.0[1]).sum::<f32>()/depth}else{0.0};
            if (compaction-self.compaction[i]).abs()>0.00002{
                return Err("saved snow column compaction differs from field".into());
            }
        }
        Ok(())
    }
    pub(super) fn sync_column(&mut self,i:usize){
        let c=&self.layer_columns[i];
        if c.len==1 {
            let l=c.top();let m=l.material();
            self.spec.depth[i]=l.depth();self.compaction[i]=m[3];
            self.strata[i]=SnowStrata{powder:m[0],crust:m[1],moisture:m[2]};return;
        }
        let depth=c.depth();
        self.spec.depth[i]=depth;
        self.compaction[i]=if depth>0.0{(c.layers[..c.len as usize].iter()
            .map(|l|l.depth()*l.0[1]).sum::<f32>()/depth).clamp(0.0,1.0)}else{0.0};
        let p=self.layer_policy.expect("columns require a policy");
        self.strata[i]=c.surface(p.surface_blend_metres,self.policy.compacted_fraction);
    }
    pub(super) fn column_deposit(&mut self,i:usize,depth:f32,material:[f32;4]){
        self.layer_columns[i].deposit(Layer::new(depth,material));self.sync_column(i);
    }
    pub(super) fn column_melt(&mut self,i:usize,depth:f32)->f32{
        let removed=self.layer_columns[i].remove_top(depth).depth();
        if self.layer_columns[i].len>0 {
            let c=&mut self.layer_columns[i];
            c.split_at_height(0.04,self.policy.compacted_fraction);
            let l=&mut c.layers[0];let wet=(removed/(l.depth()+0.0001)).min(1.0);
            l.0[2]=(l.0[2]-wet).max(0.0);l.0[3]=(l.0[3]-wet*0.8).max(0.0);
            l.0[4]=(l.0[4]+wet*0.7).min(1.0);
        }
        self.sync_column(i);removed
    }
    pub(super) fn column_wet(&mut self,i:usize,water:f32){
        let c=&mut self.layer_columns[i];
        if c.len==0{return;}
        c.split_at_height(0.04,self.policy.compacted_fraction);
        c.layers[0].0[4]=(c.layers[0].0[4]+water/c.top().depth()).min(1.0);
        self.sync_column(i);
    }
    pub(super) fn column_age(&mut self,i:usize,dt:f64,temp:f32)->bool{
        if self.layer_columns[i].len==0{return false;}
        let mut column=self.layer_columns[i];
        let m=column.top().material();
        let mut surface=SnowStrata{powder:m[0],crust:m[1],moisture:m[2]};
        if !age_snow_strata(&mut surface,dt,temp){return false;}
        column.split_at_height(0.04,self.policy.compacted_fraction);
        column.layers[0].0[2]=surface.powder;column.layers[0].0[3]=surface.crust;
        column.layers[0].0[4]=surface.moisture;
        self.layer_columns[i]=column;self.sync_column(i);true
    }
    pub(super) fn column_loose_depth(&self,i:usize)->f32{self.layer_columns[i].loose_depth()}
    pub(super) fn column_top(&self,i:usize)->Layer{self.layer_columns[i].top()}
    pub(super) fn column_transport(&mut self,i:usize,outgoing:f32,incoming:f32,material:[f32;4]){
        let c=&mut self.layer_columns[i];
        if c.len==1 {
            let top=c.top();let m=top.material();
            if incoming==0.0 || (0..4).all(|k|(m[k]-material[k]).abs()<0.035){
                let retained=(top.depth()-outgoing).max(0.0);let depth=retained+incoming;
                if depth<=0.0{*c=Column::default();}
                else{c.layers[0]=Layer::new(depth,std::array::from_fn(|k|
                    if incoming==0.0||m[k]==material[k]{m[k]}else{
                        ((retained*m[k]+incoming*material[k])/depth).clamp(0.0,1.0)}));}
                self.sync_column(i);return;
            }
        }
        c.remove_top(outgoing);c.deposit(Layer::new(incoming,material));
        self.sync_column(i);
    }
    pub(super) fn column_pack(&mut self,i:usize,packing:f32){
        if self.layer_columns[i].len==0{return;}
        let layer=&mut self.layer_columns[i].layers[0];
        layer.0[1]=layer.0[1].max((layer.0[1]+packing).min(0.85));
        layer.0[2]=(layer.0[2]-packing).max(0.0);
        self.sync_column(i);
    }
    pub(super) fn contact_layers(&mut self,x:f32,z:f32,radius:f32,strength:f32){
        if strength==0.0{return;}
        let mut scratch=self.layer_contact_scratch.take().unwrap_or_default();
        scratch.pressed.clear();scratch.rim.clear();
        let s=&self.spec;let rim_fraction=self.policy.ridge_displacement_fraction;
        let rim_width=if rim_fraction>0.0{(radius*0.9).max(s.cell_size*0.85)}else{0.0};
        let reach=radius+rim_width;
        let low=|v:f32,origin:f32,limit:usize| (((v-reach-origin)/s.cell_size).floor() as isize).clamp(0,limit as isize-1) as usize;
        let high=|v:f32,origin:f32,limit:usize| (((v+reach-origin)/s.cell_size).ceil() as isize).clamp(0,limit as isize-1) as usize;
        for row in low(z,s.origin[1],s.rows)..=high(z,s.origin[1],s.rows){
            for col in low(x,s.origin[0],s.columns)..=high(x,s.origin[0],s.columns){
                let dx=s.origin[0]+col as f32*s.cell_size-x;
                let dz=s.origin[1]+row as f32*s.cell_size-z;let d2=dx*dx+dz*dz;
                let i=row*s.columns+col;
                if d2<radius*radius{
                    let load=(1.0-d2/(radius*radius))*strength;
                    let mut c=self.layer_columns[i];
                    let changed=c.compact(self.layer_policy.unwrap().contact_depth_metres,load,
                        self.policy.contact_compaction,self.policy.compacted_fraction);
                    if changed>0.0{
                        let shift=(changed*rim_fraction).min(c.top().depth()*0.2);
                        scratch.pressed.push((i,c,shift));
                    }
                }else if d2<reach*reach && rim_fraction>0.0{
                    let t=(d2.sqrt()-radius)/rim_width;let w=4.0*t*(1.0-t);
                    if w>0.0&&self.spec.depth[i]<20.0{scratch.rim.push((i,w));}
                }
            }
        }
        let requested:f32=scratch.pressed.iter().map(|(_,_,shift)|*shift).sum();
        let capacity:f32=scratch.rim.iter().map(|(i,_)|(20.0-self.spec.depth[*i]).max(0.0)).sum();
        let volume=requested.min(capacity);
        let scale=if requested>0.0{volume/requested}else{0.0};
        let mut moved=Layer::default();
        for (i,mut c,shift) in scratch.pressed.iter().copied(){
            let piece=c.remove_top(shift*scale);
            if piece.depth()>0.0{moved=if moved.depth()>0.0{moved.merge(piece)}else{piece};}
            self.layer_columns[i]=c;self.sync_column(i);self.dirty.insert(i);
        }
        let mut remaining=moved.depth();
        for _ in 0..=scratch.rim.len(){
            if remaining<=1e-7{break;}
            let total:f32=scratch.rim.iter().filter(|(i,_)|self.spec.depth[*i]<20.0-1e-6).map(|(_,w)|*w).sum();
            if total<=0.0{break;}
            let mut used=0.0;
            for (i,w) in &scratch.rim{
                let amount=(remaining*w/total).min((20.0-self.spec.depth[*i]).max(0.0));
                if amount>0.0{self.column_deposit(*i,amount,moved.material());self.dirty.insert(*i);used+=amount;}
            }
            remaining=(remaining-used).max(0.0);if used<=1e-8{break;}
        }
        self.layer_contact_scratch=Some(scratch);
    }
}
#[cfg(test)]
mod tests;
