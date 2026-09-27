use crate::math::Vec3;

#[derive(Clone, Debug)]
pub(crate) struct Camera {
    pub(crate) position: Vec3,
    pub(crate) target: Vec3,
    pub(crate) up: Vec3,
    pub(crate) fov_y_degrees: f32,
    pub(crate) near: f32,
    pub(crate) far: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct OrbitCamera {
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
    pub(crate) distance: f32,
    pub(crate) rotate_sensitivity: f32,
    pub(crate) zoom_sensitivity: f32,
    pub(crate) min_distance: f32,
    pub(crate) max_distance: f32,
    pub(crate) min_pitch_degrees: f32,
    pub(crate) max_pitch_degrees: f32,
    pub(crate) rotate_button: u64,
}

#[derive(serde::Deserialize)]
struct OrbitDefaults {
    rotate_sensitivity: f32,
    zoom_sensitivity: f32,
    min_distance: f32,
    max_distance: f32,
    min_pitch_degrees: f32,
    max_pitch_degrees: f32,
    rotate_button: u64,
}

impl OrbitCamera {
    pub(crate) fn from_camera(camera: &Camera) -> Self {
        let offset = camera.position.sub(camera.target);
        let distance = offset.length().max(0.001);
        let yaw = offset.x.atan2(offset.z);
        let pitch = (offset.y / distance).clamp(-1.0, 1.0).asin();
        let defaults: OrbitDefaults =
            serde_json::from_str(include_str!("assets/orbit_defaults.json"))
                .expect("packaged orbit defaults must be valid");
        Self {
            yaw,
            pitch,
            distance,
            rotate_sensitivity: defaults.rotate_sensitivity,
            zoom_sensitivity: defaults.zoom_sensitivity,
            min_distance: defaults.min_distance,
            max_distance: defaults.max_distance,
            min_pitch_degrees: defaults.min_pitch_degrees,
            max_pitch_degrees: defaults.max_pitch_degrees,
            rotate_button: defaults.rotate_button,
        }
    }

    pub(crate) fn sync_pose(&mut self, camera: &Camera) {
        let offset = camera.position.sub(camera.target);
        self.distance = offset.length().max(0.001);
        self.yaw = offset.x.atan2(offset.z);
        self.pitch = (offset.y / self.distance).clamp(-1.0, 1.0).asin();
    }

    pub(crate) fn apply_mouse(&mut self, dx: f32, dy: f32, wheel_y: f32, rotating: bool) {
        if rotating {
            self.yaw -= dx * self.rotate_sensitivity;
            self.pitch = (self.pitch - dy * self.rotate_sensitivity).clamp(
                self.min_pitch_degrees.to_radians(),
                self.max_pitch_degrees.to_radians(),
            );
        }

        if wheel_y != 0.0 {
            self.distance *= (-wheel_y * self.zoom_sensitivity).exp();
            self.distance = self.distance.clamp(self.min_distance, self.max_distance);
        }
    }

    pub(crate) fn position(self, target: Vec3) -> Vec3 {
        let cos_pitch = self.pitch.cos();
        Vec3::new(
            target.x + self.distance * cos_pitch * self.yaw.sin(),
            target.y + self.distance * self.pitch.sin(),
            target.z + self.distance * cos_pitch * self.yaw.cos(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pose_updates_preserve_configured_navigation_policy() {
        let mut camera = Camera {
            position: Vec3::new(0.0, 0.0, 5.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov_y_degrees: 60.0,
            near: 0.1,
            far: 100.0,
        };
        let mut orbit = OrbitCamera::from_camera(&camera);
        orbit.rotate_sensitivity = 0.02;
        orbit.max_distance = 250.0;
        orbit.rotate_button = 3;
        orbit.min_pitch_degrees = -30.0;
        orbit.max_pitch_degrees = 40.0;
        camera.position = Vec3::new(5.0, 0.0, 0.0);
        orbit.sync_pose(&camera);
        assert_eq!(orbit.rotate_sensitivity, 0.02);
        assert_eq!(orbit.max_distance, 250.0);
        assert_eq!(orbit.rotate_button, 3);
        assert!((orbit.yaw - std::f32::consts::FRAC_PI_2).abs() < 1.0e-6);
        orbit.apply_mouse(0.0, 10000.0, 0.0, true);
        assert_eq!(orbit.pitch, (-30.0_f32).to_radians());
    }
}
