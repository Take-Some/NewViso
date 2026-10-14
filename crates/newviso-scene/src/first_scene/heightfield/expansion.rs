//! Runtime terrain uses the same geometry and collision parser as authored terrain.
use super::*;

impl Scene3dRuntime {
    pub fn append_runtime_heightfield(
        &mut self, key: &str, record: &Value, center: [f32; 3], size: [f32; 3], color: [f32; 4],
    ) -> Result<Option<(u64, Vec<[f32; 3]>, Vec<[u32; 3]>, u32)>, String> {
        if self.runtime_entity_stable_id(key).is_some() { return Ok(None); }
        // Validate geometry before allocating a scene entity.
        let mut patch = HeightfieldPatch::from_record(0, record,
            Vec3::new(center[0], center[1], center[2]),
            Vec3::new(size[0], size[1], size[2]), color)?;
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for xyz in &patch.physics_vertices { for axis in 0..3 {
            min[axis] = min[axis].min(xyz[axis]); max[axis] = max[axis].max(xyz[axis]);
        }}
        let entity = self.upsert_runtime_dynamic_entity(key, SceneRuntimeEntityDesc {
            visual: SceneRuntimeVisualKind::Cube, asset_ref: None, texture_dictionary: None,
            position: center, rotation_degrees: [0.0; 3], scale: size,
            bounds_half_extent: [size[0] * 0.5, (max[1] - min[1]).max(1.0) * 0.5, size[2] * 0.5],
            base_color: color, solid: false, visible_distance: 600.0,
            stream_distance: 650.0, fade_range: 20.0,
        })?;
        patch.entity = entity;
        let id = SceneEntityId(entity);
        let item = self.world.entity(id).ok_or("runtime terrain entity disappeared")?;
        let slot = item.render_slot.ok_or("runtime terrain render slot missing")?;
        self.world.update_spatial_from(id, item.transform, SceneBounds {
            min: Vec3::new(min[0], min[1], min[2]), max: Vec3::new(max[0], max[1], max[2]),
        }, SceneMutationSource::Engine)?;
        let collision = patch.collides.then(|| (entity, patch.physics_vertices.clone(),
            patch.triangles.clone(), patch.surface_id));
        self.terrain_meshes.insert(slot, patch);
        Ok(collision)
    }
}
