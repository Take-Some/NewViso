use super::*;
use render_api::{RenderFrameEnvelope, UiDrawList, UiLayerDrawPacket, UiLayerDrawPacketSet};

static UI_COMPOSITE_LOGGED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

fn ui_envelope(
    draw_list: Value,
    domain: render_api::UiLayerDomain,
    frame_index: u64,
) -> Result<RenderFrameEnvelope, String> {
    let draw_list: UiDrawList =
        serde_json::from_value(draw_list).map_err(|e| format!("invalid UI draw stream: {e}"))?;
    let extent = Extent2D::new(
        draw_list.screen_size_px[0].max(1),
        draw_list.screen_size_px[1].max(1),
    );
    let surface = RenderGraphResourceId(1);
    let mut graph = RenderGraphDesc::new("newviso.ui.composite")
        .add_resource(
            RenderGraphResourceDesc::external_swapchain(
                surface,
                "newviso.ui.surface",
                RenderGraphResourceUsage::ColorAttachment,
                extent,
                TextureFormat::Bgra8Srgb,
            )
            .with_semantic(RenderGraphResourceSemantic::SurfaceColor),
        )
        .add_pass(
            RenderGraphPassDesc::new(
                RenderGraphPassId(1),
                format!("ui_composite.{}", domain.as_str()),
                RenderGraphPassKind::UiComposite,
            )
            .with_domain(RenderGraphPassDomain::Presentation)
            .writes(surface, RenderGraphResourceUsage::ColorAttachment),
        );
    graph.frame_index = frame_index;
    let mut layers = UiLayerDrawPacketSet::new(frame_index);
    layers.push(
        UiLayerDrawPacket::new(domain, frame_index, draw_list)
            .with_target("engine.render.surface.primary"),
    );
    Ok(
        RenderFrameEnvelope::new(frame_index, [0.0; 4], extent, extent, true, graph)
            .with_ui_layers(layers),
    )
}

impl RenderClient<'_> {
    /// Composite UI into an already acquired scene frame through the current typed protocol.
    /// The caller retains ownership of EndFrame; no legacy immediate UI command is emitted.
    pub fn composite_ui_draw_list(
        &self,
        draw_list: Value,
        domain: render_api::UiLayerDomain,
    ) -> Result<(), String> {
        let state = self.frame_state()?;
        if !state.active {
            return Err("UI composition requires an acquired render frame".into());
        }
        let frame = ui_envelope(draw_list, domain, state.engine_frame_index)?;
        match self.typed_request(render_api::RenderServiceRequest::SubmitFrame(Box::new(
            frame,
        )))? {
            render_api::RenderServiceResponse::GraphSubmitReport(report) => {
                if report.executed_passes == 0 {
                    return Err(format!(
                        "UI composite graph executed no passes: skipped={}",
                        report.skipped_passes
                    ));
                }
                if !UI_COMPOSITE_LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                    host::info(
                        "newviso.ui",
                        format!(
                            "UI composite ready domain={} executed_passes={} skipped_passes={}",
                            domain.as_str(),
                            report.executed_passes,
                            report.skipped_passes
                        ),
                    );
                }
                Ok(())
            }
            other => Err(format!(
                "UI composition expected GraphSubmitReport, got {other:?}"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlay_routes_by_domain_and_preserves_frame_identity() {
        let mut list = UiDrawList::new();
        list.screen_size_px = [1280, 800];
        let frame = ui_envelope(
            serde_json::to_value(list).unwrap(),
            render_api::UiLayerDomain::Editor,
            42,
        )
        .unwrap();
        assert_eq!(frame.frame_index, 42);
        assert_eq!(frame.graph.frame_index, 42);
        assert_eq!(frame.graph.passes.len(), 1);
        assert_eq!(frame.graph.passes[0].label, "ui_composite.editor");
        assert_eq!(frame.graph.passes[0].kind, RenderGraphPassKind::UiComposite);
        assert_eq!(
            frame.ui_layers.packets[0].domain,
            render_api::UiLayerDomain::Editor
        );
        assert_eq!(
            frame.ui_layers.packets[0].draw_list.screen_size_px,
            [1280, 800]
        );
    }
}
