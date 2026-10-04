use super::*;

/// Local convex space reserved for an entity's interior. The renderer remains
/// independent of vehicle names, particle dictionaries and camera mode.
#[derive(Clone, Debug)]
pub(super) struct ParticleInteriorBounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub planes: Vec<[f32; 4]>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ParticleInteriorStats {
    pub clipped_triangles: usize,
    pub rendered_triangles: usize,
    pub interior_triangles: usize,
}

#[derive(Clone, Debug)]
pub(super) struct ParticleInteriorVolume {
    min: [f32; 3],
    max: [f32; 3],
    planes: Vec<[f32; 4]>,
}

fn distance(plane: [f32; 4], point: [f32; 3]) -> f32 {
    (0..3).map(|i| plane[i] * point[i]).sum::<f32>() + plane[3]
}

impl ParticleInteriorVolume {
    fn new(bounds: &ParticleInteriorBounds, transform: SceneTransform) -> Self {
        let position = transform.position;
        let scales = [transform.scale.x, transform.scale.y, transform.scale.z];
        let axes = [
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ]
        .map(|v| {
            transform_point(
                v,
                Vec3::new(1.0, 1.0, 1.0),
                transform.rotation_degrees,
                Vec3::ZERO,
            )
        });
        let mut local = Vec::with_capacity(6 + bounds.planes.len());
        for i in 0..3 {
            let mut low = [0.0; 4];
            low[i] = -1.0;
            low[3] = bounds.min[i];
            local.push(low);
            let mut high = [0.0; 4];
            high[i] = 1.0;
            high[3] = -bounds.max[i];
            local.push(high);
        }
        local.extend_from_slice(&bounds.planes);
        let planes = local
            .into_iter()
            .map(|plane| {
                let n = axes[0]
                    .mul(plane[0] / scales[0])
                    .add(axes[1].mul(plane[1] / scales[1]))
                    .add(axes[2].mul(plane[2] / scales[2]));
                let length = n.length();
                [
                    n.x / length,
                    n.y / length,
                    n.z / length,
                    (plane[3] - n.dot(position)) / length,
                ]
            })
            .collect();
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for mask in 0..8 {
            let p = transform_point(
                Vec3::new(
                    if mask & 1 == 0 {
                        bounds.min[0]
                    } else {
                        bounds.max[0]
                    },
                    if mask & 2 == 0 {
                        bounds.min[1]
                    } else {
                        bounds.max[1]
                    },
                    if mask & 4 == 0 {
                        bounds.min[2]
                    } else {
                        bounds.max[2]
                    },
                ),
                transform.scale,
                transform.rotation_degrees,
                position,
            );
            for (i, v) in [p.x, p.y, p.z].into_iter().enumerate() {
                min[i] = min[i].min(v);
                max[i] = max[i].max(v);
            }
        }
        Self { min, max, planes }
    }

    fn overlaps(&self, min: [f32; 3], max: [f32; 3]) -> bool {
        (0..3).all(|i| min[i] <= self.max[i] && max[i] >= self.min[i])
    }

    /// Swept contact prevents an outside particle from tunnelling across the
    /// cabin in a long frame. Original bursts born inside retain their motion;
    /// their interior geometry is excluded by the same render clipping below.
    pub(super) fn contact(
        &self,
        before: [f32; 3],
        after: [f32; 3],
    ) -> Option<([f32; 3], [f32; 3])> {
        let min = std::array::from_fn(|i| before[i].min(after[i]));
        let max = std::array::from_fn(|i| before[i].max(after[i]));
        if !self.overlaps(min, max) || self.planes.iter().all(|p| distance(*p, before) < 0.0) {
            return None;
        }
        let (mut enter, mut exit) = (0.0f32, 1.0f32);
        let mut normal = None;
        for plane in &self.planes {
            let a = distance(*plane, before);
            let b = distance(*plane, after);
            if a > 0.0 && b > 0.0 {
                return None;
            }
            if a >= 0.0 && b < 0.0 {
                let t = a / (a - b);
                if t >= enter {
                    enter = t;
                    normal = Some([plane[0], plane[1], plane[2]]);
                }
            } else if a < 0.0 && b > 0.0 {
                exit = exit.min(a / (a - b));
            }
            if enter > exit {
                return None;
            }
        }
        let normal = normal?;
        Some((
            std::array::from_fn(|i| before[i] + (after[i] - before[i]) * enter + normal[i] * 0.002),
            normal,
        ))
    }
}

