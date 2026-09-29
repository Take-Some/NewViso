use crate::{
    Color4, Extent2D, PostFxFrameParams, RenderDrawListKind, RenderEffectStack, RenderGraphDesc,
    RenderWorkBudget, UiLayerDrawPacketSet,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RenderFrameDomainIntent {
    #[serde(default = "default_true_domain")]
    pub render3d_enabled: bool,
    #[serde(default = "default_true_domain")]
    pub render2d_enabled: bool,
    #[serde(default)]
    pub ui_postprocess_enabled: bool,
}

impl Default for RenderFrameDomainIntent {
    #[inline]
    fn default() -> Self {
        Self {
            render3d_enabled: true,
            render2d_enabled: true,
            ui_postprocess_enabled: false,
        }
    }
}

#[inline]
fn default_true_domain() -> bool {
    true
}

/// One renderer-facing frame package inspired by mature phase/draw-list
/// renderers: the runtime submits a single envelope containing the graph,
/// declared draw-list routes and frame extents instead of negotiating scattered
/// per-version service calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderFrameEnvelope {
    pub frame_index: u64,
    pub label: Option<String>,
    pub clear_color: Color4,
    pub surface_extent: Extent2D,
    pub viewport_extent: Extent2D,
    pub viewport_is_surface: bool,
    #[serde(default)]
    pub postfx: PostFxFrameParams,
    #[serde(default)]
    pub effects: RenderEffectStack,
    #[serde(default)]
    pub domains: RenderFrameDomainIntent,
    pub graph: RenderGraphDesc,
    #[serde(default)]
    pub draw_lists: Vec<RenderDrawListKind>,
    /// Ordered retained UI domain packets consumed by RenderGraph UI composite passes.
    #[serde(default)]
    pub ui_layers: UiLayerDrawPacketSet,
    #[serde(default)]
    pub work_budget: Option<RenderWorkBudget>,
}

impl RenderFrameEnvelope {
    #[inline]
    pub fn new(
        frame_index: u64,
        clear_color: Color4,
        surface_extent: Extent2D,
        viewport_extent: Extent2D,
        viewport_is_surface: bool,
        graph: RenderGraphDesc,
    ) -> Self {
        Self {
            frame_index,
            label: graph.label.clone(),
            clear_color,
            surface_extent,
            viewport_extent,
            viewport_is_surface,
            postfx: PostFxFrameParams::default(),
            effects: RenderEffectStack::default(),
            domains: RenderFrameDomainIntent::default(),
            graph,
            draw_lists: Vec::new(),
            ui_layers: UiLayerDrawPacketSet::new(frame_index),
            work_budget: None,
        }
    }

    #[inline]
    pub fn with_postfx(mut self, postfx: PostFxFrameParams) -> Self {
        self.postfx = postfx;
        self
    }

    #[inline]
    pub fn with_effect_stack(mut self, effects: RenderEffectStack) -> Self {
        self.effects = effects;
        self
    }

    #[inline]
    pub fn with_domain_intent(mut self, domains: RenderFrameDomainIntent) -> Self {
        self.domains = domains;
        self
    }

    #[inline]
    pub fn with_draw_lists(
        mut self,
        draw_lists: impl IntoIterator<Item = RenderDrawListKind>,
    ) -> Self {
        self.draw_lists = draw_lists.into_iter().collect();
        self
    }

    #[inline]
    pub fn with_ui_layers(mut self, mut ui_layers: UiLayerDrawPacketSet) -> Self {
        ui_layers.frame_index = self.frame_index;
        for packet in &mut ui_layers.packets {
            packet.frame_index = self.frame_index;
        }
        ui_layers.sort_for_composite();
        self.ui_layers = ui_layers;
        self
    }

    #[inline]
    pub fn with_work_budget(mut self, budget: RenderWorkBudget) -> Self {
        self.work_budget = Some(budget);
        self
    }
}
