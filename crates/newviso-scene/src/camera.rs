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

pub(crate) fn previous_hiz_camera_compatible(previous: &Camera, current: &Camera) -> bool {
    const POSITION_EPSILON: f32 = 1.0e-4;
    const BASIS_EPSILON: f32 = 1.0e-6;
    const PROJECTION_EPSILON: f32 = 1.0e-6;

    let previous_forward = previous.target.sub(previous.position).normalized();
    let current_forward = current.target.sub(current.position).normalized();
    let previous_up = previous.up.normalized();
    let current_up = current.up.normalized();

    max_abs_vec3_delta(previous.position, current.position) <= POSITION_EPSILON
        && max_abs_vec3_delta(previous_forward, current_forward) <= BASIS_EPSILON
        && max_abs_vec3_delta(previous_up, current_up) <= BASIS_EPSILON
        && (previous.fov_y_degrees - current.fov_y_degrees).abs() <= PROJECTION_EPSILON
        && (previous.near - current.near).abs() <= PROJECTION_EPSILON
        && (previous.far - current.far).abs() <= PROJECTION_EPSILON
}

fn max_abs_vec3_delta(a: Vec3, b: Vec3) -> f32 {
    (a.x - b.x)
        .abs()
        .max((a.y - b.y).abs())
        .max((a.z - b.z).abs())
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
    pub(crate) configured: bool,
}

impl OrbitCamera {
    pub(crate) fn from_camera(camera: &Camera) -> Self {
        let offset = camera.position.sub(camera.target);
        let distance = offset.length().max(0.001);
        let yaw = offset.x.atan2(offset.z);
        let pitch = (offset.y / distance).clamp(-1.0, 1.0).asin();
        Self {
            yaw,
            pitch,
            distance,
            // Navigation policy is intentionally absent here. NewViso runtime
            // injects it from Shared Assets before the platform becomes live.
            rotate_sensitivity: 0.0,
            zoom_sensitivity: 0.0,
            min_distance: 0.0,
            max_distance: 0.0,
            min_pitch_degrees: 0.0,
            max_pitch_degrees: 0.0,
            rotate_button: u64::MAX,
            configured: false,
        }
    }

    pub(crate) fn sync_pose(&mut self, camera: &Camera) {
        let offset = camera.position.sub(camera.target);
        self.distance = offset.length().max(0.001);
        self.yaw = offset.x.atan2(offset.z);
        self.pitch = (offset.y / self.distance).clamp(-1.0, 1.0).asin();
    }

    pub(crate) fn apply_mouse(&mut self, dx: f32, dy: f32, wheel_y: f32, rotating: bool) {
        if !self.configured {
            return;
        }
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
    fn previous_hiz_requires_effectively_stationary_camera() {
        let base = Camera {
            position: Vec3::ZERO,
            target: Vec3::new(0.0, 0.0, -1.0),
            up: Vec3::Y,
            fov_y_degrees: 60.0,
            near: 0.1,
            far: 1000.0,
        };
        assert!(previous_hiz_camera_compatible(&base, &base));

        let mut jitter = base.clone();
        jitter.position.x += 0.00001;
        jitter.target.x += 0.00001;
        assert!(previous_hiz_camera_compatible(&base, &jitter));

        let mut rotated = base.clone();
        rotated.target.x += 0.0001;
        assert!(!previous_hiz_camera_compatible(&base, &rotated));

        let mut translated = base.clone();
        translated.position.x += 0.001;
        translated.target.x += 0.001;
        assert!(!previous_hiz_camera_compatible(&base, &translated));
    }

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
        orbit.configured = true;
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
