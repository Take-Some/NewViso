use super::*;
use newviso_model::MeshResource;
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct LayerGeometryReuse {
    sources: BTreeMap<String, (usize, usize, usize)>,
}

impl LayerGeometryReuse {
    pub(super) fn try_reuse(&self, model: &ModelResource, mesh: &MeshResource, ranges: &mut Vec<AssetDrawRange>) -> bool {
        // Entity-local skinning and fragment articulation cannot share writable vertices.
        if model.fragment.is_some() || model.skeleton.is_some() { return false; }
        let Some((source, suffix)) = mesh.name.rsplit_once(".newviso_ml.") else { return false; };
        if suffix.len() != 2 || !suffix.bytes().all(|b| b.is_ascii_digit()) { return false; }
        let Some(&(source_index, start, end)) = self.sources.get(source) else { return false; };
        if start == end || !same_geometry(&model.meshes[source_index], mesh) { return false; }
        let overlays = ranges[start..end].iter().map(|range| AssetDrawRange {
            mesh_name: Arc::from(mesh.name.as_str()),
            render_passes: mesh.render_passes,
            joint_lineage: Arc::clone(&range.joint_lineage),
            first_vertex: range.first_vertex,
            vertex_count: range.vertex_count,
            material_slot: mesh.primitives[0].material_slot,
            local_center: range.local_center,
        }).collect::<Vec<_>>();
        ranges.extend(overlays);
        true
    }

    pub(super) fn record(&mut self, index: usize, mesh: &MeshResource, start: usize, end: usize) {
        if mesh.primitives.len() == 1 && start < end && !mesh.name.contains(".newviso_ml.") {
            self.sources.entry(mesh.name.clone()).or_insert((index, start, end));
        }
    }
}

fn same_bytes(a: &Arc<[u8]>, b: &Arc<[u8]>) -> bool {
    Arc::ptr_eq(a,b) || a.as_ref() == b.as_ref()
}

fn same_geometry(a: &MeshResource, b: &MeshResource) -> bool {
    if a.primitives.len() != 1 || b.primitives.len() != 1 { return false; }
    let (a_p,b_p) = (&a.primitives[0], &b.primitives[0]);
    a_p.first_index == b_p.first_index
        && a_p.index_count == b_p.index_count
        && a_p.base_vertex == b_p.base_vertex
        && a.index_buffer.format == b.index_buffer.format
        && a.index_buffer.index_count == b.index_buffer.index_count
        && same_bytes(&a.index_buffer.data, &b.index_buffer.data)
        && a.vertex_streams.len() == b.vertex_streams.len()
        && a.vertex_streams.iter().zip(&b.vertex_streams).all(|(x,y)| {
            x.semantic == y.semantic && x.format == y.format && x.stride == y.stride
                && x.vertex_count == y.vertex_count && same_bytes(&x.data,&y.data)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use newviso_model::{
        Bounds3, IndexBuffer, IndexFormat, MeshPrimitive, MeshRenderPasses,
        VertexFormat, VertexSemantic, VertexStream,
    };
    use newviso_resource_runtime::AssetId;

    fn build_model(duplicate_count: usize) -> ModelResource {
        let bounds = Bounds3 { min: [0.0; 3], max: [1.0; 3] };
        let position_values = [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let position_bytes: Vec<u8> = position_values.iter().flat_map(|v| v.to_le_bytes()).collect();
        let index_bytes: Vec<u8> = [0u32, 1, 2].iter().flat_map(|v| v.to_le_bytes()).collect();
        let base = MeshResource {
            name: "coach_body".into(),
            bounds,
            render_passes: MeshRenderPasses::default(),
            vertex_streams: vec![VertexStream {
                semantic: VertexSemantic::Position,
                format: VertexFormat::Float32x3,
                stride: 12,
                vertex_count: 3,
                data: Arc::from(position_bytes),
            }],
            index_buffer: IndexBuffer {
                format: IndexFormat::U32,
                index_count: 3,
                data: Arc::from(index_bytes),
            },
            primitives: vec![MeshPrimitive {
                first_index: 0, index_count: 3, base_vertex: 0, material_slot: Some(0),
            }],
        };
        let mut meshes = vec![base.clone()];
        for n in 1..duplicate_count {
            let mut overlay = base.clone();
            overlay.name = format!("coach_body.newviso_ml.{n:02}");
            overlay.render_passes.shadow = false;
            overlay.primitives[0].material_slot = Some(n as u32);
            meshes.push(overlay);
        }
        ModelResource {
            id: AssetId(41), name: "coach".into(), bounds, meshes,
            material_slots: Vec::new(),
            skin_source_to_model: [1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.],
            skeleton: None, animations: Vec::new(), fragment: None,
        }
    }

    #[test]
    fn multilayer_20_draws_share_one_expanded_triangle() {
        let model = build_model(20);
        let (vertices, ranges) = expand_model_triangles(&model).unwrap();
        assert_eq!(vertices.len(), 3);
        assert_eq!(ranges.len(), 20);
        assert_eq!(ranges.iter().map(|r| r.first_vertex).collect::<Vec<_>>(), [0; 20]);
        assert!(ranges.iter().all(|r| r.vertex_count == 3));
        for (index, range) in ranges.iter().enumerate() {
            assert_eq!(range.material_slot, Some(index as u32));
            assert_eq!(range.render_passes.shadow, index == 0);
        }
    }

    #[test]
    fn changed_geometry_cannot_alias_original_even_with_layer_suffix() {
        let mut model = build_model(3);
        let mut different = model.meshes[1].vertex_streams[0].data.as_ref().to_vec();
        different[8] ^= 2;
        model.meshes[1].vertex_streams[0].data = Arc::from(different);
        let (vertices, ranges) = expand_model_triangles(&model).unwrap();
        assert_eq!(vertices.len(), 6);
        assert_eq!(ranges.len(), 3);
        assert_eq!(ranges[0].first_vertex, 0);
        assert_eq!(ranges[1].first_vertex, 3);
        assert_eq!(ranges[2].first_vertex, 0);
    }

    #[test]
    fn different_index_or_primitive_prevents_aliasing() {
        let mut model = build_model(2);
        model.meshes[1].primitives[0].base_vertex = 1;
        assert!(!same_geometry(&model.meshes[0], &model.meshes[1]));
        model.meshes[1].primitives[0].base_vertex = 0;
        model.meshes[1].index_buffer.data = Arc::from(vec![0; 12]);
        assert!(!same_geometry(&model.meshes[0], &model.meshes[1]));
    }
}
