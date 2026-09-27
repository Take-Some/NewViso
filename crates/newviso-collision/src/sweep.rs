//! Continuous sphere queries against immutable triangle collision geometry.
//! The BVH is built at residency time, never in the camera's per-frame path.
type V = [f32; 3];
fn add(a: V,b: V)->V {[a[0]+b[0],a[1]+b[1],a[2]+b[2]]}
fn sub(a: V,b: V)->V {[a[0]-b[0],a[1]-b[1],a[2]-b[2]]}
fn mul(a: V,s: f32)->V {[a[0]*s,a[1]*s,a[2]*s]}
fn dot(a: V,b: V)->f32 {a[0]*b[0]+a[1]*b[1]+a[2]*b[2]}
fn cross(a: V,b: V)->V {[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]]}

#[derive(Debug)]
struct Node { min: V, max: V, start: usize, end: usize, children: Option<(usize,usize)> }
#[derive(Debug)]
pub struct SphereSweepMesh { triangles: Vec<[V;3]>, nodes: Vec<Node> }

impl SphereSweepMesh {
    pub fn new(vertices: &[V], indices: &[[u32;3]]) -> Result<Self,String> {
        if vertices.iter().flatten().any(|v| !v.is_finite()) {
            return Err("sphere sweep mesh contains non-finite vertices".into());
        }
        let mut triangles=Vec::with_capacity(indices.len());
        for ids in indices {
            let mut tri=[[0.0;3];3];
            for i in 0..3 {
                tri[i]=*vertices.get(ids[i] as usize).ok_or("sphere sweep mesh index out of bounds")?;
            }
            triangles.push(tri);
        }
        let mut mesh=Self { triangles, nodes: Vec::new() };
        if !mesh.triangles.is_empty() { mesh.build(0,mesh.triangles.len()); }
        Ok(mesh)
    }

    fn build(&mut self,start: usize,end: usize)->usize {
        let mut min=[f32::INFINITY;3]; let mut max=[f32::NEG_INFINITY;3];
        for tri in &self.triangles[start..end] {
            for p in tri { for k in 0..3 { min[k]=min[k].min(p[k]); max[k]=max[k].max(p[k]); } }
        }
        let index=self.nodes.len();
        self.nodes.push(Node { min,max,start,end,children:None });
        if end-start>8 {
            let axis=(0..3).max_by(|a,b| (max[*a]-min[*a]).total_cmp(&(max[*b]-min[*b]))).unwrap();
            let mid=start+(end-start)/2;
            self.triangles[start..end].select_nth_unstable_by(mid-start,|a,b| {
                let ca=a[0][axis]+a[1][axis]+a[2][axis];
                let cb=b[0][axis]+b[1][axis]+b[2][axis];
                ca.total_cmp(&cb)
            });
            let left=self.build(start,mid); let right=self.build(mid,end);
            self.nodes[index].children=Some((left,right));
        }
        index
    }

    /// Returns the first contact fraction in [0,1], with both triangle sides enabled.
    pub fn sweep(&self,origin: V,delta: V,radius: f32)->Option<f32> {
        if self.nodes.is_empty() { return None; }
        let mut best=1.0; let mut hit=false; let mut stack=vec![0usize];
        while let Some(index)=stack.pop() {
            let node=&self.nodes[index];
            if sweep_sphere_aabb(origin,delta,radius,node.min,node.max).is_none_or(|t| t>best) { continue; }
            if let Some((a,b))=node.children { stack.push(a); stack.push(b); }
            else {
                for tri in &self.triangles[node.start..node.end] {
                    if let Some(t)=sweep_triangle(origin,delta,radius,*tri) {
                        if t<=best { best=t; hit=true; }
                    }
                }
            }
        }
        hit.then_some(best)
    }
}

/// Conservative sphere sweep for primitive bounds; also used as BVH broad phase.
pub fn sweep_sphere_aabb(o: V,d: V,r: f32,min: V,max: V)->Option<f32> {
    let mut enter: f32=0.0; let mut exit: f32=1.0;
    for k in 0..3 {
        let lo=min[k]-r; let hi=max[k]+r;
        if d[k].abs()<1e-8 {
            if o[k]<lo || o[k]>hi { return None; }
        } else {
            let a=(lo-o[k])/d[k]; let b=(hi-o[k])/d[k];
            enter=enter.max(a.min(b)); exit=exit.min(a.max(b));
            if enter>exit { return None; }
        }
    }
    (exit>=0.0 && enter<=1.0).then_some(enter.max(0.0))
}

