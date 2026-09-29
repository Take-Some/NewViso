use super::*;
use crate::{
    RenderGraphPassDesc, RenderGraphPassKind, RenderGraphResourceDesc, RenderGraphResourceUsage,
    RenderGraphResourceUse, RenderGraphResourceUseKind,
};

fn transient(id: u64) -> RenderGraphResourceDesc {
    RenderGraphResourceDesc::transient_texture(
        RenderGraphResourceId(id),
        format!("r{id}"),
        RenderGraphResourceUsage::StorageTexture,
        crate::Extent2D::new(8, 8),
        crate::TextureFormat::Rgba8Unorm,
    )
}

fn pass(id: u64, label: &str) -> RenderGraphPassDesc {
    RenderGraphPassDesc::new(RenderGraphPassId(id), label, RenderGraphPassKind::Custom)
}

#[test]
fn compiles_raw_war_and_waw_edges_with_stable_declaration_order() {
    let r1 = RenderGraphResourceId(1);
    let r2 = RenderGraphResourceId(2);
    let graph = RenderGraphDesc::new("hazards")
        .add_resource(transient(1))
        .add_resource(transient(2))
        .add_pass(pass(30, "produce-a").writes(r1, RenderGraphResourceUsage::StorageTexture))
        .add_pass(pass(10, "read-a").reads(r1, RenderGraphResourceUsage::SampledTexture))
        .add_pass(
            pass(20, "rewrite-a")
                .writes(r1, RenderGraphResourceUsage::StorageTexture)
                .writes(r2, RenderGraphResourceUsage::StorageTexture),
        )
        .add_pass(pass(5, "read-b").reads(r2, RenderGraphResourceUsage::SampledTexture));

    let compiled = compile_render_graph_v2(&graph).expect("graph should compile");
    assert_eq!(
        compiled.dag.execution_order,
        vec![
            RenderGraphPassId(30),
            RenderGraphPassId(10),
            RenderGraphPassId(20),
            RenderGraphPassId(5),
        ]
    );
    assert!(compiled.dag.edges.iter().any(|edge| {
        edge.producer == RenderGraphPassId(30)
            && edge.consumer == RenderGraphPassId(10)
            && edge.kind == RenderGraphDependencyKind::ReadAfterWrite
    }));
    assert!(compiled.dag.edges.iter().any(|edge| {
        edge.producer == RenderGraphPassId(10)
            && edge.consumer == RenderGraphPassId(20)
            && edge.kind == RenderGraphDependencyKind::WriteAfterRead
    }));
    assert!(compiled.dag.edges.iter().any(|edge| {
        edge.producer == RenderGraphPassId(30)
            && edge.consumer == RenderGraphPassId(20)
            && edge.kind == RenderGraphDependencyKind::WriteAfterWrite
    }));
}

#[test]
fn culls_opt_in_dead_branch_and_keeps_output_dependency_chain() {
    let main = RenderGraphResourceId(1);
    let dead = RenderGraphResourceId(2);
    let output = RenderGraphResourceId(3);
    let graph = RenderGraphDesc::new("culling")
        .add_resource(transient(1))
        .add_resource(transient(2))
        .add_resource(RenderGraphResourceDesc::external(
            output,
            "output",
            RenderGraphResourceUsage::ColorAttachment,
        ))
        .add_pass(
            pass(30, "main")
                .writes(main, RenderGraphResourceUsage::StorageTexture)
                .cullable(),
        )
        .add_pass(
            pass(10, "dead")
                .writes(dead, RenderGraphResourceUsage::StorageTexture)
                .cullable(),
        )
        .add_pass(
            pass(20, "present")
                .reads(main, RenderGraphResourceUsage::SampledTexture)
                .writes(output, RenderGraphResourceUsage::ColorAttachment)
                .cullable(),
        );

    let compiled = compile_render_graph_v2(&graph).expect("graph should compile");
    assert_eq!(compiled.culling.roots, vec![RenderGraphPassId(20)]);
    assert_eq!(
        compiled.culling.live_passes,
        vec![RenderGraphPassId(30), RenderGraphPassId(20)]
    );
    assert_eq!(compiled.culling.culled_passes, vec![RenderGraphPassId(10)]);
    assert_eq!(compiled.dag.execution_order, compiled.culling.live_passes);
    assert!(
        compiled
            .dag
            .passes
            .iter()
            .find(|pass| pass.id == RenderGraphPassId(10))
            .expect("dead pass node missing")
            .culled
    );
}

