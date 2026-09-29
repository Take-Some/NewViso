mod culling;
mod lifetime;
mod topology;

use std::collections::{BTreeMap, BTreeSet};

use culling::cull_passes;
use lifetime::analyze_resource_lifetimes;
use topology::{add_dependency, topological_order};

use super::{
    CompiledRenderGraphPass, RenderGraphBarrierStats, RenderGraphCompilation,
    RenderGraphCompileReport, RenderGraphCompiledDag, RenderGraphDependencyKind, RenderGraphDesc,
    RenderGraphLifetimeStats, RenderGraphPassId, RenderGraphResourceId,
    RenderGraphResourceLifetime, RenderGraphValidationIssue, RenderGraphValidationReport,
};

pub fn validate_and_compile_render_graph(graph: &RenderGraphDesc) -> RenderGraphValidationReport {
    match compile_render_graph_v2(graph) {
        Ok(compilation) => RenderGraphValidationReport {
            ok: true,
            errors: Vec::new(),
            warnings: compilation.report.warnings.clone(),
            compile: Some(compilation.report),
        },
        Err(errors) => RenderGraphValidationReport {
            ok: false,
            errors,
            warnings: Vec::new(),
            compile: None,
        },
    }
}

/// Compatibility entry point for existing render providers.
///
/// New frame-graph code should use [`compile_render_graph_v2`] when it needs the
/// compiled dependency DAG in addition to the summary report.
pub fn compile_render_graph(
    graph: &RenderGraphDesc,
) -> Result<RenderGraphCompileReport, Vec<RenderGraphValidationIssue>> {
    compile_render_graph_v2(graph).map(|compilation| compilation.report)
}

