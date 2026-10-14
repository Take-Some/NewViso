use super::*;

fn vector(value: [f32; 3]) -> Vec3 { Vec3::new(value[0], value[1], value[2]) }
fn array(value: Vec3) -> [f32; 3] { [value.x, value.y, value.z] }
fn basis(rotation: [f32; 3]) -> [Vec3; 3] {
    [Vec3::new(1.0, 0.0, 0.0), Vec3::Y, Vec3::new(0.0, 0.0, 1.0)]
        .map(|axis| transform_point(axis, Vec3::ONE, vector(rotation), Vec3::ZERO))
}

fn local_mark(mut mark: SceneSurfaceMark, owner: u64, position: [f32; 3], rotation: [f32; 3], scale: [f32; 3]) -> Option<SceneSurfaceMark> {
    if scale.iter().any(|x| !x.is_finite() || x.abs() < 1.0e-6) { return None; }
    let axes = basis(rotation);
    let offset = vector(mark.position).sub(vector(position));
    let normal = vector(mark.normal);
    mark.position = std::array::from_fn(|i| offset.dot(axes[i]) / scale[i]);
    // Normals use the transpose when entering local space and the inverse
    // transpose on return, including nonuniform or mirrored owner scales.
    mark.normal = array(vector(std::array::from_fn(|i| normal.dot(axes[i]) * scale[i])).normalized());
    mark.owner = Some(owner);
    Some(mark)
}

fn world_mark(mut mark: SceneSurfaceMark, position: [f32; 3], rotation: [f32; 3], scale: [f32; 3]) -> Option<SceneSurfaceMark> {
    if scale.iter().any(|x| !x.is_finite() || x.abs() < 1.0e-6) { return None; }
    let axes = basis(rotation);
    mark.position = array(transform_point(vector(mark.position), vector(scale), vector(rotation), vector(position)));
    mark.normal = array(axes[0].mul(mark.normal[0] / scale[0])
        .add(axes[1].mul(mark.normal[1] / scale[1]))
        .add(axes[2].mul(mark.normal[2] / scale[2])).normalized());
    Some(mark)
}

impl Scene3dRuntime {
    pub fn add_attached_surface_mark(&mut self, owner: u64, mark: SceneSurfaceMark) -> Result<(), String> {
        let Some((position, rotation, scale)) = self.entity_transform_values(owner) else {
            // A delayed impact can outlive its receiver. It must never turn into
            // an unowned world-space mark when that receiver has disappeared.
            return Ok(());
        };
        let Some(mark) = local_mark(mark, owner, position, rotation, scale) else { return Ok(()); };
        self.add_surface_mark(mark)
    }

    pub(super) fn surface_mark_world(&self, mark: SceneSurfaceMark) -> Option<SceneSurfaceMark> {
        let Some(owner) = mark.owner else { return Some(mark); };
        let (position, rotation, scale) = self.entity_transform_values(owner)?;
        world_mark(mark, position, rotation, scale)
    }

    pub fn clear_surface_marks_for_entity(&mut self, owner: u64) {
        self.surface_marks.retain(|mark| mark.owner != Some(owner));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mark() -> SceneSurfaceMark { SceneSurfaceMark { position: [2.0, 3.0, 4.0], normal: [0.0, 0.0, 1.0], radius: 0.022, color: [0.0, 0.0, 0.0, 1.0], owner: None } }
    fn close(a: [f32; 3], b: [f32; 3]) {
        for i in 0..3 { assert!((a[i] - b[i]).abs() < 1.0e-5, "{a:?} != {b:?}"); }
    }
    #[test]
    fn surface_mark_attachment_round_trips_rotated_nonuniform_owner() {
        let original = mark();
        let p = [7.0, -3.0, 2.0]; let r = [23.0, -54.0, 71.0]; let s = [2.0, 0.4, -3.0];
        let local = local_mark(original, 17, p, r, s).unwrap();
        let world = world_mark(local, p, r, s).unwrap();
        close(world.position, original.position); close(world.normal, original.normal);
        assert_eq!(world.owner, Some(17));
    }
    #[test]
    fn surface_mark_attachment_follows_translation_and_rotation() {
        let local = local_mark(mark(), 4, [0.0; 3], [0.0; 3], [1.0; 3]).unwrap();
        let moved = world_mark(local, [10.0, 0.0, 0.0], [0.0, 90.0, 0.0], [1.0; 3]).unwrap();
        close(moved.position, [14.0, 3.0, -2.0]); close(moved.normal, [1.0, 0.0, 0.0]);
    }
    #[test]
    fn surface_mark_attachment_rejects_singular_transforms() {
        assert!(local_mark(mark(), 1, [0.0; 3], [0.0; 3], [1.0, 0.0, 1.0]).is_none());
    }
}