#[test]
fn lifetime_analysis_uses_only_live_execution_order() {
    let main = RenderGraphResourceId(1);
    let dead = RenderGraphResourceId(2);
    let output = RenderGraphResourceId(3);
    let graph = RenderGraphDesc::new("lifetimes-after-culling")
        .add_resource(transient(1))
        .add_resource(transient(2))
        .add_resource(RenderGraphResourceDesc::external(
            output,
            "output",
            RenderGraphResourceUsage::ColorAttachment,
        ))
        .add_pass(
            pass(30, "main")
                .writes(main, RenderGraphResourceUsage::StorageTexture)
                .cullable(),
        )
        .add_pass(
            pass(10, "dead")
                .writes(dead, RenderGraphResourceUsage::StorageTexture)
                .cullable(),
        )
        .add_pass(
            pass(20, "present")
                .reads(main, RenderGraphResourceUsage::SampledTexture)
                .writes(output, RenderGraphResourceUsage::ColorAttachment)
                .cullable(),
        );

    let compiled = compile_render_graph_v2(&graph).expect("graph should compile");
    let main_lifetime = compiled
        .resource_lifetimes
        .get(main)
        .expect("main lifetime missing");
    assert_eq!(main_lifetime.first_pass, RenderGraphPassId(30));
    assert_eq!(main_lifetime.last_pass, RenderGraphPassId(20));
    assert_eq!(main_lifetime.first_execution_index, 0);
    assert_eq!(main_lifetime.last_execution_index, 1);
    assert_eq!(main_lifetime.read_count, 1);
    assert_eq!(main_lifetime.write_count, 1);
    assert_eq!(main_lifetime.create_count, 0);
    assert_eq!(main_lifetime.execution_span(), 2);
    assert_eq!(main_lifetime.history.len(), 2);
    assert_eq!(
        main_lifetime.history[0],
        RenderGraphResourceUse::new(
            0,
            RenderGraphPassId(30),
            RenderGraphResourceUseKind::Write,
            Some(RenderGraphResourceUsage::StorageTexture),
        )
    );
    assert_eq!(
        main_lifetime.history[1],
        RenderGraphResourceUse::new(
            1,
            RenderGraphPassId(20),
            RenderGraphResourceUseKind::Read,
            Some(RenderGraphResourceUsage::SampledTexture),
        )
    );
    assert!(compiled.resource_lifetimes.get(dead).is_none());
    assert!(compiled.resource_lifetimes.unused_resources.contains(&dead));

    let output_lifetime = compiled
        .resource_lifetimes
        .get(output)
        .expect("output lifetime missing");
    assert_eq!(output_lifetime.first_execution_index, 1);
    assert_eq!(output_lifetime.last_execution_index, 1);
    assert_eq!(output_lifetime.write_count, 1);
}

#[test]
fn lifetime_analysis_counts_create_as_its_own_event() {
    let resource = RenderGraphResourceId(1);
    let output = RenderGraphResourceId(2);
    let mut creator = pass(1, "create").cullable();
    creator.creates.push(resource);
    let graph = RenderGraphDesc::new("create-lifetime")
        .add_resource(transient(1))
        .add_resource(RenderGraphResourceDesc::external(
            output,
            "output",
            RenderGraphResourceUsage::ColorAttachment,
        ))
        .add_pass(creator)
        .add_pass(
            pass(2, "consume")
                .reads(resource, RenderGraphResourceUsage::SampledTexture)
                .writes(output, RenderGraphResourceUsage::ColorAttachment)
                .cullable(),
        );

    let compiled = compile_render_graph_v2(&graph).expect("graph should compile");
    let lifetime = compiled
        .resource_lifetimes
        .get(resource)
        .expect("created resource lifetime missing");
    assert_eq!(lifetime.first_pass, RenderGraphPassId(1));
    assert_eq!(lifetime.last_pass, RenderGraphPassId(2));
    assert_eq!(lifetime.create_count, 1);
    assert_eq!(lifetime.read_count, 1);
    assert_eq!(lifetime.write_count, 0);
    assert_eq!(lifetime.access_count(), 2);
    assert_eq!(lifetime.history.len(), 2);
    assert_eq!(lifetime.history[0].kind, RenderGraphResourceUseKind::Create);
    assert_eq!(lifetime.history[0].execution_index, 0);
    assert_eq!(lifetime.history[0].usage, None);
    assert_eq!(lifetime.history[1].kind, RenderGraphResourceUseKind::Read);
    assert_eq!(lifetime.history[1].execution_index, 1);
    assert_eq!(lifetime.last_live_use(), lifetime.history.last());
}

#[test]
fn compiler_builds_transient_alias_plan_from_live_lifetimes() {
    let first = RenderGraphResourceId(1);
    let second = RenderGraphResourceId(2);
    let output_a = RenderGraphResourceId(3);
    let output_b = RenderGraphResourceId(4);
    let graph = RenderGraphDesc::new("phase4-transient-plan")
        .add_resource(transient(1))
        .add_resource(transient(2))
        .add_resource(RenderGraphResourceDesc::external(
            output_a,
            "output-a",
            RenderGraphResourceUsage::ColorAttachment,
        ))
        .add_resource(RenderGraphResourceDesc::external(
            output_b,
            "output-b",
            RenderGraphResourceUsage::ColorAttachment,
        ))
        .add_pass(
            pass(1, "produce-first")
                .writes(first, RenderGraphResourceUsage::StorageTexture)
                .cullable(),
        )
        .add_pass(
            pass(2, "consume-first")
                .reads(first, RenderGraphResourceUsage::SampledTexture)
                .writes(output_a, RenderGraphResourceUsage::ColorAttachment)
                .cullable(),
        )
        .add_pass(
            pass(3, "produce-second")
                .writes(second, RenderGraphResourceUsage::StorageTexture)
                .cullable(),
        )
        .add_pass(
            pass(4, "consume-second")
                .reads(second, RenderGraphResourceUsage::SampledTexture)
                .writes(output_b, RenderGraphResourceUsage::ColorAttachment)
                .cullable(),
        );

    let compiled = compile_render_graph_v2(&graph).expect("graph should compile");
    let plan = &compiled.transient_allocation_plan;
    assert_eq!(compiled.dag.execution_order.len(), 4);
    assert_eq!(plan.slots.len(), 1);
    assert_eq!(plan.alias_groups.len(), 1);
    assert_eq!(plan.alias_reuse_count(), 1);
    assert_eq!(
        plan.slots[0].resources,
        vec![first, second],
        "disjoint compatible lifetimes should reuse one transient slot"
    );
    assert_eq!(plan.resource_to_slot.get(&first), Some(&0));
    assert_eq!(plan.resource_to_slot.get(&second), Some(&0));
    assert!(plan.ineligible_resources.is_empty());
}

