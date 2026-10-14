//! Cache immutable primitive geometry, including the expensive terrain prefix.
//! Cube snapshots contain exact float bits; terrain snapshots retain immutable
//! Arc allocations, so replacement and allocator pointer reuse cannot be missed.
use super::*;
use std::sync::Arc;

#[derive(Debug)]
enum PrimitiveEntry {
    Cube(usize, [u32; 13]),
    Terrain(usize, Arc<[f32]>),
}
impl PartialEq for PrimitiveEntry {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Cube(slot, a), Self::Cube(other_slot, b)) => slot == other_slot && a == b,
            (Self::Terrain(slot, a), Self::Terrain(other_slot, b)) => {
                slot == other_slot && Arc::ptr_eq(a, b)
            }
            _ => false,
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct PrimitiveGeometryCache {
    entries: Vec<PrimitiveEntry>,
    vertices: Option<Arc<[f32]>>,
}
impl PrimitiveGeometryCache {
    fn lookup(&self, entries: &[PrimitiveEntry]) -> Option<Arc<[f32]>> {
        (self.entries == entries)
            .then(|| self.vertices.as_ref().map(Arc::clone))
            .flatten()
    }
    fn install(&mut self, entries: Vec<PrimitiveEntry>, vertices: Arc<[f32]>) -> Arc<[f32]> {
        self.entries = entries;
        self.vertices = Some(Arc::clone(&vertices));
        vertices
    }
}

impl Scene3dRuntime {
    // A stable terrain prefix keeps its GPU offsets when camera visibility
    // changes the small cube/overlay tail. Every material pass uses this order.
    pub(super) fn primitive_main_render_slots(&self) -> Vec<usize> {
        self.terrain_first_slots(self.frame_plan.visible_render_slots.iter().copied())
    }
    pub(super) fn primitive_shadow_render_slots(&self) -> Vec<usize> {
        self.terrain_first_slots(self.world.active_render_slots())
    }
    fn terrain_first_slots(&self, slots: impl IntoIterator<Item = usize>) -> Vec<usize> {
        let (mut terrain, other): (Vec<_>, Vec<_>) = slots
            .into_iter()
            .filter(|slot| !self.water_render_slots.contains(slot))
            .partition(|slot| self.terrain_meshes.contains_key(slot));
        terrain.extend(other);
        terrain
    }
    pub(super) fn cached_primitive_main_blocks(&mut self,aspect:f32)->Vec<Arc<[f32]>>{
        vec![self.cached_primitive_main_tail(aspect)]
    }
    pub(super) fn cached_primitive_shadow_blocks(&mut self)->Vec<Arc<[f32]>>{
        vec![self.cached_primitive_shadow_tail()]
    }

    fn primitive_entries(&self, slots: impl IntoIterator<Item = usize>) -> Vec<PrimitiveEntry> {
        slots
            .into_iter()
            .filter(|slot| !self.water_render_slots.contains(slot)&&!self.terrain_meshes.contains_key(slot))
            .filter_map(|slot| {
                let cube = self.cubes.get(slot)?;
                if let Some(terrain) = self.terrain_meshes.get(&slot) {
                    Some(PrimitiveEntry::Terrain(slot, Arc::clone(&terrain.vertices)))
                } else {
                    Some(PrimitiveEntry::Cube(
                        slot,
                        [
                            cube.position.x.to_bits(),
                            cube.position.y.to_bits(),
                            cube.position.z.to_bits(),
                            cube.rotation_degrees.x.to_bits(),
                            cube.rotation_degrees.y.to_bits(),
                            cube.rotation_degrees.z.to_bits(),
                            cube.scale.x.to_bits(),
                            cube.scale.y.to_bits(),
                            cube.scale.z.to_bits(),
                            cube.base_color[0].to_bits(),
                            cube.base_color[1].to_bits(),
                            cube.base_color[2].to_bits(),
                            cube.base_color[3].to_bits(),
                        ],
                    ))
                }
            })
            .collect()
    }
    fn cached_primitive_main_tail(&mut self, aspect: f32) -> Arc<[f32]> {
        if !self.transient_spheres.is_empty()
            || !self.surface_marks.is_empty()
            || !self.overlay_quads.is_empty()
        {
            return Arc::from(self.build_cube_vertices_without_terrain(aspect));
        }
        let entries = self.primitive_entries(self.frame_plan.visible_render_slots.iter().copied());
        if let Some(vertices) = self.primitive_main_cache.lookup(&entries) {
            return vertices;
        }
        let vertices = Arc::from(self.build_cube_vertices_without_terrain(aspect));
        self.primitive_main_cache.install(entries, vertices)
    }
    fn cached_primitive_shadow_tail(&mut self) -> Arc<[f32]> {
        if !self.transient_spheres.is_empty() {
            return Arc::from(self.build_shadow_vertices_without_terrain());
        }
        let entries = self.primitive_entries(self.world.active_render_slots());
        if let Some(vertices) = self.primitive_shadow_cache.lookup(&entries) {
            return vertices;
        }
        let vertices = Arc::from(self.build_shadow_vertices_without_terrain());
        self.primitive_shadow_cache.install(entries, vertices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn primitive_mutations_visibility_and_terrain_replacement_invalidate_cached_output() {
        let terrain: Arc<[f32]> = Arc::from([0., 1., 2.]);
        let output: Arc<[f32]> = Arc::from([9.; 17]);
        let mut cache = PrimitiveGeometryCache::default();
        cache.install(
            vec![
                PrimitiveEntry::Cube(0, [0; 13]),
                PrimitiveEntry::Terrain(1, Arc::clone(&terrain)),
            ],
            Arc::clone(&output),
        );
        let unchanged = vec![
            PrimitiveEntry::Cube(0, [0; 13]),
            PrimitiveEntry::Terrain(1, Arc::clone(&terrain)),
        ];
        assert!(Arc::ptr_eq(&cache.lookup(&unchanged).unwrap(), &output));
        let mut moved = [0; 13];
        moved[0] = 1.0f32.to_bits();
        assert!(cache
            .lookup(&[
                PrimitiveEntry::Cube(0, moved),
                PrimitiveEntry::Terrain(1, Arc::clone(&terrain))
            ])
            .is_none());
        assert!(cache
            .lookup(&[PrimitiveEntry::Terrain(1, Arc::clone(&terrain))])
            .is_none());
        // Same values in a different allocation represent a replacement, not
        // permission to reuse a possibly obsolete packed mesh.
        assert!(cache
            .lookup(&[
                PrimitiveEntry::Cube(0, [0; 13]),
                PrimitiveEntry::Terrain(1, Arc::from([0., 1., 2.]))
            ])
            .is_none());
        assert!(cache
            .lookup(&[
                PrimitiveEntry::Cube(0, [0; 13]),
                PrimitiveEntry::Terrain(2, terrain)
            ])
            .is_none());
    }
}
