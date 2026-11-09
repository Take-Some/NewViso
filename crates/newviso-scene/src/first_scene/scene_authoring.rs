//! Scene-owned authoring queries. No UI or input policy belongs here.
use super::*;
use crate::math::transform_point;
#[derive(Clone, Copy, Debug)]
pub struct SceneAuthoringCamera {
    pub position: [f32; 3],
    pub target: [f32; 3],
    pub up: [f32; 3],
    pub fov_y_degrees: f32,
    pub near: f32,
    pub far: f32,
}
fn array(v: Vec3) -> [f32; 3] {
    [v.x, v.y, v.z]
}
fn vec(v: [f32; 3]) -> Vec3 {
    Vec3::new(v[0], v[1], v[2])
}
fn box_hit(o: Vec3, d: Vec3, b: SceneBounds) -> Option<f32> {
    let o = array(o);
    let d = array(d);
    let min = array(b.min);
    let max = array(b.max);
    let mut lo = 0f32;
    let mut hi = f32::INFINITY;
    for i in 0..3 {
        if d[i].abs() < 1e-10 {
            if o[i] < min[i] || o[i] > max[i] {
                return None;
            }
        } else {
            let a = (min[i] - o[i]) / d[i];
            let b = (max[i] - o[i]) / d[i];
            lo = lo.max(a.min(b));
            hi = hi.min(a.max(b));
            if hi < lo {
                return None;
            }
        }
    }
    Some(lo)
}
fn triangle_hit(o: Vec3, d: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let e = b.sub(a);
    let f = c.sub(a);
    let p = d.cross(f);
    let det = e.dot(p);
    if det.abs() < 1e-8 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o.sub(a);
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = f.dot(q) * inv;
    (t >= 0.0 && t.is_finite()).then_some(t)
}
fn inverse_ray(o: Vec3, d: Vec3, t: SceneTransform) -> (Vec3, Vec3) {
    let axes = [Vec3::new(1.0, 0.0, 0.0), Vec3::Y, Vec3::new(0.0, 0.0, 1.0)]
        .map(|a| transform_point(a, Vec3::ONE, t.rotation_degrees, Vec3::ZERO));
    let p = o.sub(t.position);
    let s = array(t.scale);
    (
        vec(std::array::from_fn(|i| p.dot(axes[i]) / s[i])),
        vec(std::array::from_fn(|i| d.dot(axes[i]) / s[i])),
    )
}
impl Scene3dRuntime {
    pub fn spawn_authoring_entity(
        &mut self,
        identity: u64,
        desc: SceneRuntimeEntityDesc,
    ) -> Result<u64, String> {
        if !self.editor_camera_detached {
            return Err("Authoring entities require editor mode".into());
        }
        let mut suffix = self.next_runtime_entity_id;
        loop {
            let key = format!("newviso.editor.{identity}.{suffix}");
            if self.world.entity_id_by_name(&key).is_none() {
                return self.upsert_runtime_dynamic_entity(&key, desc);
            }
            suffix = suffix
                .checked_add(1)
                .ok_or("Authoring entity id overflow")?;
        }
    }
    pub fn authoring_binding(&self, source_index: usize) -> Option<u64> {
        self.authoring_entity_ids
            .get(source_index)
            .copied()
            .flatten()
    }
    pub fn authoring_camera(&self) -> SceneAuthoringCamera {
        let c = &self.camera;
        SceneAuthoringCamera {
            position: array(c.position),
            target: array(c.target),
            up: array(c.up),
            fov_y_degrees: c.fov_y_degrees,
            near: c.near,
            far: c.far,
        }
    }
    pub fn authoring_bounds(&self, id: u64) -> Option<([f32; 3], [f32; 3])> {
        let e = self.world.entity(SceneEntityId(id))?;
        Some((array(e.bounds.min), array(e.bounds.max)))
    }
    pub fn set_authoring_transform(
        &mut self,
        id: u64,
        position: [f32; 3],
        rotation: [f32; 3],
        scale: [f32; 3],
    ) -> Result<(), String> {
        if !self.editor_camera_detached {
            return Err("Authoring transforms require editor mode".into());
        }
        let e = self
            .world
            .entity(SceneEntityId(id))
            .ok_or("Authoring entity disappeared")?;
        if e.render_slot
            .is_some_and(|slot| self.terrain_meshes.contains_key(&slot))
        {
            return Err("Heightfields use a baked world grid; edit their source terrain rather than a mesh gizmo".into());
        }
        if scale.iter().any(|v| !v.is_finite() || v.abs() < 0.0001) {
            return Err("Authoring scale must be finite and nonzero".into());
        }
        self.set_entity_transform_from(id, position, rotation, scale, SceneMutationSource::Engine)
    }
    /// Broad phase is sorted front-to-back; resident imported meshes use their
    /// actual local triangles, cubes their oriented box, terrain its physical grid.
    /// Unloaded assets and nonvisual actors retain an authored-bounds proxy.
    pub fn authoring_pick(
        &self,
        bindings: &[(u64, u64)],
        origin: [f32; 3],
        direction: [f32; 3],
    ) -> Option<u64> {
        let o = vec(origin);
        let d = vec(direction).normalized();
        if d.length() < 0.5 {
            return None;
        }
        let mut candidates = bindings
            .iter()
            .filter_map(|(identity, id)| {
                let e = self.world.entity(SceneEntityId(*id))?;
                if e.lifecycle != SceneLifecycle::Active || !e.visibility.all_visible() {
                    return None;
                }
                box_hit(o, d, e.bounds)
                    .filter(|distance| *distance > 0.0 || e.kind != SceneEntityKind::Camera)
                    .map(|distance| (distance, *identity, *id))
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut best = f32::INFINITY;
        let mut selected = None;
        for (broad, identity, id) in candidates {
            if broad > best {
                break;
            }
            let e = self
                .world
                .entity(SceneEntityId(id))
                .expect("candidate entity");
            let hit = if let Some(patch) = e
                .render_slot
                .and_then(|slot| self.terrain_meshes.get(&slot))
            {
                patch
                    .triangles
                    .iter()
                    .filter_map(|ids| {
                        let [a, b, c] = ids.map(|i| vec(patch.physics_vertices[i as usize]));
                        triangle_hit(o, d, a, b, c)
                    })
                    .min_by(f32::total_cmp)
            } else if e.render_slot.is_some() {
                let (o, d) = inverse_ray(o, d, e.transform);
                box_hit(
                    o,
                    d,
                    SceneBounds {
                        min: Vec3::new(-0.5, -0.5, -0.5),
                        max: Vec3::new(0.5, 0.5, 0.5),
                    },
                )
            } else if let Some(vertices) = self
                .asset_meshes
                .get(&id)
                .and_then(|mesh| self.asset_model_cache.get(&mesh.source_model_id.0))
            {
                let (o, d) = inverse_ray(o, d, e.transform);
                vertices
                    .chunks_exact(3)
                    .filter_map(|v| triangle_hit(o, d, v[0].position, v[1].position, v[2].position))
                    .min_by(f32::total_cmp)
            } else {
                Some(broad)
            };
            if let Some(t) = hit {
                if t < best {
                    best = t;
                    selected = Some(identity);
                }
            }
        }
        selected
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn precise_pick_rejects_empty_part_of_triangle_bounds_and_is_double_sided() {
        let a = vec([0.0, 0.0, 0.0]);
        let b = vec([1.0, 0.0, 0.0]);
        let c = vec([0.0, 1.0, 0.0]);
        assert!(triangle_hit(vec([0.8, 0.8, 2.0]), vec([0.0, 0.0, -1.0]), a, b, c).is_none());
        assert_eq!(
            triangle_hit(vec([0.2, 0.2, 2.0]), vec([0.0, 0.0, -1.0]), a, b, c),
            Some(2.0)
        );
        assert_eq!(
            triangle_hit(vec([0.2, 0.2, -2.0]), vec([0.0, 0.0, 1.0]), a, b, c),
            Some(2.0)
        );
    }
    #[test]
    fn inverse_ray_keeps_world_distance_for_rotated_scaled_cubes() {
        let t = SceneTransform {
            position: vec([3.0, 1.0, 0.0]),
            rotation_degrees: vec([0.0, 0.0, 90.0]),
            scale: vec([2.0, 4.0, 1.0]),
        };
        let (o, d) = inverse_ray(vec([3.0, 1.0, 5.0]), vec([0.0, 0.0, -1.0]), t);
        assert_eq!(
            box_hit(
                o,
                d,
                SceneBounds {
                    min: vec([-0.5; 3]),
                    max: vec([0.5; 3])
                }
            ),
            Some(4.5)
        );
    }
}