#[test]
fn observable_resource_uses_only_last_writer_as_culling_root() {
    let output = RenderGraphResourceId(1);
    let graph = RenderGraphDesc::new("last-output-writer")
        .add_resource(RenderGraphResourceDesc::external(
            output,
            "output",
            RenderGraphResourceUsage::ColorAttachment,
        ))
        .add_pass(
            pass(1, "first")
                .writes(output, RenderGraphResourceUsage::ColorAttachment)
                .cullable(),
        )
        .add_pass(
            pass(2, "last")
                .writes(output, RenderGraphResourceUsage::ColorAttachment)
                .cullable(),
        );

    let compiled = compile_render_graph_v2(&graph).expect("graph should compile");
    assert_eq!(compiled.culling.roots, vec![RenderGraphPassId(2)]);
    assert_eq!(
        compiled.culling.live_passes,
        vec![RenderGraphPassId(1), RenderGraphPassId(2)],
        "WAW is conservatively retained until subresource/load semantics exist"
    );
}

#[test]
fn non_cullable_pass_is_root_and_keeps_its_cullable_producer() {
    let resource = RenderGraphResourceId(1);
    let graph = RenderGraphDesc::new("side-effect-root")
        .add_resource(transient(1))
        .add_pass(
            pass(1, "producer")
                .writes(resource, RenderGraphResourceUsage::StorageTexture)
                .cullable(),
        )
        .add_pass(pass(2, "side-effect").reads(resource, RenderGraphResourceUsage::SampledTexture));

    let compiled = compile_render_graph_v2(&graph).expect("graph should compile");
    assert_eq!(compiled.culling.roots, vec![RenderGraphPassId(2)]);
    assert_eq!(
        compiled.culling.live_passes,
        vec![RenderGraphPassId(1), RenderGraphPassId(2)]
    );
    assert!(compiled.culling.culled_passes.is_empty());
}

#[test]
fn rejects_transient_read_without_producer() {
    let resource = RenderGraphResourceId(1);
    let graph = RenderGraphDesc::new("missing-producer")
        .add_resource(transient(1))
        .add_pass(pass(1, "read").reads(resource, RenderGraphResourceUsage::SampledTexture));

    let errors = compile_render_graph_v2(&graph).expect_err("graph must be rejected");
    assert!(errors
        .iter()
        .any(|issue| issue.code == "render_graph.missing_producer"));
}

#[test]
fn rejects_duplicate_resource_creator() {
    let resource = RenderGraphResourceId(1);
    let mut a = pass(1, "a");
    a.creates.push(resource);
    let mut b = pass(2, "b");
    b.creates.push(resource);
    let graph = RenderGraphDesc::new("duplicate-create")
        .add_resource(transient(1))
        .add_pass(a)
        .add_pass(b);

    let errors = compile_render_graph_v2(&graph).expect_err("graph must be rejected");
    assert!(errors
        .iter()
        .any(|issue| issue.code == "render_graph.duplicate_create"));
}

#[test]
fn rejects_unknown_created_resource() {
    let mut creator = pass(1, "creator");
    creator.creates.push(RenderGraphResourceId(99));
    let graph = RenderGraphDesc::new("unknown-create").add_pass(creator);

    let errors = compile_render_graph_v2(&graph).expect_err("graph must be rejected");
    assert!(errors
        .iter()
        .any(|issue| issue.code == "render_graph.unknown_created_resource"));
}

#[test]
fn topological_order_detects_cycles_defensively() {
    let a = RenderGraphPassId(1);
    let b = RenderGraphPassId(2);
    let dependencies = BTreeMap::from([(a, BTreeSet::from([b])), (b, BTreeSet::from([a]))]);
    let reverse = BTreeMap::from([(a, BTreeSet::from([b])), (b, BTreeSet::from([a]))]);
    let order = BTreeMap::from([(a, 0), (b, 1)]);

    let errors =
        topological_order(&dependencies, &reverse, &order).expect_err("cycle must be rejected");
    assert_eq!(errors[0].code, "render_graph.cycle");
}
