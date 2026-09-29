use serde::{Deserialize, Serialize};

mod asset;
pub use asset::*;

/// Canonical renderer-owned hair/groom gateway.
pub const ENGINE_RENDER_HAIR_GATEWAY_ID: &str = "engine.render.hair";
pub const ENGINE_RENDER_HAIR_ASSETS_GATEWAY_ID: &str = "engine.render.hair.assets";
pub const ENGINE_RENDER_HAIR_SIMULATION_GATEWAY_ID: &str = "engine.render.hair.simulation";
pub const ENGINE_RENDER_HAIR_STRANDS_GATEWAY_ID: &str = "engine.render.hair.strands";
pub const ENGINE_RENDER_HAIR_SHADOWS_GATEWAY_ID: &str = "engine.render.hair.shadows";
pub const ENGINE_RENDER_HAIR_DEBUG_GATEWAY_ID: &str = "engine.render.hair.debug";
pub const HAIR_RUNTIME_CONTRACT_V1: &str = "newengine.render.hair/v1";

pub const HAIR_STRANDS_CAPABILITY_ID: &str = "render.hair.strands";
pub const HAIR_GPU_SIMULATION_CAPABILITY_ID: &str = "render.hair.gpu_simulation";
pub const HAIR_SKINNING_CAPABILITY_ID: &str = "render.hair.skinning";
pub const HAIR_CAPSULE_COLLISION_CAPABILITY_ID: &str = "render.hair.collision.capsules";
pub const HAIR_SDF_COLLISION_CAPABILITY_ID: &str = "render.hair.collision.sdf";
pub const HAIR_SHADOWS_CAPABILITY_ID: &str = "render.hair.shadows";
pub const HAIR_LOD_CAPABILITY_ID: &str = "render.hair.lod";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct HairGroomRef(pub String);

