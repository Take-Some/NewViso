use serde::{Deserialize, Serialize};

pub type Vec3 = [f32; 3];

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct NavBuildConfig {
    pub max_slope_degrees: f32,
    pub weld_epsilon: f32,
    pub max_snap_distance: f32,
}

impl Default for NavBuildConfig {
    fn default() -> Self {
        Self {
            max_slope_degrees: 50.0,
            weld_epsilon: 0.02,
            max_snap_distance: 4.0,
        }
    }
}

impl NavBuildConfig {
    pub fn validate(self) -> Result<(), String> {
        if !self.max_slope_degrees.is_finite()
            || !(0.0..89.0).contains(&self.max_slope_degrees)
            || !self.weld_epsilon.is_finite()
            || !(0.0001..=2.0).contains(&self.weld_epsilon)
            || !self.max_snap_distance.is_finite()
            || !(0.01..=1000.0).contains(&self.max_snap_distance)
        {
            return Err("invalid navmesh build config".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NavTileSource {
    pub id: String,
    pub vertices: Vec<Vec3>,
    pub triangles: Vec<[u32; 3]>,
}

impl NavTileSource {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() || self.id.len() > 128 {
            return Err("navmesh tile id must be non-empty and <= 128 bytes".to_owned());
        }
        if self.vertices.is_empty() || self.triangles.is_empty() {
            return Err(format!("navmesh tile '{}' is empty", self.id));
        }
        if self.vertices.iter().flatten().any(|v| !v.is_finite()) {
            return Err(format!(
                "navmesh tile '{}' contains non-finite vertices",
                self.id
            ));
        }
        let count = self.vertices.len() as u64;
        if self
            .triangles
            .iter()
            .flatten()
            .any(|index| u64::from(*index) >= count)
        {
            return Err(format!(
                "navmesh tile '{}' has out-of-range triangle index",
                self.id
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OffMeshLink {
    pub id: String,
    pub start: Vec3,
    pub end: Vec3,
    #[serde(default = "default_true")]
    pub bidirectional: bool,
    #[serde(default = "default_cost")]
    pub cost_scale: f32,
    #[serde(default)]
    pub kind: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}
fn default_cost() -> f32 {
    1.0
}

impl OffMeshLink {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty()
            || self.id.len() > 128
            || self
                .start
                .iter()
                .chain(self.end.iter())
                .any(|v| !v.is_finite())
            || !self.cost_scale.is_finite()
            || !(0.001..=1000.0).contains(&self.cost_scale)
        {
            return Err("invalid off-mesh link".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DynamicObstacle {
    pub id: String,
    pub center: Vec3,
    pub radius: f32,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl DynamicObstacle {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty()
            || self.id.len() > 128
            || self.center.iter().any(|v| !v.is_finite())
            || !self.radius.is_finite()
            || !(0.01..=1000.0).contains(&self.radius)
        {
            return Err("invalid navigation obstacle".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PathRequest {
    pub id: u64,
    pub start: Vec3,
    pub end: Vec3,
    pub agent_radius: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PathStatus {
    Found,
    NoStartPolygon,
    NoEndPolygon,
    NoPath,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PathResult {
    pub id: u64,
    pub status: PathStatus,
    pub corridor: Vec<u64>,
    pub waypoints: Vec<Vec3>,
    pub visited_polygons: usize,
}
