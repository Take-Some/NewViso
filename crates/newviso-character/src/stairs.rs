//! Bounded, sweep-validated stair assistance for upright dynamic capsules.
//! The provider still resolves horizontal contacts. No impulses are cached and
//! no horizontal pose is advanced by this controller.
use super::*;

#[derive(Clone, Copy, Debug)]
struct Lift {
    probe: Vec3,
    direction: Vec3,
    target_y: f32,
    elapsed: f32,
    lifetime: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StairController {
    lift: Option<Lift>,
    supported: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct StairAdjustment {
    pub y: f32,
    pub normal: Vec3,
    pub entity: Option<u64>,
    pub lifting: bool,
}

impl StairController {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn update<W: CharacterCollisionWorld>(
        &mut self,
        world: &W,
        config: CharacterConfig,
        position: Vec3,
        velocity: Vec3,
        intent: Vec3,
        grounded: bool,
        dt: f32,
    ) -> Option<StairAdjustment> {
        if dt <= 0.0 || !dt.is_finite() || config.step_height <= 0.0 {
            self.reset();
            return None;
        }
        // An actual jump or strong external vertical impulse owns the body.
        // Assistance never catches an actor falling from a ledge.
        if velocity[1] > config.step_speed || velocity[1] < -config.ground_snap_speed {
            self.reset();
            return None;
        }
        let controller = CharacterController { config };
        let speed = horizontal_length(intent);
        let direction = if speed > 1e-5 {
            [intent[0] / speed, 0.0, intent[2] / speed]
        } else {
            [0.0; 3]
        };
        let epsilon = config.skin.max(0.002);
        let toe = (config.radius * 0.08).max(epsilon);
        let extent = config.half_height + config.radius;

        // Keep descent tied to the previous support. A new airborne body cannot
        // acquire ground from a long snap probe.
        let was_supported = grounded || self.supported;
        // Only the bottom hemisphere can be a supporting sole. Raising its
        // query above a reachable riser avoids a side overlap masking the floor;
        // a middle/head sphere must never pull the capsule down into a step.
        let down = config.step_height + config.ground_snap_distance + 3.0 * epsilon;
        let origin = [
            position[0],
            position[1] - config.half_height + config.step_height + epsilon,
            position[2],
        ];
        let full = world
            .sweep_sphere(origin, [0.0, -down, 0.0], config.radius)
            .filter(|h| h.normal[1] >= config.walkable_normal_y())
            .map(|h| {
                (
                    h,
                    origin[1] - down * h.fraction + config.half_height + epsilon,
                )
            });
        // A steep rounded triangle nosing must not hide the lower tread.
        let support = full.or_else(|| {
            let foot = [
                position[0],
                position[1] - extent + config.step_height + toe + epsilon,
                position[2],
            ];
            world
                .sweep_sphere(foot, [0.0, -down, 0.0], toe)
                .filter(|h| h.normal[1] >= config.walkable_normal_y())
                .map(|h| (h, foot[1] - down * h.fraction - toe + extent + epsilon))
        });
        // Contact-envelope normals curve around a capsule/triangle nosing.
        // Posture stability needs the actual reachable tread plane instead.
        let ray_origin = [
            position[0],
            position[1] - extent + config.step_height + epsilon,
            position[2],
        ];
        let plane = world
            .sweep_sphere(ray_origin, [0.0, -down, 0.0], 0.0)
            .filter(|h| h.normal[1] >= config.walkable_normal_y());
        let ground = support.map(|(mut hit, _)| {
            if let Some(plane) = plane {
                hit.normal = plane.normal;
            }
            hit
        });
        let ground_y = support.map(|s| s.1);
        let touching = ground_y.is_some_and(|y| (position[1] - y).abs() <= 3.0 * epsilon);

        if let Some(lift) = &mut self.lift {
            lift.elapsed += dt;
            if speed <= 1e-5
                || dot(direction, lift.direction) < 0.5
                || lift.elapsed > lift.lifetime
                || horizontal_length(sub(position, lift.probe))
                    > 2.0 * config.radius + speed * dt + 2.0 * toe + 2.0 * epsilon
            {
                self.lift = None;
            }
        }

        if self.lift.is_none() && was_supported && speed > 1e-5 {
            // Find the nearest riser, then probe just beyond its leading edge.
            // The narrow tread probe cannot mistake the padded AABB of the
            // capsule for a floor, or reach a distant landing across a gap.
            let look = config.radius + speed * dt + epsilon;
            let horizontal = mul(direction, look);
            if let Some(wall) = controller
                .sweep_capsule(world, add(position, [0.0, epsilon, 0.0]), horizontal)
                .filter(|h| {
                    h.normal[1] < config.walkable_normal_y() && dot(horizontal, h.normal) < -1e-6
                })
            {
                let advance = look * wall.fraction + config.radius + 2.0 * toe;
                let probe = add(position, mul(direction, advance));
                let origin = [
                    probe[0],
                    position[1] - extent + config.step_height + toe + epsilon,
                    probe[2],
                ];
                let down = config.step_height + 2.0 * epsilon;
                if let Some(hit) = world
                    .sweep_sphere(origin, [0.0, -down, 0.0], toe)
                    .filter(|h| h.normal[1] >= config.walkable_normal_y())
                {
                    let target_y = origin[1] - down * hit.fraction - toe + extent + epsilon;
                    let rise = target_y - position[1];
                    if rise > epsilon && rise <= config.step_height + epsilon {
                        let up = [0.0, rise, 0.0];
                        let elevated = add(position, up);
                        if clear(controller.sweep_capsule(world, position, up), up)
                            && clear(
                                controller.sweep_capsule(
                                    world,
                                    elevated,
                                    mul(direction, speed * dt),
                                ),
                                mul(direction, speed * dt),
                            )
                        {
                            self.lift = Some(Lift {
                                probe: [probe[0], target_y, probe[2]],
                                direction,
                                target_y,
                                elapsed: 0.0,
                                lifetime: 2.0 * rise / config.step_speed.max(0.01)
                                    + 2.0 * config.radius / speed.max(0.1),
                            });
                        }
                    }
                }
            }
        }

        if let Some(mut lift) = self.lift {
            // Re-query the planned tread: disappearing/moving supports never
            // leave a suspended body pinned to a stale world-space target.
            let origin = [
                lift.probe[0],
                lift.target_y - extent + toe + epsilon,
                lift.probe[2],
            ];
            let hit = world
                .sweep_sphere(origin, [0.0, -4.0 * epsilon, 0.0], toe)
                .filter(|h| h.normal[1] >= config.walkable_normal_y());
            if let Some(hit) = hit {
                let rate = config
                    .step_speed
                    .min(speed * config.step_height / config.radius);
                let lift_by = (lift.target_y - position[1]).max(0.0).min(rate * dt);
                let up = [0.0, lift_by, 0.0];
                if clear(controller.sweep_capsule(world, position, up), up) {
                    let y = position[1] + lift_by;
                    let reached = y >= lift.target_y - epsilon;
                    if reached && touching {
                        self.lift = None;
                    } else {
                        if let Some(entity) = hit.entity {
                            let motion = mul(world.support_velocity(entity), dt);
                            lift.probe = add(lift.probe, motion);
                            lift.target_y += motion[1];
                        }
                        self.lift = Some(lift);
                    }
                    self.supported = true;
                    return Some(StairAdjustment {
                        y,
                        normal: hit.normal,
                        entity: hit.entity,
                        lifting: true,
                    });
                }
            }
            self.lift = None;
        }
        self.supported = touching;
        if was_supported {
            if let (Some(hit), Some(y)) = (ground, ground_y) {
                let drop = (position[1] - y).max(0.0);
                if drop <= config.ground_snap_distance + epsilon {
                    // Resolve shallow contact slop within the bounded support
                    // transfer, avoiding a solver ejection at a flight's end.
                    let rise = (y - position[1]).max(0.0);
                    let correction = if rise <= 3.0 * epsilon {
                        rise.min(config.step_speed * dt)
                    } else {
                        0.0
                    };
                    let up = [0.0, correction, 0.0];
                    let correction = if clear(controller.sweep_capsule(world, position, up), up) {
                        correction
                    } else {
                        0.0
                    };
                    let y = position[1] + correction - drop.min(config.ground_snap_speed * dt);
                    self.supported = true;
                    return Some(StairAdjustment {
                        y,
                        normal: hit.normal,
                        entity: hit.entity,
                        lifting: false,
                    });
                }
            }
        }
        None
    }
}
fn clear(hit: Option<SweepHit>, delta: Vec3) -> bool {
    hit.is_none_or(|h| h.fraction >= 1.0 - 1e-5 || dot(delta, h.normal) >= -1e-6)
}

#[cfg(test)]
mod tests;
