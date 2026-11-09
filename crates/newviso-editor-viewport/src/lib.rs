//! Backend-independent editor projection, gizmos, navigation and drag transactions.
mod math;
pub use math::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub position: V3,
    pub rotation_degrees: V3,
    pub scale: V3,
}
impl Default for Transform {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            rotation_degrees: [0.0; 3],
            scale: [1.0; 3],
        }
    }
}
impl Transform {
    pub fn valid(self) -> bool {
        self.position
            .iter()
            .chain(self.rotation_degrees.iter())
            .chain(self.scale.iter())
            .all(|v| v.is_finite() && (*v as f32).is_finite())
            && self.scale.iter().all(|v| v.abs() >= 0.0001)
    }
    pub fn point(self, p: V3) -> V3 {
        add(
            self.position,
            rotate(
                std::array::from_fn(|i| p[i] * self.scale[i]),
                self.rotation_degrees,
            ),
        )
    }
    pub fn inverse_ray(self, r: Ray) -> Ray {
        let origin = unrotate(sub(r.origin, self.position), self.rotation_degrees);
        let direction = unrotate(r.direction, self.rotation_degrees);
        Ray {
            origin: std::array::from_fn(|i| origin[i] / self.scale[i]),
            direction: std::array::from_fn(|i| direction[i] / self.scale[i]),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: V3,
    pub max: V3,
}
impl Bounds {
    pub fn center(self) -> V3 {
        mul(add(self.min, self.max), 0.5)
    }
    pub fn radius(self) -> f64 {
        length(sub(self.max, self.min)) * 0.5
    }
    pub fn corners(self) -> [V3; 8] {
        std::array::from_fn(|i| {
            std::array::from_fn(|k| {
                if i & (1 << k) == 0 {
                    self.min[k]
                } else {
                    self.max[k]
                }
            })
        })
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub position: V3,
    pub target: V3,
    pub up: V3,
    pub fov_y_degrees: f64,
    pub near: f64,
    pub far: f64,
    pub size: [u32; 2],
}
impl Camera {
    pub fn forward(self) -> V3 {
        normal(sub(self.target, self.position))
    }
    pub fn right(self) -> V3 {
        normal(cross(self.forward(), self.up))
    }
    pub fn vertical(self) -> V3 {
        normal(cross(self.right(), self.forward()))
    }
    pub fn ray(self, p: [f64; 2]) -> Ray {
        let h = self.size[1].max(1) as f64;
        let w = self.size[0].max(1) as f64;
        let t = (self.fov_y_degrees.to_radians() * 0.5).tan();
        Ray {
            origin: self.position,
            direction: normal(add(
                self.forward(),
                add(
                    mul(self.right(), (2.0 * p[0] / w - 1.0) * t * w / h),
                    mul(self.vertical(), (1.0 - 2.0 * p[1] / h) * t),
                ),
            )),
        }
    }
    pub fn depth(self, p: V3) -> f64 {
        dot(sub(p, self.position), self.forward())
    }
    pub fn project(self, p: V3) -> Option<[f64; 2]> {
        let v = sub(p, self.position);
        let z = dot(v, self.forward());
        if z < self.near.max(0.001) || z > self.far {
            return None;
        }
        let h = self.size[1].max(1) as f64;
        let k = h / (2.0 * (self.fov_y_degrees.to_radians() * 0.5).tan() * z);
        Some([
            self.size[0] as f64 * 0.5 + dot(v, self.right()) * k,
            h * 0.5 - dot(v, self.vertical()) * k,
        ])
    }
    pub fn units_per_pixel(self, p: V3) -> f64 {
        2.0 * self.depth(p).max(self.near * 2.0) * (self.fov_y_degrees.to_radians() * 0.5).tan()
            / self.size[1].max(1) as f64
    }
    pub fn focus(&mut self, b: Bounds) {
        let dir = self.forward();
        let dist = (b.radius().max(0.5) / (self.fov_y_degrees.to_radians() * 0.4).sin())
            .max(self.near * 8.0);
        self.target = b.center();
        self.position = sub(self.target, mul(dir, dist));
    }
    /// Fit the selection inside the unobstructed editor area, retaining the
    /// renderer's full-surface projection rather than guessing a camera aspect.
    pub fn focus_in_rect(&mut self, b: Bounds, rect: [f64; 4]) {
        let focal =
            self.size[1].max(1) as f64 / (2.0 * (self.fov_y_degrees.to_radians() * 0.5).tan());
        let half = ((rect[2] - rect[0]).min(rect[3] - rect[1]) * 0.5 - 36.0).max(32.0);
        let radius = b.radius().max(0.25);
        let distance = (radius + radius * focal / half).max(self.near * 8.0);
        let center = [(rect[0] + rect[2]) * 0.5, (rect[1] + rect[3]) * 0.5];
        let offset = add(
            mul(
                self.right(),
                (center[0] - self.size[0] as f64 * 0.5) * distance / focal,
            ),
            mul(
                self.vertical(),
                -(center[1] - self.size[1] as f64 * 0.5) * distance / focal,
            ),
        );
        self.position = sub(sub(b.center(), mul(self.forward(), distance)), offset);
        self.target = sub(b.center(), offset);
    }
    pub fn fly(&mut self, mouse: [f64; 2], movement: V3, dt: f64, speed: f64) {
        let f = self.forward();
        let yaw = f[0].atan2(-f[2]) + mouse[0] * 0.003;
        let pitch = (f[1].asin() - mouse[1] * 0.003).clamp(-1.553, 1.553);
        let f = [
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            -yaw.cos() * pitch.cos(),
        ];
        let r = normal(cross(f, [0.0, 1.0, 0.0]));
        let m = add(
            add(mul(r, movement[0]), mul([0.0, 1.0, 0.0], movement[1])),
            mul(f, movement[2]),
        );
        self.position = add(
            self.position,
            mul(
                if length(m) > 1.0 { normal(m) } else { m },
                dt.clamp(0.0, 0.1) * speed,
            ),
        );
        self.target = add(self.position, f);
        self.up = [0.0, 1.0, 0.0];
    }
    pub fn pan(&mut self, delta: [f64; 2], distance: f64) {
        let units = 2.0 * distance.max(0.1) * (self.fov_y_degrees.to_radians() * 0.5).tan()
            / self.size[1].max(1) as f64;
        let v = add(
            mul(self.right(), -delta[0] * units),
            mul(self.vertical(), delta[1] * units),
        );
        self.position = add(self.position, v);
        self.target = add(self.target, v);
    }
    pub fn dolly(&mut self, amount: f64) {
        let distance = length(sub(self.target, self.position)).max(1.0);
        let v = mul(self.forward(), amount * distance * 0.12);
        self.position = add(self.position, v);
        self.target = add(self.target, v);
    }
    pub fn orbit(&mut self, pivot: V3, delta: [f64; 2]) {
        let relative = sub(self.position, pivot);
        let radius = length(relative).max(0.1);
        let pitch = (relative[1] / radius).clamp(-1.0, 1.0).asin();
        let change = ((pitch + delta[1] * 0.005).clamp(-1.553, 1.553)) - pitch;
        let yaw = -delta[0] * 0.005;
        let right = rotate_axis(self.right(), [0.0, 1.0, 0.0], yaw);
        let turn = |v| rotate_axis(rotate_axis(v, [0.0, 1.0, 0.0], yaw), right, -change);
        let up = self.vertical();
        self.position = add(pivot, turn(relative));
        self.target = add(pivot, turn(sub(self.target, pivot)));
        self.up = turn(up);
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    #[default]
    Translate,
    Rotate,
    Scale,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Space {
    #[default]
    World,
    Local,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    Axis(usize),
    Plane(usize, usize),
    Center,
}
#[derive(Clone, Copy, Debug)]
pub struct Snap {
    pub enabled: bool,
    pub translation: f64,
    pub rotation_degrees: f64,
    pub scale: f64,
}
impl Default for Snap {
    fn default() -> Self {
        Self {
            enabled: false,
            translation: 0.5,
            rotation_degrees: 15.0,
            scale: 0.1,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct ScreenLine {
    pub a: [f64; 2],
    pub b: [f64; 2],
    pub color: [u8; 4],
    pub width: f64,
}
#[derive(Clone, Debug)]
pub struct Gizmo {
    pub tool: Tool,
    pub space: Space,
    pub snap: Snap,
    pub dpi: f64,
    pub active: Option<Handle>,
}
impl Default for Gizmo {
    fn default() -> Self {
        Self {
            tool: Tool::Translate,
            space: Space::World,
            snap: Snap::default(),
            dpi: 1.0,
            active: None,
        }
    }
}
impl Gizmo {
    pub fn basis(&self, t: Transform) -> [V3; 3] {
        std::array::from_fn(|i| {
            if self.space == Space::Local || self.tool == Tool::Scale {
                rotate(axis(i), t.rotation_degrees)
            } else {
                axis(i)
            }
        })
    }
    pub fn length(&self, c: Camera, t: Transform) -> f64 {
        c.units_per_pixel(t.position) * 94.0 * self.dpi
    }
    pub fn geometry(
        &self,
        c: Camera,
        t: Transform,
        hover: Option<Handle>,
    ) -> Vec<(ScreenLine, Handle)> {
        let mut out = vec![];
        let basis = self.basis(t);
        let len = self.length(c, t);
        let colors = [[242, 72, 64, 255], [80, 210, 105, 255], [72, 144, 255, 255]];
        let mut line = |a: V3, b: V3, h: Handle, color: [u8; 4]| {
            if let (Some(a), Some(b)) = (c.project(a), c.project(b)) {
                out.push((
                    ScreenLine {
                        a,
                        b,
                        color: if self.active == Some(h) || hover == Some(h) {
                            [255, 224, 80, 255]
                        } else {
                            color
                        },
                        width: 2.6 * self.dpi,
                    },
                    h,
                ));
            }
        };
        for i in 0..3 {
            let h = Handle::Axis(i);
            if self.tool == Tool::Rotate {
                let u = basis[(i + 1) % 3];
                let v = basis[(i + 2) % 3];
                for k in 0..96 {
                    let p = |n: usize| {
                        let a = n as f64 * std::f64::consts::TAU / 96.0;
                        add(t.position, mul(add(mul(u, a.cos()), mul(v, a.sin())), len))
                    };
                    line(p(k), p(k + 1), h, colors[i]);
                }
            } else {
                if let (Some(a), Some(b)) = (
                    c.project(t.position),
                    c.project(add(t.position, mul(basis[i], len))),
                ) {
                    if (a[0] - b[0]).hypot(a[1] - b[1]) < 12.0 * self.dpi {
                        continue;
                    }
                }
                let start = add(t.position, mul(basis[i], len * 0.16));
                let end = add(t.position, mul(basis[i], len));
                line(start, end, h, colors[i]);
                let side = cross(basis[i], c.forward());
                let side = if length(side) < 0.1 {
                    c.right()
                } else {
                    normal(side)
                };
                if self.tool == Tool::Translate {
                    for sign in [-1.0, 1.0] {
                        line(
                            end,
                            add(
                                add(end, mul(basis[i], -len * 0.15)),
                                mul(side, sign * len * 0.06),
                            ),
                            h,
                            colors[i],
                        );
                    }
                } else {
                    let u = mul(side, len * 0.05);
                    let v = mul(basis[i], len * 0.05);
                    for (a, b) in [
                        (add(u, v), sub(u, v)),
                        (sub(u, v), mul(add(u, v), -1.0)),
                        (mul(add(u, v), -1.0), sub(v, u)),
                        (sub(v, u), add(u, v)),
                    ] {
                        line(add(end, a), add(end, b), h, colors[i]);
                    }
                }
            }
        }
        if self.tool == Tool::Translate {
            for (a, b) in [(0, 1), (1, 2), (0, 2)] {
                let h = Handle::Plane(a, b);
                let p = |x: f64, y: f64| {
                    add(
                        t.position,
                        mul(add(mul(basis[a], x), mul(basis[b], y)), len),
                    )
                };
                for (x, y) in [
                    ((0.25, 0.25), (0.43, 0.25)),
                    ((0.43, 0.25), (0.43, 0.43)),
                    ((0.43, 0.43), (0.25, 0.43)),
                    ((0.25, 0.43), (0.25, 0.25)),
                ] {
                    line(p(x.0, x.1), p(y.0, y.1), h, [225, 210, 95, 255]);
                }
            }
        }
        if self.tool != Tool::Rotate {
            if let Some(p) = c.project(t.position) {
                let r = 5.0 * self.dpi;
                for (a, b) in [
                    ([-r, -r], [r, -r]),
                    ([r, -r], [r, r]),
                    ([r, r], [-r, r]),
                    ([-r, r], [-r, -r]),
                ] {
                    out.push((
                        ScreenLine {
                            a: [p[0] + a[0], p[1] + a[1]],
                            b: [p[0] + b[0], p[1] + b[1]],
                            color: [250, 250, 240, 255],
                            width: 2.0 * self.dpi,
                        },
                        Handle::Center,
                    ));
                }
            }
        }
        out
    }
    pub fn hit(&self, c: Camera, t: Transform, p: [f64; 2]) -> Option<Handle> {
        if self.tool != Tool::Rotate
            && c.project(t.position)
                .is_some_and(|center| (center[0] - p[0]).hypot(center[1] - p[1]) <= 8.0 * self.dpi)
        {
            return Some(Handle::Center);
        }
        self.geometry(c, t, None)
            .into_iter()
            .filter_map(|(l, h)| {
                let d = segment_distance(p, l.a, l.b);
                (d < 8.0 * self.dpi).then_some((d, h))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|v| v.1)
    }
    pub fn begin(&mut self, c: Camera, t: Transform, p: [f64; 2], h: Handle) -> Option<Drag> {
        let d = Drag::new(c, t, p, h, self)?;
        self.active = Some(h);
        Some(d)
    }
}
#[derive(Clone, Debug)]
pub struct Drag {
    pub original: Transform,
    pub current: Transform,
    pub handle: Handle,
    camera: Camera,
    start: [f64; 2],
    basis: [V3; 3],
    tool: Tool,
    len: f64,
    anchor: Option<V3>,
    axis_start: Option<f64>,
    last_angle: f64,
    total_angle: f64,
}
impl Drag {
    fn new(c: Camera, t: Transform, p: [f64; 2], h: Handle, g: &Gizmo) -> Option<Self> {
        if !t.valid() || c.project(t.position).is_none() {
            return None;
        }
        let b = g.basis(t);
        let ray = c.ray(p);
        let normal = match h {
            Handle::Axis(i) if g.tool == Tool::Rotate => b[i],
            Handle::Plane(i, j) => normal(cross(b[i], b[j])),
            _ => c.forward(),
        };
        Some(Self {
            original: t,
            current: t,
            handle: h,
            camera: c,
            start: p,
            basis: b,
            tool: g.tool,
            len: g.length(c, t),
            anchor: ray.plane(t.position, normal),
            axis_start: if let Handle::Axis(i) = h {
                ray.axis_parameter(t.position, b[i])
            } else {
                None
            },
            last_angle: 0.0,
            total_angle: 0.0,
        })
    }
    pub fn update(&mut self, p: [f64; 2], snap: Snap) -> Transform {
        let ray = self.camera.ray(p);
        let mut t = self.original;
        let snap_value = |v: f64, step: f64| {
            if snap.enabled && step > 0.0 {
                (v / step).round() * step
            } else {
                v
            }
        };
        match self.tool {
            Tool::Translate => {
                let delta = match self.handle {
                    Handle::Axis(i) => {
                        let value = ray
                            .axis_parameter(t.position, self.basis[i])
                            .zip(self.axis_start)
                            .map(|(a, b)| a - b)
                            .unwrap_or(0.0);
                        mul(self.basis[i], snap_value(value, snap.translation))
                    }
                    h => {
                        let plane = if let Handle::Plane(i, j) = h {
                            normal(cross(self.basis[i], self.basis[j]))
                        } else {
                            self.camera.forward()
                        };
                        let v = ray
                            .plane(t.position, plane)
                            .zip(self.anchor)
                            .map(|(a, b)| sub(a, b))
                            .unwrap_or([0.0; 3]);
                        if let Handle::Plane(i, j) = h {
                            add(
                                mul(
                                    self.basis[i],
                                    snap_value(dot(v, self.basis[i]), snap.translation),
                                ),
                                mul(
                                    self.basis[j],
                                    snap_value(dot(v, self.basis[j]), snap.translation),
                                ),
                            )
                        } else {
                            std::array::from_fn(|i| snap_value(v[i], snap.translation))
                        }
                    }
                };
                t.position = add(t.position, delta);
            }
            Tool::Rotate => {
                if let Handle::Axis(i) = self.handle {
                    let a = self.basis[i];
                    let angle = ray
                        .plane(t.position, a)
                        .zip(self.anchor)
                        .map(|(now, start)| {
                            let from = normal(sub(start, t.position));
                            let to = normal(sub(now, t.position));
                            dot(a, cross(from, to)).atan2(dot(from, to)).to_degrees()
                        })
                        .unwrap_or((p[0] - self.start[0] - (p[1] - self.start[1])) * 0.5);
                    let delta = (angle - self.last_angle + 180.0).rem_euclid(360.0) - 180.0;
                    self.total_angle += delta;
                    self.last_angle = angle;
                    t.rotation_degrees = compose_rotation(
                        t.rotation_degrees,
                        a,
                        snap_value(self.total_angle, snap.rotation_degrees),
                    );
                }
            }
            Tool::Scale => {
                let factor = if let Handle::Axis(i) = self.handle {
                    ray.axis_parameter(t.position, self.basis[i])
                        .zip(self.axis_start)
                        .map(|(a, b)| 1.0 + (a - b) / self.len.max(1e-6))
                        .unwrap_or(1.0)
                } else {
                    ((p[0] - self.start[0] - (p[1] - self.start[1])) / 120.0).exp()
                };
                for i in 0..3 {
                    if self.handle == Handle::Center || self.handle == Handle::Axis(i) {
                        let magnitude = t.scale[i].abs() * factor.clamp(0.001, 1000.0);
                        t.scale[i] =
                            snap_value(magnitude, snap.scale).max(0.001) * t.scale[i].signum();
                    }
                }
            }
        }
        if t.valid() {
            self.current = t;
        }
        self.current
    }
}
pub fn bounds_lines(c: Camera, b: Bounds) -> Vec<ScreenLine> {
    let corners = b.corners();
    let mut out = vec![];
    for i in 0..8 {
        for k in 0..3 {
            let j = i ^ (1 << k);
            if i < j {
                if let (Some(a), Some(b)) = (c.project(corners[i]), c.project(corners[j])) {
                    out.push(ScreenLine {
                        a,
                        b,
                        color: [255, 180, 56, 255],
                        width: 1.5,
                    });
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;