impl Scene3dRuntime {
    /// Fit the largest authored surface triangle, facing away from a supplied
    /// interior point. Used for sloping glass without clipping the air above
    /// a bonnet merely because it lies inside a window's rectangular bounds.
    pub fn entity_fragment_interior_plane(
        &self,
        entity: u64,
        names: &[String],
        inside: [f32; 3],
    ) -> Option<[f32; 4]> {
        let mesh = self.asset_meshes.get(&entity)?;
        let inside = Vec3::new(inside[0], inside[1], inside[2]);
        let mut best: Option<(f32, [f32; 4])> = None;
        for range in mesh
            .local_draw_ranges
            .iter()
            .filter(|r| names.iter().any(|n| n.as_str() == r.mesh_name.as_ref()))
        {
            let start =
                (mesh.first_vertex as usize + range.first_vertex as usize) * FLOATS_PER_VERTEX;
            let end = start + range.vertex_count as usize * FLOATS_PER_VERTEX;
            for triangle in self
                .asset_vertex_data
                .get(start..end)?
                .chunks_exact(3 * FLOATS_PER_VERTEX)
            {
                let point = |i: usize| {
                    Vec3::new(
                        triangle[i * FLOATS_PER_VERTEX],
                        triangle[i * FLOATS_PER_VERTEX + 1],
                        triangle[i * FLOATS_PER_VERTEX + 2],
                    )
                };
                let a = point(0);
                let cross = point(1).sub(a).cross(point(2).sub(a));
                let area = cross.length();
                if area < 1.0e-6 || best.as_ref().is_some_and(|(old, _)| *old >= area) {
                    continue;
                }
                let mut normal = cross.mul(area.recip());
                if normal.dot(inside.sub(a)) > 0.0 {
                    normal = normal.mul(-1.0);
                }
                best = Some((area, [normal.x, normal.y, normal.z, -normal.dot(a) + 0.005]));
            }
        }
        best.map(|(_, plane)| plane)
    }

    pub fn clear_entity_particle_interior(&mut self, entity: u64) {
        self.particle_interiors.remove(&entity);
    }

    pub fn entity_particle_interior_configured(&self, entity: u64) -> bool {
        self.particle_interiors.contains_key(&entity)
    }

    pub fn set_entity_particle_interior(
        &mut self,
        entity: u64,
        min: [f32; 3],
        max: [f32; 3],
        planes: Vec<[f32; 4]>,
    ) -> Result<(), String> {
        if min.iter().chain(max.iter()).any(|v| !v.is_finite())
            || (0..3).any(|i| max[i] - min[i] <= 0.001)
            || planes.len() > 8
            || planes.iter().any(|p| {
                p.iter().any(|v| !v.is_finite())
                    || p[..3].iter().map(|v| v * v).sum::<f32>() < 1.0e-8
            })
        {
            return Err("particle interior bounds must be finite and non-degenerate".to_owned());
        }
        self.particle_interiors
            .insert(entity, ParticleInteriorBounds { min, max, planes });
        Ok(())
    }

    pub(super) fn particle_interior_volumes(&self) -> Vec<ParticleInteriorVolume> {
        self.particle_interiors
            .iter()
            .filter_map(|(id, bounds)| {
                let entity = self.world.entity(SceneEntityId(*id))?;
                let scale = entity.transform.scale;
                ([scale.x, scale.y, scale.z]
                    .iter()
                    .all(|s| s.is_finite() && s.abs() > 1.0e-6))
                .then(|| ParticleInteriorVolume::new(bounds, entity.transform))
            })
            .collect()
    }
}

type Vertex = [f32; FLOATS_PER_VERTEX];

// A triangle intersected by at most fourteen convex planes has at most
// seventeen vertices. Stack storage avoids allocations for each clip plane.
struct Polygon {
    vertices: [Vertex; 24],
    len: usize,
}

impl Polygon {
    fn empty() -> Self {
        Self {
            vertices: [[0.0; FLOATS_PER_VERTEX]; 24],
            len: 0,
        }
    }
    fn push(&mut self, v: Vertex) {
        self.vertices[self.len] = v;
        self.len += 1;
    }
    fn append(&self, out: &mut Vec<f32>) {
        for i in 1..self.len.saturating_sub(1) {
            for v in [self.vertices[0], self.vertices[i], self.vertices[i + 1]] {
                out.extend_from_slice(&v);
            }
        }
    }
}

