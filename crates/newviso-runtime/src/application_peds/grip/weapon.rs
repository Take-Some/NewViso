//! Physical weapon mass and inertia from authored item metadata.
use super::*;
#[derive(Clone, Debug, serde::Deserialize)]
pub(super) struct WeaponPhysics {
    pub(super) mass: f32,
    pub(super) bounds_min: [f32; 3],
    pub(super) bounds_max: [f32; 3],
    center_of_mass: Option<[f32; 3]>,
    inertia_diagonal: Option<[f32; 3]>,
}
impl WeaponPhysics {
    pub(super) fn read(definition: &ItemDefinition) -> Result<Self, String> {
        let physical: Self = serde_json::from_value(
            definition
                .metadata
                .get("physics")
                .cloned()
                .ok_or_else(|| format!("weapon '{}' has no physical data", definition.id))?,
        )
        .map_err(|e| format!("invalid weapon physics '{}': {e}", definition.id))?;
        if !physical.mass.is_finite()
            || physical.mass <= 0.0
            || physical
                .bounds_min
                .iter()
                .chain(&physical.bounds_max)
                .chain(physical.center_of_mass.iter().flatten())
                .chain(physical.inertia_diagonal.iter().flatten())
                .any(|v| !v.is_finite())
            || physical
                .inertia_diagonal
                .is_some_and(|i| i.iter().any(|v| *v <= 0.0))
            || (0..3).any(|i| physical.bounds_min[i] >= physical.bounds_max[i])
        {
            return Err(format!("invalid weapon mass/bounds '{}'", definition.id));
        }
        Ok(physical)
    }
    pub(super) fn center(&self) -> [f32; 3] {
        self.center_of_mass.unwrap_or_else(|| {
            std::array::from_fn(|i| (self.bounds_min[i] + self.bounds_max[i]) * 0.5)
        })
    }
    pub(super) fn inertia(&self) -> [f32; 3] {
        if let Some(inertia) = self.inertia_diagonal {
            return inertia;
        }
        let length: [f32; 3] = std::array::from_fn(|i| self.bounds_max[i] - self.bounds_min[i]);
        std::array::from_fn(|i| {
            self.mass / 12.0 * (length[(i + 1) % 3].powi(2) + length[(i + 2) % 3].powi(2))
        })
    }
}
