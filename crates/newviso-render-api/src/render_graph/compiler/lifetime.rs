use std::collections::BTreeMap;

use super::super::{
    CompiledResourceLifetime, RenderGraphDesc, RenderGraphPassId, RenderGraphResourceId,
    RenderGraphResourceLifetimeReport, RenderGraphResourceUsage, RenderGraphResourceUse,
    RenderGraphResourceUseKind,
};

#[derive(Clone)]
struct ResourceLifetimeAccumulator {
    first_pass: RenderGraphPassId,
    last_pass: RenderGraphPassId,
    first_execution_index: u32,
    last_execution_index: u32,
    create_count: u32,
    read_count: u32,
    write_count: u32,
    history: Vec<RenderGraphResourceUse>,
}

impl ResourceLifetimeAccumulator {
    #[inline]
    fn new(pass: RenderGraphPassId, execution_index: u32) -> Self {
        Self {
            first_pass: pass,
            last_pass: pass,
            first_execution_index: execution_index,
            last_execution_index: execution_index,
            create_count: 0,
            read_count: 0,
            write_count: 0,
            history: Vec::new(),
        }
    }

    #[inline]
    fn record(
        &mut self,
        pass: RenderGraphPassId,
        execution_index: u32,
        kind: RenderGraphResourceUseKind,
        usage: Option<RenderGraphResourceUsage>,
    ) {
        self.last_pass = pass;
        self.last_execution_index = execution_index;
        match kind {
            RenderGraphResourceUseKind::Create => {
                self.create_count = self.create_count.saturating_add(1)
            }
            RenderGraphResourceUseKind::Read => self.read_count = self.read_count.saturating_add(1),
            RenderGraphResourceUseKind::Write => {
                self.write_count = self.write_count.saturating_add(1)
            }
        }
        self.history.push(RenderGraphResourceUse::new(
            execution_index,
            pass,
            kind,
            usage,
        ));
    }
}

pub(super) fn analyze_resource_lifetimes(
    graph: &RenderGraphDesc,
    live_execution_order: &[RenderGraphPassId],
) -> RenderGraphResourceLifetimeReport {
    let passes = graph
        .passes
        .iter()
        .map(|pass| (pass.id, pass))
        .collect::<BTreeMap<_, _>>();
    let mut accumulators = BTreeMap::<RenderGraphResourceId, ResourceLifetimeAccumulator>::new();

    for (execution_index, pass_id) in live_execution_order.iter().copied().enumerate() {
        let execution_index = execution_index.min(u32::MAX as usize) as u32;
        let pass = passes
            .get(&pass_id)
            .expect("live render graph pass missing from declarative graph");

        for resource in &pass.creates {
            let entry = accumulators
                .entry(*resource)
                .or_insert_with(|| ResourceLifetimeAccumulator::new(pass_id, execution_index));
            entry.record(
                pass_id,
                execution_index,
                RenderGraphResourceUseKind::Create,
                None,
            );
        }
        for read in &pass.reads {
            let entry = accumulators
                .entry(read.resource)
                .or_insert_with(|| ResourceLifetimeAccumulator::new(pass_id, execution_index));
            entry.record(
                pass_id,
                execution_index,
                RenderGraphResourceUseKind::Read,
                Some(read.usage),
            );
        }
        for write in &pass.writes {
            let entry = accumulators
                .entry(write.resource)
                .or_insert_with(|| ResourceLifetimeAccumulator::new(pass_id, execution_index));
            entry.record(
                pass_id,
                execution_index,
                RenderGraphResourceUseKind::Write,
                Some(write.usage),
            );
        }
    }

    let mut resources = Vec::new();
    let mut unused_resources = Vec::new();
    for resource in &graph.resources {
        if let Some(entry) = accumulators.remove(&resource.id) {
            resources.push(CompiledResourceLifetime {
                resource: resource.id,
                first_pass: entry.first_pass,
                last_pass: entry.last_pass,
                first_execution_index: entry.first_execution_index,
                last_execution_index: entry.last_execution_index,
                create_count: entry.create_count,
                read_count: entry.read_count,
                write_count: entry.write_count,
                history: entry.history,
            });
        } else {
            unused_resources.push(resource.id);
        }
    }

    RenderGraphResourceLifetimeReport {
        resources,
        unused_resources,
    }
}
