#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Vec3 {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) z: f32,
}

impl Vec3 {
    pub(crate) const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    pub(crate) const ONE: Self = Self::new(1.0, 1.0, 1.0);
    pub(crate) const Y: Self = Self::new(0.0, 1.0, 0.0);

    pub(crate) const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub(crate) fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }

    pub(crate) fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }

    pub(crate) fn mul(self, scalar: f32) -> Self {
        Self::new(self.x * scalar, self.y * scalar, self.z * scalar)
    }

    pub(crate) fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    pub(crate) fn cross(self, rhs: Self) -> Self {
        Self::new(
            self.y * rhs.z - self.z * rhs.y,
            self.z * rhs.x - self.x * rhs.z,
            self.x * rhs.y - self.y * rhs.x,
        )
    }

    pub(crate) fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    pub(crate) fn normalized(self) -> Self {
        let len = self.length();
        if len <= f32::EPSILON {
            Self::ZERO
        } else {
            Self::new(self.x / len, self.y / len, self.z / len)
        }
    }
}

pub(crate) fn transform_point(
    point: Vec3,
    scale: Vec3,
    rotation_degrees: Vec3,
    translation: Vec3,
) -> Vec3 {
    let mut p = Vec3::new(point.x * scale.x, point.y * scale.y, point.z * scale.z);

    let rx = rotation_degrees.x.to_radians();
    let ry = rotation_degrees.y.to_radians();
    let rz = rotation_degrees.z.to_radians();

    p = Vec3::new(
        p.x,
        p.y * rx.cos() - p.z * rx.sin(),
        p.y * rx.sin() + p.z * rx.cos(),
    );
    p = Vec3::new(
        p.x * ry.cos() + p.z * ry.sin(),
        p.y,
        -p.x * ry.sin() + p.z * ry.cos(),
    );
    p = Vec3::new(
        p.x * rz.cos() - p.y * rz.sin(),
        p.x * rz.sin() + p.y * rz.cos(),
        p.z,
    );

    Vec3::new(
        p.x + translation.x,
        p.y + translation.y,
        p.z + translation.z,
    )
}