/// Compiles the declarative render graph into a validated dependency DAG.
///
/// Phase 3 computes live resource lifetime intervals after conservative pass culling.
/// Phase 4 then derives a deterministic transient allocation/alias plan from those
/// intervals. Backend allocation, barrier generation and queue scheduling remain later
/// phases. Raw hazard edges are retained for diagnostics and later compilers.
pub fn compile_render_graph_v2(
    graph: &RenderGraphDesc,
) -> Result<RenderGraphCompilation, Vec<RenderGraphValidationIssue>> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut resources = BTreeMap::new();
    let mut pass_ids = BTreeSet::new();
    let mut pass_order = BTreeMap::new();
    let mut lifetime = RenderGraphLifetimeStats::default();

    for resource in &graph.resources {
        if resources.insert(resource.id, resource).is_some() {
            errors.push(
                RenderGraphValidationIssue::new(
                    "render_graph.duplicate_resource",
                    "render graph contains duplicate resource id",
                )
                .with_resource(resource.id),
            );
        }
        match resource.lifetime {
            RenderGraphResourceLifetime::Persistent | RenderGraphResourceLifetime::External => {
                lifetime.persistent = lifetime.persistent.saturating_add(1);
            }
            RenderGraphResourceLifetime::TransientFrame
            | RenderGraphResourceLifetime::Frames(_) => {
                lifetime.transient = lifetime.transient.saturating_add(1);
            }
        }
    }

    let mut creator_by_resource = BTreeMap::<RenderGraphResourceId, RenderGraphPassId>::new();

    for (index, pass) in graph.passes.iter().enumerate() {
        if !pass_ids.insert(pass.id) {
            errors.push(
                RenderGraphValidationIssue::new(
                    "render_graph.duplicate_pass",
                    "render graph contains duplicate pass id",
                )
                .with_pass(pass.id),
            );
        } else {
            pass_order.insert(pass.id, index);
        }

        for created in &pass.creates {
            if !resources.contains_key(created) {
                errors.push(
                    RenderGraphValidationIssue::new(
                        "render_graph.unknown_created_resource",
                        "render graph pass creates a resource that is not declared",
                    )
                    .with_pass(pass.id)
                    .with_resource(*created),
                );
                continue;
            }
            if let Some(previous) = creator_by_resource.insert(*created, pass.id) {
                errors.push(
                    RenderGraphValidationIssue::new(
                        "render_graph.duplicate_create",
                        format!(
                            "render graph resource is created by more than one pass (previous producer {:?})",
                            previous
                        ),
                    )
                    .with_pass(pass.id)
                    .with_resource(*created),
                );
            }
        }

        for access in pass.reads.iter().chain(pass.writes.iter()) {
            if !resources.contains_key(&access.resource) {
                errors.push(
                    RenderGraphValidationIssue::new(
                        "render_graph.unknown_resource",
                        "render graph pass references a resource that is not declared",
                    )
                    .with_pass(pass.id)
                    .with_resource(access.resource),
                );
            }
        }

        for draw_list in &pass.draw_lists {
            if !draw_list.is_compatible_with_pass(pass.kind) {
                warnings.push(
                    RenderGraphValidationIssue::new(
                        "render_graph.draw_list_route_mismatch",
                        format!(
                            "draw-list '{}' is unusual for render pass kind {:?}",
                            draw_list.label(),
                            pass.kind
                        ),
                    )
                    .with_pass(pass.id),
                );
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let mut last_writer = BTreeMap::<RenderGraphResourceId, RenderGraphPassId>::new();
    let mut last_readers = BTreeMap::<RenderGraphResourceId, BTreeSet<RenderGraphPassId>>::new();
    let mut dependencies: BTreeMap<RenderGraphPassId, BTreeSet<RenderGraphPassId>> = graph
        .passes
        .iter()
        .map(|pass| (pass.id, BTreeSet::new()))
        .collect();
    let mut reverse_edges: BTreeMap<RenderGraphPassId, BTreeSet<RenderGraphPassId>> = graph
        .passes
        .iter()
        .map(|pass| (pass.id, BTreeSet::new()))
        .collect();
    let mut edge_keys = BTreeSet::new();
    let mut edges = Vec::new();
    let mut barriers = RenderGraphBarrierStats::default();

    for pass in &graph.passes {
        for created in &pass.creates {
            if let Some(previous_writer) = last_writer.get(created).copied() {
                if previous_writer != pass.id {
                    errors.push(
                        RenderGraphValidationIssue::new(
                            "render_graph.recreate_resource",
                            "render graph resource is created after it was already produced",
                        )
                        .with_pass(pass.id)
                        .with_resource(*created),
                    );
                }
            }
            last_writer.insert(*created, pass.id);
            last_readers.remove(created);
        }

        for read in &pass.reads {
            if let Some(writer) = last_writer.get(&read.resource).copied() {
                if writer != pass.id {
                    add_dependency(
                        writer,
                        pass.id,
                        read.resource,
                        RenderGraphDependencyKind::ReadAfterWrite,
                        &mut dependencies,
                        &mut reverse_edges,
                        &mut edge_keys,
                        &mut edges,
                    );
                    barriers.read_after_write = barriers.read_after_write.saturating_add(1);
                }
            } else {
                let resource = resources
                    .get(&read.resource)
                    .expect("validated render graph resource disappeared");
                if matches!(
                    resource.lifetime,
                    RenderGraphResourceLifetime::TransientFrame
                ) {
                    errors.push(
                        RenderGraphValidationIssue::new(
                            "render_graph.missing_producer",
                            "transient render graph resource is read before any pass produces it",
                        )
                        .with_pass(pass.id)
                        .with_resource(read.resource),
                    );
                } else {
                    barriers.external_imports = barriers.external_imports.saturating_add(1);
                }
            }
            last_readers
                .entry(read.resource)
                .or_default()
                .insert(pass.id);
        }

        for write in &pass.writes {
            if let Some(writer) = last_writer.get(&write.resource).copied() {
                if writer != pass.id {
                    add_dependency(
                        writer,
                        pass.id,
                        write.resource,
                        RenderGraphDependencyKind::WriteAfterWrite,
                        &mut dependencies,
                        &mut reverse_edges,
                        &mut edge_keys,
                        &mut edges,
                    );
                    barriers.write_after_write = barriers.write_after_write.saturating_add(1);
                }
            }
            if let Some(readers) = last_readers.remove(&write.resource) {
                for reader in readers {
                    if reader != pass.id {
                        add_dependency(
                            reader,
                            pass.id,
                            write.resource,
                            RenderGraphDependencyKind::WriteAfterRead,
                            &mut dependencies,
                            &mut reverse_edges,
                            &mut edge_keys,
                            &mut edges,
                        );
                        barriers.write_after_read = barriers.write_after_read.saturating_add(1);
                    }
                }
            }
            last_writer.insert(write.resource, pass.id);
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let raw_execution_order = topological_order(&dependencies, &reverse_edges, &pass_order)?;
    let culling = cull_passes(
        graph,
        &dependencies,
        &last_writer,
        &resources,
        &raw_execution_order,
    );
    let culled = culling
        .culled_passes
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let execution_order = culling.live_passes.clone();
    let resource_lifetimes = analyze_resource_lifetimes(graph, &execution_order);
    let transient_allocation_plan =
        super::plan_transient_resource_allocations(graph, &resource_lifetimes);
    let compiled_passes = graph
        .passes
        .iter()
        .enumerate()
        .map(|(index, pass)| CompiledRenderGraphPass {
            id: pass.id,
            declaration_index: index as u32,
            producers: dependencies
                .get(&pass.id)
                .map(|items| items.iter().copied().collect())
                .unwrap_or_default(),
            consumers: reverse_edges
                .get(&pass.id)
                .map(|items| items.iter().copied().collect())
                .unwrap_or_default(),
            culled: culled.contains(&pass.id),
        })
        .collect();

    let dag = RenderGraphCompiledDag {
        passes: compiled_passes,
        edges,
        execution_order: execution_order.clone(),
    };
    let report = RenderGraphCompileReport {
        pass_count: graph.passes.len() as u32,
        resource_count: graph.resources.len() as u32,
        execution_order,
        lifetime,
        barriers,
        warnings,
    };

    Ok(RenderGraphCompilation {
        dag,
        report,
        culling,
        resource_lifetimes,
        transient_allocation_plan,
    })
}

#[cfg(test)]
mod tests;
