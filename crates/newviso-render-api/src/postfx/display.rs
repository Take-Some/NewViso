use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToneMapOperator {
    /// Lightweight default for the first production HDR path. Stable, cheap and predictable.
    AcesApprox,
    Reinhard,
    None,
}

impl Default for ToneMapOperator {
    #[inline]
    fn default() -> Self {
        Self::AcesApprox
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AntiAliasingMode {
    None,
    /// Single-pass post-tonemap/HDR-compatible edge filter. This is the stable default until
    /// graph-level MSAA resolve resources are available on every provider.
    Fxaa,
    /// Temporal anti-aliasing. Requires provider-owned history color and velocity/depth inputs when available.
    Taa,
    /// Provider advertises the mode, but execution is valid only when the frame graph allocates
    /// multisampled scene/depth targets and explicit resolves.
    Msaa2x,
    Msaa4x,
    Msaa8x,
}

impl Default for AntiAliasingMode {
    #[inline]
    fn default() -> Self {
        Self::Fxaa
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ToneMapDisplayParams {
    /// Linear exposure multiplier. 1.0 means authored lighting is used as-is.
    #[serde(default = "default_exposure")]
    pub exposure: f32,
    /// Gamma/display transfer exponent. 2.2 is the non-HDR-monitor display baseline.
    #[serde(default = "default_gamma")]
    pub gamma: f32,
    /// Optional black floor lift before display encoding. Keep at 0 for physically based output.
    #[serde(default)]
    pub black_lift: f32,
    #[serde(default)]
    pub operator: ToneMapOperator,
}

impl Default for ToneMapDisplayParams {
    #[inline]
    fn default() -> Self {
        Self {
            exposure: default_exposure(),
            gamma: default_gamma(),
            black_lift: 0.0,
            operator: ToneMapOperator::AcesApprox,
        }
    }
}

#[inline]
fn default_exposure() -> f32 {
    1.12
}
#[inline]
fn default_gamma() -> f32 {
    2.2
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BloomParams {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// HDR threshold where bloom starts extracting highlights.
    #[serde(default = "default_bloom_threshold")]
    pub threshold: f32,
    /// Soft knee width around the threshold. Prevents harsh popping around bright edges.
    #[serde(default = "default_bloom_knee")]
    pub knee: f32,
    /// Final bloom contribution in linear HDR before tone mapping.
    #[serde(default = "default_bloom_intensity")]
    pub intensity: f32,
    /// Sampling radius multiplier. 1.0 keeps the fast production kernel.
    #[serde(default = "default_bloom_radius")]
    pub radius: f32,
}

impl Default for BloomParams {
    #[inline]
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: default_bloom_threshold(),
            knee: default_bloom_knee(),
            intensity: default_bloom_intensity(),
            radius: default_bloom_radius(),
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FxaaParams {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Standard FXAA edge threshold. Lower values catch more edges and cost the same.
    #[serde(default = "default_fxaa_edge_threshold")]
    pub edge_threshold: f32,
    /// Minimum luma range before filtering begins.
    #[serde(default = "default_fxaa_edge_threshold_min")]
    pub edge_threshold_min: f32,
    /// Sub-pixel blending strength. 0 disables softening, 1 uses full local blend.
    #[serde(default = "default_fxaa_subpixel_quality")]
    pub subpixel_quality: f32,
}

impl Default for FxaaParams {
    #[inline]
    fn default() -> Self {
        Self {
            enabled: true,
            edge_threshold: default_fxaa_edge_threshold(),
            edge_threshold_min: default_fxaa_edge_threshold_min(),
            subpixel_quality: default_fxaa_subpixel_quality(),
        }
    }
}

#[inline]
fn default_fxaa_edge_threshold() -> f32 {
    0.125
}
#[inline]
fn default_fxaa_edge_threshold_min() -> f32 {
    0.0312
}
#[inline]
fn default_fxaa_subpixel_quality() -> f32 {
    0.75
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TaaParams {
    #[serde(default)]
    pub enabled: bool,
    /// History blend factor. Higher values are smoother but can ghost more.
    #[serde(default = "default_taa_feedback")]
    pub feedback: f32,
    /// Neighborhood clamp strength for anti-ghosting.
    #[serde(default = "default_taa_neighborhood_clamping")]
    pub neighborhood_clamping: f32,
    /// Jitter scale applied to view jitter from the camera/view gateway.
    #[serde(default = "default_taa_jitter_scale")]
    pub jitter_scale: f32,
}

impl Default for TaaParams {
    #[inline]
    fn default() -> Self {
        Self {
            enabled: false,
            feedback: default_taa_feedback(),
            neighborhood_clamping: default_taa_neighborhood_clamping(),
            jitter_scale: default_taa_jitter_scale(),
        }
    }
}

#[inline]
fn default_taa_feedback() -> f32 {
    0.92
}
#[inline]
fn default_taa_neighborhood_clamping() -> f32 {
    1.0
}
#[inline]
fn default_taa_jitter_scale() -> f32 {
    1.0
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ColorGradeParams {
    #[serde(default = "default_saturation")]
    pub saturation: f32,
    #[serde(default = "default_contrast")]
    pub contrast: f32,
    /// Warm/cool offset. Positive warms highlights slightly, negative cools them.
    #[serde(default)]
    pub temperature: f32,
    #[serde(default = "default_vignette_strength")]
    pub vignette_strength: f32,
    #[serde(default = "default_local_contrast_strength")]
    pub local_contrast_strength: f32,
    #[serde(default = "default_dither_strength")]
    pub dither_strength: f32,
}

impl Default for ColorGradeParams {
    #[inline]
    fn default() -> Self {
        Self {
            saturation: default_saturation(),
            contrast: default_contrast(),
            temperature: 0.0,
            vignette_strength: default_vignette_strength(),
            local_contrast_strength: default_local_contrast_strength(),
            dither_strength: default_dither_strength(),
        }
    }
}

#[inline]
fn default_saturation() -> f32 {
    1.06
}
#[inline]
fn default_contrast() -> f32 {
    1.03
}
#[inline]
fn default_vignette_strength() -> f32 {
    0.10
}
#[inline]
fn default_local_contrast_strength() -> f32 {
    0.055
}
#[inline]
fn default_dither_strength() -> f32 {
    1.0
}
