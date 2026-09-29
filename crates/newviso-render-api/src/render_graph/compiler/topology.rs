use std::collections::{BTreeMap, BTreeSet};

use super::super::{
    RenderGraphDependencyEdge, RenderGraphDependencyKind, RenderGraphPassId, RenderGraphResourceId,
    RenderGraphValidationIssue,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn add_dependency(
    producer: RenderGraphPassId,
    consumer: RenderGraphPassId,
    resource: RenderGraphResourceId,
    kind: RenderGraphDependencyKind,
    dependencies: &mut BTreeMap<RenderGraphPassId, BTreeSet<RenderGraphPassId>>,
    reverse_edges: &mut BTreeMap<RenderGraphPassId, BTreeSet<RenderGraphPassId>>,
    edge_keys: &mut BTreeSet<(
        RenderGraphPassId,
        RenderGraphPassId,
        RenderGraphResourceId,
        RenderGraphDependencyKind,
    )>,
    edges: &mut Vec<RenderGraphDependencyEdge>,
) {
    dependencies.entry(consumer).or_default().insert(producer);
    reverse_edges.entry(producer).or_default().insert(consumer);
    if edge_keys.insert((producer, consumer, resource, kind)) {
        edges.push(RenderGraphDependencyEdge {
            producer,
            consumer,
            resource,
            kind,
        });
    }
}

pub(super) fn topological_order(
    dependencies: &BTreeMap<RenderGraphPassId, BTreeSet<RenderGraphPassId>>,
    reverse_edges: &BTreeMap<RenderGraphPassId, BTreeSet<RenderGraphPassId>>,
    declaration_order: &BTreeMap<RenderGraphPassId, usize>,
) -> Result<Vec<RenderGraphPassId>, Vec<RenderGraphValidationIssue>> {
    let mut indegree: BTreeMap<RenderGraphPassId, usize> = dependencies
        .iter()
        .map(|(pass, deps)| (*pass, deps.len()))
        .collect();
    let mut ready = BTreeSet::<(usize, RenderGraphPassId)>::new();

    for (pass, count) in &indegree {
        if *count == 0 {
            ready.insert((*declaration_order.get(pass).unwrap_or(&usize::MAX), *pass));
        }
    }

    let mut execution_order = Vec::with_capacity(indegree.len());
    while let Some(&(index, pass)) = ready.iter().next() {
        ready.remove(&(index, pass));
        execution_order.push(pass);

        if let Some(consumers) = reverse_edges.get(&pass) {
            for consumer in consumers {
                let count = indegree
                    .get_mut(consumer)
                    .expect("dependency graph consumer missing from indegree map");
                *count = count.saturating_sub(1);
                if *count == 0 {
                    ready.insert((
                        *declaration_order.get(consumer).unwrap_or(&usize::MAX),
                        *consumer,
                    ));
                }
            }
        }
    }

    if execution_order.len() != indegree.len() {
        let blocked = indegree
            .into_iter()
            .filter_map(|(pass, count)| (count > 0).then_some(format!("{:?}", pass)))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(vec![RenderGraphValidationIssue::new(
            "render_graph.cycle",
            format!("render graph has a dependency cycle involving passes: {blocked}"),
        )]);
    }

    Ok(execution_order)
}