fn point_in_triangle(p: V,t: [V;3],n: V)->bool {
    (0..3).all(|i| dot(cross(sub(t[(i+1)%3],t[i]),sub(p,t[i])),n)>=-1e-6)
}
fn sphere_hit(o: V,d: V,c: V,r: f32)->Option<f32> {
    let oc=sub(o,c); let a=dot(d,d); let c=dot(oc,oc)-r*r;
    if c<=0.0 { return Some(0.0); }
    if a<1e-12 { return None; }
    let b=dot(oc,d); let h=b*b-a*c;
    if h<0.0 { return None; }
    let t=(-b-h.sqrt())/a;
    (0.0..=1.0).contains(&t).then_some(t)
}
fn edge_hit(o: V,d: V,a: V,b: V,r: f32)->Option<f32> {
    let edge=sub(b,a); let len2=dot(edge,edge); let rel=sub(o,a);
    let mut best=sphere_hit(o,d,a,r).into_iter().chain(sphere_hit(o,d,b,r)).min_by(f32::total_cmp);
    if len2<1e-12 { return best; }
    let u=dot(rel,edge)/len2;
    let closest=add(a,mul(edge,u.clamp(0.0,1.0)));
    if dot(sub(o,closest),sub(o,closest))<=r*r { return Some(0.0); }
    let perp_o=sub(rel,mul(edge,u));
    let du=dot(d,edge)/len2; let perp_d=sub(d,mul(edge,du));
    let aa=dot(perp_d,perp_d); let bb=dot(perp_o,perp_d);
    let cc=dot(perp_o,perp_o)-r*r; let h=bb*bb-aa*cc;
    if aa>1e-12 && h>=0.0 {
        let t=(-bb-h.sqrt())/aa; let along=u+t*du;
        if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&along) {
            best=Some(best.map_or(t,|old| old.min(t)));
        }
    }
    best
}
fn sweep_triangle(o: V,d: V,r: f32,t: [V;3])->Option<f32> {
    let mut best=None::<f32>;
    let n=cross(sub(t[1],t[0]),sub(t[2],t[0])); let len=dot(n,n).sqrt();
    if len>1e-8 {
        let n=mul(n,1.0/len); let distance=dot(sub(o,t[0]),n);
        if distance.abs()<=r && point_in_triangle(sub(o,mul(n,distance)),t,n) {
            return Some(0.0);
        }
        let speed=dot(d,n);
        if speed.abs()>1e-8 {
            for sign in [-1.0,1.0] {
                let time=(sign*r-distance)/speed;
                if (0.0..=1.0).contains(&time) {
                    let point=sub(add(o,mul(d,time)),mul(n,sign*r));
                    if point_in_triangle(point,t,n) { best=Some(best.map_or(time,|old| old.min(time))); }
                }
            }
        }
    }
    for i in 0..3 {
        if let Some(time)=edge_hit(o,d,t[i],t[(i+1)%3],r) {
            best=Some(best.map_or(time,|old| old.min(time)));
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wall()->SphereSweepMesh {
        SphereSweepMesh::new(&[[-2.0,-2.0,0.0],[2.0,-2.0,0.0],[2.0,2.0,0.0],[-2.0,2.0,0.0]],&[[0,1,2],[0,2,3]]).unwrap()
    }
    #[test] fn wall_stops_sphere_from_both_sides() {
        for sign in [-1.0,1.0] {
            let t=wall().sweep([0.0,0.0,2.0*sign],[0.0,0.0,-4.0*sign],0.2).unwrap();
            assert!((t-0.45).abs()<1e-5);
        }
    }
    #[test] fn radius_catches_edges_that_center_ray_misses() {
        assert!(wall().sweep([2.1,0.0,2.0],[0.0,0.0,-4.0],0.2).is_some());
        assert!(wall().sweep([2.3,0.0,2.0],[0.0,0.0,-4.0],0.2).is_none());
        assert!(wall().sweep([2.1,2.1,2.0],[0.0,0.0,-4.0],0.2).is_some());
    }
    #[test] fn initial_overlap_and_long_motion_are_detected() {
        assert_eq!(wall().sweep([0.0,0.0,0.1],[0.0,0.0,4.0],0.2),Some(0.0));
        assert!(wall().sweep([0.0,0.0,50.0],[0.0,0.0,-100.0],0.2).is_some());
        assert!(wall().sweep([0.0,0.0,2.0],[1.0,0.0,0.0],0.2).is_none());
    }
    #[test] fn bvh_selects_nearest_surface_and_preserves_empty_space() {
        let mut v=vec![]; let mut ids=vec![];
        for z in 0..20 {
            let i=v.len() as u32;
            v.extend([[-1.0,-1.0,z as f32],[1.0,-1.0,z as f32],[0.0,1.0,z as f32]]);
            ids.push([i,i+1,i+2]);
        }
        let mesh=SphereSweepMesh::new(&v,&ids).unwrap();
        assert!((mesh.sweep([0.0,0.0,22.0],[0.0,0.0,-30.0],0.2).unwrap()-2.8/30.0).abs()<1e-5);
        assert!(mesh.sweep([4.0,0.0,22.0],[0.0,0.0,-30.0],0.2).is_none());
    }
}
