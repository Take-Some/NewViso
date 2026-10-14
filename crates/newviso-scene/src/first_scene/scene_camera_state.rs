//! camera state.
use super::*;

impl Scene3dRuntime {
    pub fn configure_orbit_controls(
        &mut self,
        rotate_button: u64,
        min_pitch: f32,
        max_pitch: f32,
    ) -> Result<(), String> {
        if !min_pitch.is_finite()
            || !max_pitch.is_finite()
            || min_pitch <= -90.0
            || max_pitch >= 90.0
            || min_pitch >= max_pitch
        {
            return Err("invalid orbit pitch limits".into());
        }
        self.orbit.rotate_button = rotate_button;
        self.orbit.min_pitch_degrees = min_pitch;
        self.orbit.max_pitch_degrees = max_pitch;
        Ok(())
    }

    pub fn configure_orbit(
        &mut self,
        rotate_sensitivity: f32,
        zoom_sensitivity: f32,
        min_distance: f32,
        max_distance: f32,
    ) -> Result<(), String> {
        if ![
            rotate_sensitivity,
            zoom_sensitivity,
            min_distance,
            max_distance,
        ]
        .iter()
        .all(|value| value.is_finite())
            || rotate_sensitivity <= 0.0
            || zoom_sensitivity <= 0.0
            || min_distance <= 0.0
            || max_distance < min_distance
        {
            return Err("invalid orbit camera settings".to_owned());
        }

        self.orbit.rotate_sensitivity = rotate_sensitivity;
        self.orbit.zoom_sensitivity = zoom_sensitivity;
        self.orbit.min_distance = min_distance;
        self.orbit.max_distance = max_distance;
        self.orbit.configured = true;
        self.orbit.distance = self.orbit.distance.clamp(min_distance, max_distance);
        self.camera.position = self.orbit.position(self.camera.target);
        Ok(())
    }

    /// The constrained render camera is also the source of camera-aimed shots.
    pub fn camera_pose_values(&self) -> ([f32; 3], [f32; 3], [f32; 3]) {
        let forward = self.camera.target.sub(self.camera.position).normalized();
        (
            [self.camera.position.x, self.camera.position.y, self.camera.position.z],
            [forward.x, forward.y, forward.z],
            [self.camera.up.x, self.camera.up.y, self.camera.up.z],
        )
    }

    pub fn set_camera_near_clip(&mut self, near: f32) -> Result<(), String> {
        if !near.is_finite() || near < 0.001 || near >= self.camera.far {
            return Err("camera near_clip must be finite, at least 0.001 and below far".to_owned());
        }
        self.camera.near = near;
        self.sync_runtime_camera_to_flecs()
    }

    pub fn set_camera_pose(
        &mut self,
        position: [f32; 3],
        target: [f32; 3],
        up: Option<[f32; 3]>,
        fov_y_degrees: Option<f32>,
    ) -> Result<(), String> {
        if position
            .iter()
            .chain(target.iter())
            .any(|value| !value.is_finite())
        {
            return Err("scene.camera.set contains a non-finite position or target".to_owned());
        }
        let position = Vec3::new(position[0], position[1], position[2]);
        let target = Vec3::new(target[0], target[1], target[2]);
        if target.sub(position).length() < 0.0001 {
            return Err("scene.camera.set target must differ from position".to_owned());
        }

        self.camera.position = position;
        self.camera.target = target;
        if let Some(up) = up {
            if up.iter().any(|value| !value.is_finite()) {
                return Err("scene.camera.set contains a non-finite up vector".to_owned());
            }
            let up = Vec3::new(up[0], up[1], up[2]);
            if up.length() < 0.0001 {
                return Err("scene.camera.set up vector must be non-zero".to_owned());
            }
            self.camera.up = up.normalized();
        }
        if let Some(fov) = fov_y_degrees {
            if !fov.is_finite() || !(1.0..179.0).contains(&fov) {
                return Err(
                    "scene.camera.set fov_y_degrees must be finite and in 1..179".to_owned(),
                );
            }
            self.camera.fov_y_degrees = fov;
        }
        self.orbit.sync_pose(&self.camera);
        self.sync_runtime_camera_to_flecs()
    }
}
