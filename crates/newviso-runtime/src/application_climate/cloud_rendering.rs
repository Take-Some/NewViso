use super::*;

/// Detail scales live in Shared XML; animated density and colors come from TC.
#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub(super) struct CloudRenderingPolicy {
    pub edge_detail_scale: f32,
    pub overall_detail_scale: f32,
}

impl Default for CloudRenderingPolicy {
    fn default() -> Self {
        Self { edge_detail_scale: 16.0, overall_detail_scale: 8.0 }
    }
}

impl CloudRenderingPolicy {
    pub(super) fn validate(&self) -> Result<(), String> {
        if [self.edge_detail_scale, self.overall_detail_scale]
            .iter().any(|v| !v.is_finite() || *v <= 0.0 || *v > 1024.0)
        {
            return Err("invalid Shared sky-cloud detail scales".into());
        }
        Ok(())
    }

    pub(super) fn sky_clouds(
        &self, frame: &TimecycleFrame, sky_scale: f32,
    ) -> newviso_scene::KeyframedSkyCloudDesc {
        let n = |key: &str, fallback| frame.number(key, fallback);
        let square = |key: &str, fallback| frame.color(key, fallback).map(|c| c * c);
        let hdr = n("sky_cloud_hdr", 0.8) * sky_scale;
        let small = frame.color("sky_small_cloud_col", [0.9; 3]).map(|c| c * hdr);
        newviso_scene::KeyframedSkyCloudDesc {
            generation: [
                n("sky_cloud_gen_frequency", 2.0), n("sky_cloud_gen_scale", 48.0),
                n("sky_cloud_gen_threshold", 0.65), n("sky_cloud_gen_softness", 0.2).max(1.0e-5),
            ],
            density: [
                n("sky_cloud_base_strength", 0.9), n("sky_cloud_density_mult", 2.24),
                n("sky_cloud_density_bias", 0.66), n("sky_cloud_fadeout", 0.3),
            ],
            lighting: [
                n("sky_cloud_shadow_strength", 0.2), n("sky_cloud_offset", 0.1),
                n("sky_cloud_overall_color", 0.5), hdr,
            ],
            detail: [
                n("sky_cloud_edge_strength", 0.5), self.edge_detail_scale,
                n("sky_cloud_overall_strength", 0.5), self.overall_detail_scale,
            ],
            small: [
                n("sky_small_cloud_detail_scale", 3.6), n("sky_small_cloud_detail_strength", 0.5),
                n("sky_small_cloud_density_mult", 1.7), n("sky_small_cloud_density_bias", 0.558),
            ],
            small_color: [
                small[0], small[1], small[2], n("sky_cloud_gen_density_offset", 0.15),
            ],
            base_color: square("sky_cloud_base_col", [0.75; 3]),
            mid_color: square("sky_cloud_mid_col", [0.95; 3]),
            shadow_color: square("sky_cloud_shadow_col", [0.8; 3]),
        }
    }
}

/// Clouds.cpp uses TC sun/moon colors and its own lunar intensity, not disc HDR.
pub(super) fn cloud_light(
    frame: &TimecycleFrame, celestial: &newviso_environment::CelestialFrame,
) -> ([f32; 3], f32) {
    let sun = frame.color("sky_sun_col", [1.0; 3]);
    let moon = frame.color("sky_moon_col", [0.8, 0.85, 1.0]);
    let lunar = if celestial.moon_fade > 0.0 {
        // CSkySettings rotates (0,1,0) from -2.90 to +2.70 radians.
        0.5 + 0.5 * (-2.90 + 5.60 * celestial.lunar_phase).cos()
    } else { 1.0 };
    (std::array::from_fn(|i| {
        let c = sun[i] * celestial.sun_fade + moon[i] * celestial.moon_fade;
        c * c * lunar
    }), lunar)
}

#[cfg(test)]
mod tests {
    use super::*;
    use newviso_environment::CelestialFrame;
    fn frame() -> TimecycleFrame {
        let mut frame = TimecycleFrame::default();
        // Channel values exercise the original signed and >2.0 artist ranges.
        frame.channels.insert("sky_cloud_density_mult".into(), 2.7969);
        frame.channels.insert("sky_cloud_shadow_strength".into(), -0.2676);
        for (key, value) in [("r", 0.2), ("g", 0.3), ("b", 0.4)] {
            frame.channels.insert(format!("sky_small_cloud_col_{key}"), value);
            frame.channels.insert(format!("sky_cloud_base_col_{key}"), value);
        }
        frame.channels.insert("sky_cloud_hdr".into(), 10.0);
        frame
    }
    #[test]
    fn sky_clouds_preserve_source_density_and_color_spaces() {
        let clouds = CloudRenderingPolicy::default().sky_clouds(&frame(), 1.0);
        assert!((clouds.density[1] - 2.7969).abs() < 1e-6);
        assert!((clouds.lighting[0] + 0.2676).abs() < 1e-6);
        assert_eq!(&clouds.small_color[..3], &[2.0, 3.0, 4.0]);
        assert!((clouds.base_color[0] - 0.04).abs() < 1e-6);
        assert_eq!(clouds.detail[1], 16.0);
        assert_eq!(clouds.detail[3], 8.0);
    }
    #[test]
    fn cloud_light_tracks_lunar_phase_without_sun_disc_hdr() {
        let mut c = CelestialFrame {
            sun_direction: [0.0, -1.0, 0.0], moon_direction: [0.0, 1.0, 0.0],
            light_direction: [0.0, 1.0, 0.0], sun_fade: 0.0, moon_fade: 1.0,
            light_intensity: 1.0, lunar_phase: 0.5,
        };
        let (full, factor) = cloud_light(&TimecycleFrame::default(), &c);
        assert!((factor - 0.9975021).abs() < 1e-6);
        c.lunar_phase = 0.0;
        let (new, _) = cloud_light(&TimecycleFrame::default(), &c);
        assert!(full[0] > new[0] * 60.0);
        c.sun_fade = 1.0; c.moon_fade = 0.0;
        assert_eq!(cloud_light(&TimecycleFrame::default(), &c), ([1.0; 3], 1.0));
    }
}
