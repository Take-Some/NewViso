use super::frame::default_dof_far_plane;
use super::*;

#[test]
fn postfx_frame_params_accept_old_payload_without_quality() {
    let json = r#"{
        "display":{"exposure":1.0,"gamma":2.2,"black_lift":0.0,"operator":"AcesApprox"},
        "sun":{"screen_position":[0.5,0.5],"color":[1.0,0.94,0.82],"intensity":3.2,"visibility":1.0,"disk_radius":0.018,"flare_strength":0.2,"ray_strength":0.16}
    }"#;
    let decoded: PostFxFrameParams =
        serde_json::from_str(json).expect("old postfx payload must remain valid");
    assert!(decoded.quality.bloom.enabled);
    assert!(decoded.quality.fxaa.enabled);
    assert_eq!(decoded.quality.anti_aliasing, AntiAliasingMode::Fxaa);
    assert!(!decoded.quality.ssao.enabled);
}

#[test]
fn postfx_frame_params_accept_old_payload_without_view_intent() {
    let json = r#"{
        "display":{"exposure":1.0,"gamma":2.2,"black_lift":0.0,"operator":"AcesApprox"},
        "sun":{"screen_position":[0.5,0.5],"color":[1.0,0.94,0.82],"intensity":3.2,"visibility":1.0,"disk_radius":0.018,"flare_strength":0.2,"ray_strength":0.16},
        "quality":{"anti_aliasing":"Fxaa"}
    }"#;
    let decoded: PostFxFrameParams =
        serde_json::from_str(json).expect("old postfx payload must remain valid");
    assert_eq!(decoded.view.motion_blur.strength, 0.0);
    assert_eq!(decoded.view.dof.far_end, default_dof_far_plane());
}

#[test]
fn postfx_pipeline_defaults_include_aaa_pass_order() {
    let desc = PostFxPipelineDesc::default();
    assert!(desc.passes.contains(&PostFxPassKind::ExposureAdaptation));
    assert!(desc.passes.contains(&PostFxPassKind::Ssao));
    assert!(desc.passes.contains(&PostFxPassKind::AdaptiveDof));
    assert!(desc.passes.contains(&PostFxPassKind::LensArtefacts));
    assert!(desc.passes.contains(&PostFxPassKind::PostScan));
    assert!(desc.passes.contains(&PostFxPassKind::Bloom));
    assert!(desc.passes.contains(&PostFxPassKind::Fxaa));
    assert!(desc.passes.contains(&PostFxPassKind::TaaResolve));
    assert!(desc.passes.contains(&PostFxPassKind::MsaaResolve));
    assert!(desc.passes.contains(&PostFxPassKind::Dither));
}
