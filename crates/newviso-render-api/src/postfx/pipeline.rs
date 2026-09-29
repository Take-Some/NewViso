use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PostFxPassKind {
    Exposure,
    ExposureAdaptation,
    Ssao,
    DepthReduction,
    AdaptiveDof,
    LensArtefacts,
    PostScan,
    Bloom,
    ColorGrade,
    Tonemap,
    Fxaa,
    TaaResolve,
    MsaaResolve,
    DisplayEncode,
    UiBackdropBlur,
    UiComposite,
    SunDisk,
    SunLensFlare,
    SunRays,
    Dither,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostFxPipelineDesc {
    pub label: String,
    pub hdr_scene_color: bool,
    #[serde(default)]
    pub display: ToneMapDisplayParams,
    #[serde(default)]
    pub quality: PostFxQualityParams,
    #[serde(default = "default_postfx_passes")]
    pub passes: Vec<PostFxPassKind>,
}

impl Default for PostFxPipelineDesc {
    #[inline]
    fn default() -> Self {
        Self {
            label: "runtime.hdr_to_display".to_owned(),
            hdr_scene_color: true,
            display: ToneMapDisplayParams::default(),
            quality: PostFxQualityParams::default(),
            passes: default_postfx_passes(),
        }
    }
}

#[inline]
fn default_postfx_passes() -> Vec<PostFxPassKind> {
    vec![
        PostFxPassKind::Exposure,
        PostFxPassKind::ExposureAdaptation,
        PostFxPassKind::Ssao,
        PostFxPassKind::DepthReduction,
        PostFxPassKind::AdaptiveDof,
        PostFxPassKind::LensArtefacts,
        PostFxPassKind::PostScan,
        PostFxPassKind::Bloom,
        PostFxPassKind::SunDisk,
        PostFxPassKind::SunLensFlare,
        PostFxPassKind::SunRays,
        PostFxPassKind::ColorGrade,
        PostFxPassKind::Tonemap,
        PostFxPassKind::Fxaa,
        PostFxPassKind::TaaResolve,
        PostFxPassKind::MsaaResolve,
        PostFxPassKind::DisplayEncode,
        PostFxPassKind::Dither,
        PostFxPassKind::UiBackdropBlur,
        PostFxPassKind::UiComposite,
    ]
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PostFxPassStats {
    pub executed_passes: u32,
    pub last_postfx_ms: f32,
    pub hdr_scene_color: bool,
}
