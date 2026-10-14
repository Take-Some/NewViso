use super::*;
use newviso_render_client::{
    EnvironmentAmbientParams, EnvironmentColorParams, EnvironmentExposureParams,
    EnvironmentFogParams, EnvironmentRenderParams, FilmicCurveParams,
};

/// Artist values from original/clock/visualsettings.dat; older catalogs remain loadable.
pub(super) fn reference_exposure_defaults() -> EnvironmentExposureParams {
    EnvironmentExposureParams {
        enabled: true,
        curve_scale: -106.2,
        curve_power: 0.007,
        curve_offset: 104.7,
        maximum_ev: 3.0,
        step_multiplier: 1.5,
        maximum_step: 3.0,
        ..Default::default()
    }
}
fn radiance(f: &TimecycleFrame, name: &str, intensity: &str) -> [f32; 3] {
    f.color(name, [0.0; 3])
        .map(|c| c.max(0.0) * f.number(intensity, 0.0).max(0.0))
}
fn filmic(f: &TimecycleFrame, bright: bool) -> FilmicCurveParams {
    let suffix = if bright { "_bright" } else { "" };
    let n = |key: &str, default: f32| {
        f.number(&format!("postfx_tonemap_filmic_{key}{suffix}"), default)
    };
    FilmicCurveParams {
        a: n("a", 0.22),
        b: n("b", 0.30),
        c: n("c", 0.10),
        d: n("d", 0.20),
        e: n("e", 0.01),
        f: n("f", 0.30),
        white_point: n("w", 4.0),
    }
}
pub(super) fn frame_environment(
    f: &TimecycleFrame,
    celestial: &newviso_environment::CelestialFrame,
    mut exposure: EnvironmentExposureParams,
    seconds: f32,
) -> EnvironmentRenderParams {
    let n = |key: &str, default| f.number(key, default);
    let c = |key: &str, default| f.color(key, default);
    let rgba = |key: &str, cutoff: &str| {
        let color = c(key, [0.0; 3]);
        [color[0], color[1], color[2], n(cutoff, 0.0)]
    };
    exposure.enabled = true;
    exposure.minimum_ev = n("postfx_exposure_min", -3.5);
    exposure.maximum_ev = n("postfx_exposure_max", 3.0);
    exposure.tweak_ev = n("postfx_exposure", 0.0);
    let mut p = EnvironmentRenderParams {
        enabled: true,
        time_seconds: seconds,
        exposure,
        ambient: EnvironmentAmbientParams {
            natural_down: radiance(
                f,
                "light_natural_amb_down_col",
                "light_natural_amb_down_intensity",
            ),
            natural_up: radiance(
                f,
                "light_natural_amb_up_col",
                "light_natural_amb_up_intensity",
            )
            .map(|x| x * n("light_natural_amb_up_intensity_mult", 1.0)),
            artificial_exterior_down: radiance(
                f,
                "light_artificial_ext_down_col",
                "light_artificial_ext_down_intensity",
            ),
            artificial_exterior_up: radiance(
                f,
                "light_artificial_ext_up_col",
                "light_artificial_ext_up_intensity",
            ),
            artificial_interior_down: radiance(
                f,
                "light_artificial_int_down_col",
                "light_artificial_int_down_intensity",
            ),
            artificial_interior_up: radiance(
                f,
                "light_artificial_int_up_col",
                "light_artificial_int_up_intensity",
            ),
            directional_color: radiance(
                f,
                "light_directional_amb_col",
                "light_directional_amb_intensity",
            )
            .map(|x| x * n("light_directional_amb_intensity_mult", 1.0)),
            directional_direction_ws: celestial.sun_direction,
            down_wrap: n("light_amb_down_wrap", 1.0),
            natural_multiplier: n("natural_ambient_multiplier", 1.0),
            artificial_multiplier: n("artificial_ambient_multiplier", 1.0),
            occlusion_multiplier: n("light_amb_occ_mult", 1.0),
        },
        fog: EnvironmentFogParams {
            density: n("fog_density", 0.0) / 10000.0,
            start_distance: n("fog_start", 0.0),
            base_height: n("fog_base_height", 0.0),
            height_falloff: n("fog_falloff", 0.0) / 1000.0,
            alpha: n("fog_alpha", 0.0),
            haze_density: n("fog_haze_density", 0.0) / 10000.0,
            haze_start: n("fog_haze_start", 0.0),
            haze_alpha: n("fog_haze_alpha", 0.0),
            horizon_tint_scale: n("fog_horizon_tint_scale", 0.0),
            far_clip: n("far_clip", 10000.0),
            sun_color: c("fog_col", [0.0; 3]).map(|x| x * n("fog_hdr", 1.0)),
            celestial_fade: celestial.sun_fade.max(celestial.moon_fade),
            atmosphere_color: c("fog_east_col", [0.0; 3]).map(|x| x * n("fog_hdr", 1.0)),
            ground_color: c("fog_near_col", [0.0; 3]).map(|x| x * n("fog_hdr", 1.0)),
            haze_color: c("fog_haze_col", [0.0; 3]).map(|x| x * n("fog_haze_hdr", 1.0)),
            moon_color: c("fog_moon_col", [0.0; 3]).map(|x| x * n("fog_hdr", 1.0)),
            sun_direction_ws: celestial.sun_direction,
            moon_direction_ws: celestial.moon_direction,
            sun_power: n("fog_sun_lighting_calc_pow", 8.0),
            moon_power: n("fog_moon_lighting_calc_pow", 18.0),
            shape_bottom: n("fog_shape_bottom", 0.0),
            shape_top: n("fog_shape_top", 0.0),
            log10_visibility: n("fog_shape_log_10_of_visibility", 5.0),
            shape_weights: std::array::from_fn(|i| {
                n(
                    &format!("fog_shape_weight_{i}"),
                    if i == 0 { 1.0 } else { 0.0 },
                )
            }),
        },
        color: EnvironmentColorParams {
            correction: rgba("postfx_correct_col", "postfx_correct_cutoff"),
            shift: rgba("postfx_shift_col", "postfx_shift_cutoff"),
            desaturation: n("postfx_desaturation", 1.0),
            gradient_top: c("postfx_grad_top_col", [1.0; 3]),
            gradient_middle: c("postfx_grad_middle_col", [1.0; 3]),
            gradient_bottom: c("postfx_grad_bottom_col", [1.0; 3]),
            gradient_midpoint: n("postfx_grad_midpoint", 0.5),
            gradient_top_midpoint: n("postfx_grad_top_middle_midpoint", 0.5),
            gradient_bottom_midpoint: n("postfx_grad_middle_bottom_midpoint", 0.5),
            vignette_color: c("postfx_vignetting_col", [0.0; 3]),
            vignette_intensity: n("postfx_vignetting_intensity", 0.0),
            vignette_radius: n("postfx_vignetting_radius", 1.0),
            vignette_contrast: n("postfx_vignetting_contrast", 0.0),
            noise: n("postfx_noise", 0.0),
            noise_size: n("postfx_noise_size", 1.0),
            bloom_threshold: n("postfx_bright_pass_thresh", 1.0),
            bloom_width: n("postfx_bright_pass_thresh_width", 0.35),
            bloom_intensity: n("postfx_intensity_bloom", 0.0),
            scanline: [
                n("postfx_scanlineintensity", 0.0),
                n("postfx_scanline_frequency_0", 0.0),
                n("postfx_scanline_frequency_1", 0.0),
                n("postfx_scanline_speed", 0.0) * seconds,
            ],
        },
        // Defaults from visualsettings, replaced only when the source TC enables overrides.
        filmic_bright: FilmicCurveParams::default(),
        filmic_dark: FilmicCurveParams {
            e: 0.0,
            ..Default::default()
        },
        filmic_exposure_bright: -0.5,
        filmic_exposure_dark: 3.0,
        ..Default::default()
    };
    let dark = n("postfx_tonemap_filmic_override_dark", 0.0) > 0.0;
    let bright = n("postfx_tonemap_filmic_override_bright", 0.0) > 0.0;
    if dark || bright {
        p.filmic_dark = filmic(f, false);
        p.filmic_exposure_dark = n("postfx_tonemap_filmic_exposure_dark", 1.0);
        if dark && !bright {
            p.filmic_bright = p.filmic_dark;
            p.filmic_exposure_bright = p.filmic_exposure_dark;
        } else if dark && bright {
            p.filmic_bright = filmic(f, true);
            p.filmic_exposure_bright = n("postfx_tonemap_filmic_exposure_bright", -2.0);
        }
    }
    // Viewer-dependent fog-color blending is evaluated identically in both GPU paths.
    // The fade is packed into unused sun-color W, alongside source radiance.
    p
}
