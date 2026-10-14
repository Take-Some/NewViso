//! Differential checks against the previous expanded-vertex implementation.
use super::*;
impl HeightfieldPatch {
    fn reference_detailed_snow_vertices(&self,color:[f32;4])->Vec<f32>{
        let fine=self.render_subdivisions;
        let cols=self.columns;
        let rows=self.physics_vertices.len()/cols;
        let width=(cols-1)*fine+1;
        let height=(rows-1)*fine+1;
        let dx=self.physics_vertices[1][0]-self.physics_vertices[0][0];
        let dz=self.physics_vertices[cols][2]-self.physics_vertices[0][2];
        let initial=self.physics_vertices[0];
        let mut grid=Vec::<[f32;3]>::with_capacity(width*height);
        let mut mask_grid=Vec::<[f32;3]>::with_capacity(width*height);
        let mut depth_grid=Vec::<f32>::with_capacity(width*height);
        for iz in 0..height{
            for ix in 0..width{
                let cx=(ix/fine).min(cols-2);
                let cz=(iz/fine).min(rows-2);
                let u=(ix-cx*fine) as f32/fine as f32;
                let v=(iz-cz*fine) as f32/fine as f32;
                let a=cz*cols+cx;let b=a+1;let c=a+cols;let d=c+1;
                let weights=if u+v<=1.0{
                    [(a,1.0-u-v),(b,u),(c,v)]
                }else{
                    [(b,1.0-v),(c,1.0-u),(d,u+v-1.0)]
                };
                let mut y=0.0;let mut ground=0.0;
                let mut mask=[0.0;3];
                for (k,w) in weights{
                    y+=self.physics_vertices[k][1]*w;
                    ground+=self.ground_heights[k]*w;
                    for channel in 0..3 {mask[channel]+=self.strata[k][channel]*w;}
                }
                let x=initial[0]+ix as f32*dx/fine as f32;
                let z=initial[2]+iz as f32*dz/fine as f32;
                let noise=self.detail_noise.get(iz*width+ix).copied().unwrap_or_else(||snow_relief_noise(x,z));
                let mut detail=cached_snow_relief((y-ground).max(0.0),self.render_relief,noise);
                // Enforce the same coarse interpolation along patch borders,
                // regardless of whether the neighbour is LOD 1, 2 or 3.
                // Prevents T-junction gaps with adjacent lower-detail patches.
                if ix==0||ix==width-1 {
                    let col=if ix==0 {0}else{cols-1};
                    let a=cz*cols+col;
                    let b=(cz+1)*cols+col;
                    let value=|i:usize|{
                        let p=self.physics_vertices[i];
                        cached_snow_relief((p[1]-self.ground_heights[i]).max(0.0),
                            self.render_relief,self.coarse_noise[i])
                    };
                    detail=value(a)*(1.0-v)+value(b)*v;
                }else if iz==0||iz==height-1 {
                    let row=if iz==0 {0}else{rows-1};
                    let a=row*cols+cx;
                    let b=a+1;
                    let value=|i:usize|{
                        let p=self.physics_vertices[i];
                        cached_snow_relief((p[1]-self.ground_heights[i]).max(0.0),
                            self.render_relief,self.coarse_noise[i])
                    };
                    detail=value(a)*(1.0-u)+value(b)*u;
                }
                grid.push([x,y+detail,z]);
                mask_grid.push(mask);
                depth_grid.push((y-ground).max(0.0));
            }
        }
        let mut normals=Vec::with_capacity(grid.len());
        for iz in 0..height{
            for ix in 0..width{
                let l=ix.saturating_sub(1);let r=(ix+1).min(width-1);
                let d=iz.saturating_sub(1);let u=(iz+1).min(height-1);
                let hx=(grid[iz*width+r][1]-grid[iz*width+l][1])/
                    ((r-l) as f32*dx/fine as f32);
                let hz=(grid[u*width+ix][1]-grid[d*width+ix][1])/
                    ((u-d) as f32*dz/fine as f32);
                normals.push(Vec3::new(-hx,1.0,-hz).normalized());
            }
        }
        let mut vertices=Vec::with_capacity((width-1)*(height-1)*6*FLOATS_PER_VERTEX);
        for iz in 0..height-1{
            for ix in 0..width-1{
                let a=iz*width+ix;
                let b=a+1;let c=a+width;let d=c+1;
                let x0=grid[a][0];let z0=grid[a][2];
                let cell_dx=grid[b][0]-x0;let cell_dz=grid[c][2]-z0;
                // Refinement is sparse: footprint cells use 12.5 cm samples,
                // all other cells retain their previous 0.5-1.0 m spacing.
                let touched=self.microprints.iter().any(|p|{
                    let dx=(p[0]-x0-cell_dx*0.5).abs();
                    let dz=(p[1]-z0-cell_dz*0.5).abs();
                    dx<cell_dx*0.5+p[4].max(p[5])*1.6+0.12&&
                    dz<cell_dz*0.5+p[4].max(p[5])*1.6+0.12
                });
                if !touched{
                    for tri in [[a,c,b],[b,c,d]]{
                        for idx in tri{
                            let p=grid[idx];
                            append_snow_vertex(
                                &mut vertices,Vec3::new(p[0],p[1],p[2]),normals[idx],
                                color,mask_grid[idx],true
                            );
                        }
                    }
                    continue;
                }
                let div=((cell_dx/0.125).ceil() as usize).clamp(2,8);
                for rz in 0..div{
                    for rx in 0..div{
                        for triangle in [[(0,0),(0,1),(1,0)],[(1,0),(0,1),(1,1)]]{
                            for (ox,oz) in triangle{
                                let u=(rx+ox) as f32/div as f32;
                                let v=(rz+oz) as f32/div as f32;
                                let (wa,wb,wc,wd)=if u+v<=1.0{
                                    (1.0-u-v,u,v,0.0)
                                }else{(0.0,1.0-v,1.0-u,u+v-1.0)};
                                let y=grid[a][1]*wa+grid[b][1]*wb+
                                    grid[c][1]*wc+grid[d][1]*wd;
                                let mask=[0,1,2].map(|k|
                                    mask_grid[a][k]*wa+mask_grid[b][k]*wb+
                                    mask_grid[c][k]*wc+mask_grid[d][k]*wd);
                                let remaining_depth=depth_grid[a]*wa+depth_grid[b]*wb+
                                    depth_grid[c]*wc+depth_grid[d]*wd;
                                let x=x0+u*cell_dx;let z=z0+v*cell_dz;
                                let mut offset=0.0_f32;
                                let mut slope_x=0.0_f32;
                                let mut slope_z=0.0_f32;
                                for stamp in &self.microprints{
                                    let dx=x-stamp[0];let dz=z-stamp[1];
                                    let along=dx*stamp[2]+dz*stamp[3];
                                    let side=-dx*stamp[3]+dz*stamp[2];
                                    let r2=(along/stamp[5]).powi(2)+(side/stamp[4]).powi(2);
                                    let core=(1.0-r2).max(0.0).powi(2);
                                    let rim=if (1.0..2.25).contains(&r2) {
                                        let t=(r2-1.0)/1.25;
                                        4.0*t*(1.0-t)
                                    }else{0.0};
                                    // Stamps cannot exceed the remaining
                                    // powder, and do not reappear after melt.
                                    let exposed=(remaining_depth/0.08).clamp(0.0,1.0);
                                    let affected=stamp[6]*(mask[0]*(1.0-mask[1])*
                                        (1.0-mask[2])).clamp(0.0,1.0)*exposed;
                                    offset+=affected*(-core+0.22*rim);
                                    // Derivative of the elliptical crater and
                                    // its raised snow rim. This makes the wall
                                    // respond to light as real geometry, not a
                                    // level white-painted triangle.
                                    let slope_r2=if r2<1.0 {
                                        2.0*(1.0-r2)
                                    }else if r2<2.25 {
                                        let t=(r2-1.0)/1.25;
                                        0.22*4.0*(1.0-2.0*t)/1.25
                                    }else{0.0};
                                    let drdx=2.0*along*stamp[2]/stamp[5].powi(2)
                                        -2.0*side*stamp[3]/stamp[4].powi(2);
                                    let drdz=2.0*along*stamp[3]/stamp[5].powi(2)
                                        +2.0*side*stamp[2]/stamp[4].powi(2);
                                    slope_x+=affected*slope_r2*drdx;
                                    slope_z+=affected*slope_r2*drdz;
                                }
                                let clamped=offset.clamp(-remaining_depth*0.6,0.045);
                                if clamped!=offset {slope_x=0.0;slope_z=0.0;}
                                offset=clamped;
                                let nx=normals[a].x*wa+normals[b].x*wb+
                                    normals[c].x*wc+normals[d].x*wd;
                                let ny=normals[a].y*wa+normals[b].y*wb+
                                    normals[c].y*wc+normals[d].y*wd;
                                let nz=normals[a].z*wa+normals[b].z*wb+
                                    normals[c].z*wc+normals[d].z*wd;
                                let normal=Vec3::new(
                                    nx-slope_x.clamp(-0.95,0.95)*ny*0.48,ny,
                                    nz-slope_z.clamp(-0.95,0.95)*ny*0.48
                                ).normalized();
                                append_snow_vertex(&mut vertices,
                                    Vec3::new(x,y+offset,z),normal,color,mask,true);
                            }
                        }
                    }
                }
            }
        }
        // Extremely sparse, camera-region-only snow fluff islands (a three-
        // facet cap each). They are integrated into the same surface draw,
        // carry the snow material and never enter the collision triangles.
        // Loose powder required; wet and icy surfaces remain smooth.
        if self.fluff_height>0.0 {
            for iz in (1..height-1).step_by(2){
                for ix in (1..width-1).step_by(2){
                    let idx=iz*width+ix;
                    let mask=mask_grid[idx];
                    let powder=(mask[0]*(1.0-mask[1])*(1.0-mask[2])).clamp(0.0,1.0);
                    let p=grid[idx];
                    let ground_base=self.ground_heights[(iz/fine).min(rows-1)*cols+
                        (ix/fine).min(cols-1)];
                    // Constant island count prevents one disappearing snow
                    // puff from shifting every downstream GPU geometry block.
                    // Wet/melted snow emits mathematically degenerate triangles
                    // (zero radius and height), with no visible shell.
                    let visible=powder*((p[1]-ground_base)/0.025).clamp(0.0,1.0);
                    let radius=self.fluff_radius.get(idx).copied().unwrap_or_else(||
                        (0.065+0.035*(((p[0]*17.13+p[2]*13.7).sin()+1.0)*0.5)).min(dx/fine as f32*0.30));
                    let r=radius*visible;
                    let tall=self.fluff_height*visible;
                    let norm=normals[idx];
                    let base=|ox:f32,oz:f32|->Vec3{
                        let dy=-(norm.x*ox+norm.z*oz)/norm.y.max(0.2);
                        Vec3::new(p[0]+ox,p[1]+dy+0.001,p[2]+oz)
                    };
                    let apex=Vec3::new(p[0],p[1]+tall,p[2]);
                    let triangle=[[base(-r,-r*0.4),base(r*0.95,-r*0.3),apex],
                        [base(r*0.95,-r*0.3),base(r*0.45,r),apex],
                        [base(r*0.45,r),base(-r,-r*0.4),apex]];
                    // Material tint stays in COLOR0; the three physical snow
                    // masks are packed into UV.xy and tangent.z instead.
                    for tri in triangle{
                        for point in tri{
                            append_snow_vertex(
                                &mut vertices,point,norm,color,mask,true
                            );
                        }
                    }
                }
            }
        }
        vertices
    }
}
#[test]
fn packed_vertex_layout_matches_the_shared_geometry_writer(){
    for snow in [false,true]{
        let p=Vec3::new(2.1,-3.2,4.3);let n=Vec3::new(-0.2,0.95,0.17);let color=[0.8,0.9,1.0,1.245];
        let mask=[0.73,0.12,0.06];let mut old=Vec::new();
        geometry::append_world_vertex(&mut old,p,n,color);
        if snow{let size=old.len();old[size-6]=mask[0];old[size-5]=mask[1];old[size-2]=mask[2];}
        assert_eq!(old,packed_snow_vertex(p,n,color,mask,snow));
    }
}
#[test]
fn indexed_packing_and_local_stamp_filter_preserve_every_render_float(){
    for fine in [1,2,3] {
        let input=json!({"collider":{"solid":true},"mesh":{"heightfield":{
            "columns":5,"rows":5,"heights":(0..25).map(|i|0.6+(i%5) as f32*0.03).collect::<Vec<_>>(),
            "ground_heights":vec![0.0;25],"surface_kind":"snow","deformable":true,
            "render_subdivisions":fine,"render_relief":0.026,"fluff_height":0.028}}});
        let mut patch=HeightfieldPatch::from_record(42,&input,Vec3::ZERO,Vec3::new(4.0,1.0,4.0),[0.8,0.9,1.0,1.0]).unwrap();
        patch.strata[7]=[0.5,0.2,0.1];
        for stamps in [vec![],vec![[0.2,0.3,0.6,0.8,0.11,0.23,0.07],
            [1.4,-0.8,-0.8,0.6,0.23,0.43,0.16],[50.0,50.0,1.0,0.0,0.11,0.2,0.07]]] {
            patch.microprints=stamps;
            assert_eq!(patch.reference_detailed_snow_vertices([0.8,0.9,1.0,1.0]),
                patch.detailed_snow_vertices([0.8,0.9,1.0,1.0]),"fine={fine}");
        }
    }
}
