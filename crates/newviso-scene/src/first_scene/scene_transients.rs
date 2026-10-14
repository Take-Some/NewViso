//! transients.
use super::*;

impl Scene3dRuntime {
    pub fn set_transient_spheres(
        &mut self,
        spheres: Vec<SceneTransientSphere>,
    ) -> Result<(), String> {
        if spheres.len() > self.render_policy.transient_sphere_capacity {
            return Err(format!(
                "scene.transient_spheres.set exceeds the configured limit of {}",
                self.render_policy.transient_sphere_capacity
            ));
        }
        for sphere in &spheres {
            if sphere.position.iter().any(|value| !value.is_finite())
                || sphere
                    .rotation_degrees
                    .iter()
                    .any(|value| !value.is_finite())
                || !sphere.radius.is_finite()
                || sphere.radius <= 0.0
                || sphere.color.iter().any(|value| !value.is_finite())
                || sphere
                    .marker_color
                    .is_some_and(|color| color.iter().any(|value| !value.is_finite()))
                || (sphere.marker_color.is_some()
                    && (sphere
                        .marker_direction
                        .iter()
                        .any(|value| !value.is_finite())
                        || sphere
                            .marker_direction
                            .iter()
                            .map(|value| value * value)
                            .sum::<f32>()
                            <= 1.0e-8
                        || !sphere.marker_threshold.is_finite()
                        || !(-1.0..=1.0).contains(&sphere.marker_threshold)))
            {
                return Err("scene.transient_spheres.set contains invalid sphere data".to_owned());
            }
        }
        self.transient_spheres = spheres;
        Ok(())
    }

    pub fn add_surface_mark(&mut self, mut mark: SceneSurfaceMark) -> Result<(), String> {
        let length_sq = mark.normal.iter().map(|value| value * value).sum::<f32>();
        if mark.position.iter().any(|value| !value.is_finite())
            || mark.normal.iter().any(|value| !value.is_finite())
            || !length_sq.is_finite()
            || length_sq <= 1.0e-8
            || !mark.radius.is_finite()
            || !(0.001..=0.5).contains(&mark.radius)
            || mark.color.iter().any(|value| !value.is_finite())
        {
            return Err("scene.surface_mark.add contains invalid mark data".to_owned());
        }
        let inv_length = length_sq.sqrt().recip();
        mark.normal = mark.normal.map(|value| value * inv_length);
        let capacity = self.render_policy.overlay_quad_capacity;
        if capacity == 0 {
            return Ok(());
        }
        if self.surface_marks.len() >= capacity {
            let overflow = self.surface_marks.len() + 1 - capacity;
            self.surface_marks.drain(0..overflow);
        }
        self.surface_marks.push(mark);
        Ok(())
    }

    pub fn clear_surface_marks(&mut self) {
        self.surface_marks.clear();
    }

    pub fn set_overlay_quads(&mut self, quads: Vec<SceneOverlayQuad>) -> Result<(), String> {
        if quads.len() > self.render_policy.overlay_quad_capacity {
            return Err(format!(
                "scene.overlay_quads.set exceeds the configured limit of {}",
                self.render_policy.overlay_quad_capacity
            ));
        }
        for quad in &quads {
            if quad.rect.iter().any(|value| !value.is_finite())
                || quad.color.iter().any(|value| !value.is_finite())
            {
                return Err("scene.overlay_quads.set contains non-finite data".to_owned());
            }
        }
        self.overlay_quads = quads;
        Ok(())
    }
}
