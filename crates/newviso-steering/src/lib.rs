use serde::{Deserialize, Serialize};

pub type Vec2 = [f32; 2];

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct SteeringNeighbor {
    pub id: u64,
    pub position: Vec2,
    pub velocity: Vec2,
    pub radius: f32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct SteeringObstacle {
    pub position: Vec2,
    pub radius: f32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct SteeringInput {
    pub position: Vec2,
    pub velocity: Vec2,
    pub desired_velocity: Vec2,
    pub radius: f32,
    pub max_speed: f32,
    pub time_horizon: f32,
    pub separation_weight: f32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct SteeringOutput {
    pub velocity: Vec2,
    pub desired_velocity: Vec2,
    pub avoidance_cost: f32,
    pub constrained: bool,
}

pub fn choose_velocity(
    input: SteeringInput,
    neighbors: &[SteeringNeighbor],
    obstacles: &[SteeringObstacle],
) -> Result<SteeringOutput, String> {
    validate(input, neighbors, obstacles)?;

    let desired = clamp_length(input.desired_velocity, input.max_speed);
    let mut candidates = Vec::with_capacity(50);
    candidates.push(desired);
    candidates.push([0.0, 0.0]);

    let desired_angle = desired[1].atan2(desired[0]);
    for speed_scale in [1.0_f32, 0.75, 0.5] {
        for offset in [
            0.0_f32,
            0.22,
            -0.22,
            0.45,
            -0.45,
            0.78,
            -0.78,
            1.15,
            -1.15,
            1.57,
            -1.57,
            2.35,
            -2.35,
            std::f32::consts::PI,
        ] {
            let angle = desired_angle + offset;
            candidates.push([
                angle.cos() * input.max_speed * speed_scale,
                angle.sin() * input.max_speed * speed_scale,
            ]);
        }
    }

    let mut best = desired;
    let mut best_cost = f32::INFINITY;

    for candidate in candidates {
        let mut cost =
            length(sub(candidate, desired)) * 0.45 + length(sub(candidate, input.velocity)) * 0.06;
        for neighbor in neighbors {
            let combined = input.radius + neighbor.radius;
            let t = time_to_collision(
                input.position,
                candidate,
                neighbor.position,
                neighbor.velocity,
                combined,
            );
            cost += collision_penalty(t, input.time_horizon);
            let dist = length(sub(neighbor.position, input.position));
            if dist < combined * 1.35 {
                let overlap =
                    ((combined * 1.35 - dist) / (combined * 1.35).max(0.001)).clamp(0.0, 1.0);
                let away = normalize_or_zero(sub(input.position, neighbor.position));
                let relative_candidate = sub(candidate, neighbor.velocity);
                let separating_speed = dot(relative_candidate, away);
                let directional_penalty = if separating_speed > 0.0 {
                    1.0 / (1.0 + separating_speed)
                } else {
                    2.0 + (-separating_speed).min(input.max_speed)
                };
                cost += input.separation_weight * overlap * directional_penalty;
            }
        }
        for obstacle in obstacles {
            let combined = input.radius + obstacle.radius;
            let t = time_to_collision(
                input.position,
                candidate,
                obstacle.position,
                [0.0, 0.0],
                combined,
            );
            cost += collision_penalty(t, input.time_horizon) * 1.25;
        }

        if cost < best_cost {
            best_cost = cost;
            best = candidate;
        }
    }

    Ok(SteeringOutput {
        velocity: best,
        desired_velocity: desired,
        avoidance_cost: best_cost,
        constrained: length(sub(best, desired)) > 1.0e-4,
    })
}

fn collision_penalty(time: Option<f32>, horizon: f32) -> f32 {
    match time {
        Some(t) if t <= 0.0 => 1000.0,
        Some(t) if t < horizon => 12.0 * (1.0 - t / horizon).powi(2) + 1.0 / (t + 0.05),
        _ => 0.0,
    }
}

fn time_to_collision(p0: Vec2, v0: Vec2, p1: Vec2, v1: Vec2, combined_radius: f32) -> Option<f32> {
    let p = sub(p1, p0);
    let v = sub(v1, v0);
    let c = dot(p, p) - combined_radius * combined_radius;
    if c <= 0.0 {
        return Some(0.0);
    }
    let a = dot(v, v);
    if a <= 1.0e-8 {
        return None;
    }
    let b = dot(p, v);
    if b >= 0.0 {
        return None;
    }
    let discriminant = b * b - a * c;
    if discriminant < 0.0 {
        return None;
    }
    let t = (-b - discriminant.sqrt()) / a;
    (t >= 0.0).then_some(t)
}

fn validate(
    input: SteeringInput,
    neighbors: &[SteeringNeighbor],
    obstacles: &[SteeringObstacle],
) -> Result<(), String> {
    if input
        .position
        .iter()
        .chain(input.velocity.iter())
        .chain(input.desired_velocity.iter())
        .any(|v| !v.is_finite())
        || !input.radius.is_finite()
        || !(0.01..=10.0).contains(&input.radius)
        || !input.max_speed.is_finite()
        || !(0.01..=100.0).contains(&input.max_speed)
        || !input.time_horizon.is_finite()
        || !(0.05..=30.0).contains(&input.time_horizon)
        || !input.separation_weight.is_finite()
        || !(0.0..=100.0).contains(&input.separation_weight)
    {
        return Err("invalid steering input".to_owned());
    }
    if neighbors.iter().any(|n| {
        n.position
            .iter()
            .chain(n.velocity.iter())
            .any(|v| !v.is_finite())
            || !n.radius.is_finite()
            || n.radius <= 0.0
    }) {
        return Err("invalid steering neighbor".to_owned());
    }
    if obstacles.iter().any(|o| {
        o.position.iter().any(|v| !v.is_finite()) || !o.radius.is_finite() || o.radius <= 0.0
    }) {
        return Err("invalid steering obstacle".to_owned());
    }
    Ok(())
}

fn normalize_or_zero(v: Vec2) -> Vec2 {
    let len = length(v);
    if len <= 1.0e-8 {
        [0.0, 0.0]
    } else {
        mul(v, 1.0 / len)
    }
}

fn clamp_length(v: Vec2, max: f32) -> Vec2 {
    let len = length(v);
    if len <= max || len <= 1.0e-8 {
        v
    } else {
        mul(v, max / len)
    }
}
fn length(v: Vec2) -> f32 {
    dot(v, v).sqrt()
}
fn sub(a: Vec2, b: Vec2) -> Vec2 {
    [a[0] - b[0], a[1] - b[1]]
}
fn mul(v: Vec2, s: f32) -> Vec2 {
    [v[0] * s, v[1] * s]
}
fn dot(a: Vec2, b: Vec2) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> SteeringInput {
        SteeringInput {
            position: [0.0, 0.0],
            velocity: [1.0, 0.0],
            desired_velocity: [2.0, 0.0],
            radius: 0.35,
            max_speed: 2.0,
            time_horizon: 2.0,
            separation_weight: 2.0,
        }
    }

    #[test]
    fn free_path_preserves_desired_velocity() {
        let output = choose_velocity(input(), &[], &[]).unwrap();
        assert!((output.velocity[0] - 2.0).abs() < 1.0e-6);
        assert!(output.velocity[1].abs() < 1.0e-6);
        assert!(!output.constrained);
    }

    #[test]
    fn head_on_neighbor_causes_avoidance() {
        let neighbor = SteeringNeighbor {
            id: 2,
            position: [2.0, 0.0],
            velocity: [-2.0, 0.0],
            radius: 0.35,
        };
        let output = choose_velocity(input(), &[neighbor], &[]).unwrap();
        assert!(output.constrained);
        assert!(output.velocity[1].abs() > 0.1 || output.velocity[0] < 1.5);
    }

    #[test]
    fn static_obstacle_changes_course() {
        let obstacle = SteeringObstacle {
            position: [1.3, 0.0],
            radius: 0.45,
        };
        let output = choose_velocity(input(), &[], &[obstacle]).unwrap();
        assert!(output.constrained);
    }

    #[test]
    fn overlap_prefers_separation() {
        let neighbor = SteeringNeighbor {
            id: 2,
            position: [0.3, 0.0],
            velocity: [0.0, 0.0],
            radius: 0.35,
        };
        let output = choose_velocity(input(), &[neighbor], &[]).unwrap();
        assert!(output.avoidance_cost.is_finite());
        assert!(output.constrained);
    }
}
