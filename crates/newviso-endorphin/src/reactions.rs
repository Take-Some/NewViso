#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ReactionClip {
    pub clip_ref: String,
    pub duration_seconds: f32,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct DirectionalClips {
    pub front: ReactionClip,
    pub back: ReactionClip,
    pub left: ReactionClip,
    pub right: ReactionClip,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ReactionClips {
    pub knockdown: DirectionalClips,
    // Each entry matches the FINAL pose of its corresponding knockdown clip.
    // A front impact may finish on the back, so names are impact directions.
    pub get_up: DirectionalClips,
    pub airborne: ReactionClip,
    pub landing: ReactionClip,
    pub hard_landing: ReactionClip,
    #[serde(default)]
    pub hit: Option<DirectionalClips>,
}

impl ReactionClips {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(hit) = &self.hit {
            for c in [&hit.front, &hit.back, &hit.left, &hit.right] {
                if c.clip_ref.trim().is_empty()
                    || !c.duration_seconds.is_finite()
                    || !(0.05..=60.0).contains(&c.duration_seconds)
                {
                    return Err("invalid hit reaction clip".into());
                }
            }
        }
        for clip in [
            &self.knockdown.front,
            &self.knockdown.back,
            &self.knockdown.left,
            &self.knockdown.right,
            &self.get_up.front,
            &self.get_up.back,
            &self.get_up.left,
            &self.get_up.right,
            &self.airborne,
            &self.landing,
            &self.hard_landing,
        ] {
            if clip.clip_ref.trim().is_empty()
                || clip.clip_ref.len() > 256
                || !clip.clip_ref.contains('@')
                || !clip.duration_seconds.is_finite()
                || !(0.05..=60.0).contains(&clip.duration_seconds)
            {
                return Err("invalid pedestrian reaction clip or duration".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FallDirection {
    Front,
    Back,
    Left,
    Right,
}

impl DirectionalClips {
    pub fn select(&self, direction: FallDirection) -> ReactionClip {
        match direction {
            FallDirection::Front => &self.front,
            FallDirection::Back => &self.back,
            FallDirection::Left => &self.left,
            FallDirection::Right => &self.right,
        }
        .clone()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReactionPhase {
    Airborne,
    Knockdown,
    Down,
    GetUp,
    Landing,
}

impl ReactionPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Airborne => "airborne",
            Self::Knockdown => "fall",
            Self::Down => "down",
            Self::GetUp => "get_up",
            Self::Landing => "land",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ReactionState {
    pub phase: ReactionPhase,
    pub cause: &'static str,
    pub direction: FallDirection,
    pub clip: ReactionClip,
    pub elapsed: f32,
    pub minimum_vertical_speed: f32,
    pub generation: u64,
}

impl ReactionState {
    pub fn clip_time(&self) -> f32 {
        if self.phase == ReactionPhase::Down {
            self.clip.duration_seconds
        } else {
            self.elapsed.min(self.clip.duration_seconds)
        }
    }
}

pub fn impact_direction(push: [f32; 3], heading: f32) -> FallDirection {
    let (s, c) = heading.to_radians().sin_cos();
    let front = -push[0] * s - push[2] * c;
    let right = -push[0] * c + push[2] * s;
    if front.abs() >= right.abs() {
        if front >= 0.0 {
            FallDirection::Front
        } else {
            FallDirection::Back
        }
    } else if right >= 0.0 {
        FallDirection::Right
    } else {
        FallDirection::Left
    }
}

pub fn capsule_vertical_extent(rotation: [f32; 4], radius: f32, half_height: f32) -> f32 {
    let [x, _, z, _] = rotation;
    radius + half_height * (1.0 - 2.0 * (x * x + z * z)).abs().min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn impact_directions_follow_actor_heading_and_incoming_force() {
        assert_eq!(
            impact_direction([0.0, 0.0, -5.0], 0.0),
            FallDirection::Front
        );
        assert_eq!(impact_direction([0.0, 0.0, 5.0], 0.0), FallDirection::Back);
        assert_eq!(impact_direction([5.0, 0.0, 0.0], 0.0), FallDirection::Left);
        assert_eq!(
            impact_direction([-5.0, 0.0, 0.0], 0.0),
            FallDirection::Right
        );
        assert_eq!(
            impact_direction([-5.0, 0.0, 0.0], 90.0),
            FallDirection::Front
        );
    }
    #[test]
    fn presentation_anchor_accounts_for_tipped_capsule_extent() {
        assert!((capsule_vertical_extent([0.0, 0.0, 0.0, 1.0], 0.3, 0.6) - 0.9).abs() < 1e-5);
        assert!(
            (capsule_vertical_extent([0.70710677, 0.0, 0.0, 0.70710677], 0.3, 0.6) - 0.3).abs()
                < 1e-5
        );
    }
}
