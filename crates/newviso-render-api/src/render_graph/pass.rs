use serde::{Deserialize, Serialize};

use super::{
    RenderDrawListKind, RenderGraphPassId, RenderGraphPassKind, RenderGraphQueueKind,
    RenderGraphResourceId, RenderGraphResourceRef, RenderGraphResourceUsage,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderGraphPassDomain {
    Unknown,
    Render3d,
    Render2d,
    PostProcess,
    Presentation,
}

impl Default for RenderGraphPassDomain {
    #[inline]
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RenderGraphPassFlags {
    /// Opt-in dead-pass elimination. False is intentionally conservative for
    /// compatibility and for passes with side effects not represented as resources.
    #[serde(default)]
    pub allow_culling: bool,
    #[serde(default)]
    pub allow_async_compute: bool,
    #[serde(default)]
    pub debug_marker: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderGraphPassDesc {
    pub id: RenderGraphPassId,
    pub label: String,
    #[serde(default)]
    pub kind: RenderGraphPassKind,
    #[serde(default)]
    pub domain: RenderGraphPassDomain,
    #[serde(default)]
    pub queue: RenderGraphQueueKind,
    #[serde(default)]
    pub reads: Vec<RenderGraphResourceRef>,
    #[serde(default)]
    pub writes: Vec<RenderGraphResourceRef>,
    #[serde(default)]
    pub creates: Vec<RenderGraphResourceId>,
    #[serde(default)]
    pub draw_lists: Vec<RenderDrawListKind>,
    #[serde(default)]
    pub flags: RenderGraphPassFlags,
}

impl RenderGraphPassDesc {
    #[inline]
    pub fn new(id: RenderGraphPassId, label: impl Into<String>, kind: RenderGraphPassKind) -> Self {
        Self {
            id,
            label: label.into(),
            kind,
            domain: RenderGraphPassDomain::Unknown,
            queue: RenderGraphQueueKind::Graphics,
            reads: Vec::new(),
            writes: Vec::new(),
            creates: Vec::new(),
            draw_lists: Vec::new(),
            flags: RenderGraphPassFlags::default(),
        }
    }

    #[inline]
    pub fn with_domain(mut self, domain: RenderGraphPassDomain) -> Self {
        self.domain = domain;
        self
    }

    #[inline]
    pub fn reads(
        mut self,
        resource: RenderGraphResourceId,
        usage: RenderGraphResourceUsage,
    ) -> Self {
        self.reads
            .push(RenderGraphResourceRef::read(resource, usage));
        self
    }

    #[inline]
    pub fn writes(
        mut self,
        resource: RenderGraphResourceId,
        usage: RenderGraphResourceUsage,
    ) -> Self {
        self.writes
            .push(RenderGraphResourceRef::write(resource, usage));
        self
    }

    #[inline]
    pub fn with_culling(mut self, allow_culling: bool) -> Self {
        self.flags.allow_culling = allow_culling;
        self
    }

    #[inline]
    pub fn cullable(self) -> Self {
        self.with_culling(true)
    }

    #[inline]
    pub fn draw_list(mut self, kind: RenderDrawListKind) -> Self {
        if !self.draw_lists.contains(&kind) {
            self.draw_lists.push(kind);
        }
        self
    }
}
