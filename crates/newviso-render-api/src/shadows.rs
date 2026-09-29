use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShadowTechnique {
    None,
    DirectionalDepthMap,
    CascadedShadowMaps,
    PointCubeMap,
    SpotDepthMap,
}

impl Default for ShadowTechnique {
    #[inline]
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CascadedShadowDesc {
    #[serde(default = "default_cascade_count")]
    pub cascade_count: u32,
    #[serde(default = "default_cascade_lambda")]
    pub split_lambda: f32,
    #[serde(default = "default_shadow_distance")]
    pub max_distance: f32,
    #[serde(default = "default_shadow_atlas_size")]
    pub atlas_size: u32,
    #[serde(default = "default_true")]
    pub stable_snap: bool,
}

impl Default for CascadedShadowDesc {
    #[inline]
    fn default() -> Self {
        Self {
            cascade_count: default_cascade_count(),
            split_lambda: default_cascade_lambda(),
            max_distance: default_shadow_distance(),
            atlas_size: default_shadow_atlas_size(),
            stable_snap: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PcssShadowFilterDesc {
    #[serde(default = "default_pcss_blocker_samples")]
    pub blocker_samples: u32,
    #[serde(default = "default_pcss_filter_samples")]
    pub filter_samples: u32,
    #[serde(default = "default_pcss_light_radius")]
    pub light_radius: f32,
}

impl Default for PcssShadowFilterDesc {
    #[inline]
    fn default() -> Self {
        Self {
            blocker_samples: default_pcss_blocker_samples(),
            filter_samples: default_pcss_filter_samples(),
            light_radius: default_pcss_light_radius(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ShadowQualityDesc {
    pub technique: ShadowTechnique,
    pub resolution: u32,
    pub cascade_count: u32,
    pub max_distance: f32,
    pub bias: f32,
    pub normal_bias: f32,
    pub softness: f32,
    #[serde(default)]
    pub cascades: CascadedShadowDesc,
    #[serde(default)]
    pub pcss: PcssShadowFilterDesc,
}

impl Default for ShadowQualityDesc {
    #[inline]
    fn default() -> Self {
        Self {
            technique: ShadowTechnique::CascadedShadowMaps,
            resolution: 2048,
            cascade_count: default_cascade_count(),
            max_distance: default_shadow_distance(),
            bias: 0.0015,
            normal_bias: 0.02,
            softness: 0.35,
            cascades: CascadedShadowDesc::default(),
            pcss: PcssShadowFilterDesc::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ShadowPassStats {
    pub enabled: bool,
    pub cascades: u32,
    pub shadow_draws: u32,
    pub shadow_map_resolution: u32,
    pub last_shadow_pass_ms: f32,
}

#[inline]
fn default_true() -> bool {
    true
}
#[inline]
fn default_cascade_count() -> u32 {
    4
}
#[inline]
fn default_cascade_lambda() -> f32 {
    0.65
}
#[inline]
fn default_shadow_distance() -> f32 {
    192.0
}
#[inline]
fn default_shadow_atlas_size() -> u32 {
    4096
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shadow_quality_defaults_to_csm_pcss_contract() {
        let quality = ShadowQualityDesc::default();
        assert_eq!(quality.technique, ShadowTechnique::CascadedShadowMaps);
        assert_eq!(quality.cascade_count, 4);
        assert_eq!(quality.cascades.atlas_size, 4096);
        assert_eq!(quality.pcss.blocker_samples, 8);
    }
}
