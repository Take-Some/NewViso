//! Continuous indexed terrain heightfield for project-owned scene primitives.
//! CPU vertices and the triangle collision mesh share exactly the same heights.
use super::*;
use std::sync::Arc;
mod expansion;

#[derive(Clone, Debug)]
pub(super) struct HeightfieldPatch {
    pub entity: u64,
    pub collides: bool,
    pub deformable: bool,
    pub surface_id: u32,
    pub vertices: Arc<[f32]>,
    pub physics_vertices: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
    columns: usize,
    /// Material edits do not require rebuilding the physics collider.
    height_revision: u64,
    coarse_noise: Vec<f32>,
    detail_noise: Vec<f32>,
    fluff_radius: Vec<f32>,
    // Opaque snow is explicitly marked for its own shader. Small deterministic
    // render-only grain never modifies the physical snow or saved grid.
    snow_surface: bool,
    wet_soil: bool,
    liquid_masks: Vec<[f32;2]>,
    render_relief: f32,
    fluff_height: f32,
    render_subdivisions: usize,
    ground_heights: Vec<f32>,
    strata: Vec<[f32;3]>,
    /// [world_x,world_z,forward_x,forward_z,half_width,half_length,depth]
    microprints: Vec<[f32;7]>,
}

/// Small spatially stable relief. Shared patch borders produce identical
/// world-space noise; zero or melted snow generates no offset.
#[cfg(test)]
fn snow_relief_at(x:f32,z:f32,depth:f32,strength:f32)->f32{
    if depth<=0.0||strength<=0.0 {return 0.0;}
    cached_snow_relief(depth,strength,snow_relief_noise(x,z))
}
fn snow_relief_noise(x:f32,z:f32)->f32 {
    let broad=(x*3.17+z*1.63).sin()*(z*2.47-x*0.92).cos();
    let granular=(x*10.4+z*5.76).sin()*(z*11.3-x*2.1).cos();
    (0.56+0.27*broad+0.17*granular).clamp(0.0,1.0)
}
fn cached_snow_relief(depth:f32,strength:f32,noise:f32)->f32 {
    if depth<=0.0||strength<=0.0 {return 0.0;}
    strength*(depth/0.06).clamp(0.0,1.0)*noise
}

/// Snow material payload uses UV.xy + tangent.z, NOT vertex colour.
 // Some backend pipelines unconditionally multiply COLOR0 into diffuse.
 // Packing the masks there would turn fresh powder bright red.
fn packed_snow_vertex(pos:Vec3,n:Vec3,color:[f32;4],mask:[f32;3],snow:bool)->[f32;FLOATS_PER_VERTEX]{
    [pos.x,pos.y,pos.z,0.0,n.x,n.y,n.z,color[0],color[1],color[2],color[3],
        if snow{mask[0]}else{0.0},if snow{mask[1]}else{0.0},
        1.0,0.0,if snow{mask[2]}else{0.0},1.0]
}
pub(super) fn append_snow_vertex(out:&mut Vec<f32>,pos:Vec3,n:Vec3,
    color:[f32;4],mask:[f32;3],snow:bool){
    out.extend_from_slice(&packed_snow_vertex(pos,n,color,mask,snow));
}
impl HeightfieldPatch {
    /// Exact vertical intersection with the same two triangles used by rendering
    /// and collision. A distant hill inside the broad bounds is not a rain roof.
    pub(super) fn upward_ray_hits(&self, origin: Vec3, min_distance: f32) -> bool {
        self.surface_height(origin.x, origin.z)
            .is_some_and(|height| height >= origin.y + min_distance.max(0.0))
    }

    pub(super) fn surface_height(&self, world_x: f32, world_z: f32) -> Option<f32> {
        let points = &self.physics_vertices;
        let rows = points.len() / self.columns;
        let first = points[0];
        let step_x = points[1][0] - first[0];
        let step_z = points[self.columns][2] - first[2];
        let x = (world_x - first[0]) / step_x;
        let z = (world_z - first[2]) / step_z;
        if !x.is_finite()
            || !z.is_finite()
            || x < 0.0
            || z < 0.0
            || x > (self.columns - 1) as f32
            || z > (rows - 1) as f32
        {
            return None;
        }
        let col = (x.floor() as usize).min(self.columns - 2);
        let row = (z.floor() as usize).min(rows - 2);
        let u = x - col as f32;
        let v = z - row as f32;
        let a = row * self.columns + col;
        let b = a + 1;
        let c = a + self.columns;
        let d = c + 1;
        let height = if u + v <= 1.0 {
            points[a][1] + (points[b][1] - points[a][1]) * u + (points[c][1] - points[a][1]) * v
        } else {
            points[b][1] * (1.0 - v) + points[c][1] * (1.0 - u) + points[d][1] * (u + v - 1.0)
        };
        Some(height)
    }

