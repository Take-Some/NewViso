//! Model-independent sole geometry and closest feasible tread placement.
use super::*;
#[derive(Debug)]
pub(super) struct BoundSoles {
    pub skeleton: Arc<newviso_model::ModelSkeleton>,
    pub profile: Arc<Vec<LegBinding>>,
    pub scale: [f32; 3],
    pub soles: Vec<NeutralSole>,
    pub floor_offset: f32,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct NeutralSole {
    pub corners: [[f32; 3]; 4],
    pub up: [f32; 3],
}
impl NeutralSole {
    pub fn bind(
        foot: [f32; 16],
        toe: [f32; 16],
        leg: &LegBinding,
        scale: f32,
        up: [f32; 3],
    ) -> Option<Self> {
        let q = m::rotation(foot);
        let up = m::unit(up).ok()?;
        let span = m::sub(m::position(toe), m::position(foot));
        let forward = m::unit(m::sub(span, up.map(|v| v * m::dot(span, up)))).ok()?;
        let right = m::cross(up, forward);
        let length = m::dot(span, forward);
        let heel = leg.heel_length.unwrap_or(leg.sole_height) * scale;
        let width = leg.sole_half_width.unwrap_or(leg.sole_height) * scale;
        let height = leg.sole_height * scale;
        let corners = [
            [-width, -heel],
            [width, -heel],
            [width, length],
            [-width, length],
        ]
        .map(|[w, l]| {
            m::rotate(
                m::inverse(q),
                m::sub(
                    m::add(right.map(|v| v * w), forward.map(|v| v * l)),
                    up.map(|v| v * height),
                ),
            )
        });
        Some(Self {
            corners,
            up: m::rotate(m::inverse(q), up),
        })
    }
}
#[derive(Clone)]
pub(super) struct Placement {
    pub surface_entity: u64,
    pub complete: bool,
    pub has_surface: bool,
    pub contact_point: [f32; 3],
    pub target: [f32; 3],
    pub rotation: [f32; 4],
    pub corners: [[f32; 3]; 4],
    pub normal: [f32; 3],
    pub surface_id: String,
    pub score: f32,
    pub horizontal_error: f32,
}
pub(super) fn on_surface(
    surface: &newviso_collision::SupportSurface,
    offset: [f32; 3],
    entity: u64,
    sole: NeutralSole,
    foot: [f32; 3],
    rotation: [f32; 4],
    _hip: [f32; 3],
    _reach: f32,
    lift: f32,
    config: newviso_character::CharacterConfig,
    terrain_oriented: bool,
) -> Option<Placement> {
    if surface.normal[1] < config.max_slope_degrees.to_radians().cos() {
        return None;
    }
    let rotation = if terrain_oriented {
        rotation
    } else {
        m::multiply(m::align_axis([0.0, 1.0, 0.0], surface.normal), rotation)
    };
    let corners = sole.corners.map(|p| m::rotate(rotation, p));
    // Query skin erodes usable support. It is a safety margin, not a centre bias.
    let centre = m::add(corners[0], corners[2]).map(|v| v * 0.5);
    let margin = corners.map(|p| {
        let d = m::sub(p, centre);
        let length = m::length(d);
        let extra = if length > 1e-6 {
            d.map(|v| v * config.skin / length)
        } else {
            [0.0; 3]
        };
        [p[0] + extra[0], p[2] + extra[2]]
    });
    let fitted = surface.fit_footprint(
        [foot[0] - offset[0], foot[2] - offset[2]],
        margin,
        config.radius,
    )?;
    let floor = fitted[1] + offset[1];
    let sole_offset = corners
        .iter()
        .map(|p| -p[1] - (surface.normal[0] * p[0] + surface.normal[2] * p[2]) / surface.normal[1])
        .fold(f32::NEG_INFINITY, f32::max);
    let target = [
        fitted[0] + offset[0],
        floor + sole_offset + lift,
        fitted[2] + offset[2],
    ];

    let world_corners = corners.map(|p| m::add(p, target));
    let contact_point = *world_corners
        .iter()
        .min_by(|a, b| m::dot(**a, surface.normal).total_cmp(&m::dot(**b, surface.normal)))?;
    Some(Placement {
        surface_entity: entity,
        contact_point,
        complete: true,
        has_surface: true,
        target,
        rotation,
        corners: corners.map(|p| m::add(p, target)),
        normal: surface.normal,
        surface_id: format!("{entity}:{}", surface.id),
        horizontal_error: (target[0] - foot[0]).powi(2) + (target[2] - foot[2]).powi(2),
        score: m::dot(m::sub(target, foot), m::sub(target, foot)),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sole_fits_heel_and_toe_preserving_off_centre_stance_and_rejects_short_tread() {
        let leg = LegBinding {
            hip: "a".into(),
            knee: "b".into(),
            foot: "c".into(),
            toe: "d".into(),
            sole_height: 0.08,
            sole_half_width: Some(0.05),
            heel_length: Some(0.07),
        };
        let foot = m::matrix([0.2, 0.08, 0.12], [0.0, 0.0, 0.0, 1.0]);
        let sole = NeutralSole::bind(
            foot,
            m::matrix([0.2, 0.08, 0.3], [0.0, 0.0, 0.0, 1.0]),
            &leg,
            1.0,
            [0.0, 1.0, 0.0],
        )
        .unwrap();
        let s = newviso_collision::SupportSurface::from_quad(
            0,
            [
                [0.0, 0.0, 0.0],
                [3.0, 0.0, 0.0],
                [3.0, 0.0, 0.4],
                [0.0, 0.0, 0.4],
            ],
        )
        .unwrap();
        let p = on_surface(
            &s,
            [0.0; 3],
            1,
            sole,
            [0.2, 0.08, 0.12],
            [0.0, 0.0, 0.0, 1.0],
            [0.2, 0.8, 0.12],
            1.0,
            0.0,
            Default::default(),
            false,
        )
        .unwrap();
        assert!((p.target[0] - 0.2).abs() < 1e-5 && (p.target[2] - 0.12).abs() < 1e-5);
        assert!(
            p.corners
                .iter()
                .all(|p| p[2] > 0.0 && p[2] < 0.4 && p[1].abs() < 1e-5)
        );
        let short = newviso_collision::SupportSurface::from_quad(
            0,
            [
                [0.0, 0.0, 0.0],
                [3.0, 0.0, 0.0],
                [3.0, 0.0, 0.2],
                [0.0, 0.0, 0.2],
            ],
        )
        .unwrap();
        assert!(
            on_surface(
                &short,
                [0.0; 3],
                1,
                sole,
                [0.2, 0.08, 0.12],
                [0.0, 0.0, 0.0, 1.0],
                [0.2, 0.8, 0.12],
                1.0,
                0.0,
                Default::default(),
                false
            )
            .is_none()
        );
    }
    #[test]
    fn terrain_tilt_preserves_authored_heel_roll_and_clearance() {
        let leg = LegBinding {
            hip: "a".into(),
            knee: "b".into(),
            foot: "c".into(),
            toe: "d".into(),
            sole_height: 0.08,
            sole_half_width: Some(0.05),
            heel_length: Some(0.07),
        };
        let sole = NeutralSole::bind(
            m::matrix([0.0; 3], [0.0, 0.0, 0.0, 1.0]),
            m::matrix([0.0, 0.0, 0.18], [0.0, 0.0, 0.0, 1.0]),
            &leg,
            1.0,
            [0.0, 1.0, 0.0],
        )
        .unwrap();
        let s = newviso_collision::SupportSurface::from_quad(
            0,
            [
                [-2.0, 0.0, -2.0],
                [2.0, 0.0, -2.0],
                [2.0, 0.4, 2.0],
                [-2.0, 0.4, 2.0],
            ],
        )
        .unwrap();
        let p = on_surface(
            &s,
            [0.0; 3],
            1,
            sole,
            [0.0, 0.3, 0.0],
            m::axis_angle([0.4, 0.0, 0.0]),
            [0.0, 0.9, 0.0],
            1.0,
            0.04,
            Default::default(),
            false,
        )
        .unwrap();
        let gaps = p.corners.map(|p| p[1] - s.height(p[0], p[2]));
        assert!(gaps.iter().all(|gap| *gap >= 0.04 - 1e-5));
        assert!((gaps.into_iter().fold(f32::INFINITY, f32::min) - 0.04).abs() < 1e-5);
        assert!((m::dot(m::rotate(p.rotation, sole.up), s.normal) - 0.4_f32.cos()).abs() < 1e-5);
    }
}

/// Valid vertical translations of the complete hierarchy, using both reach limits.
pub(super) fn root_interval(
    hip: [f32; 3],
    target: [f32; 3],
    upper: f32,
    lower: f32,
    max_drop: f32,
    reserve: f32,
) -> Option<(f32, f32)> {
    let d = m::sub(target, hip);
    let horizontal = d[0] * d[0] + d[2] * d[2];
    let reach = upper + lower - reserve;
    if reach <= 0.0 {
        return None;
    }
    if horizontal > reach * reach {
        return None;
    }
    let dy = (reach * reach - horizontal).sqrt();
    let minimum = (upper - lower).abs() + 0.001;
    let lower_fold = if horizontal < minimum * minimum {
        d[1] + (minimum * minimum - horizontal).sqrt()
    } else {
        f32::NEG_INFINITY
    };
    let low = (d[1] - dy).max(-max_drop).max(lower_fold);
    let high = (d[1] + dy).min(0.0);
    if low > high {
        return None;
    }
    let offset = high;
    let distance = (horizontal + (d[1] - offset).powi(2)).sqrt();
    if distance < (upper - lower).abs() + 0.001 {
        return None;
    }
    Some((low, high))
}

#[cfg(test)]
mod posture_tests {
    use super::*;
    #[test]
    fn capsule_anticipation_is_resolved_by_bounded_posture_not_longer_bones() {
        let hip = [0.0, 1.0, 0.0];
        let target = [0.0, 0.08, 0.0];
        let interval = root_interval(hip, target, 0.4, 0.4, 0.35, 0.016).unwrap();
        assert!(interval.1 < -0.13 && interval.1 > -0.15);
        assert!(root_interval(hip, target, 0.4, 0.4, 0.05, 0.016).is_none());
        assert!(root_interval(hip, [1.0, 0.1, 0.0], 0.4, 0.4, 0.35, 0.016).is_none());
    }
    #[test]
    fn neutral_up_is_derived_from_the_skeleton_not_assumed_world_y() {
        let leg = LegBinding {
            hip: "a".into(),
            knee: "b".into(),
            foot: "c".into(),
            toe: "d".into(),
            sole_height: 0.08,
            sole_half_width: Some(0.05),
            heel_length: Some(0.07),
        };
        let sole = NeutralSole::bind(
            m::matrix([0.0; 3], [0.0, 0.0, 0.0, 1.0]),
            m::matrix([0.0, 0.18, 0.0], [0.0, 0.0, 0.0, 1.0]),
            &leg,
            1.0,
            [0.0, 0.0, 1.0],
        )
        .unwrap();
        assert!(sole.corners.iter().all(|p| (p[2] + 0.08).abs() < 1e-5));
    }
}