impl HairGroomRef {
    #[inline]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn normalized(mut self) -> Result<Self, String> {
        self.0 = self.0.trim().replace('\\', "/");
        if self.0.is_empty() || self.0.len() > 512 {
            return Err("hair groom reference must contain 1..=512 bytes".to_owned());
        }
        if self.0.starts_with('/') || self.0.contains(":/") || self.0.contains("../") {
            return Err(
                "hair groom reference must be a project/VFS-relative logical asset id".to_owned(),
            );
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HairQualityTier {
    Off,
    Low,
    #[default]
    Medium,
    High,
    Ultra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HairSimulationMode {
    Disabled,
    #[default]
    GuideStrands,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HairCollisionMode {
    None,
    #[default]
    Capsules,
    Sdf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HairTransparencyMode {
    AlphaBlend,
    #[default]
    AlphaToCoverage,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HairLodSettingsV1 {
    /// Distance at which strand density starts to reduce.
    pub density_start_distance: f32,
    /// Distance at which only the minimum strand density is retained.
    pub density_end_distance: f32,
    /// Minimum fraction of rendered strands at maximum distance.
    pub minimum_density: f32,
    /// Maximum distance at which simulation remains active.
    pub simulation_distance: f32,
}

impl Default for HairLodSettingsV1 {
    fn default() -> Self {
        Self {
            density_start_distance: 8.0,
            density_end_distance: 35.0,
            minimum_density: 0.2,
            simulation_distance: 25.0,
        }
    }
}

impl HairLodSettingsV1 {
    #[inline]
    pub fn sanitized(self) -> Self {
        let start = finite_or(self.density_start_distance, 8.0).clamp(0.0, 100_000.0);
        let end = finite_or(self.density_end_distance, 35.0).clamp(start.max(0.001), 100_000.0);
        Self {
            density_start_distance: start,
            density_end_distance: end,
            minimum_density: finite_or(self.minimum_density, 0.2).clamp(0.0, 1.0),
            simulation_distance: finite_or(self.simulation_distance, 25.0).clamp(0.0, 100_000.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HairSimulationSettingsV1 {
    pub mode: HairSimulationMode,
    pub collision: HairCollisionMode,
    pub gravity_scale: f32,
    pub damping: f32,
    pub stretch_stiffness: f32,
    pub bend_stiffness: f32,
    pub root_stiffness: f32,
    pub wind_response: f32,
    pub solver_iterations: u8,
    pub max_delta_seconds: f32,
}

impl Default for HairSimulationSettingsV1 {
    fn default() -> Self {
        Self {
            mode: HairSimulationMode::GuideStrands,
            collision: HairCollisionMode::Capsules,
            gravity_scale: 1.0,
            damping: 0.08,
            stretch_stiffness: 0.95,
            bend_stiffness: 0.55,
            root_stiffness: 0.9,
            wind_response: 1.0,
            solver_iterations: 4,
            max_delta_seconds: 1.0 / 30.0,
        }
    }
}

impl HairSimulationSettingsV1 {
    #[inline]
    pub fn sanitized(self) -> Self {
        Self {
            mode: self.mode,
            collision: self.collision,
            gravity_scale: finite_or(self.gravity_scale, 1.0).clamp(-4.0, 4.0),
            damping: finite_or(self.damping, 0.08).clamp(0.0, 1.0),
            stretch_stiffness: finite_or(self.stretch_stiffness, 0.95).clamp(0.0, 1.0),
            bend_stiffness: finite_or(self.bend_stiffness, 0.55).clamp(0.0, 1.0),
            root_stiffness: finite_or(self.root_stiffness, 0.9).clamp(0.0, 1.0),
            wind_response: finite_or(self.wind_response, 1.0).clamp(0.0, 8.0),
            solver_iterations: self.solver_iterations.clamp(1, 16),
            max_delta_seconds: finite_or(self.max_delta_seconds, 1.0 / 30.0)
                .clamp(1.0 / 240.0, 0.1),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HairMaterialSettingsV1 {
    pub base_color: [f32; 3],
    pub roughness: f32,
    pub secondary_specular: f32,
    pub melanin: f32,
    pub redness: f32,
    pub opacity: f32,
    pub strand_width_mm: f32,
    pub tip_scale: f32,
    pub transparency: HairTransparencyMode,
}

impl Default for HairMaterialSettingsV1 {
    fn default() -> Self {
        Self {
            base_color: [0.18, 0.08, 0.035],
            roughness: 0.35,
            secondary_specular: 0.45,
            melanin: 0.55,
            redness: 0.15,
            opacity: 1.0,
            strand_width_mm: 0.06,
            tip_scale: 0.25,
            transparency: HairTransparencyMode::AlphaToCoverage,
        }
    }
}

impl HairMaterialSettingsV1 {
    #[inline]
    pub fn sanitized(self) -> Self {
        Self {
            base_color: self.base_color.map(|v| finite_or(v, 0.0).clamp(0.0, 64.0)),
            roughness: finite_or(self.roughness, 0.35).clamp(0.02, 1.0),
            secondary_specular: finite_or(self.secondary_specular, 0.45).clamp(0.0, 4.0),
            melanin: finite_or(self.melanin, 0.55).clamp(0.0, 1.0),
            redness: finite_or(self.redness, 0.15).clamp(0.0, 1.0),
            opacity: finite_or(self.opacity, 1.0).clamp(0.0, 1.0),
            strand_width_mm: finite_or(self.strand_width_mm, 0.06).clamp(0.005, 4.0),
            tip_scale: finite_or(self.tip_scale, 0.25).clamp(0.0, 1.0),
            transparency: self.transparency,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HairInstanceDescV1 {
    pub instance_id: u64,
    pub groom: HairGroomRef,
    pub quality: HairQualityTier,
    pub root_transform: [f32; 16],
    /// Opaque provider-neutral pose id. Zero/None means rigid groom root only.
    #[serde(default)]
    pub skin_pose_id: Option<u64>,
    pub wind_velocity: [f32; 3],
    pub simulation: HairSimulationSettingsV1,
    pub material: HairMaterialSettingsV1,
    pub lod: HairLodSettingsV1,
    pub casts_shadows: bool,
    pub receives_shadows: bool,
}

impl Default for HairInstanceDescV1 {
    fn default() -> Self {
        Self {
            instance_id: 0,
            groom: HairGroomRef::new("hair/default.groom"),
            quality: HairQualityTier::Medium,
            root_transform: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            skin_pose_id: None,
            wind_velocity: [0.0; 3],
            simulation: HairSimulationSettingsV1::default(),
            material: HairMaterialSettingsV1::default(),
            lod: HairLodSettingsV1::default(),
            casts_shadows: true,
            receives_shadows: true,
        }
    }
}

impl HairInstanceDescV1 {
    pub fn normalized(mut self) -> Result<Self, String> {
        if self.instance_id == 0 {
            return Err("hair instance_id must be non-zero".to_owned());
        }
        self.groom = self.groom.normalized()?;
        if !self.root_transform.iter().all(|v| v.is_finite()) {
            return Err("hair root transform contains non-finite data".to_owned());
        }
        if self.skin_pose_id == Some(0) {
            return Err("hair skin_pose_id must be non-zero when present".to_owned());
        }
        if !self.wind_velocity.iter().all(|v| v.is_finite()) {
            self.wind_velocity = [0.0; 3];
        }
        self.simulation = self.simulation.sanitized();
        self.material = self.material.sanitized();
        self.lod = self.lod.sanitized();
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct HairRuntimeStatsV1 {
    pub active_instances: u32,
    pub resident_grooms: u32,
    pub simulated_guides: u32,
    pub rendered_strands: u32,
    pub collision_primitives: u32,
    pub lod_culled_instances: u32,
    pub capacity_drops: u64,
}

#[inline]
fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_gateway_stays_under_engine_render() {
        assert_eq!(ENGINE_RENDER_HAIR_GATEWAY_ID, "engine.render.hair");
        assert!(ENGINE_RENDER_HAIR_SIMULATION_GATEWAY_ID.starts_with("engine.render.hair."));
        assert!(ENGINE_RENDER_HAIR_STRANDS_GATEWAY_ID.starts_with("engine.render.hair."));
    }

    #[test]
    fn groom_refs_are_logical_and_never_absolute_paths() {
        assert_eq!(
            HairGroomRef::new(" characters\\abby\\hair.groom ")
                .normalized()
                .unwrap()
                .as_str(),
            "characters/abby/hair.groom"
        );
        assert!(HairGroomRef::new("C:/project/hair.groom")
            .normalized()
            .is_err());
        assert!(HairGroomRef::new("../outside/hair.groom")
            .normalized()
            .is_err());
    }

    #[test]
    fn simulation_settings_are_bounded_for_gpu_runtime() {
        let settings = HairSimulationSettingsV1 {
            damping: f32::NAN,
            solver_iterations: u8::MAX,
            max_delta_seconds: 5.0,
            ..Default::default()
        }
        .sanitized();
        assert_eq!(settings.damping, 0.08);
        assert_eq!(settings.solver_iterations, 16);
        assert_eq!(settings.max_delta_seconds, 0.1);
    }

    #[test]
    fn instance_rejects_non_finite_root_transform() {
        let mut instance = HairInstanceDescV1 {
            instance_id: 7,
            ..Default::default()
        };
        instance.root_transform[5] = f32::NAN;
        assert!(instance.normalized().is_err());
    }
}
