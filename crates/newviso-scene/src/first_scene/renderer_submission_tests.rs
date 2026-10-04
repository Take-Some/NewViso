use super::*;

#[test]
fn deferred_graph_exposes_reference_postfx_side_signals() {
    let graph = main_deferred_hdr_render_graph(7, 1920, 1080, FrameCameraContext::default());
    let kinds = graph
        .passes
        .iter()
        .map(|pass| pass.kind)
        .collect::<Vec<_>>();
    assert!(graph.visibility.gpu_visibility_enabled);
    assert!(graph.visibility.hiz_enabled);
    assert!(!graph.visibility.pvs_sort_enabled);
    assert!(!graph.visibility.zone_cull_enabled);
    assert_eq!(graph.camera, FrameCameraContext::default().sanitized());
    assert_eq!(
        kinds,
        vec![
            RenderGraphPassKind::VisibilityCull,
            RenderGraphPassKind::ForwardOpaque,
            RenderGraphPassKind::GBuffer,
            RenderGraphPassKind::DeferredLighting,
            RenderGraphPassKind::Transparent,
            RenderGraphPassKind::ScreenSpaceReflections,
            RenderGraphPassKind::BloomExtract,
            RenderGraphPassKind::PostFx,
        ]
    );
    assert!(graph
        .resources
        .iter()
        .any(
            |resource| resource.label.as_deref() == Some("newviso.main.visibility.candidates")
                && resource.usage == RenderGraphResourceUsage::StorageBuffer
        ));
    assert!(graph
        .resources
        .iter()
        .any(
            |resource| resource.label.as_deref() == Some("newviso.main.visibility.indirect")
                && resource.usage == RenderGraphResourceUsage::StorageBuffer
        ));
    assert!(graph
        .resources
        .iter()
        .any(|resource| resource.semantic == RenderGraphResourceSemantic::ScreenSpaceReflection));
    assert!(graph
        .resources
        .iter()
        .any(|resource| resource.semantic == RenderGraphResourceSemantic::BloomComposite));
    assert!(graph.resources.iter().any(|resource| resource.semantic
        == RenderGraphResourceSemantic::LitColor
        && resource.format == Some(RenderTextureFormat::Rgba16Float)));
}

#[test]
fn material_runs_preserve_material_and_offset_boundaries() {
    let stride = HIZ_INDIRECT_STRIDE;
    let draws = [
        (7, 0),
        (7, stride),
        (8, 2 * stride),
        (8, 4 * stride),
        (7, 5 * stride),
    ];
    assert_eq!(
        material_indirect_runs(&draws).collect::<Vec<_>>(),
        vec![
            (7, 0, 2),
            (8, 2 * stride, 1),
            (8, 4 * stride, 1),
            (7, 5 * stride, 1)
        ]
    );
    assert_eq!(material_indirect_runs(&[]).count(), 0);
}

fn test_batch(ids: &[u64]) -> GpuResidentInstanceBatch {
    GpuResidentInstanceBatch {
        model_id: 7,
        first_instance: 100,
        instance_count: ids.len() as u32,
        stable_ids: ids.to_vec(),
        sphere: [0.0, 0.0, 0.0, 1.0],
    }
}

#[test]
fn visible_batch_runs_coalesce_contiguous_members() {
    let batch = test_batch(&[10, 11, 12, 13, 14, 15, 16]);
    let visible_slots = BTreeMap::from([(10, 100), (11, 101), (13, 103), (14, 104), (15, 105)]);
    assert_eq!(
        visible_batch_runs(&batch, &visible_slots).collect::<Vec<_>>(),
        vec![
            VisibleBatchRun {
                first_member: 0,
                member_count: 2,
            },
            VisibleBatchRun {
                first_member: 3,
                member_count: 3,
            },
        ]
    );
}

#[test]
fn visible_batch_runs_keep_full_batch_single() {
    let batch = test_batch(&[20, 21, 22]);
    let visible_slots = BTreeMap::from([(20, 100), (21, 101), (22, 102)]);
    assert_eq!(
        visible_batch_runs(&batch, &visible_slots).collect::<Vec<_>>(),
        vec![VisibleBatchRun {
            first_member: 0,
            member_count: 3,
        }]
    );
}
