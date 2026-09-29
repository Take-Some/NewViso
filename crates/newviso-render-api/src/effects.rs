use serde::{Deserialize, Serialize};

/// Stable renderer effect identifiers.
///
/// Effects are first-class pipeline objects: the runtime describes *what* it wants,
/// the provider decides *how* to realize it, cache resources and bind native passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RenderEffectKind {
    PcssShadows,
    CascadedShadows,
    ShadowAtlas,
    Bloom,
    ScreenSpaceAmbientOcclusion,
    ToneMap,
    Fxaa,
    Taa,
    Msaa,
    Tessellation,
    ColorGrade,
    DepthOfField,
    MotionBlur,
    SunDisk,
    SunLensFlare,
    SunRays,
    Vignette,
    Dither,
    UiComposite,
    WaterSurface,
    WaterReflection,
    GrassSurface,
    TreeImposter,
}

impl RenderEffectKind {
    #[inline]
    pub const fn stable_id(self) -> &'static str {
        match self {
            Self::PcssShadows => "render.effect.pcss_shadows",
            Self::CascadedShadows => "render.effect.cascaded_shadows",
            Self::ShadowAtlas => "render.effect.shadow_atlas",
            Self::Bloom => "render.effect.bloom",
            Self::ScreenSpaceAmbientOcclusion => "render.effect.ssao",
            Self::ToneMap => "render.effect.tonemap",
            Self::Fxaa => "render.effect.fxaa",
            Self::Taa => "render.effect.taa",
            Self::Msaa => "render.effect.msaa",
            Self::Tessellation => "render.effect.tessellation",
            Self::ColorGrade => "render.effect.color_grade",
            Self::DepthOfField => "render.effect.depth_of_field",
            Self::MotionBlur => "render.effect.motion_blur",
            Self::SunDisk => "render.effect.sun_disk",
            Self::SunLensFlare => "render.effect.sun_lens_flare",
            Self::SunRays => "render.effect.sun_rays",
            Self::Vignette => "render.effect.vignette",
            Self::Dither => "render.effect.dither",
            Self::UiComposite => "render.effect.ui_composite",
            Self::WaterSurface => "render.effect.water_surface",
            Self::WaterReflection => "render.effect.water_reflection",
            Self::GrassSurface => "render.effect.grass_surface",
            Self::TreeImposter => "render.effect.tree_imposter",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderEffectStage {
    PreDepth,
    Shadow,
    Geometry,
    Lighting,
    PostProcess,
    Composite,
}

impl Default for RenderEffectStage {
    #[inline]
    fn default() -> Self {
        Self::PostProcess
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderEffectQuality {
    Off,
    Low,
    Medium,
    High,
    Ultra,
}

impl Default for RenderEffectQuality {
    #[inline]
    fn default() -> Self {
        Self::High
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderEffectCachePolicy {
    /// Pure pass-level state. Rebuild only when the provider pipeline cache is reset.
    PipelineState,
    /// Keeps frame-history images or temporal state across frames.
    TemporalHistory,
    /// Keeps transient render graph targets and descriptor state for a few frames.
    FrameResources,
    /// No long-lived cached resource required.
    Stateless,
}

impl Default for RenderEffectCachePolicy {
    #[inline]
    fn default() -> Self {
        Self::PipelineState
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PcssShadowEffectConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_shadow_cascade_count")]
    pub cascade_count: u32,
    #[serde(default = "default_shadow_resolution")]
    pub resolution: u32,
    #[serde(default = "default_shadow_atlas_size")]
    pub atlas_size: u32,
    #[serde(default = "default_shadow_max_distance")]
    pub max_distance: f32,
    #[serde(default = "default_pcss_blocker_samples")]
    pub blocker_samples: u32,
    #[serde(default = "default_pcss_filter_samples")]
    pub filter_samples: u32,
    #[serde(default = "default_pcss_light_radius")]
    pub light_radius: f32,
    #[serde(default = "default_shadow_bias")]
    pub depth_bias: f32,
    #[serde(default = "default_shadow_normal_bias")]
    pub normal_bias: f32,
}

impl Default for PcssShadowEffectConfig {
    #[inline]
    fn default() -> Self {
        Self {
            enabled: true,
            cascade_count: default_shadow_cascade_count(),
            resolution: default_shadow_resolution(),
            atlas_size: default_shadow_atlas_size(),
            max_distance: default_shadow_max_distance(),
            blocker_samples: default_pcss_blocker_samples(),
            filter_samples: default_pcss_filter_samples(),
            light_radius: default_pcss_light_radius(),
            depth_bias: default_shadow_bias(),
            normal_bias: default_shadow_normal_bias(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BloomEffectConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_bloom_threshold")]
    pub threshold: f32,
    #[serde(default = "default_bloom_knee")]
    pub knee: f32,
    #[serde(default = "default_bloom_intensity")]
    pub intensity: f32,
    #[serde(default = "default_bloom_radius")]
    pub radius: f32,
    #[serde(default = "default_bloom_mips")]
    pub mips: u32,
}

impl Default for BloomEffectConfig {
    #[inline]
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: default_bloom_threshold(),
            knee: default_bloom_knee(),
            intensity: default_bloom_intensity(),
            radius: default_bloom_radius(),
            mips: default_bloom_mips(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaaEffectConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_taa_feedback")]
    pub feedback: f32,
    #[serde(default = "default_taa_clamping")]
    pub neighborhood_clamping: f32,
    #[serde(default = "default_taa_jitter_scale")]
    pub jitter_scale: f32,
}

impl Default for TaaEffectConfig {
    #[inline]
    fn default() -> Self {
        Self {
            enabled: true,
            feedback: default_taa_feedback(),
            neighborhood_clamping: default_taa_clamping(),
            jitter_scale: default_taa_jitter_scale(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MsaaSampleCount {
    X1,
    X2,
    X4,
    X8,
}

impl Default for MsaaSampleCount {
    #[inline]
    fn default() -> Self {
        Self::X1
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MsaaEffectConfig {
    #[serde(default)]
    pub samples: MsaaSampleCount,
    #[serde(default = "default_true")]
    pub explicit_resolve: bool,
}

impl Default for MsaaEffectConfig {
    #[inline]
    fn default() -> Self {
        Self {
            samples: MsaaSampleCount::X4,
            explicit_resolve: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TessellationEffectConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_tess_factor")]
    pub factor: f32,
    #[serde(default = "default_tess_min_distance")]
    pub min_distance: f32,
    #[serde(default = "default_tess_max_distance")]
    pub max_distance: f32,
    #[serde(default = "default_tess_displacement_scale")]
    pub displacement_scale: f32,
}

impl Default for TessellationEffectConfig {
    #[inline]
    fn default() -> Self {
        Self {
            enabled: false,
            factor: default_tess_factor(),
            min_distance: default_tess_min_distance(),
            max_distance: default_tess_max_distance(),
            displacement_scale: default_tess_displacement_scale(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RenderEffectConfig {
    PcssShadows(PcssShadowEffectConfig),
    CascadedShadows(PcssShadowEffectConfig),
    ShadowAtlas(PcssShadowEffectConfig),
    Bloom(BloomEffectConfig),
    Taa(TaaEffectConfig),
    Msaa(MsaaEffectConfig),
    Tessellation(TessellationEffectConfig),
    /// Provider-specific or future effect. The contract stays stable because the
    /// effect object still has a stable id, stage and cache policy.
    Json(serde_json::Value),
    Empty,
}

impl Default for RenderEffectConfig {
    #[inline]
    fn default() -> Self {
        Self::Empty
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderEffectObject {
    pub kind: RenderEffectKind,
    #[serde(default)]
    pub stage: RenderEffectStage,
    #[serde(default)]
    pub quality: RenderEffectQuality,
    #[serde(default)]
    pub base: RenderEffectConfig,
    #[serde(default)]
    pub tuned: Option<RenderEffectConfig>,
    #[serde(default)]
    pub cache_policy: RenderEffectCachePolicy,
    #[serde(default)]
    pub cache_key: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl RenderEffectObject {
    #[inline]
    pub fn new(kind: RenderEffectKind, stage: RenderEffectStage, base: RenderEffectConfig) -> Self {
        Self {
            kind,
            stage,
            quality: RenderEffectQuality::High,
            base,
            tuned: None,
            cache_policy: RenderEffectCachePolicy::PipelineState,
            cache_key: Some(kind.stable_id().to_owned()),
            enabled: true,
        }
    }

    #[inline]
    pub fn tuned(mut self, config: RenderEffectConfig) -> Self {
        self.tuned = Some(config);
        self
    }

    #[inline]
    pub fn with_quality(mut self, quality: RenderEffectQuality) -> Self {
        self.quality = quality;
        self
    }

    #[inline]
    pub fn with_cache_policy(mut self, policy: RenderEffectCachePolicy) -> Self {
        self.cache_policy = policy;
        self
    }

    #[inline]
    pub fn with_cache_key(mut self, key: impl Into<String>) -> Self {
        let key = key.into();
        self.cache_key = if key.trim().is_empty() {
            None
        } else {
            Some(key)
        };
        self
    }

    #[inline]
    pub fn effective_config(&self) -> &RenderEffectConfig {
        self.tuned.as_ref().unwrap_or(&self.base)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderEffectStack {
    pub label: String,
    #[serde(default = "default_effect_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub effects: Vec<RenderEffectObject>,
}

impl RenderEffectStack {
    #[inline]
    pub fn aaa_default() -> Self {
        Self {
            label: "runtime.aaa_effect_stack".to_owned(),
            schema_version: default_effect_schema_version(),
            effects: vec![
                RenderEffectObject::new(
                    RenderEffectKind::CascadedShadows,
                    RenderEffectStage::Shadow,
                    RenderEffectConfig::CascadedShadows(PcssShadowEffectConfig::default()),
                )
                .with_cache_policy(RenderEffectCachePolicy::FrameResources),
                RenderEffectObject::new(
                    RenderEffectKind::PcssShadows,
                    RenderEffectStage::Shadow,
                    RenderEffectConfig::PcssShadows(PcssShadowEffectConfig::default()),
                )
                .with_cache_policy(RenderEffectCachePolicy::PipelineState),
                RenderEffectObject::new(
                    RenderEffectKind::Tessellation,
                    RenderEffectStage::Geometry,
                    RenderEffectConfig::Tessellation(TessellationEffectConfig::default()),
                ),
                RenderEffectObject::new(
                    RenderEffectKind::Bloom,
                    RenderEffectStage::PostProcess,
                    RenderEffectConfig::Bloom(BloomEffectConfig::default()),
                )
                .with_cache_policy(RenderEffectCachePolicy::FrameResources),
                RenderEffectObject::new(
                    RenderEffectKind::ScreenSpaceAmbientOcclusion,
                    RenderEffectStage::PostProcess,
                    RenderEffectConfig::Empty,
                )
                .with_cache_policy(RenderEffectCachePolicy::FrameResources),
                RenderEffectObject::new(
                    RenderEffectKind::Taa,
                    RenderEffectStage::PostProcess,
                    RenderEffectConfig::Taa(TaaEffectConfig::default()),
                )
                .with_cache_policy(RenderEffectCachePolicy::TemporalHistory),
                RenderEffectObject::new(
                    RenderEffectKind::Fxaa,
                    RenderEffectStage::PostProcess,
                    RenderEffectConfig::Empty,
                ),
                RenderEffectObject::new(
                    RenderEffectKind::Msaa,
                    RenderEffectStage::Geometry,
                    RenderEffectConfig::Msaa(MsaaEffectConfig::default()),
                )
                .with_cache_policy(RenderEffectCachePolicy::FrameResources),
                RenderEffectObject::new(
                    RenderEffectKind::DepthOfField,
                    RenderEffectStage::PostProcess,
                    RenderEffectConfig::Empty,
                )
                .with_cache_policy(RenderEffectCachePolicy::TemporalHistory),
                RenderEffectObject::new(
                    RenderEffectKind::ColorGrade,
                    RenderEffectStage::PostProcess,
                    RenderEffectConfig::Empty,
                ),
                RenderEffectObject::new(
                    RenderEffectKind::SunLensFlare,
                    RenderEffectStage::PostProcess,
                    RenderEffectConfig::Empty,
                ),
                RenderEffectObject::new(
                    RenderEffectKind::SunRays,
                    RenderEffectStage::PostProcess,
                    RenderEffectConfig::Empty,
                ),
                RenderEffectObject::new(
                    RenderEffectKind::Vignette,
                    RenderEffectStage::PostProcess,
                    RenderEffectConfig::Empty,
                ),
                RenderEffectObject::new(
                    RenderEffectKind::Dither,
                    RenderEffectStage::PostProcess,
                    RenderEffectConfig::Empty,
                ),
                RenderEffectObject::new(
                    RenderEffectKind::WaterSurface,
                    RenderEffectStage::Geometry,
                    RenderEffectConfig::Empty,
                )
                .with_cache_policy(RenderEffectCachePolicy::FrameResources),
                RenderEffectObject::new(
                    RenderEffectKind::WaterReflection,
                    RenderEffectStage::PostProcess,
                    RenderEffectConfig::Empty,
                )
                .with_cache_policy(RenderEffectCachePolicy::FrameResources),
                RenderEffectObject::new(
                    RenderEffectKind::GrassSurface,
                    RenderEffectStage::Geometry,
                    RenderEffectConfig::Empty,
                )
                .with_cache_policy(RenderEffectCachePolicy::FrameResources),
                RenderEffectObject::new(
                    RenderEffectKind::TreeImposter,
                    RenderEffectStage::Geometry,
                    RenderEffectConfig::Empty,
                )
                .with_cache_policy(RenderEffectCachePolicy::FrameResources),
                RenderEffectObject::new(
                    RenderEffectKind::UiComposite,
                    RenderEffectStage::Composite,
                    RenderEffectConfig::Empty,
                )
                .with_cache_policy(RenderEffectCachePolicy::Stateless),
            ],
        }
    }

    #[inline]
    pub fn enabled_effects(&self) -> impl Iterator<Item = &RenderEffectObject> {
        self.effects.iter().filter(|effect| effect.enabled)
    }

    #[inline]
    pub fn find(&self, kind: RenderEffectKind) -> Option<&RenderEffectObject> {
        self.effects.iter().find(|effect| effect.kind == kind)
    }
}

impl Default for RenderEffectStack {
    #[inline]
    fn default() -> Self {
        Self::aaa_default()
    }
}

#[inline]
fn default_true() -> bool {
    true
}
#[inline]
fn default_effect_schema_version() -> u32 {
    1
}
#[inline]
fn default_shadow_cascade_count() -> u32 {
    4
}
#[inline]
fn default_shadow_resolution() -> u32 {
    2048
}
#[inline]
fn default_shadow_atlas_size() -> u32 {
    4096
}
#[inline]
fn default_shadow_max_distance() -> f32 {
    192.0
}
#[inline]
fn default_pcss_blocker_samples() -> u32 {
    8
}
#[inline]
fn default_pcss_filter_samples() -> u32 {
    16
}
#[inline]
fn default_pcss_light_radius() -> f32 {
    0.035
}
#[inline]
fn default_shadow_bias() -> f32 {
    0.0015
}
#[inline]
fn default_shadow_normal_bias() -> f32 {
    0.02
}
#[inline]
fn default_bloom_threshold() -> f32 {
    0.85
}
#[inline]
fn default_bloom_knee() -> f32 {
    0.35
}
#[inline]
fn default_bloom_intensity() -> f32 {
    0.085
}
#[inline]
fn default_bloom_radius() -> f32 {
    1.0
}
#[inline]
fn default_bloom_mips() -> u32 {
    5
}
#[inline]
fn default_taa_feedback() -> f32 {
    0.92
}
#[inline]
fn default_taa_clamping() -> f32 {
    1.0
}
#[inline]
fn default_taa_jitter_scale() -> f32 {
    1.0
}
#[inline]
fn default_tess_factor() -> f32 {
    4.0
}
#[inline]
fn default_tess_min_distance() -> f32 {
    8.0
}
#[inline]
fn default_tess_max_distance() -> f32 {
    96.0
}
#[inline]
fn default_tess_displacement_scale() -> f32 {
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aaa_effect_stack_contains_required_effect_objects() {
        let stack = RenderEffectStack::aaa_default();
        for kind in [
            RenderEffectKind::PcssShadows,
            RenderEffectKind::CascadedShadows,
            RenderEffectKind::Bloom,
            RenderEffectKind::ScreenSpaceAmbientOcclusion,
            RenderEffectKind::Taa,
            RenderEffectKind::Msaa,
            RenderEffectKind::Tessellation,
            RenderEffectKind::DepthOfField,
            RenderEffectKind::SunLensFlare,
            RenderEffectKind::WaterSurface,
            RenderEffectKind::GrassSurface,
            RenderEffectKind::TreeImposter,
        ] {
            assert!(stack.find(kind).is_some(), "missing {:?}", kind);
        }
    }

    #[test]
    fn effect_object_keeps_base_and_tuned_config_separate() {
        let base = RenderEffectConfig::Bloom(BloomEffectConfig::default());
        let tuned = RenderEffectConfig::Bloom(BloomEffectConfig {
            intensity: 0.2,
            ..BloomEffectConfig::default()
        });
        let effect = RenderEffectObject::new(
            RenderEffectKind::Bloom,
            RenderEffectStage::PostProcess,
            base,
        )
        .tuned(tuned);
        assert!(matches!(effect.base, RenderEffectConfig::Bloom(_)));
        assert!(
            matches!(effect.effective_config(), RenderEffectConfig::Bloom(cfg) if (cfg.intensity - 0.2).abs() < f32::EPSILON)
        );
    }
}
