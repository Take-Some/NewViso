//! Cached, conservative interception of one precipitation flux.
//! Patch areas are horizontal m²; square footprints match the surface-cap
//! fallback. Cell clipping and height ordering prevent double-counted roofs.
use super::SnowFieldSpec;
use crate::surface_snow::SnowSurfacePatch;
use std::collections::BTreeMap;
#[derive(Clone,Debug)]
pub struct SnowPrecipitationPlan {
    pub ground_exposure:Vec<f32>,
    pub surface_catch:Vec<f32>,
    pub source_area_m2:f64,
}
#[derive(Clone,Copy)]
struct Cover {patch:usize,rect:[f64;4]}
impl SnowPrecipitationPlan {
    pub fn new(spec:&SnowFieldSpec,patches:&[SnowSurfacePatch])->Result<Self,String>{
        spec.validate()?;
        if patches.len()>512{return Err("too many snow precipitation catchments".into());}
        for patch in patches{patch.validate()?;}
        let cell=spec.cell_size as f64;let area=cell*cell;
        let mut covers=BTreeMap::<usize,Vec<Cover>>::new();
        let mut order=(0..patches.len()).collect::<Vec<_>>();
        order.sort_by(|a,b|patches[*b].position[1].total_cmp(&patches[*a].position[1])
            .then_with(||patches[*a].id.cmp(&patches[*b].id)));
        for k in order {
            let p=&patches[k];
            if p.catch_fraction==0.0{continue;}
            let half=(p.area as f64).sqrt()*0.5;
            let x=(p.position[0] as f64-spec.origin[0] as f64)/cell;
            let z=(p.position[2] as f64-spec.origin[1] as f64)/cell;
            let h=half/cell;
            let start_x=((x-h+0.5).floor() as isize).max(0);
            let end_x=((x+h+0.5).ceil() as isize).min(spec.columns as isize);
            let start_z=((z-h+0.5).floor() as isize).max(0);
            let end_z=((z+h+0.5).ceil() as isize).min(spec.rows as isize);
            for row in start_z..end_z {for col in start_x..end_x {
                let i=row as usize*spec.columns+col as usize;
                if p.position[1]<=spec.ground[i]{continue;}
                let rect=[(x-h-col as f64).max(-0.5),(z-h-row as f64).max(-0.5),
                    (x+h-col as f64).min(0.5),(z+h-row as f64).min(0.5)];
                if rect[2]>rect[0]&&rect[3]>rect[1]{
                    covers.entry(i).or_default().push(Cover{patch:k,rect});
                }
            }}
        }
        let mut ground=vec![1.0;spec.depth.len()];
        let mut caught=vec![0.0_f64;patches.len()];
        for (i,cell_covers) in covers {
            let mut xs=vec![-0.5,0.5];let mut zs=xs.clone();
            for c in &cell_covers{xs.extend([c.rect[0],c.rect[2]]);zs.extend([c.rect[1],c.rect[3]]);}
            xs.sort_by(f64::total_cmp);xs.dedup();zs.sort_by(f64::total_cmp);zs.dedup();
            let mut exposed=0.0;
            for x in xs.windows(2){for z in zs.windows(2){
                let mid=[(x[0]+x[1])*0.5,(z[0]+z[1])*0.5];
                let tile=(x[1]-x[0])*(z[1]-z[0]);
                let mut remaining=1.0;
                for c in &cell_covers {
                    if mid[0]>c.rect[0]&&mid[0]<c.rect[2]&&mid[1]>c.rect[1]&&mid[1]<c.rect[3]{
                        let intercepted=remaining*patches[c.patch].catch_fraction as f64;
                        caught[c.patch]+=intercepted*tile*area;
                        remaining-=intercepted;
                        if remaining==0.0{break;}
                    }
                }
                exposed+=remaining*tile;
            }}
            ground[i]=exposed.clamp(0.0,1.0) as f32;
        }
        Ok(Self{ground_exposure:ground,
            surface_catch:caught.iter().zip(patches).map(|(v,p)|(*v/p.area as f64).clamp(0.0,1.0) as f32).collect(),
            source_area_m2:spec.depth.len() as f64*area})
    }
}
#[cfg(test)]
mod tests;
