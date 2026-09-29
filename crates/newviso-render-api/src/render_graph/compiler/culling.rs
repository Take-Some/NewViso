use std::collections::{BTreeMap, BTreeSet};

use super::super::{
    RenderGraphCullingReport, RenderGraphDesc, RenderGraphPassId, RenderGraphResourceDesc,
    RenderGraphResourceId, RenderGraphResourceLifetime,
};

pub(super) fn cull_passes(
    graph: &RenderGraphDesc,
    dependencies: &BTreeMap<RenderGraphPassId, BTreeSet<RenderGraphPassId>>,
    last_writer: &BTreeMap<RenderGraphResourceId, RenderGraphPassId>,
    resources: &BTreeMap<RenderGraphResourceId, &RenderGraphResourceDesc>,
    raw_execution_order: &[RenderGraphPassId],
) -> RenderGraphCullingReport {
    let mut root_set = BTreeSet::new();

    for pass in &graph.passes {
        if !pass.flags.allow_culling {
            root_set.insert(pass.id);
        }
    }

    for resource in resources.values() {
        let observable = !matches!(
            resource.lifetime,
            RenderGraphResourceLifetime::TransientFrame
        ) || resource.semantic.is_surface_color();
        if observable {
            if let Some(writer) = last_writer.get(&resource.id).copied() {
                root_set.insert(writer);
            }
        }
    }

    let roots = raw_execution_order
        .iter()
        .copied()
        .filter(|pass| root_set.contains(pass))
        .collect::<Vec<_>>();
    let mut live = root_set;
    let mut stack = roots.clone();

    while let Some(pass) = stack.pop() {
        if let Some(producers) = dependencies.get(&pass) {
            for producer in producers {
                if live.insert(*producer) {
                    stack.push(*producer);
                }
            }
        }
    }

    let live_passes = raw_execution_order
        .iter()
        .copied()
        .filter(|pass| live.contains(pass))
        .collect::<Vec<_>>();
    let culled_passes = raw_execution_order
        .iter()
        .copied()
        .filter(|pass| !live.contains(pass))
        .collect::<Vec<_>>();

    RenderGraphCullingReport {
        roots,
        live_passes,
        culled_passes,
    }
}
