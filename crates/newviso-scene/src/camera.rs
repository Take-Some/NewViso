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
            rotate_sensitivity: 0.005,
            zoom_sensitivity: 0.0015,
            min_distance: 2.0,
            max_distance: 40.0,
        }
    }

    pub(crate) fn apply_mouse(&mut self, dx: f32, dy: f32, wheel_y: f32, rotating: bool) {
        if rotating {
            self.yaw -= dx * self.rotate_sensitivity;
            self.pitch = (self.pitch - dy * self.rotate_sensitivity)
                .clamp(-85.0_f32.to_radians(), 85.0_f32.to_radians());
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

pub(crate) fn world_to_ndc(world: Vec3, camera: &Camera, aspect: f32) -> Vec3 {
    let forward = camera.target.sub(camera.position).normalized();
    let right = forward.cross(camera.up).normalized();
    let up = right.cross(forward).normalized();
    let relative = world.sub(camera.position);

    let view_x = right.dot(relative);
    let view_y = up.dot(relative);
    let view_z = -forward.dot(relative);

    let near = camera.near.max(0.001);
    let far = camera.far.max(near + 0.001);
    let tan_half = (camera.fov_y_degrees.to_radians() * 0.5).tan().max(0.0001);
    let safe_aspect = aspect.max(0.0001);

    // Right-handed Vulkan projection with z in [0, 1]. We pre-divide here because
    // the bootstrap debug shader consumes NDC directly and writes w=1.
    let w = (-view_z).max(0.0001);
    let clip_x = view_x / (tan_half * safe_aspect);
    let clip_y = -view_y / tan_half;
    let clip_z = (far / (near - far)) * view_z + (far * near / (near - far));

    Vec3::new(clip_x / w, clip_y / w, clip_z / w)
}
