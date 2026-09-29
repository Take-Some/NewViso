use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::{Path, PathBuf},
};

mod runtime_settings;
pub use runtime_settings::*;
mod validation;
mod world_persistence;
use validation::*;
pub use world_persistence::ProjectWorldPersistence;

pub const PROJECT_MANIFEST_NAME: &str = "project.json";
pub const PROJECT_SCHEMA_V1: &str = "newviso.project.v1";
pub const RUNTIME_SETTINGS_SCHEMA_V1: &str = "newviso.project.runtime.v1";
pub const ENVIRONMENT_SCHEMA_V1: &str = "newviso.environment.v1";
pub const SCRIPTS_SCHEMA_V1: &str = "newviso.scripts.v1";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectManifest {
    pub schema: String,
    pub project: ProjectIdentity,
    #[serde(default)]
    pub paths: ProjectPaths,
    pub files: ProjectFiles,
    /// Project-owned scripting graph. The manifest names one root entrypoint;
    /// relative child imports are resolved by the selected scripting provider.
    #[serde(default)]
    pub scripts: Option<ProjectScriptEntrypoint>,
    #[serde(default)]
    pub capabilities: ProjectCapabilities,
    #[serde(default)]
    pub providers: ProjectProviders,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectIdentity {
    pub id: String,
    pub name: String,
    #[serde(default = "default_project_version")]
    pub version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectPaths {
    pub assets: PathBuf,
    pub content: PathBuf,
    pub cache: PathBuf,
}

impl Default for ProjectPaths {
    fn default() -> Self {
        Self {
            assets: PathBuf::from("assets"),
            content: PathBuf::from("content"),
            cache: PathBuf::from(".newviso/cache"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectFiles {
    pub runtime: String,
    pub environment: String,
    pub scene: String,
    #[serde(default)]
    pub scripts: Option<String>,
    #[serde(default)]
    pub ui: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectScriptEntrypoint {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub provider: String,
    pub entrypoint: String,
    #[serde(default)]
    pub lifecycle: ProjectScriptLifecycle,
    #[serde(default)]
    pub permissions: Vec<ProjectScriptPermission>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectScriptLifecycle {
    pub start: Option<String>,
    pub frame: Option<String>,
    pub event: Option<String>,
    pub shutdown: Option<String>,
}

impl Default for ProjectScriptLifecycle {
    fn default() -> Self {
        Self {
            start: Some("on_start".to_owned()),
            frame: Some("on_frame".to_owned()),
            event: Some("on_event".to_owned()),
            shutdown: Some("on_shutdown".to_owned()),
        }
    }
}

impl ProjectScriptEntrypoint {
    pub fn as_runtime_config(&self) -> ProjectScripts {
        ProjectScripts {
            schema: SCRIPTS_SCHEMA_V1.to_owned(),
            enabled: self.enabled,
            provider: self.provider.clone(),
            modules: vec![ProjectScriptModule {
                asset: self.entrypoint.clone(),
                on_start: self.lifecycle.start.clone(),
                on_frame: self.lifecycle.frame.clone(),
                on_event: self.lifecycle.event.clone(),
                on_shutdown: self.lifecycle.shutdown.clone(),
                permissions: self.permissions.clone(),
            }],
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectCapabilities {
    pub required: Vec<ProjectCapabilityRequest>,
    pub optional: Vec<ProjectCapabilityRequest>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectCapabilityRequest {
    pub id: String,
    #[serde(default = "default_capability_version")]
    pub min_version: u32,
    #[serde(default)]
    pub provider: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectProviders {
    pub logging: Option<String>,
    pub input: Option<String>,
    pub assets: Option<String>,
    pub ecs: Option<String>,
    pub platform: Option<String>,
    pub renderer: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectSkyClouds {
    pub enabled: bool,
    pub coverage: f32,
    pub density: f32,
    pub softness: f32,
    pub scale: f32,
    pub detail_scale: f32,
    pub speed: [f32; 2],
    pub horizon_fade: f32,
    pub macro_scale: f32,
    pub macro_strength: f32,
    pub detail_strength: f32,
    pub micro_strength: f32,
    pub erosion_strength: f32,
    pub warp_strength: f32,
    pub shape_contrast: f32,
    pub shear_speed: [f32; 2],
    pub seed_offset: [f32; 2],
    pub large_speed: f32,
    pub small_speed: f32,
    pub overall_detail_speed: f32,
    pub edge_detail_speed: f32,
    pub noise_phase_scale: f32,
}

impl Default for ProjectSkyClouds {
    fn default() -> Self {
        Self {
            // A world has a usable atmosphere by default. Projects can disable
            // or fully replace this profile, but they should not have to wire
            // basic clouds merely to avoid an empty sky.
            enabled: true,
            coverage: 0.35,
            density: 0.65,
            softness: 0.28,
            scale: 0.8,
            detail_scale: 4.0,
            speed: [0.01, 0.0035],
            horizon_fade: 0.18,
            macro_scale: 0.22,
            macro_strength: 0.62,
            detail_strength: 0.22,
            micro_strength: 0.09,
            erosion_strength: 0.54,
            warp_strength: 0.07,
            shape_contrast: 0.96,
            shear_speed: [0.0, 0.0],
            seed_offset: [13.37, -8.21],
            large_speed: 5.0,
            small_speed: 1.0,
            overall_detail_speed: 1.0,
            edge_detail_speed: 1.0,
            noise_phase_scale: 0.01,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectVolumetricClouds {
    pub enabled: bool,
    /// Bottom/top of the participating cloud volume in world Y meters.
    pub base_altitude: f32,
    pub top_altitude: f32,
    /// Maximum camera-to-cloud integration distance.
    pub max_distance: f32,
    /// Fraction of the main viewport rendered by the raymarch pass.
    pub resolution_scale: f32,
    pub ray_steps: u32,
    pub light_steps: u32,
    pub coverage: f32,
    pub density: f32,
    pub shape_scale: f32,
    pub detail_scale: f32,
    pub detail_strength: f32,
    pub erosion_strength: f32,
    pub extinction: f32,
    pub scattering: f32,
    pub ambient: f32,
    pub phase_forward: f32,
    pub powder_strength: f32,
    pub temporal_blend: f32,
    pub jitter_strength: f32,
}

impl Default for ProjectVolumetricClouds {
    fn default() -> Self {
        Self {
            enabled: false,
            base_altitude: 800.0,
            top_altitude: 2200.0,
            max_distance: 20_000.0,
            resolution_scale: 0.5,
            ray_steps: 48,
            light_steps: 6,
            coverage: 0.45,
            density: 1.0,
            shape_scale: 0.00032,
            detail_scale: 0.0018,
            detail_strength: 0.42,
            erosion_strength: 0.55,
            extinction: 0.012,
            scattering: 1.0,
            ambient: 0.28,
            phase_forward: 0.55,
            powder_strength: 0.45,
            temporal_blend: 0.88,
            jitter_strength: 1.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectSkyEnvironment {
    pub model: String,
    #[serde(default = "default_sky_dome_scale")]
    pub dome_scale: f32,
    #[serde(default)]
    pub horizon_level: f32,
    #[serde(default)]
    pub base_noise_texture: Option<String>,
    #[serde(default)]
    pub starfield_texture: Option<String>,
    #[serde(default)]
    pub detail_noise_texture: Option<String>,
    #[serde(default)]
    pub billboard_texture: Option<String>,
    #[serde(default)]
    pub clouds: ProjectSkyClouds,
    #[serde(default)]
    pub volumetric_clouds: ProjectVolumetricClouds,
}

impl Default for ProjectSkyEnvironment {
    fn default() -> Self {
        Self {
            model: "models/skydome.ydd@skydome_high".to_owned(),
            dome_scale: default_sky_dome_scale(),
            horizon_level: 0.0,
            base_noise_texture: None,
            starfield_texture: None,
            detail_noise_texture: None,
            billboard_texture: None,
            clouds: ProjectSkyClouds::default(),
            volumetric_clouds: ProjectVolumetricClouds::default(),
        }
    }
}

fn default_project_sky() -> Option<ProjectSkyEnvironment> {
    Some(ProjectSkyEnvironment::default())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectWeatherEnvironment {
    pub enabled: bool,
    pub current: String,
    pub next: String,
    pub blend: f32,
}

impl Default for ProjectWeatherEnvironment {
    fn default() -> Self {
        Self {
            enabled: true,
            current: "CLEAR".to_owned(),
            next: "CLEAR".to_owned(),
            blend: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectAtmosphericCloudAnimMode {
    #[default]
    Combine,
    Sculpt,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectAtmosphericCloudUvLayer {
    pub enabled: bool,
    pub mode: ProjectAtmosphericCloudAnimMode,
    pub velocity: [f32; 2],
    pub scale: f32,
    pub weight: f32,
}

impl Default for ProjectAtmosphericCloudUvLayer {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: ProjectAtmosphericCloudAnimMode::Combine,
            velocity: [0.0, 0.0],
            scale: 1.0,
            weight: 1.0,
        }
    }
}

fn default_atmospheric_cloud_uv_layers() -> [ProjectAtmosphericCloudUvLayer; 3] {
    [ProjectAtmosphericCloudUvLayer::default(); 3]
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectAtmosphericCloudLayer {
    pub id: String,
    /// Optional authored semantic mesh. When omitted, NewViso builds a generic
    /// world-horizontal cloud sheet so projects can use the runtime without a source-format asset.
    pub model: Option<String>,
    /// Mesh index inside the decoded ModelResource. GTA CloudHat drawables can
    /// contain more than one geometry/material section.
    pub model_mesh_index: usize,
    /// Legacy density texture alias used by pre-CloudsPS authored projects.
    pub texture: Option<String>,
    /// Native GTA CloudsPS sampler closure. Imported CloudHat assets populate
    /// these by shader-parameter hash rather than filename heuristics.
    pub density_texture: Option<String>,
    pub normal_texture: Option<String>,
    pub detail_density_texture: Option<String>,
    pub detail_normal_texture: Option<String>,
    pub detail_density2_texture: Option<String>,
    pub detail_normal2_texture: Option<String>,
    /// Native shader constants retained per drawable/material.
    pub density_shift_scale: [f32; 4],
    pub scatter: [f32; 4],
    pub piercing: [f32; 4],
    pub scale_diffuse_fill_ambient: [f32; 4],
    pub wrap_lighting: [f32; 4],
    pub rescale_uv: [[f32; 2]; 3],
    pub layer_anim_scale: [[f32; 2]; 3],
    pub position: [f32; 3],
    pub rotation_degrees: [f32; 3],
    pub scale: [f32; 3],
    pub angular_velocity_degrees: [f32; 3],
    pub rotation_scale: f32,
    /// Per-axis camera following. In the Y-up runtime [1,0,1] follows the
    /// observer horizontally while preserving authored altitude.
    pub camera_position_scale: [f32; 3],
    pub altitude_min: Option<f32>,
    pub altitude_min_fade: f32,
    pub altitude_max: Option<f32>,
    pub altitude_max_fade: f32,
    pub transition_seconds: f32,
    pub transition_in_time_percent: f32,
    pub transition_out_time_percent: f32,
    pub transition_delay_percent: f32,
    /// Optional per-CloudHat midpoint. Falls back to atmospheric_clouds.transition_midpoint.
    pub transition_midpoint: Option<f32>,
    /// Optional per-CloudHat alpha range. Falls back to atmospheric_clouds.transition_alpha_range.
    pub transition_alpha_range: Option<f32>,
    pub cost_factor: f32,
    pub soft_intersection_distance: f32,
    pub density: f32,
    pub softness: f32,
    pub opacity: f32,
    pub color: [f32; 3],
    pub weather_weights: BTreeMap<String, f32>,
    #[serde(default = "default_atmospheric_cloud_uv_layers")]
    pub uv_layers: [ProjectAtmosphericCloudUvLayer; 3],
}

impl Default for ProjectAtmosphericCloudLayer {
    fn default() -> Self {
        Self {
            id: String::new(),
            model: None,
            model_mesh_index: 0,
            texture: None,
            density_texture: None,
            normal_texture: None,
            detail_density_texture: None,
            detail_normal_texture: None,
            detail_density2_texture: None,
            detail_normal2_texture: None,
            density_shift_scale: [0.0, 1.0, 0.0, 0.0],
            scatter: [-0.75, 0.5625, 2.1, 1.0],
            piercing: [1.0, 1.0, 1.0, 1.0],
            scale_diffuse_fill_ambient: [1.0, 1.0, 1.0, 0.0],
            wrap_lighting: [1.0, 1.0, 1.0, 0.0],
            rescale_uv: [[1.0, 1.0]; 3],
            layer_anim_scale: [[1.0, 1.0]; 3],
            position: [0.0, 1200.0, 0.0],
            rotation_degrees: [0.0; 3],
            scale: [1000.0, 100.0, 1000.0],
            angular_velocity_degrees: [0.0; 3],
            rotation_scale: 1.0,
            camera_position_scale: [1.0, 0.0, 1.0],
            altitude_min: None,
            altitude_min_fade: 0.0,
            altitude_max: None,
            altitude_max_fade: 0.0,
            transition_seconds: 5.0,
            transition_in_time_percent: 1.0,
            transition_out_time_percent: 1.0,
            transition_delay_percent: 0.0,
            transition_midpoint: None,
            transition_alpha_range: None,
            cost_factor: 0.5,
            soft_intersection_distance: 12.0,
            density: 0.65,
            softness: 0.12,
            opacity: 1.0,
            color: [1.0; 3],
            weather_weights: BTreeMap::new(),
            uv_layers: default_atmospheric_cloud_uv_layers(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectAtmosphericClouds {
    pub enabled: bool,
    pub cloud_hat_speed: f32,
    pub wind_min_speed: f32,
    pub wind_max_speed: f32,
    pub altitude_scroll_scale: f32,
    pub global_alpha: f32,
    pub transition_midpoint: f32,
    pub transition_alpha_range: f32,
    pub streaming_budget: f32,
    pub soft_depth_resolution: u32,
    pub layers: Vec<ProjectAtmosphericCloudLayer>,
}

impl Default for ProjectAtmosphericClouds {
    fn default() -> Self {
        Self {
            enabled: false,
            cloud_hat_speed: 1.0,
            wind_min_speed: 0.0,
            wind_max_speed: 5.0,
            altitude_scroll_scale: 0.0,
            global_alpha: 1.0,
            transition_midpoint: 0.5,
            transition_alpha_range: 0.0,
            streaming_budget: 1.0,
            soft_depth_resolution: 512,
            layers: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectEnvironment {
    pub schema: String,
    #[serde(default = "default_clear_color")]
    pub clear_color: [f32; 4],
    #[serde(default = "default_project_sky")]
    pub sky: Option<ProjectSkyEnvironment>,
    #[serde(default)]
    pub weather: ProjectWeatherEnvironment,
    #[serde(default)]
    pub atmospheric_clouds: ProjectAtmosphericClouds,
}

impl Default for ProjectEnvironment {
    fn default() -> Self {
        Self {
            schema: ENVIRONMENT_SCHEMA_V1.to_owned(),
            clear_color: default_clear_color(),
            sky: default_project_sky(),
            weather: ProjectWeatherEnvironment::default(),
            atmospheric_clouds: ProjectAtmosphericClouds::default(),
        }
    }
}

impl ProjectEnvironment {
    pub fn from_value(value: serde_json::Value) -> Result<Self, ProjectError> {
        let environment: Self = serde_json::from_value(value)
            .map_err(|error| ProjectError::Asset(format!("environment decode failed: {error}")))?;
        if environment.schema != ENVIRONMENT_SCHEMA_V1 {
            return Err(ProjectError::Asset(format!(
                "unsupported environment schema '{}', expected '{}'",
                environment.schema, ENVIRONMENT_SCHEMA_V1
            )));
        }
        if environment
            .clear_color
            .iter()
            .any(|value| !value.is_finite())
        {
            return Err(ProjectError::Asset(
                "environment clear_color contains a non-finite value".to_owned(),
            ));
        }
        if environment.weather.enabled {
            let current = environment.weather.current.trim();
            let next = environment.weather.next.trim();
            if current.is_empty()
                || next.is_empty()
                || current.len() > 128
                || next.len() > 128
                || current.chars().any(char::is_control)
                || next.chars().any(char::is_control)
                || !environment.weather.blend.is_finite()
                || !(0.0..=1.0).contains(&environment.weather.blend)
            {
                return Err(ProjectError::Asset(
                    "environment.weather contains invalid current/next/blend".to_owned(),
                ));
            }
        }
        if let Some(sky) = &environment.sky {
            validate_logical_asset_path("environment.sky.model", &sky.model)?;
            if !sky.model.contains('@') {
                return Err(ProjectError::Asset(
                    "environment.sky.model must address a specific semantic entry".to_owned(),
                ));
            }
            if !sky.dome_scale.is_finite()
                || !(100.0..=1_000_000.0).contains(&sky.dome_scale)
                || !sky.horizon_level.is_finite()
            {
                return Err(ProjectError::Asset(
                    "environment.sky dome_scale/horizon_level is invalid".to_owned(),
                ));
            }
            if let Some(texture) = sky.billboard_texture.as_deref() {
                validate_logical_asset_path("environment.sky.billboard_texture", texture)?;
                if !texture.contains('@') {
                    return Err(ProjectError::Asset(
                        "environment.sky.billboard_texture must address a specific semantic entry"
                            .to_owned(),
                    ));
                }
            }
            for (label, texture) in [
                (
                    "environment.sky.base_noise_texture",
                    sky.base_noise_texture.as_deref(),
                ),
                (
                    "environment.sky.starfield_texture",
                    sky.starfield_texture.as_deref(),
                ),
                (
                    "environment.sky.detail_noise_texture",
                    sky.detail_noise_texture.as_deref(),
                ),
            ] {
                if let Some(texture) = texture {
                    validate_logical_asset_path(label, texture)?;
                    if !texture.contains('@') {
                        return Err(ProjectError::Asset(format!(
                            "{label} must address a specific semantic entry"
                        )));
                    }
                }
            }
            let clouds = &sky.clouds;
            if !clouds.coverage.is_finite()
                || !(0.0..=1.0).contains(&clouds.coverage)
                || !clouds.density.is_finite()
                || !(0.0..=2.0).contains(&clouds.density)
                || !clouds.softness.is_finite()
                || !(0.01..=1.0).contains(&clouds.softness)
                || !clouds.scale.is_finite()
                || !(0.05..=32.0).contains(&clouds.scale)
                || !clouds.detail_scale.is_finite()
                || !(0.1..=64.0).contains(&clouds.detail_scale)
                || clouds
                    .speed
                    .iter()
                    .any(|value| !value.is_finite() || value.abs() > 4.0)
                || !clouds.horizon_fade.is_finite()
                || !(0.0..=1.0).contains(&clouds.horizon_fade)
                || !clouds.macro_scale.is_finite()
                || !(0.02..=8.0).contains(&clouds.macro_scale)
                || !clouds.macro_strength.is_finite()
                || !(0.0..=2.0).contains(&clouds.macro_strength)
                || !clouds.detail_strength.is_finite()
                || !(0.0..=2.0).contains(&clouds.detail_strength)
                || !clouds.micro_strength.is_finite()
                || !(0.0..=2.0).contains(&clouds.micro_strength)
                || !clouds.erosion_strength.is_finite()
                || !(0.0..=2.0).contains(&clouds.erosion_strength)
                || !clouds.warp_strength.is_finite()
                || !(0.0..=1.0).contains(&clouds.warp_strength)
                || !clouds.shape_contrast.is_finite()
                || !(0.25..=4.0).contains(&clouds.shape_contrast)
                || clouds
                    .shear_speed
                    .iter()
                    .any(|value| !value.is_finite() || value.abs() > 4.0)
                || clouds
                    .seed_offset
                    .iter()
                    .any(|value| !value.is_finite() || value.abs() > 4096.0)
                || !clouds.large_speed.is_finite()
                || clouds.large_speed.abs() > 64.0
                || !clouds.small_speed.is_finite()
                || clouds.small_speed.abs() > 64.0
                || !clouds.overall_detail_speed.is_finite()
                || clouds.overall_detail_speed.abs() > 64.0
                || !clouds.edge_detail_speed.is_finite()
                || clouds.edge_detail_speed.abs() > 64.0
                || !clouds.noise_phase_scale.is_finite()
                || !(0.0..=1.0).contains(&clouds.noise_phase_scale)
            {
                return Err(ProjectError::Asset(
                    "environment.sky.clouds contains invalid parameters".to_owned(),
                ));
            }

            let volume = &sky.volumetric_clouds;
            if !volume.base_altitude.is_finite()
                || !volume.top_altitude.is_finite()
                || volume.top_altitude <= volume.base_altitude
                || !volume.max_distance.is_finite()
                || !(100.0..=200_000.0).contains(&volume.max_distance)
                || !volume.resolution_scale.is_finite()
                || !(0.125..=1.0).contains(&volume.resolution_scale)
                || !(8..=128).contains(&volume.ray_steps)
                || !(1..=24).contains(&volume.light_steps)
                || !volume.coverage.is_finite()
                || !(0.0..=1.0).contains(&volume.coverage)
                || !volume.density.is_finite()
                || !(0.0..=8.0).contains(&volume.density)
                || !volume.shape_scale.is_finite()
                || !(1.0e-6..=1.0).contains(&volume.shape_scale)
                || !volume.detail_scale.is_finite()
                || !(1.0e-6..=4.0).contains(&volume.detail_scale)
                || !volume.detail_strength.is_finite()
                || !(0.0..=2.0).contains(&volume.detail_strength)
                || !volume.erosion_strength.is_finite()
                || !(0.0..=2.0).contains(&volume.erosion_strength)
                || !volume.extinction.is_finite()
                || !(1.0e-5..=2.0).contains(&volume.extinction)
                || !volume.scattering.is_finite()
                || !(0.0..=8.0).contains(&volume.scattering)
                || !volume.ambient.is_finite()
                || !(0.0..=4.0).contains(&volume.ambient)
                || !volume.phase_forward.is_finite()
                || !(-0.95..=0.95).contains(&volume.phase_forward)
                || !volume.powder_strength.is_finite()
                || !(0.0..=4.0).contains(&volume.powder_strength)
                || !volume.temporal_blend.is_finite()
                || !(0.0..=0.98).contains(&volume.temporal_blend)
                || !volume.jitter_strength.is_finite()
                || !(0.0..=2.0).contains(&volume.jitter_strength)
            {
                return Err(ProjectError::Asset(
                    "environment.sky.volumetric_clouds contains invalid parameters".to_owned(),
                ));
            }
        }

        let atmospheric = &environment.atmospheric_clouds;
        if !atmospheric.cloud_hat_speed.is_finite()
            || atmospheric.cloud_hat_speed.abs() > 64.0
            || !atmospheric.wind_min_speed.is_finite()
            || atmospheric.wind_min_speed < 0.0
            || !atmospheric.wind_max_speed.is_finite()
            || atmospheric.wind_max_speed <= atmospheric.wind_min_speed
            || !atmospheric.altitude_scroll_scale.is_finite()
            || atmospheric.altitude_scroll_scale.abs() > 16.0
            || !atmospheric.global_alpha.is_finite()
            || !(0.0..=1.0).contains(&atmospheric.global_alpha)
            || !atmospheric.transition_midpoint.is_finite()
            || !(0.0..=1.0).contains(&atmospheric.transition_midpoint)
            || !atmospheric.transition_alpha_range.is_finite()
            || !(0.0..=1.0).contains(&atmospheric.transition_alpha_range)
            || !atmospheric.streaming_budget.is_finite()
            || !(0.001..=16.0).contains(&atmospheric.streaming_budget)
            || !(64..=2048).contains(&atmospheric.soft_depth_resolution)
            || !atmospheric.soft_depth_resolution.is_power_of_two()
            || atmospheric.layers.len() > 256
        {
            return Err(ProjectError::Asset(
                "environment.atmospheric_clouds contains invalid global parameters".to_owned(),
            ));
        }

        let mut atmospheric_ids = BTreeSet::new();
        for (index, layer) in atmospheric.layers.iter().enumerate() {
            let id = layer.id.trim();
            if id.is_empty() || id.len() > 128 || !atmospheric_ids.insert(id.to_owned()) {
                return Err(ProjectError::Asset(format!(
                    "environment.atmospheric_clouds.layers[{index}] has an empty, duplicate, or overlong id"
                )));
            }
            if let Some(model) = layer.model.as_deref() {
                validate_logical_asset_path(
                    "environment.atmospheric_clouds.layers[].model",
                    model,
                )?;
                if !model.contains('@') {
                    return Err(ProjectError::Asset(format!(
                        "environment.atmospheric_clouds.layers[{index}].model must address a semantic entry"
                    )));
                }
            }
            if layer.model_mesh_index > 1024 {
                return Err(ProjectError::Asset(format!(
                    "environment.atmospheric_clouds.layers[{index}].model_mesh_index is out of range"
                )));
            }
            for (label, texture) in [
                ("texture", layer.texture.as_deref()),
                ("density_texture", layer.density_texture.as_deref()),
                ("normal_texture", layer.normal_texture.as_deref()),
                (
                    "detail_density_texture",
                    layer.detail_density_texture.as_deref(),
                ),
                (
                    "detail_normal_texture",
                    layer.detail_normal_texture.as_deref(),
                ),
                (
                    "detail_density2_texture",
                    layer.detail_density2_texture.as_deref(),
                ),
                (
                    "detail_normal2_texture",
                    layer.detail_normal2_texture.as_deref(),
                ),
            ] {
                if let Some(texture) = texture {
                    validate_logical_asset_path(
                        "environment.atmospheric_clouds.layers[].texture",
                        texture,
                    )?;
                    if !texture.contains('@') {
                        return Err(ProjectError::Asset(format!(
                            "environment.atmospheric_clouds.layers[{index}].{label} must address a semantic entry"
                        )));
                    }
                }
            }
            let finite_vecs = layer
                .position
                .iter()
                .chain(layer.rotation_degrees.iter())
                .chain(layer.scale.iter())
                .chain(layer.angular_velocity_degrees.iter())
                .chain(layer.camera_position_scale.iter())
                .chain(layer.color.iter())
                .all(|value| value.is_finite());
            if !finite_vecs
                || layer
                    .scale
                    .iter()
                    .any(|value| value.abs() < 1.0e-4 || value.abs() > 1_000_000.0)
                || layer
                    .camera_position_scale
                    .iter()
                    .any(|value| value.abs() > 4.0)
                || !layer.rotation_scale.is_finite()
                || layer.rotation_scale.abs() > 64.0
                || layer.altitude_min.is_some_and(|value| !value.is_finite())
                || layer.altitude_max.is_some_and(|value| !value.is_finite())
                || layer
                    .altitude_min
                    .zip(layer.altitude_max)
                    .is_some_and(|(min, max)| min >= max)
                || !layer.altitude_min_fade.is_finite()
                || layer.altitude_min_fade < 0.0
                || !layer.altitude_max_fade.is_finite()
                || layer.altitude_max_fade < 0.0
                || !layer.transition_seconds.is_finite()
                || !(0.0..=3600.0).contains(&layer.transition_seconds)
                || !layer.transition_in_time_percent.is_finite()
                || !(0.0..=1.0).contains(&layer.transition_in_time_percent)
                || !layer.transition_out_time_percent.is_finite()
                || !(0.0..=1.0).contains(&layer.transition_out_time_percent)
                || !layer.transition_delay_percent.is_finite()
                || !(0.0..=1.0).contains(&layer.transition_delay_percent)
                || layer
                    .transition_midpoint
                    .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
                || layer
                    .transition_alpha_range
                    .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
                || !layer.cost_factor.is_finite()
                || !(0.0..=16.0).contains(&layer.cost_factor)
                || !layer.soft_intersection_distance.is_finite()
                || !(0.0..=10000.0).contains(&layer.soft_intersection_distance)
                || !layer.density.is_finite()
                || !(0.0..=2.0).contains(&layer.density)
                || !layer.softness.is_finite()
                || !(0.001..=1.0).contains(&layer.softness)
                || !layer.opacity.is_finite()
                || !(0.0..=1.0).contains(&layer.opacity)
                || layer
                    .color
                    .iter()
                    .any(|value| !(0.0..=64.0).contains(value))
                || layer
                    .weather_weights
                    .values()
                    .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
                || layer
                    .density_shift_scale
                    .iter()
                    .chain(layer.scatter.iter())
                    .chain(layer.piercing.iter())
                    .chain(layer.scale_diffuse_fill_ambient.iter())
                    .chain(layer.wrap_lighting.iter())
                    .chain(layer.rescale_uv.iter().flatten())
                    .chain(layer.layer_anim_scale.iter().flatten())
                    .any(|value| !value.is_finite() || value.abs() > 65536.0)
                || layer.uv_layers.iter().any(|uv| {
                    !uv.velocity
                        .iter()
                        .all(|value| value.is_finite() && value.abs() <= 64.0)
                        || !uv.scale.is_finite()
                        || !(0.001..=1024.0).contains(&uv.scale)
                        || !uv.weight.is_finite()
                        || !(0.0..=32.0).contains(&uv.weight)
                })
            {
                return Err(ProjectError::Asset(format!(
                    "environment.atmospheric_clouds.layers[{index}] contains invalid parameters"
                )));
            }
        }
        Ok(environment)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectScripts {
    pub schema: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub provider: String,
    #[serde(default)]
    pub modules: Vec<ProjectScriptModule>,
}

impl ProjectScripts {
    pub fn from_value(value: serde_json::Value) -> Result<Self, ProjectError> {
        let scripts: Self = serde_json::from_value(value).map_err(|error| {
            ProjectError::Asset(format!("scripts config decode failed: {error}"))
        })?;
        if scripts.schema != SCRIPTS_SCHEMA_V1 {
            return Err(ProjectError::Asset(format!(
                "unsupported scripts schema '{}', expected '{}'",
                scripts.schema, SCRIPTS_SCHEMA_V1
            )));
        }
        if scripts.enabled && scripts.provider.trim().is_empty() {
            return Err(ProjectError::Asset(
                "scripts provider must not be empty when scripting is enabled".to_owned(),
            ));
        }
        for module in &scripts.modules {
            validate_logical_asset_path("scripts.modules[].asset", &module.asset)?;
        }
        Ok(scripts)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectScriptModule {
    pub asset: String,
    #[serde(default)]
    pub on_start: Option<String>,
    #[serde(default)]
    pub on_frame: Option<String>,
    #[serde(default)]
    pub on_event: Option<String>,
    #[serde(default)]
    pub on_shutdown: Option<String>,
    #[serde(default)]
    pub permissions: Vec<ProjectScriptPermission>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectScriptPermission {
    pub id: String,
    #[serde(default)]
    pub scope: String,
}

#[derive(Clone, Debug)]
pub struct ResolvedProject {
    pub manifest_path: PathBuf,
    pub root: PathBuf,
    pub manifest: ProjectManifest,
    pub assets_dir: PathBuf,
    pub content_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl ResolvedProject {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ProjectError> {
        let requested = path.as_ref();
        let requested = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(ProjectError::CurrentDirectory)?
                .join(requested)
        };

        let manifest_path = if requested.is_dir() {
            requested.join(PROJECT_MANIFEST_NAME)
        } else {
            requested
        };

        if !manifest_path.is_file() {
            return Err(ProjectError::ManifestMissing(manifest_path));
        }

        let bytes = fs::read(&manifest_path).map_err(|source| ProjectError::Read {
            path: manifest_path.clone(),
            source,
        })?;

        let manifest = serde_json::from_slice::<ProjectManifest>(&bytes).map_err(|source| {
            ProjectError::Parse {
                path: manifest_path.clone(),
                source,
            }
        })?;

        validate_manifest(&manifest)?;

        let root = manifest_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();

        let assets_dir = resolve_project_path(&root, &manifest.paths.assets);
        let content_dir = resolve_project_path(&root, &manifest.paths.content);
        let cache_dir = resolve_project_path(&root, &manifest.paths.cache);

        Ok(Self {
            manifest_path,
            root,
            manifest,
            assets_dir,
            content_dir,
            cache_dir,
        })
    }
}

#[derive(Debug)]
pub enum ProjectError {
    CurrentDirectory(std::io::Error),
    ManifestMissing(PathBuf),
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    Invalid(String),
    Asset(String),
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CurrentDirectory(error) => {
                write!(f, "failed to resolve launch directory: {error}")
            }
            Self::ManifestMissing(path) => {
                write!(f, "project manifest not found: {}", path.display())
            }
            Self::Read { path, source } => {
                write!(
                    f,
                    "failed to read project manifest '{}': {source}",
                    path.display()
                )
            }
            Self::Parse { path, source } => {
                write!(f, "invalid project manifest '{}': {source}", path.display())
            }
            Self::Invalid(message) => write!(f, "invalid project manifest: {message}"),
            Self::Asset(message) => write!(f, "invalid project asset: {message}"),
        }
    }
}

impl std::error::Error for ProjectError {}

fn default_project_version() -> String {
    "0.1.0".to_owned()
}

fn default_capability_version() -> u32 {
    1
}

fn default_clear_color() -> [f32; 4] {
    [0.0, 0.0, 0.0, 1.0]
}

fn default_sky_dome_scale() -> f32 {
    20_000.0
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("newviso-project-{label}-{nonce}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn directory_resolves_project_json_and_paths() {
        let root = temp_dir("load");
        fs::write(
            root.join(PROJECT_MANIFEST_NAME),
            r#"{
              "schema": "newviso.project.v1",
              "project": {"id": "demo", "name": "Demo"},
              "files": {
                "runtime": "config/runtime.json",
                "environment": "environments/default.environment.json",
                "scene": "scenes/main.scene.json"
              }
            }"#,
        )
        .unwrap();

        let project = ResolvedProject::load(&root).unwrap();
        assert_eq!(project.manifest.project.id, "demo");
        assert_eq!(project.assets_dir, root.join("assets"));
        assert_eq!(project.manifest.files.scene, "scenes/main.scene.json");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn manifest_declares_project_owned_script_entrypoint() {
        let root = temp_dir("script-entrypoint");
        fs::write(
            root.join(PROJECT_MANIFEST_NAME),
            r#"{
              "schema": "newviso.project.v1",
              "project": {"id": "scripted-demo", "name": "Scripted Demo"},
              "files": {
                "runtime": "config/runtime.json",
                "environment": "environments/default.environment.json",
                "scene": "scenes/main.scene.json"
              },
              "scripts": {
                "enabled": true,
                "provider": "engine.scripting.typescript",
                "entrypoint": "scripts/main.ysc"
              }
            }"#,
        )
        .unwrap();

        let project = ResolvedProject::load(&root).unwrap();
        let scripts = project
            .manifest
            .scripts
            .as_ref()
            .expect("scripted manifest must declare scripts");
        assert_eq!(scripts.provider, "engine.scripting.typescript");
        assert_eq!(scripts.entrypoint, "scripts/main.ysc");
        assert!(project.manifest.files.scripts.is_none());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn environment_defaults_provide_sky_clouds_and_weather() {
        let value = serde_json::json!({
            "schema": "newviso.environment.v1"
        });
        let environment = ProjectEnvironment::from_value(value).unwrap();
        let sky = environment
            .sky
            .expect("default world must include a sky dome");
        assert_eq!(sky.model, "models/skydome.ydd@skydome_high");
        assert!(sky.clouds.enabled);
        assert!(sky.clouds.coverage > 0.0);
        assert!(sky.clouds.density > 0.0);
        assert!(environment.weather.enabled);
        assert_eq!(environment.weather.current, "CLEAR");
        assert_eq!(environment.weather.next, "CLEAR");
        assert_eq!(environment.weather.blend, 0.0);
    }

    #[test]
    fn environment_defaults_are_explicitly_overridable_or_disableable() {
        let value = serde_json::json!({
            "schema": "newviso.environment.v1",
            "sky": null,
            "weather": {
                "enabled": false,
                "current": "CUSTOM_A",
                "next": "CUSTOM_B",
                "blend": 0.25
            }
        });
        let environment = ProjectEnvironment::from_value(value).unwrap();
        assert!(environment.sky.is_none());
        assert!(!environment.weather.enabled);
        assert_eq!(environment.weather.current, "CUSTOM_A");
        assert_eq!(environment.weather.next, "CUSTOM_B");
        assert_eq!(environment.weather.blend, 0.25);
    }

    #[test]
    fn environment_cloud_profile_is_valid_project_data() {
        let value = serde_json::json!({
            "schema": "newviso.environment.v1",
            "clear_color": [0.0, 0.0, 0.0, 1.0],
            "sky": {
                "model": "models/sky.asset@main",
                "dome_scale": 20000.0,
                "horizon_level": 0.0,
                "clouds": {
                    "enabled": true,
                    "coverage": 0.45,
                    "density": 0.8,
                    "softness": 0.2,
                    "scale": 1.5,
                    "detail_scale": 3.0,
                    "speed": [0.01, 0.0],
                    "horizon_fade": 0.2,
                    "large_speed": 5.0,
                    "small_speed": 1.0,
                    "overall_detail_speed": 1.0,
                    "edge_detail_speed": 1.0,
                    "noise_phase_scale": 0.01
                },
                "volumetric_clouds": {
                    "enabled": true,
                    "base_altitude": 900.0,
                    "top_altitude": 2300.0,
                    "resolution_scale": 0.5,
                    "ray_steps": 48,
                    "light_steps": 6
                }
            }
        });
        let environment = ProjectEnvironment::from_value(value).unwrap();
        let sky = environment
            .sky
            .expect("project environment must preserve sky config");
        assert_eq!(sky.dome_scale, 20_000.0);
        assert_eq!(sky.horizon_level, 0.0);
        assert!(sky.clouds.enabled);
        assert_eq!(sky.clouds.coverage, 0.45);
        assert_eq!(sky.clouds.speed, [0.01, 0.0]);
        assert_eq!(sky.clouds.large_speed, 5.0);
        assert_eq!(sky.clouds.small_speed, 1.0);
        assert_eq!(sky.clouds.overall_detail_speed, 1.0);
        assert_eq!(sky.clouds.edge_detail_speed, 1.0);
        assert_eq!(sky.clouds.noise_phase_scale, 0.01);
        assert!(sky.volumetric_clouds.enabled);
        assert_eq!(sky.volumetric_clouds.base_altitude, 900.0);
        assert_eq!(sky.volumetric_clouds.top_altitude, 2300.0);
        assert_eq!(sky.volumetric_clouds.ray_steps, 48);
    }

    #[test]
    fn atmospheric_cloud_layers_preserve_authored_animation_contract() {
        let value: serde_json::Value = serde_json::from_str(
            r#"{
              "schema": "newviso.environment.v1",
              "atmospheric_clouds": {
                "enabled": true,
                "cloud_hat_speed": 1.0,
                "wind_min_speed": 0.0,
                "wind_max_speed": 5.0,
                "altitude_scroll_scale": 0.001,
                "global_alpha": 0.8,
                "transition_midpoint": 0.35,
                "transition_alpha_range": 0.2,
                "streaming_budget": 1.0,
                "soft_depth_resolution": 512,
                "layers": [{
                  "id": "storm_front",
                  "texture": "textures/clouds.asset@noise",
                  "position": [0.0, 200.0, 0.0],
                  "scale": [300.0, 40.0, 300.0],
                  "angular_velocity_degrees": [0.0, 0.05, 0.0],
                  "camera_position_scale": [1.0, 0.0, 1.0],
                  "altitude_min": 20.0,
                  "altitude_min_fade": 10.0,
                  "altitude_max": 500.0,
                  "altitude_max_fade": 50.0,
                  "transition_seconds": 8.0,
                  "transition_in_time_percent": 0.75,
                  "transition_out_time_percent": 0.6,
                  "transition_delay_percent": 0.2,
                  "transition_midpoint": 0.42,
                  "transition_alpha_range": 0.31,
                  "cost_factor": 0.45,
                  "soft_intersection_distance": 16.0,
                  "density": 0.7,
                  "softness": 0.15,
                  "opacity": 0.6,
                  "weather_weights": {"clear": 0.2, "rain": 1.0},
                  "uv_layers": [
                    {"enabled": true, "mode": "combine", "velocity": [0.01, 0.0], "scale": 1.0, "weight": 1.0},
                    {"enabled": true, "mode": "combine", "velocity": [-0.005, 0.002], "scale": 2.0, "weight": 18.424999},
                    {"enabled": true, "mode": "sculpt", "velocity": [0.002, -0.003], "scale": 4.0, "weight": 0.3}
                  ]
                }]
              }
            }"#,
        )
        .unwrap();
        let environment = ProjectEnvironment::from_value(value).unwrap();
        let clouds = environment.atmospheric_clouds;
        assert!(clouds.enabled);
        assert_eq!(clouds.layers.len(), 1);
        assert_eq!(clouds.layers[0].id, "storm_front");
        assert_eq!(
            clouds.layers[0].uv_layers[2].mode,
            ProjectAtmosphericCloudAnimMode::Sculpt
        );
        assert!((clouds.layers[0].uv_layers[1].weight - 18.424999).abs() < 1.0e-6);
        assert_eq!(clouds.transition_midpoint, 0.35);
        assert_eq!(clouds.transition_alpha_range, 0.2);
        assert_eq!(clouds.streaming_budget, 1.0);
        assert_eq!(clouds.soft_depth_resolution, 512);
        assert_eq!(clouds.layers[0].transition_in_time_percent, 0.75);
        assert_eq!(clouds.layers[0].transition_out_time_percent, 0.6);
        assert_eq!(clouds.layers[0].transition_delay_percent, 0.2);
        assert_eq!(clouds.layers[0].transition_midpoint, Some(0.42));
        assert_eq!(clouds.layers[0].transition_alpha_range, Some(0.31));
        assert_eq!(clouds.layers[0].cost_factor, 0.45);
        assert_eq!(clouds.layers[0].soft_intersection_distance, 16.0);
        assert_eq!(clouds.layers[0].weather_weights["rain"], 1.0);
    }

    #[test]
    fn local_firstfps_environment_fixture_is_valid_when_present() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../Projects/FirstFPS/environments/yard.environment.json");
        if !path.is_file() {
            eprintln!(
                "skipping local FirstFPS environment fixture: {}",
                path.display()
            );
            return;
        }
        let bytes = std::fs::read(&path).expect("read FirstFPS environment fixture");
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).expect("parse FirstFPS environment JSON");
        let environment =
            ProjectEnvironment::from_value(value).expect("validate FirstFPS environment");
        assert!(environment.atmospheric_clouds.enabled);
        assert_eq!(environment.atmospheric_clouds.layers.len(), 114);
    }

    #[test]
    fn rejects_escaping_project_asset_reference() {
        let root = temp_dir("escape");
        fs::write(
            root.join(PROJECT_MANIFEST_NAME),
            r#"{
              "schema": "newviso.project.v1",
              "project": {"id": "demo", "name": "Demo"},
              "files": {
                "runtime": "../runtime.json",
                "environment": "environment.json",
                "scene": "scene.json"
              }
            }"#,
        )
        .unwrap();

        assert!(ResolvedProject::load(&root).is_err());
        let _ = fs::remove_dir_all(root);
    }
}
