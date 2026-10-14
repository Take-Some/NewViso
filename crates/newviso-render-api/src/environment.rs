//! Source-independent environment lighting and HDR frame contract.
//! Values are linear radiance. Display transfer belongs to the final composite.
use serde::{Deserialize, Serialize};

pub const ENVIRONMENT_UNIFORM_LANES: usize = 64;
pub const ENVIRONMENT_SHADER_SOURCE: &str = include_str!("../shaders/environment.glsl");

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EnvironmentAmbientParams {
    pub natural_down: [f32; 3],
    pub natural_up: [f32; 3],
    pub artificial_exterior_down: [f32; 3],
    pub artificial_exterior_up: [f32; 3],
    pub artificial_interior_down: [f32; 3],
    pub artificial_interior_up: [f32; 3],
    pub directional_color: [f32; 3],
    pub directional_direction_ws: [f32; 3],
    pub down_wrap: f32,
    pub natural_multiplier: f32,
    pub artificial_multiplier: f32,
    pub occlusion_multiplier: f32,
}
impl Default for EnvironmentAmbientParams {
    fn default() -> Self {
        Self {
            natural_down: [0.0; 3],
            natural_up: [0.0; 3],
            artificial_exterior_down: [0.0; 3],
            artificial_exterior_up: [0.0; 3],
            artificial_interior_down: [0.0; 3],
            artificial_interior_up: [0.0; 3],
            directional_color: [0.0; 3],
            directional_direction_ws: [0.0, 1.0, 0.0],
            down_wrap: 0.0,
            natural_multiplier: 1.0,
            artificial_multiplier: 1.0,
            occlusion_multiplier: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EnvironmentFogParams {
    pub density: f32,
    pub start_distance: f32,
    pub base_height: f32,
    pub height_falloff: f32,
    pub alpha: f32,
    pub haze_density: f32,
    pub haze_start: f32,
    pub haze_alpha: f32,
    pub horizon_tint_scale: f32,
    pub far_clip: f32,
    pub sun_color: [f32; 3],
    pub celestial_fade: f32,
    pub atmosphere_color: [f32; 3],
    pub ground_color: [f32; 3],
    pub haze_color: [f32; 3],
    pub moon_color: [f32; 3],
    pub sun_direction_ws: [f32; 3],
    pub moon_direction_ws: [f32; 3],
    pub sun_power: f32,
    pub moon_power: f32,
    pub shape_bottom: f32,
    pub shape_top: f32,
    pub log10_visibility: f32,
    pub shape_weights: [f32; 4],
}
impl Default for EnvironmentFogParams {
    fn default() -> Self {
        Self {
            density: 0.0,
            start_distance: 0.0,
            base_height: 0.0,
            height_falloff: 0.0,
            alpha: 0.0,
            haze_density: 0.0,
            haze_start: 0.0,
            haze_alpha: 0.0,
            horizon_tint_scale: 0.0,
            far_clip: 10000.0,
            sun_color: [0.0; 3],
            celestial_fade: 1.0,
            atmosphere_color: [0.0; 3],
            ground_color: [0.0; 3],
            haze_color: [0.0; 3],
            moon_color: [0.0; 3],
            sun_direction_ws: [0.0, 1.0, 0.0],
            moon_direction_ws: [0.0, -1.0, 0.0],
            sun_power: 8.0,
            moon_power: 18.0,
            shape_bottom: 0.0,
            shape_top: 0.0,
            log10_visibility: 5.0,
            shape_weights: [1.0, 0.0, 0.0, 0.0],
        }
    }
}

/// A/B/C/D/E/F/W rational filmic curve. W maps to display-linear white.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilmicCurveParams {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
    pub white_point: f32,
}
impl Default for FilmicCurveParams {
    fn default() -> Self {
        Self {
            a: 0.22,
            b: 0.30,
            c: 0.10,
            d: 0.20,
            e: 0.01,
            f: 0.30,
            white_point: 4.0,
        }
    }
}
impl FilmicCurveParams {
    pub fn evaluate(self, x: f32) -> f32 {
        let raw = |v: f32| {
            (v * (self.a * v + self.c * self.b) + self.d * self.e)
                / (v * (self.a * v + self.b) + self.d * self.f).max(1.0e-8)
                - self.e / self.f.max(1.0e-8)
        };
        (raw(x.max(0.0)) / raw(self.white_point.max(1.0e-5)).max(1.0e-8)).clamp(0.0, 1.0)
    }
    pub fn blend(self, b: Self, t: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * t.clamp(0.0, 1.0);
        Self {
            a: mix(self.a, b.a),
            b: mix(self.b, b.b),
            c: mix(self.c, b.c),
            d: mix(self.d, b.d),
            e: mix(self.e, b.e),
            f: mix(self.f, b.f),
            white_point: mix(self.white_point, b.white_point),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EnvironmentExposureParams {
    pub enabled: bool,
    pub curve_scale: f32,
    pub curve_power: f32,
    pub curve_offset: f32,
    pub minimum_ev: f32,
    pub maximum_ev: f32,
    pub tweak_ev: f32,
    pub push: f32,
    pub step_multiplier: f32,
    pub minimum_step: f32,
    pub maximum_step: f32,
    pub focus: f32,
}
impl Default for EnvironmentExposureParams {
    fn default() -> Self {
        Self {
            enabled: false,
            curve_scale: -106.6890745,
            curve_power: 0.0041897015,
            curve_offset: 104.6036417,
            minimum_ev: -3.5,
            maximum_ev: 5.0,
            tweak_ev: 0.0,
            push: 0.0,
            step_multiplier: 9.0,
            minimum_step: 0.15,
            maximum_step: 15.0,
            focus: 0.0,
        }
    }
}
impl EnvironmentExposureParams {
    pub fn target_ev(self, luminance: f32) -> f32 {
        let target = (self.curve_scale * luminance.max(1.0e-6).powf(self.curve_power)
            + self.curve_offset
            + self.tweak_ev)
            .clamp(self.minimum_ev, self.maximum_ev);
        (target + target.abs() * self.push * target.signum())
            .clamp(self.minimum_ev, self.maximum_ev)
    }
    pub fn adapt_ev(self, previous: f32, target: f32, dt: f32) -> f32 {
        if dt <= 0.0 {
            return previous;
        }
        let diff = target - previous;
        let rate = (self.step_multiplier * dt).clamp(0.0, 1.0) / dt;
        let step = (diff.abs() * rate).min(self.maximum_step);
        (if step <= self.minimum_step {
            target
        } else {
            (previous + step * diff.signum() * dt).clamp(previous.min(target), previous.max(target))
        })
        .clamp(self.minimum_ev, self.maximum_ev)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EnvironmentColorParams {
    pub correction: [f32; 4],
    pub shift: [f32; 4],
    pub desaturation: f32,
    pub gradient_top: [f32; 3],
    pub gradient_middle: [f32; 3],
    pub gradient_bottom: [f32; 3],
    pub gradient_midpoint: f32,
    pub gradient_top_midpoint: f32,
    pub gradient_bottom_midpoint: f32,
    pub vignette_color: [f32; 3],
    pub vignette_intensity: f32,
    pub vignette_radius: f32,
    pub vignette_contrast: f32,
    pub noise: f32,
    pub noise_size: f32,
    pub bloom_threshold: f32,
    pub bloom_width: f32,
    pub bloom_intensity: f32,
    pub scanline: [f32; 4],
}
impl Default for EnvironmentColorParams {
    fn default() -> Self {
        Self {
            correction: [1.0, 1.0, 1.0, 0.0],
            shift: [1.0, 1.0, 1.0, 0.0],
            desaturation: 1.0,
            gradient_top: [1.0; 3],
            gradient_middle: [1.0; 3],
            gradient_bottom: [1.0; 3],
            gradient_midpoint: 0.5,
            gradient_top_midpoint: 0.5,
            gradient_bottom_midpoint: 0.5,
            vignette_color: [0.0; 3],
            vignette_intensity: 0.0,
            vignette_radius: 10.0,
            vignette_contrast: 0.0,
            noise: 0.0,
            noise_size: 1.0,
            bloom_threshold: 0.85,
            bloom_width: 0.35,
            bloom_intensity: 0.0,
            scanline: [0.0; 4],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EnvironmentHeatHazeParams {
    pub start_distance: f32,
    pub far_distance: f32,
    pub minimum_intensity: f32,
    pub maximum_intensity: f32,
    pub layer1: [f32; 4],
    pub layer2: [f32; 4],
    pub displacement: [f32; 4],
}
impl Default for EnvironmentHeatHazeParams {
    fn default() -> Self {
        Self {
            start_distance: 0.0,
            far_distance: 500.0,
            minimum_intensity: 0.0,
            maximum_intensity: 0.0,
            layer1: [1.0, 1.0, 0.0, 0.0],
            layer2: [1.0, 1.0, 0.0, 0.0],
            displacement: [0.0; 4],
        }
    }
}

/// Optional frame environment overlay. Disabled packets preserve existing providers.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EnvironmentRenderParams {
    pub enabled: bool,
    pub ambient: EnvironmentAmbientParams,
    pub fog: EnvironmentFogParams,
    pub filmic_bright: FilmicCurveParams,
    pub filmic_dark: FilmicCurveParams,
    pub filmic_exposure_bright: f32,
    pub filmic_exposure_dark: f32,
    pub gamma: f32,
    pub exposure: EnvironmentExposureParams,
    pub color: EnvironmentColorParams,
    pub heat_haze: EnvironmentHeatHazeParams,
    pub wetness: f32,
    pub snow: f32,
    pub lightning: f32,
    pub time_seconds: f32,
}
impl Default for EnvironmentRenderParams {
    fn default() -> Self {
        Self {
            enabled: false,
            ambient: Default::default(),
            fog: Default::default(),
            filmic_bright: Default::default(),
            filmic_dark: Default::default(),
            filmic_exposure_bright: -2.0,
            filmic_exposure_dark: 1.0,
            gamma: 2.2,
            exposure: Default::default(),
            color: Default::default(),
            heat_haze: Default::default(),
            wetness: 0.0,
            snow: 0.0,
            lightning: 0.0,
            time_seconds: 0.0,
        }
    }
}

/// std140 vec4 lanes shared by forward/deferred surfaces and native PostFX.
/// Camera matrices, per-material masks and temporal exposure remain separate.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug)]
pub struct EnvironmentGpuBlock {
    pub lanes: [[f32; 4]; ENVIRONMENT_UNIFORM_LANES],
}
impl EnvironmentRenderParams {
    pub fn uniform_block(self, dt: f32, history_valid: bool) -> EnvironmentGpuBlock {
        let mut p = [[0.0; 4]; ENVIRONMENT_UNIFORM_LANES];
        let v = |c: [f32; 3], w: f32| [c[0], c[1], c[2], w];
        p[0] = [
            if self.enabled { 1.0 } else { 0.0 },
            dt.max(0.0),
            if history_valid { 1.0 } else { 0.0 },
            self.time_seconds,
        ];
        let a = self.ambient;
        p[1] = v(a.natural_down, a.down_wrap);
        p[2] = v(a.natural_up, a.natural_multiplier);
        p[3] = v(a.artificial_exterior_down, a.artificial_multiplier);
        p[4] = v(a.artificial_exterior_up, a.occlusion_multiplier);
        p[5] = v(a.artificial_interior_down, 0.0);
        p[6] = v(a.artificial_interior_up, 0.0);
        p[7] = v(a.directional_color, 0.0);
        p[8] = v(a.directional_direction_ws, 0.0);
        let f = self.fog;
        p[9] = [f.density, f.start_distance, f.base_height, f.alpha];
        p[10] = [f.height_falloff, f.haze_density, f.haze_start, f.haze_alpha];
        p[11] = [f.horizon_tint_scale, f.sun_power, f.moon_power, f.far_clip];
        p[12] = v(f.sun_color, f.celestial_fade);
        p[13] = v(f.atmosphere_color, 0.0);
        p[14] = v(f.ground_color, 0.0);
        p[15] = v(f.haze_color, 0.0);
        p[16] = v(f.moon_color, 0.0);
        p[17] = v(f.sun_direction_ws, 0.0);
        p[18] = v(f.moon_direction_ws, 0.0);
        p[19] = [f.shape_bottom, f.shape_top, f.log10_visibility, 0.0];
        p[20] = f.shape_weights;
        let b = self.filmic_bright;
        let d = self.filmic_dark;
        p[21] = [b.a, b.b, b.c, b.d];
        p[22] = [b.e, b.f, b.white_point, self.filmic_exposure_bright];
        p[23] = [d.a, d.b, d.c, d.d];
        p[24] = [d.e, d.f, d.white_point, self.filmic_exposure_dark];
        p[25] = [
            self.gamma,
            if self.exposure.enabled { 1.0 } else { 0.0 },
            0.0,
            0.0,
        ];
        let e = self.exposure;
        p[26] = [e.curve_scale, e.curve_power, e.curve_offset, e.tweak_ev];
        p[27] = [e.minimum_ev, e.maximum_ev, e.push, e.step_multiplier];
        p[28] = [e.minimum_step, e.maximum_step, e.focus, 0.0];
        let c = self.color;
        p[29] = c.correction;
        p[30] = c.shift;
        p[31] = v(c.gradient_top, c.gradient_midpoint);
        p[32] = v(c.gradient_middle, c.gradient_top_midpoint);
        p[33] = v(c.gradient_bottom, c.gradient_bottom_midpoint);
        p[34] = v(c.vignette_color, c.vignette_intensity);
        p[35] = [
            c.vignette_radius,
            c.vignette_contrast,
            c.desaturation,
            c.noise,
        ];
        let h = self.heat_haze;
        p[36] = [
            h.start_distance,
            h.far_distance,
            h.minimum_intensity,
            h.maximum_intensity,
        ];
        p[37] = h.layer1;
        p[38] = h.layer2;
        p[39] = h.displacement;
        p[40] = [c.noise_size, 0.0, 0.0, 0.0];
        p[41] = [self.wetness, self.snow, self.lightning, 0.0];
        p[42] = [c.bloom_threshold, c.bloom_width, c.bloom_intensity, 0.0];
        p[43] = c.scanline;
        EnvironmentGpuBlock { lanes: p }
    }
    pub fn validate(self) -> Result<(), String> {
        if self
            .uniform_block(0.0, false)
            .lanes
            .into_iter()
            .flatten()
            .any(|v| !v.is_finite())
        {
            return Err("environment render parameters must be finite".into());
        }
        if self.exposure.maximum_ev < self.exposure.minimum_ev {
            return Err("environment exposure maximum must not be below minimum".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filmic_black_white_and_monotonic_hdr() {
        let p = FilmicCurveParams::default();
        assert!(p.evaluate(0.0).abs() < 1e-7);
        assert!((p.evaluate(p.white_point) - 1.0).abs() < 1e-6);
        let mut last = 0.0;
        for i in 0..1000 {
            let x = p.evaluate(i as f32 / 100.0);
            assert!(x >= last);
            last = x;
        }
        let expected = 0.5505822;
        assert!((p.evaluate(1.0) - expected).abs() < 1e-5);
    }
    #[test]
    fn exposure_follows_luminance_and_clamps_source_ev() {
        let p = EnvironmentExposureParams::default();
        assert!(p.target_ev(0.1) > p.target_ev(1.0));
        assert!(p.target_ev(1.0) > p.target_ev(100.0));
        assert_eq!(p.target_ev(1e20), p.minimum_ev);
        assert!(p.adapt_ev(-3.5, 2.0, 1.0 / 60.0) > -3.5);
        assert_eq!(p.adapt_ev(-3.5, 2.0, 0.0), -3.5);
        let x = p.adapt_ev(-3.5, 2.0, 1.0 / 30.0);
        let y = p.adapt_ev(-3.5, 2.0, 1.0 / 60.0);
        assert!((x + 3.5 - 2.0 * (y + 3.5)).abs() < 1e-5);
    }
    #[test]
    fn environment_gpu_std140_and_old_packet_defaults() {
        assert_eq!(std::mem::size_of::<EnvironmentGpuBlock>(), 1024);
        assert_eq!(std::mem::align_of::<EnvironmentGpuBlock>(), 16);
        let p: EnvironmentRenderParams = serde_json::from_str("{}").unwrap();
        assert!(!p.enabled);
        p.validate().unwrap();
        assert_eq!(p.uniform_block(0.0, false).lanes[0][0], 0.0);
    }
}