fn split_polygon(polygon: &Polygon, plane: [f32; 4], outside: bool) -> Polygon {
    let mut out = Polygon::empty();
    if polygon.len == 0 {
        return out;
    }
    let mut previous = polygon.vertices[polygon.len - 1];
    let mut a = distance(plane, [previous[0], previous[1], previous[2]]);
    for current in polygon.vertices[..polygon.len].iter().copied() {
        let b = distance(plane, [current[0], current[1], current[2]]);
        let keep_a = if outside { a >= 0.0 } else { a <= 0.0 };
        let keep_b = if outside { b >= 0.0 } else { b <= 0.0 };
        if keep_a != keep_b {
            let t = (a / (a - b)).clamp(0.0, 1.0);
            out.push(std::array::from_fn(|i| {
                previous[i] + (current[i] - previous[i]) * t
            }));
        }
        if keep_b {
            out.push(current);
        }
        previous = current;
        a = b;
    }
    out
}

fn clip_triangle(triangle: &[f32], volume: &ParticleInteriorVolume, out: &mut Vec<f32>) -> bool {
    let mut remaining = Polygon::empty();
    for v in triangle.chunks_exact(FLOATS_PER_VERTEX) {
        remaining.push(v.try_into().unwrap());
    }
    if volume.planes.iter().any(|p| {
        remaining.vertices[..remaining.len]
            .iter()
            .all(|v| distance(*p, [v[0], v[1], v[2]]) >= -0.000001)
    }) {
        out.extend_from_slice(triangle);
        return false;
    }
    for plane in &volume.planes {
        let outside = split_polygon(&remaining, *plane, true);
        outside.append(out);
        remaining = split_polygon(&remaining, *plane, false);
        if remaining.len < 3 {
            break;
        }
    }
    true
}

pub(super) fn clip_particle_vertices(
    out: &mut Vec<f32>,
    first: usize,
    volumes: &[ParticleInteriorVolume],
    stats: &mut ParticleInteriorStats,
) {
    if first == out.len() {
        return;
    }
    if !volumes.is_empty() {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for v in out[first..].chunks_exact(FLOATS_PER_VERTEX) {
            for i in 0..3 {
                min[i] = min[i].min(v[i]);
                max[i] = max[i].max(v[i]);
            }
        }
        let candidates = volumes
            .iter()
            .filter(|v| v.overlaps(min, max))
            .collect::<Vec<_>>();
        if !candidates.is_empty() {
            let mut input = out.split_off(first);
            let mut clipped = Vec::with_capacity(input.len());
            for volume in &candidates {
                clipped.clear();
                for triangle in input.chunks_exact(3 * FLOATS_PER_VERTEX) {
                    if clip_triangle(triangle, volume, &mut clipped) {
                        stats.clipped_triangles += 1;
                    }
                }
                std::mem::swap(&mut input, &mut clipped);
                if input.is_empty() {
                    break;
                }
            }
            // Each retained triangle must lie wholly outside at least one
            // boundary of every cabin it touched, including interpolated edges.
            for triangle in input.chunks_exact(3 * FLOATS_PER_VERTEX) {
                if candidates.iter().any(|volume| {
                    !volume.planes.iter().any(|p| {
                        triangle
                            .chunks_exact(FLOATS_PER_VERTEX)
                            .all(|v| distance(*p, [v[0], v[1], v[2]]) >= -0.0001)
                    })
                }) {
                    stats.interior_triangles += 1;
                }
            }
            out.extend_from_slice(&input);
        }
    }
    stats.rendered_triangles += (out.len() - first) / (3 * FLOATS_PER_VERTEX);
}

