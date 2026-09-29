use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RenderPolicy {
    pub runtime_cube_capacity: usize,
    pub transient_sphere_capacity: usize,
    pub overlay_quad_capacity: usize,
    pub particle_capacity: usize,
    pub lens_flare_capacity: usize,
    pub flare_element_capacity: usize,
    pub shadow_resolution: u32,
}

impl Default for RenderPolicy {
    fn default() -> Self {
        // Deliberately unconfigured. Engine/project policy is injected before
        // renderer initialization from Shared Assets and project overrides.
        Self {
            runtime_cube_capacity: 0,
            transient_sphere_capacity: 0,
            overlay_quad_capacity: 0,
            particle_capacity: 0,
            lens_flare_capacity: 0,
            flare_element_capacity: 0,
            shadow_resolution: 0,
        }
    }
}

impl RenderPolicy {
    fn validate(&self, cubes: usize) -> Result<(), String> {
        let invalid = || "render capacity exceeds the vertex-count contract".to_owned();
        let vertices = cubes
            .checked_add(self.runtime_cube_capacity)
            .and_then(|v| v.checked_mul(CUBE_VERTEX_COUNT as usize))
            .ok_or_else(invalid)?;
        let spheres = self
            .transient_sphere_capacity
            .checked_mul(geometry::SPHERE_VERTEX_COUNT as usize)
            .ok_or_else(invalid)?;
        let overlays = self
            .overlay_quad_capacity
            .checked_mul(6)
            .ok_or_else(invalid)?;
        let particles = self.particle_capacity.checked_mul(6).ok_or_else(invalid)?;
        let total = vertices
            .checked_add(spheres)
            .and_then(|v| v.checked_add(overlays))
            .and_then(|v| v.checked_add(particles))
            .ok_or_else(invalid)?;
        let flares = self
            .lens_flare_capacity
            .checked_mul(self.flare_element_capacity)
            .and_then(|v| v.checked_mul(6))
            .ok_or_else(invalid)?;
        if total == 0 || total > u32::MAX as usize || flares > u32::MAX as usize {
            return Err(invalid());
        }
        if self.shadow_resolution == 0 {
            return Err("shadow_resolution must be greater than zero".into());
        }
        Ok(())
    }

    fn patched(&self, patch: &Value, cubes: usize) -> Result<Self, String> {
        let patch = patch
            .as_object()
            .ok_or("scene.render.configure settings must be an object")?;
        let mut value = serde_json::to_value(self).map_err(|e| e.to_string())?;
        let fields = value.as_object_mut().expect("render settings object");
        for (key, entry) in patch {
            fields.insert(key.clone(), entry.clone());
        }
        let next: Self =
            serde_json::from_value(value).map_err(|e| format!("invalid render settings: {e}"))?;
        next.validate(cubes)?;
        Ok(next)
    }
}

impl Scene3dRuntime {
    /// GPU allocations are fixed for a renderer lifetime. Configure in startup
    /// commands or on_start, before the initial allocations are created.
    pub fn configure_render_policy(&mut self, patch: &Value) -> Result<(), String> {
        if self.gpu.is_some() || self.gpu_sky.is_some() {
            return Err("scene.render.configure requires startup_commands or on_start; GPU resources already exist".into());
        }
        let next = self.render_policy.patched(patch, self.cubes.len())?;
        if self.transient_spheres.len() > next.transient_sphere_capacity
            || self.overlay_quads.len() > next.overlay_quad_capacity
            || self.particles.len() > next.particle_capacity
            || self.lens_flares.len() > next.lens_flare_capacity
            || self
                .lens_flares
                .values()
                .any(|flare| flare.elements.len() > next.flare_element_capacity)
        {
            return Err("render capacity is smaller than existing scene content".into());
        }
        self.render_policy = next;
        Ok(())
    }

    pub(super) fn validate_render_policy(&self) -> Result<(), String> {
        self.render_policy.validate(self.cubes.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn render_patch_validates_limits_and_keeps_other_settings() {
        let policy = RenderPolicy {
            runtime_cube_capacity: 4096,
            transient_sphere_capacity: 256,
            overlay_quad_capacity: 256,
            particle_capacity: 4096,
            lens_flare_capacity: 16,
            flare_element_capacity: 16,
            shadow_resolution: 2048,
        };
        let next = policy
            .patched(
                &json!({"shadow_resolution": 1024, "runtime_cube_capacity": 8192}),
                5,
            )
            .unwrap();
        assert_eq!(next.shadow_resolution, 1024);
        assert_eq!(next.runtime_cube_capacity, 8192);
        assert_eq!(next.overlay_quad_capacity, policy.overlay_quad_capacity);
        for patch in [
            json!({"shadow_resolution": 0}),
            json!({"unknown": 4}),
            json!({"runtime_cube_capacity": u64::MAX}),
            json!({"flare_element_capacity": u64::MAX}),
        ] {
            assert!(policy.patched(&patch, 5).is_err());
        }
    }
}
