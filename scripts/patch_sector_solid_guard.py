from pathlib import Path
p=Path("crates/newviso-runtime/src/application_sector_streaming.rs")
s=p.read_text(encoding="utf-8")
def patch(a,b):
 global s
 if s.count(a)!=1: raise Exception(("mismatch",a[:80],s.count(a)))
 s=s.replace(a,b)
patch("        stable_ids: Vec<u64>,\n    },\n    Resident {\n        names: Vec<String>,\n        stable_ids: Vec<u64>,\n    },","        stable_ids: Vec<u64>,\n        solid_ids: Vec<u64>,\n    },\n    Resident {\n        names: Vec<String>,\n        stable_ids: Vec<u64>,\n        solid_ids: Vec<u64>,\n    },")
patch("                        entities, cursor: 0, names: Vec::new(), stable_ids: Vec::new(),","                        entities, cursor: 0, names: Vec::new(), stable_ids: Vec::new(),\n                        solid_ids: Vec::new(),")
patch("SectorStage::Ready { entities, cursor, names, stable_ids }","SectorStage::Ready { entities, cursor, names, stable_ids, solid_ids }")
patch("let stable_id = app.scene.upsert_streamed_static_mesh(&name, desc, bounds)?;","let is_solid = desc.solid;\n                    let stable_id = app.scene.upsert_streamed_static_mesh(&name, desc, bounds)?;")
patch("names.push(name);\n                    stable_ids.push(stable_id);","names.push(name);\n                    stable_ids.push(stable_id);\n                    if is_solid { solid_ids.push(stable_id); }")
patch("SectorStage::Ready { names, stable_ids, .. }) = self.stages.remove(&id) {\n                    self.stages.insert(id, SectorStage::Resident { names, stable_ids });","SectorStage::Ready { names, stable_ids, solid_ids, .. }) = self.stages.remove(&id) {\n                    self.stages.insert(id, SectorStage::Resident { names, stable_ids, solid_ids });")
patch("SectorStage::Resident { names, stable_ids } => {","SectorStage::Resident { names, stable_ids, .. } => {")
patch("Some(SectorStage::Resident { stable_ids, .. }) => {\n                    if stable_ids.iter().any(|id|\n                        app.scene_materialization_pending.contains(id)\n                        || !app.scene_stream_claims.contains_key(id)\n                    ) {","Some(SectorStage::Resident { solid_ids, .. }) => {\n                    if solid_ids.iter().any(|id| {\n                        let bounds=app.scene.streamed_entity_bounds_xz(*id);\n                        let near=bounds.is_some_and(|b| {\n                            let dx=(b[0]-focus[0]).max(focus[0]-b[2]).max(0.0);\n                            let dz=(b[1]-focus[2]).max(focus[2]-b[3]).max(0.0);\n                            dx*dx+dz*dz <= 120.0*120.0\n                        });\n                        near && (app.scene_materialization_pending.contains(id)\n                            || !app.scene_stream_claims.contains_key(id))\n                    }) {")
p.write_text(s,encoding="utf-8")
q=Path("crates/newviso-scene/src/first_scene/scene_entities.rs")
t=q.read_text(encoding="utf-8")
needle="    pub fn upsert_streamed_static_mesh(\n"
assert t.count(needle)==1
snippet="""    /// Precise world-space collider bounds for local sector safety checks.
    pub fn streamed_entity_bounds_xz(&self, stable_id: u64) -> Option<[f32; 4]> {
        let entity = self.world.entity(SceneEntityId(stable_id))?;
        Some([entity.bounds.min.x, entity.bounds.min.z,
              entity.bounds.max.x, entity.bounds.max.z])
    }

"""
q.write_text(t.replace(needle,snippet+needle),encoding="utf-8")
print("PATCHED_COLLISION_GUARD_FOR_NEAR_SOLIDS")