#[cfg(test)]
mod tests {
    use super::*;
    fn box_volume(rotation: Vec3, scale: Vec3, position: Vec3) -> ParticleInteriorVolume {
        ParticleInteriorVolume::new(
            &ParticleInteriorBounds {
                min: [-1.0; 3],
                max: [1.0; 3],
                planes: vec![],
            },
            SceneTransform {
                position,
                rotation_degrees: rotation,
                scale,
            },
        )
    }
    fn triangle(points: [[f32; 3]; 3]) -> Vec<f32> {
        let mut out = vec![];
        for p in points {
            geometry::append_particle_vertex(
                &mut out,
                Vec3::new(p[0], p[1], p[2]),
                Vec3::new(0.0, 0.0, 1.0),
                [1.0, 0.0, 1.0, 0.8],
                [p[0] * 0.25 + 0.5, p[1] * 0.25 + 0.5],
            );
        }
        out
    }
    fn area(vertices: &[f32]) -> f32 {
        vertices
            .chunks_exact(3 * FLOATS_PER_VERTEX)
            .map(|t| {
                let p = |i: usize| {
                    Vec3::new(
                        t[i * FLOATS_PER_VERTEX],
                        t[i * FLOATS_PER_VERTEX + 1],
                        t[i * FLOATS_PER_VERTEX + 2],
                    )
                };
                p(1).sub(p(0)).cross(p(2).sub(p(0))).length() * 0.5
            })
            .sum()
    }
    #[test]
    fn crossing_sprite_keeps_exterior_area_and_texture_coordinates() {
        let volume = box_volume(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO);
        let mut vertices = triangle([[-2.0, -2.0, 0.0], [2.0, -2.0, 0.0], [0.0, 2.0, 0.0]]);
        let before = area(&vertices);
        let mut stats = ParticleInteriorStats::default();
        clip_particle_vertices(&mut vertices, 0, &[volume], &mut stats);
        assert!((before - 8.0).abs() < 1e-5);
        assert!(
            (area(&vertices) - 4.5).abs() < 1e-5,
            "area={}",
            area(&vertices)
        );
        assert!(stats.clipped_triangles > 0 && stats.rendered_triangles > 0);
        assert_eq!(stats.interior_triangles, 0);
        for v in vertices.chunks_exact(FLOATS_PER_VERTEX) {
            assert!((v[11] - (v[0] * 0.25 + 0.5)).abs() < 1e-6);
            assert!((v[12] - (v[1] * 0.25 + 0.5)).abs() < 1e-6);
            assert!((v[10] - 0.8).abs() < 1e-6);
        }
    }
    #[test]
    fn wholly_inside_geometry_is_removed_but_unrelated_exterior_is_unchanged() {
        let volume = box_volume(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO);
        let mut inside = triangle([[0.0, 0.0, 0.0], [0.5, 0.0, 0.0], [0.0, 0.5, 0.0]]);
        let mut stats = ParticleInteriorStats::default();
        clip_particle_vertices(&mut inside, 0, &[volume.clone()], &mut stats);
        assert!(inside.is_empty());
        let mut outside = triangle([[2.0, 0.0, 0.0], [2.5, 0.0, 0.0], [2.0, 0.5, 0.0]]);
        let before = outside.clone();
        clip_particle_vertices(&mut outside, 0, &[volume], &mut stats);
        assert_eq!(outside, before);
    }
    #[test]
    fn rotated_scaled_moving_cabin_blocks_swept_external_motion() {
        for rotation in [
            Vec3::ZERO,
            Vec3::new(23.0, 90.0, 17.0),
            Vec3::new(180.0, -45.0, 0.0),
        ] {
            let scale = Vec3::new(2.0, 0.75, 1.25);
            let position = Vec3::new(10.0, 3.0, -7.0);
            let volume = box_volume(rotation, scale, position);
            let point = |x: f32| {
                let v = transform_point(Vec3::new(x, 0.0, 0.0), scale, rotation, position);
                [v.x, v.y, v.z]
            };
            let (hit, _) = volume
                .contact(point(-4.0), point(4.0))
                .expect("sweep must hit the cabin even when its endpoint is beyond it");
            assert!(volume.planes.iter().any(|p| distance(*p, hit) > 0.001));
            assert!(volume.contact(point(-4.0), point(-3.0)).is_none());
            let mut sprite = triangle([point(-2.0), point(2.0), {
                let v = transform_point(Vec3::new(0.0, 2.0, 0.0), scale, rotation, position);
                [v.x, v.y, v.z]
            }]);
            let mut stats = ParticleInteriorStats::default();
            clip_particle_vertices(&mut sprite, 0, &[volume], &mut stats);
            assert!(stats.clipped_triangles > 0 && !sprite.is_empty());
            assert_eq!(stats.interior_triangles, 0);
        }
    }
    #[test]
    fn sloping_windscreen_preserves_air_outside_the_glass() {
        let bounds = ParticleInteriorBounds {
            min: [-1.0; 3],
            max: [1.0; 3],
            planes: vec![[0.0, 1.0, -1.0, 0.0]],
        };
        let volume = ParticleInteriorVolume::new(
            &bounds,
            SceneTransform {
                position: Vec3::ZERO,
                rotation_degrees: Vec3::ZERO,
                scale: Vec3::new(1.0, 1.0, 1.0),
            },
        );
        let mut exterior = triangle([[-0.2, 0.7, 0.0], [0.2, 0.7, 0.0], [0.0, 0.9, 0.0]]);
        let before = exterior.clone();
        let mut stats = ParticleInteriorStats::default();
        clip_particle_vertices(&mut exterior, 0, &[volume], &mut stats);
        assert_eq!(exterior, before);
    }
}
