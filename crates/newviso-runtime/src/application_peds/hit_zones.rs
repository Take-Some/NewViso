//! Shared anatomical damage resolution for every pedestrian combat style.
//! Height is normalized against the authored capsule, not a universal world Y coordinate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HitZone { Head, Neck, Torso, Abdomen, Arm, Leg }

impl HitZone {
    pub(super) fn parse(name: &str) -> Option<Self> {
        match name {
            "head" => Some(Self::Head),
            "neck" => Some(Self::Neck),
            "torso" | "chest" => Some(Self::Torso),
            "abdomen" | "pelvis" => Some(Self::Abdomen),
            "arm" | "hand" | "left_arm" | "left_hand" | "right_arm" | "right_hand" => Some(Self::Arm),
            "leg" | "foot" => Some(Self::Leg),
            _ => None,
        }
    }
    pub(super) fn multiplier(self) -> f32 {
        match self {
            Self::Head => 2.5,
            Self::Neck => 1.65,
            Self::Torso => 1.0,
            Self::Abdomen => 0.9,
            Self::Arm => 0.62,
            Self::Leg => 0.72,
        }
    }
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Head => "head", Self::Neck => "neck", Self::Torso => "torso",
            Self::Abdomen => "abdomen", Self::Arm => "arm", Self::Leg => "leg",
        }
    }
}
pub(super) fn zone_from_contact(
    point: [f32; 3],
    feet: [f32; 3],
    capsule_height: f32,
    capsule_radius: f32,
) -> HitZone {
    let h = ((point[1] - feet[1]) / capsule_height.max(0.01)).clamp(0.0, 1.0);
    let lateral = (point[0] - feet[0]).hypot(point[2] - feet[2]);
    if h >= 0.88 { HitZone::Head }
    else if h >= 0.81 { HitZone::Neck }
    else if h < 0.40 { HitZone::Leg }
    else if lateral > capsule_radius * 0.72 && h >= 0.52 { HitZone::Arm }
    else if h < 0.54 { HitZone::Abdomen }
    else { HitZone::Torso }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn vertical_body_regions_are_stable() {
        assert_eq!(zone_from_contact([0.0,1.70,0.0], [0.0;3],1.8,0.3),HitZone::Head);
        assert_eq!(zone_from_contact([0.0,1.15,0.0], [0.0;3],1.8,0.3),HitZone::Torso);
        assert_eq!(zone_from_contact([0.0,0.3,0.0], [0.0;3],1.8,0.3),HitZone::Leg);
        assert_eq!(zone_from_contact([0.28,1.2,0.0], [0.0;3],1.8,0.3),HitZone::Arm);
    }
    #[test] fn multipliers_and_aliases() {
        assert!(HitZone::Head.multiplier() > HitZone::Torso.multiplier());
        assert_eq!(HitZone::parse("pelvis"),Some(HitZone::Abdomen));
        assert_eq!(HitZone::parse("garbage"),None);
    }
}