    pub fn from_record(
        entity: u64,
        record: &Value,
        center: Vec3,
        size: Vec3,
        color: [f32; 4],
    ) -> Result<Self, String> {
        let h = record
            .pointer("/mesh/heightfield")
            .ok_or("heightfield requires mesh.heightfield")?;
        let cols = h.get("columns").and_then(Value::as_u64).unwrap_or(0) as usize;
        let rows = h.get("rows").and_then(Value::as_u64).unwrap_or(0) as usize;
        if !(3..=257).contains(&cols)
            || !(3..=257).contains(&rows)
            || cols * rows > 66_049
            || !size.x.is_finite()
            || !size.z.is_finite()
            || size.x <= 0.0
            || size.z <= 0.0
        {
            return Err(
                "heightfield rows and columns must be 3..=257 and size must be positive".into(),
            );
        }
        let heights = h
            .get("heights")
            .and_then(Value::as_array)
            .ok_or("heightfield missing heights array")?;
        if heights.len() != cols * rows {
            return Err(format!(
                "heightfield expects {} height values, got {}",
                cols * rows,
                heights.len()
            ));
        }
        let z = heights
            .iter()
            .map(|v| {
                v.as_f64()
                    .map(|f| f as f32)
                    .filter(|v| v.is_finite())
                    .ok_or("heightfield has non-finite/non-numeric height")
            })
            .collect::<Result<Vec<_>, _>>()?;
        let snow_surface=h.get("surface_kind").and_then(Value::as_str)==Some("snow");
        let wet_soil=h.get("surface_kind").and_then(Value::as_str)==Some("ground_wet");
        let render_relief=h.get("render_relief_metres").and_then(Value::as_f64).unwrap_or(0.0) as f32;
        if !render_relief.is_finite()||!(0.0..=0.03).contains(&render_relief)
            || (!snow_surface&&render_relief>0.0) {
            return Err("invalid snow render relief".into());
        }
        let fluff_height=h.get("fluff_height_metres").and_then(Value::as_f64)
            .unwrap_or(0.0) as f32;
        if !fluff_height.is_finite()||!(0.0..=0.05).contains(&fluff_height)
            || (!snow_surface&&fluff_height>0.0){
            return Err("invalid snow fluff height".into());
        }
        let ground_heights = if snow_surface {
            let heights=h.get("ground_heights").and_then(Value::as_array)
                .ok_or("snow render material requires ground heights")?;
            if heights.len()!=cols*rows {return Err("snow ground grid size mismatch".into());}
            heights.iter().map(|v|v.as_f64().map(|x|x as f32)
              .filter(|x|x.is_finite()).ok_or("invalid snow ground height"))
              .collect::<Result<Vec<f32>,_>>()?
        }else{Vec::new()};
        let render_subdivisions=h.get("render_subdivisions")
            .and_then(Value::as_u64).unwrap_or(1) as usize;
        if !(1..=3).contains(&render_subdivisions) ||
            (!snow_surface && render_subdivisions!=1) {
            return Err("snow rendering subdivisions must be in 1..=3".into());
        }
        let mut vertex_color=color;
        let strata=vec![[1.0,0.0,0.0];if snow_surface{cols*rows}else{0}];
        if snow_surface {vertex_color[3]=1.125;}
        if wet_soil {vertex_color[3]=1.245;}
        let step_x = size.x / (cols - 1) as f32;
        let step_z = size.z / (rows - 1) as f32;
        let at = |x: usize, y: usize| z[y * cols + x];
        let mut points = Vec::with_capacity(cols * rows);
        let mut normals = Vec::with_capacity(cols * rows);
        for row in 0..rows {
            for col in 0..cols {
                let x = center.x - size.x * 0.5 + col as f32 * step_x;
                let world_z = center.z - size.z * 0.5 + row as f32 * step_z;
                let h0 = at(col, row);
                let left = at(col.saturating_sub(1), row);
                let right = at((col + 1).min(cols - 1), row);
                let down = at(col, row.saturating_sub(1));
                let up = at(col, (row + 1).min(rows - 1));
                let hx = (right - left)
                    / ((if col == 0 || col == cols - 1 {
                        1.0
                    } else {
                        2.0
                    }) * step_x);
                let hz = (up - down)
                    / ((if row == 0 || row == rows - 1 {
                        1.0
                    } else {
                        2.0
                    }) * step_z);
                let normal = Vec3::new(-hx, 1.0, -hz).normalized();
                points.push([x, center.y + h0 * size.y, world_z]);
                normals.push(normal);
            }
        }
        let coarse_noise=points.iter().map(|p|snow_relief_noise(p[0],p[2])).collect::<Vec<_>>();
        let mut triangles = Vec::with_capacity((cols - 1) * (rows - 1) * 2);
        let mut vertices = Vec::with_capacity(triangles.capacity() * 3 * FLOATS_PER_VERTEX);
        for row in 0..rows - 1 {
            for col in 0..cols - 1 {
                let a = (row * cols + col) as u32;
                let b = a + 1;
                let c = a + cols as u32;
                let d = c + 1;
                // Winding: +Y-facing for X,Z grid.
                for indices in [[a, c, b], [b, c, d]] {
                    triangles.push(indices);
                    for idx in indices {
                        let idx = idx as usize;
                        let point = points[idx];
                        let depth=if snow_surface {
                            (point[1]-ground_heights[idx]).max(0.0)
                        }else{0.0};
                        let elevation=cached_snow_relief(depth,render_relief,coarse_noise[idx]);
                        append_snow_vertex(
                            &mut vertices,
                            Vec3::new(point[0], point[1]+elevation, point[2]),
                            normals[idx],vertex_color,
                            if snow_surface{strata[idx]}else{[0.0;3]},snow_surface,
                        );
                        if wet_soil {
                            let n=vertices.len();
                            vertices[n-6]=0.0;
                            vertices[n-5]=0.0;
                        }
                    }
                }
            }
        }
        let mut result=Self {
            entity,
            surface_id: record
                .pointer("/material/surface_id")
                .and_then(Value::as_u64)
                .map(u32::try_from)
                .transpose()
                .map_err(|_| "terrain surface_id exceeds u32")?
                .unwrap_or(0),
            deformable: record
                .pointer("/mesh/heightfield/deformable")
                .and_then(Value::as_bool)
                == Some(true),
            collides: record.pointer("/collider/solid").and_then(Value::as_bool) == Some(true),
            vertices: Arc::from(vertices),
            physics_vertices: points,
            height_revision: 0,
            coarse_noise,detail_noise:Vec::new(),fluff_radius:Vec::new(),
            triangles,
            columns: cols,
            snow_surface,wet_soil,liquid_masks:if wet_soil{
                vec![[0.0,0.0];cols*rows]
            }else{Vec::new()},render_relief,fluff_height,render_subdivisions,ground_heights,strata,
            microprints:Vec::new(),
        };
        if result.render_subdivisions>1 {
            result.vertices=Arc::from(result.detailed_snow_vertices(vertex_color));
        }
        result.cache_detail_noise();
        Ok(result)
    }
}
impl HeightfieldPatch {
    /// Render-only sub-grid, explicitly opt-in, with exact coarse triangle
    /// interpolation for the physical surface. Contact/collision still use
    /// the original grid, so this detail cannot destabilize character motion.
    fn set_microprints(&mut self, prints:&[[f32;7]])->bool {
        if !self.snow_surface||!self.deformable{return false;}
        let p0=self.physics_vertices[0];
        let last=*self.physics_vertices.last().unwrap();
        let incoming:Vec<[f32;7]>=prints.iter().copied().filter(|p|{
            p[0]>=p0[0]-0.65&&p[0]<=last[0]+0.65&&
            p[1]>=p0[2]-0.65&&p[1]<=last[2]+0.65
        }).collect();
        if incoming==self.microprints{return false;}
        self.microprints=incoming;
        let color=[self.vertices[7],self.vertices[8],self.vertices[9],self.vertices[10]];
        self.vertices=Arc::from(self.detailed_snow_vertices(color));
        true
    }
    /// World x/z never change during snow deformation. Relief noise and
    /// grain radii are static; only their depth/powder visibility is dynamic.
    fn cache_detail_noise(&mut self) {
        let fine=self.render_subdivisions;
        let cols=self.columns;
        let rows=self.physics_vertices.len()/cols;
        let width=(cols-1)*fine+1;let height=(rows-1)*fine+1;
        let dx=self.physics_vertices[1][0]-self.physics_vertices[0][0];
        let dz=self.physics_vertices[cols][2]-self.physics_vertices[0][2];
        let initial=self.physics_vertices[0];
        self.detail_noise=Vec::with_capacity(width*height);
        self.fluff_radius=if self.fluff_height>0.0 {Vec::with_capacity(width*height)}else{Vec::new()};
        for iz in 0..height {for ix in 0..width {
            let x=initial[0]+ix as f32*dx/fine as f32;
            let z=initial[2]+iz as f32*dz/fine as f32;
            self.detail_noise.push(snow_relief_noise(x,z));
            if self.fluff_height>0.0 {
                self.fluff_radius.push((0.065+0.035*(((x*17.13+z*13.7).sin()+1.0)*0.5))
                    .min(dx/fine as f32*0.30));
            }
        }}
    }
    fn detailed_snow_vertices(&self,color:[f32;4])->Vec<f32>{
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
        let packed=grid.iter().enumerate().map(|(i,p)|
            packed_snow_vertex(Vec3::new(p[0],p[1],p[2]),normals[i],color,mask_grid[i],true)).collect::<Vec<_>>();
        let mut vertices=Vec::with_capacity((width-1)*(height-1)*6*FLOATS_PER_VERTEX);
        let mut active_stamps=Vec::<&[f32;7]>::with_capacity(self.microprints.len());
        for iz in 0..height-1{
            for ix in 0..width-1{
                let a=iz*width+ix;
                let b=a+1;let c=a+width;let d=c+1;
                let x0=grid[a][0];let z0=grid[a][2];
                let cell_dx=grid[b][0]-x0;let cell_dz=grid[c][2]-z0;
                // Refinement is sparse: footprint cells use 12.5 cm samples,
                // all other cells retain their previous 0.5-1.0 m spacing.
                active_stamps.clear();
                active_stamps.extend(self.microprints.iter().filter(|p|{
                    let dx=(p[0]-x0-cell_dx*0.5).abs();
                    let dz=(p[1]-z0-cell_dz*0.5).abs();
                    dx<cell_dx*0.5+p[4].max(p[5])*1.6+0.12&&
                    dz<cell_dz*0.5+p[4].max(p[5])*1.6+0.12
                }));
                if active_stamps.is_empty(){
                    for tri in [[a,c,b],[b,c,d]]{
                        for idx in tri{
                            vertices.extend_from_slice(&packed[idx]);
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
                                for stamp in &active_stamps{
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
    /// Render-only wet ground and ice film. Geometry and all collision
    /// triangles stay identical. The soil shader recognizes alpha=1.245
    /// and reads the liquid mask from UV.xy, not vertex RGB.
    fn set_liquid_masks(&mut self,changes:&BTreeMap<(i64,i64),[f32;2]>)->bool {
        if !self.wet_soil{return false;}
        let key=|x:f32,z:f32|((x*10000.0).round() as i64,
                               (z*10000.0).round() as i64);
        let mut dirty=false;
        for (idx,pos) in self.physics_vertices.iter().enumerate(){
            if let Some(v)=changes.get(&key(pos[0],pos[2])){
                let value=[v[0].clamp(0.0,1.0),v[1].clamp(0.0,1.0)];
                if self.liquid_masks[idx]!=value{
                    self.liquid_masks[idx]=value;
                    dirty=true;
                }
            }
        }
        if !dirty{return false;}
        let mut vertices=Vec::with_capacity(self.triangles.len()*3*FLOATS_PER_VERTEX);
        let tint=[self.vertices[7],self.vertices[8],self.vertices[9],1.245];
        let columns=self.columns;
        let rows=self.physics_vertices.len()/columns;
        let dx=self.physics_vertices[1][0]-self.physics_vertices[0][0];
        let dz=self.physics_vertices[columns][2]-self.physics_vertices[0][2];
        for triangle in &self.triangles{
            for idx in triangle{
                let i=*idx as usize;
                let x=i%columns;let z=i/columns;
                let l=self.physics_vertices[z*columns+x.saturating_sub(1)][1];
                let r=self.physics_vertices[z*columns+(x+1).min(columns-1)][1];
                let d=self.physics_vertices[z.saturating_sub(1)*columns+x][1];
                let u=self.physics_vertices[(z+1).min(rows-1)*columns+x][1];
                let norm=Vec3::new(-(r-l)/(if x==0||x==columns-1{dx}else{2.0*dx}),
                                   1.0,-(u-d)/(if z==0||z==rows-1{dz}else{2.0*dz}))
                    .normalized();
                let p=self.physics_vertices[i];
                geometry::append_world_vertex(&mut vertices,Vec3::new(p[0],p[1],p[2]),norm,tint);
                let j=vertices.len();
                vertices[j-6]=self.liquid_masks[i][0];
                vertices[j-5]=self.liquid_masks[i][1];
            }
        }
        self.vertices=Arc::from(vertices);
        true
    }
    #[cfg(test)]
    fn apply_surface_changes(&mut self, changed:&BTreeMap<(i64,i64),[f32;4]>)->bool {
        self.apply_surface_lookup(|x,z|changed.get(&((x*10000.0).round() as i64,
            (z*10000.0).round() as i64)).copied())
    }
    fn apply_surface_lookup(&mut self, lookup:impl Fn(f32,f32)->Option<[f32;4]>)->bool {
        if !self.deformable {
            return false;
        }
        let mut dirty = false;
        let mut height_dirty = false;
        for (i,p) in self.physics_vertices.iter_mut().enumerate() {
            if let Some(values)=lookup(p[0],p[2]){
                if p[1]!=values[0]{p[1]=values[0];dirty=true;height_dirty=true;}
                if self.snow_surface {
                    let mask=[values[1],values[2],values[3]];
                    if self.strata[i]!=mask {self.strata[i]=mask;dirty=true;}
                }
            }
        }
        if !dirty {
            return false;
        }
        if height_dirty {self.height_revision=self.height_revision.wrapping_add(1);}
        let color=[self.vertices[7],self.vertices[8],self.vertices[9],self.vertices[10]];
        // Detailed geometry already computes its own normals and vertices.
        // Avoid generating a complete coarse mesh that would be discarded.
        if self.render_subdivisions>1||!self.microprints.is_empty() {
            self.vertices=Arc::from(self.detailed_snow_vertices(color));
            return true;
        }
        let points = &self.physics_vertices;
        let columns = self.columns;
        let rows = points.len() / columns;
        let dx = points[1][0] - points[0][0];
        let dz = points[columns][2] - points[0][2];
        let color = [
            self.vertices[7],
            self.vertices[8],
            self.vertices[9],
            self.vertices[10],
        ];
        // Pack each indexed grid point once. Six expanded triangle vertices
        // share its relief, normal and material; the render topology is unchanged.
        let mut packed=Vec::with_capacity(points.len());
        for row in 0..rows {
            for col in 0..columns {
                let l=col.saturating_sub(1);let r=(col+1).min(columns-1);
                let d=row.saturating_sub(1);let u=(row+1).min(rows-1);
                let hx=(points[row*columns+r][1]-points[row*columns+l][1])/((r-l) as f32*dx);
                let hz=(points[u*columns+col][1]-points[d*columns+col][1])/((u-d) as f32*dz);
                let i=row*columns+col;let p=points[i];
                let depth=if self.snow_surface{(p[1]-self.ground_heights[i]).max(0.0)}else{0.0};
                let elevation=cached_snow_relief(depth,self.render_relief,self.coarse_noise[i]);
                packed.push(packed_snow_vertex(Vec3::new(p[0],p[1]+elevation,p[2]),
                    Vec3::new(-hx,1.0,-hz).normalized(),color,
                    if self.snow_surface{self.strata[i]}else{[0.0;3]},self.snow_surface));
            }
        }
        let mut vertices=Vec::with_capacity(self.vertices.len());
        for triangle in &self.triangles {for index in triangle{
            vertices.extend_from_slice(&packed[*index as usize]);
        }}
        self.vertices=Arc::from(vertices);
        true
    }
}

impl Scene3dRuntime {
    pub fn heightfield_surface_id(&self, entity: u64) -> Option<u32> {
        self.terrain_meshes
            .values()
            .find(|p| p.entity == entity)
            .map(|p| p.surface_id)
    }
    pub fn update_ground_liquids(&mut self,changes:&[[f32;5]])->Result<usize,String>{
        if changes.iter().any(|p|!p.iter().all(|v|v.is_finite())||p[3]<0.0||p[4]<0.0){
            return Err("invalid ground liquid payload".into());
        }
        let key=|x:f32,z:f32|((x*10000.0).round() as i64,
                               (z*10000.0).round() as i64);
        let masks=changes.iter().map(|p|(key(p[0],p[2]),[p[3],p[4]]))
            .collect::<BTreeMap<_,_>>();
        Ok(self.terrain_meshes.values_mut()
            .map(|mesh|mesh.set_liquid_masks(&masks)).filter(|b|*b).count())
    }
    /// Render-only 12.5 cm detail for sparse native footstep stamps.
    /// Physics collider is never rebuilt here.
    pub fn update_snow_microprints(&mut self, prints:&[[f32;7]])->Result<usize,String>{
        if prints.len()>256||prints.iter().any(|p|{
            !p.iter().all(|v|v.is_finite()) || p[4]<=0.0||p[5]<=0.0||
            p[6]<0.0||p[6]>0.16
        }){return Err("invalid terrain microprint payload".into());}
        Ok(self.terrain_meshes.values_mut()
            .map(|patch|patch.set_microprints(prints))
            .filter(|changed|*changed).count())
    }
    /// Replace changed grid vertices in explicitly deformable terrain only.
    /// Returns identical collision data; the runtime installs it before the next physics step.
    pub fn deform_heightfields(
        &mut self,
        changes: &[[f32; 6]],
    ) -> Result<Vec<(u64, Vec<[f32; 3]>, Vec<[u32; 3]>)>, String> {
        if changes.iter().any(|p| !p.iter().all(|v| v.is_finite())) {
            return Err("non-finite terrain deformation".into());
        }
        let key = |x: f32, z: f32| ((x * 10000.0).round() as i64, (z * 10000.0).round() as i64);
        let changed = changes
            .iter()
            .map(|p| (key(p[0], p[2]), [p[1],p[3],p[4],p[5]]))
            .collect::<std::collections::HashMap<_, _>>();
        let mut result = Vec::new();
        for patch in self.terrain_meshes.values_mut().filter(|p| p.deformable) {
            let revision=patch.height_revision;
            if !patch.apply_surface_lookup(|x,z|changed.get(&key(x,z)).copied()) {
                continue;
            }
            if revision==patch.height_revision {continue;}
            let mut min = [f32::INFINITY; 3];
            let mut max = [f32::NEG_INFINITY; 3];
            for p in &patch.physics_vertices {
                for axis in 0..3 {
                    min[axis] = min[axis].min(p[axis]);
                    max[axis] = max[axis].max(p[axis]);
                }
            }
            let id = SceneEntityId(patch.entity);
            if let Some(entity) = self.world.entity(id) {
                self.world.update_spatial_from(
                    id,
                    entity.transform,
                    SceneBounds {
                        min: Vec3::new(min[0], min[1], min[2]),
                        max: Vec3::new(max[0], max[1], max[2]),
                    },
                    SceneMutationSource::Engine,
                )?;
            }
            if patch.collides {
                result.push((
                    patch.entity,
                    patch.physics_vertices.clone(),
                    patch.triangles.clone(),
                ));
            }
        }
        Ok(result)
    }
}
#[cfg(test)]
mod performance_tests;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_relief_matches_direct_noise_and_material_edits_keep_collision_revision() {
        for x in [-112.0,-0.3,0.0,1.75,95.0] {for z in [-64.0,0.2,8.0] {
            for depth in [0.0,0.001,0.03,0.06,1.4] {
                assert_eq!(snow_relief_at(x,z,depth,0.026),
                    cached_snow_relief(depth,0.026,snow_relief_noise(x,z)));
            }
        }}
        let input=json!({"collider":{"solid":true},"mesh":{"heightfield":{
            "columns":3,"rows":3,"heights":vec![0.5;9],"ground_heights":vec![0.0;9],
            "surface_kind":"snow","deformable":true,"render_subdivisions":2}}});
        let mut mesh=HeightfieldPatch::from_record(42,&input,Vec3::ZERO,Vec3::new(2.0,1.0,2.0),[1.0;4]).unwrap();
        let revision=mesh.height_revision;let vertices=mesh.physics_vertices.clone();
        assert!(mesh.apply_surface_changes(&BTreeMap::from([((0,0),[0.5,0.5,0.2,0.3])])));
        assert_eq!(revision,mesh.height_revision);assert_eq!(vertices,mesh.physics_vertices);
        assert!(mesh.apply_surface_changes(&BTreeMap::from([((0,0),[0.4,0.5,0.2,0.3])])));
        assert_ne!(revision,mesh.height_revision);
    }

    #[test]
    #[ignore="CPU benchmark with caller-supplied scene; invoke explicitly"]
    fn benchmark_authored_heightfield_geometry() {
        let path=std::env::var("NEWVISO_TERRAIN_BENCH_SCENE").expect("supply authored scene path");
        let scene:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let mut patches=Vec::new();
        for (id,record) in scene["entities"].as_array().unwrap().iter().enumerate() {
            if record.pointer("/mesh/primitive").and_then(Value::as_str)!=Some("heightfield") {continue;}
            let vector=|key:&str| {
                let values=record.pointer(key).unwrap().as_array().unwrap();
                Vec3::new(values[0].as_f64().unwrap() as f32,values[1].as_f64().unwrap() as f32,values[2].as_f64().unwrap() as f32)
            };
            let patch=HeightfieldPatch::from_record(id as u64,record,
                vector("/transform/position"),vector("/transform/scale"),[1.0;4]).unwrap();
            if patch.deformable {patches.push(patch);}
        }
        let mut coarse=0.0;let mut detailed=0.0;let mut changed=std::collections::HashMap::new();
        for patch in &patches {
            for p in &patch.physics_vertices {
                changed.insert(((p[0]*10000.0).round() as i64,(p[2]*10000.0).round() as i64),
                    [p[1]+0.01,0.99,0.0,0.0]);
            }
        }
        let mut count=0;
        for mut patch in patches {
            let start=std::time::Instant::now();
            assert!(patch.apply_surface_lookup(|x,z|changed.get(&((x*10000.0).round() as i64,(z*10000.0).round() as i64)).copied()));
            let elapsed=start.elapsed().as_secs_f64()*1000.0;
            if patch.render_subdivisions>1 {detailed+=elapsed;}else{coarse+=elapsed;}
            count+=patch.vertices.len()/FLOATS_PER_VERTEX;
        }
        eprintln!("authored terrain: coarse_ms={coarse:.3} detailed_ms={detailed:.3} total_ms={:.3} render_vertices={count}",coarse+detailed);
    }

    #[test]
    fn snow_deformation_preserves_colour_and_updates_render_and_collision_together() {
        let input = json!({"collider":{"solid":true},"mesh":{"heightfield":{"columns":3,"rows":3,
            "heights":vec![1;9],"deformable":true}}});
        let color = [0.4, 0.5, 0.6, 0.96];
        let mut mesh =
            HeightfieldPatch::from_record(42, &input, Vec3::ZERO, Vec3::new(2.0, 1.0, 2.0), color)
                .unwrap();
        let old = Arc::clone(&mesh.vertices);
        assert!(mesh.apply_surface_changes(&BTreeMap::from([((0, 0), [0.2,1.0,0.0,0.0])])));
        assert_eq!(mesh.surface_height(0.0, 0.0), Some(0.2));
        assert_eq!(mesh.physics_vertices[4][1], 0.2);
        assert!(!Arc::ptr_eq(&old, &mesh.vertices));
        assert!(mesh
            .vertices
            .chunks_exact(FLOATS_PER_VERTEX)
            .all(|v| v[7..11] == color));
        assert!(mesh
            .vertices
            .chunks_exact(FLOATS_PER_VERTEX)
            .filter(|v| v[0] == 0.0 && v[2] == 0.0)
            .all(|v| v[1] == 0.2));
        assert!(!mesh.apply_surface_changes(&BTreeMap::from([((0, 0), [0.2,1.0,0.0,0.0])])));
        mesh.deformable = false;
        assert!(!mesh.apply_surface_changes(&BTreeMap::from([((0, 0), [-10.0,1.0,0.0,0.0])])));
    }
    #[test]
    fn snow_material_has_real_render_relief_without_modifying_collision(){
        let input=json!({"collider":{"solid":true},
          "mesh":{"heightfield":{"columns":3,"rows":3,
           "heights":[0.3,0.3,0.3,0.3,0.3,0.3,0.3,0.3,0.3],
           "ground_heights":[0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0],
           "surface_kind":"snow","render_relief_metres":0.02,
           "render_subdivisions":2,"deformable":true}}});
        let mut mesh=HeightfieldPatch::from_record(11,&input,Vec3::ZERO,
            Vec3::new(2.0,1.0,2.0),[0.9,0.95,1.0,1.0]).unwrap();
        assert!(mesh.snow_surface);
        assert_eq!(mesh.physics_vertices.len(),9);
        assert_eq!(mesh.triangles.len(),8);
        assert_eq!(mesh.vertices.len(),4*4*6*FLOATS_PER_VERTEX);
        assert!(mesh.vertices.chunks_exact(FLOATS_PER_VERTEX).all(|v|v[10]==1.125));
        assert!(mesh.vertices.chunks_exact(FLOATS_PER_VERTEX).any(|v|v[1]>0.3));
        assert_eq!(mesh.surface_height(0.0,0.0),Some(0.3));
        assert!(mesh.apply_surface_changes(&BTreeMap::from([((0,0),[0.0,0.0,0.0,0.0])])));
        assert_eq!(mesh.surface_height(0.0,0.0),Some(0.0));
        assert!(mesh.vertices.chunks_exact(FLOATS_PER_VERTEX)
            .filter(|v|v[0]==0.0&&v[2]==0.0)
            .all(|v|v[1]==0.0));
        let invalid=json!({"mesh":{"heightfield":{"columns":3,"rows":3,
          "heights":[0,0,0,0,0,0,0,0,0],
          "surface_kind":"snow","render_relief_metres":0.05}}});
        assert!(HeightfieldPatch::from_record(12,&invalid,Vec3::ZERO,
           Vec3::new(2.0,1.0,2.0),[1.0;4]).is_err());
    }
    #[test]
    fn powder_fluff_is_render_only_and_vanishes_when_wet_or_melted(){
        let input=json!({"mesh":{"heightfield":{"columns":3,"rows":3,
          "heights":[0.4,0.4,0.4,0.4,0.4,0.4,0.4,0.4,0.4],
          "ground_heights":[0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0],
          "surface_kind":"snow","render_subdivisions":2,
          "fluff_height_metres":0.03,"deformable":true}}});
        let mut mesh=HeightfieldPatch::from_record(99,&input,Vec3::ZERO,
             Vec3::new(2.0,1.0,2.0),[1.0;4]).unwrap();
        let surface_vertices=4*4*6*FLOATS_PER_VERTEX;
        assert!(mesh.vertices.len()>surface_vertices,"powder adds shell faces");
        assert!(mesh.vertices.chunks_exact(FLOATS_PER_VERTEX)
            .all(|v|v[7]>0.9&&v[8]>0.9&&v[9]>0.9),
            "snow fluff cannot reuse RGB vertex tint as strata masks");
        assert_eq!(mesh.triangles.len(),8,"fluff is never a collision mesh");
        let muted=(0..3).flat_map(|z|(0..3).map(move |x|{
            let xx=x as i64*10000-10000;
            let zz=z as i64*10000-10000;
            ((xx,zz),[0.4,0.02,0.85,0.9])
        })).collect();
        assert!(mesh.apply_surface_changes(&muted));
        assert!(mesh.vertices.len()>surface_vertices);
        assert!(mesh.vertices[surface_vertices..].chunks_exact(FLOATS_PER_VERTEX)
            .all(|v|v[1]<=0.402),"wet caps collapse into degenerate triangles");
        assert!(mesh.physics_vertices.iter().all(|v|v[1]==0.4));
        let bare=(0..3).flat_map(|z|(0..3).map(move |x|{
            ((x as i64*10000-10000,z as i64*10000-10000),
             [0.0,0.0,0.0,0.0])
        })).collect();
        assert!(mesh.apply_surface_changes(&bare));
        assert!(mesh.vertices.len()>surface_vertices);
        assert!(mesh.vertices[surface_vertices..].chunks_exact(FLOATS_PER_VERTEX)
            .all(|v|v[1]<=0.002),"melted caps have no visible geometry");
    }
    #[test]
    fn local_microprint_refines_snow_not_collision(){
        let input=json!({"collider":{"solid":true},"mesh":{"heightfield":{
          "columns":3,"rows":3,"deformable":true,
          "heights":[0.5,0.5,0.5,0.5,0.5,0.5,0.5,0.5,0.5],
          "ground_heights":[0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0],
          "surface_kind":"snow","render_subdivisions":2}}});
        let mut mesh=HeightfieldPatch::from_record(77,&input,Vec3::ZERO,
            Vec3::new(2.0,1.0,2.0),[0.95,0.95,0.95,1.0]).unwrap();
        let saved_collisions=mesh.physics_vertices.clone();
        let original=mesh.vertices.len();
        let stamp=[0.12,0.16,0.0,1.0,0.1,0.23,0.08];
        assert!(mesh.set_microprints(&[stamp]));
        assert!(mesh.vertices.len()>original,"footstep refinements add local detail");
        assert!(mesh.vertices.chunks_exact(FLOATS_PER_VERTEX)
            .any(|v|(v[0]-0.12).abs()<0.04&&(v[2]-0.16).abs()<0.04&&v[1]<0.5));
        assert_eq!(mesh.physics_vertices,saved_collisions);
        assert!(!mesh.set_microprints(&[stamp]));
        assert!(mesh.set_microprints(&[]));
        assert_eq!(mesh.vertices.len(),original);
        assert_eq!(mesh.physics_vertices,saved_collisions);
    }
    #[test]
    fn water_and_ice_visuals_update_soil_without_affecting_collisions(){
        let v=json!({"collider":{"solid":false},"mesh":{"heightfield":{
            "columns":3,"rows":3,"heights":[0,0,0,0,0,0,0,0,0],
            "surface_kind":"ground_wet","deformable":false}}});
        let mut m=HeightfieldPatch::from_record(2,&v,Vec3::ZERO,
            Vec3::new(2.0,1.0,2.0),[0.25,0.21,0.17,1.0]).unwrap();
        assert!(m.wet_soil);
        let physical=m.physics_vertices.clone();
        let tris=m.triangles.clone();
        assert!(m.set_liquid_masks(&BTreeMap::from([
          ((0,0),[0.45,0.8])
        ])));
        assert_eq!(m.physics_vertices,physical);
        assert_eq!(m.triangles,tris);
        assert!(m.vertices.chunks_exact(FLOATS_PER_VERTEX)
          .any(|v|v[0]==0.0&&v[2]==0.0&&v[11]==0.45&&v[12]==0.8));
        assert!(m.vertices.chunks_exact(FLOATS_PER_VERTEX)
          .all(|v|(v[10]-1.245).abs()<0.0001));
        assert!(!m.set_liquid_masks(&BTreeMap::from([((0,0),[0.45,0.8])])));
    }
    #[test]
    fn snow_layer_masks_update_without_moving_collision(){
        let input=json!({"mesh":{"heightfield":{"columns":3,"rows":3,
          "heights":[0.4,0.4,0.4,0.4,0.4,0.4,0.4,0.4,0.4],
          "ground_heights":[0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0],
          "surface_kind":"snow","render_subdivisions":2,"deformable":true}}});
        let mut mesh=HeightfieldPatch::from_record(1,&input,Vec3::ZERO,
           Vec3::new(2.0,1.0,2.0),[1.0;4]).unwrap();
        let original=mesh.physics_vertices.clone();
        assert!(mesh.apply_surface_changes(&BTreeMap::from([
            ((0,0),[0.4,0.05,0.8,0.55])
        ])));
        assert_eq!(mesh.physics_vertices,original);
        assert_eq!(mesh.strata[4],[0.05,0.8,0.55]);
        assert!(mesh.vertices.chunks_exact(FLOATS_PER_VERTEX)
             .any(|v|v[0]==0.0&&v[2]==0.0&&v[11]<=0.05001&&v[12]>=0.799));
        assert!(mesh.vertices.chunks_exact(FLOATS_PER_VERTEX)
             .all(|v|v[7]>0.9&&v[8]>0.9&&v[9]>0.9));
    }
    #[test]
    fn mixed_lod_edges_share_exactly_the_same_snow_height(){
        let record=|fine:usize|json!({"mesh":{"heightfield":{
           "columns":3,"rows":3,"deformable":true,
           "heights":[0.6,0.6,0.6,0.6,0.6,0.6,0.6,0.6,0.6],
           "ground_heights":[0,0,0,0,0,0,0,0,0],
           "surface_kind":"snow","render_relief_metres":0.028,
           "render_subdivisions":fine}}});
        let fine=HeightfieldPatch::from_record(1,&record(2),
            Vec3::new(-1.0,0.0,0.0),Vec3::new(2.0,1.0,2.0),[1.0;4]).unwrap();
        let coarse=HeightfieldPatch::from_record(2,&record(1),
            Vec3::new(1.0,0.0,0.0),Vec3::new(2.0,1.0,2.0),[1.0;4]).unwrap();
        let at=|patch:&HeightfieldPatch,x:f32,z:f32|->f32{
            patch.vertices.chunks_exact(FLOATS_PER_VERTEX)
              .find(|v|(v[0]-x).abs()<1e-5&&(v[2]-z).abs()<1e-5)
              .map(|v|v[1]).unwrap()
        };
        for z in [-1.0,0.0,1.0]{
            assert!((at(&fine,0.0,z)-at(&coarse,0.0,z)).abs()<1e-5);
        }
        for (z,z0,z1) in [(-0.5,-1.0,0.0),(0.5,0.0,1.0)]{
            let interpolated=0.5*(at(&coarse,0.0,z0)+at(&coarse,0.0,z1));
            assert!((at(&fine,0.0,z)-interpolated).abs()<1e-5,
                "a fine border must follow the coarse edge exactly");
        }
        assert_eq!(fine.physics_vertices.len(),9);
        assert_eq!(coarse.physics_vertices.len(),9);
    }
    #[test]
    fn heightfield_rain_shelter_uses_local_surface() {
        let input = json!({"mesh":{"heightfield":{"columns":3,"rows":3,
            "heights":[0,0,12,0,0,12,0,0,12]}}});
        let mesh = HeightfieldPatch::from_record(
            42,
            &input,
            Vec3::ZERO,
            Vec3::new(2.0, 1.0, 2.0),
            [1.0; 4],
        )
        .unwrap();
        // The broad bounds contain a 12m hill, but the camera's cell is open.
        assert!(!mesh.upward_ray_hits(Vec3::new(-0.5, 8.0, -0.5), 0.02));
        // Both triangles interpolate the slope to 6m, not the global maximum.
        assert!(mesh.upward_ray_hits(Vec3::new(0.5, 5.0, -0.75), 0.02));
        assert!(mesh.upward_ray_hits(Vec3::new(0.5, 5.0, -0.25), 0.02));
        assert!(!mesh.upward_ray_hits(Vec3::new(0.5, 7.0, -0.25), 0.02));
        assert!(mesh.upward_ray_hits(Vec3::new(1.0, 11.0, 1.0), 0.02));
        assert!(!mesh.upward_ray_hits(Vec3::new(1.1, 5.0, 0.0), 0.02));
    }

    #[test]
    fn heightfield_has_continuous_edge_and_true_triangle_normals() {
        let input = json!({"mesh":{"heightfield":{"columns":3,"rows":3,
            "heights":[0,0,0,0,1,0,0,0,0]}}});
        let mesh = HeightfieldPatch::from_record(
            42,
            &input,
            Vec3::ZERO,
            Vec3::new(2.0, 1.0, 2.0),
            [0.4, 0.5, 0.6, 0.96],
        )
        .unwrap();
        assert_eq!(mesh.triangles.len(), 8);
        assert_eq!(mesh.vertices.len(), 8 * 3 * FLOATS_PER_VERTEX);
        assert_eq!(mesh.physics_vertices[4], [0.0, 1.0, 0.0]);
        assert!(mesh
            .vertices
            .chunks_exact(FLOATS_PER_VERTEX)
            .all(|v| v[5] > 0.0));
    }
}
