//! One authority decision shared by movement, combat and host diagnostics.
//! Animation may still supply goals; this contract controls direct voluntary actions.
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyOwner {
    Locomotion,
    Endorphin,
    Recovery,
    Impact,
    PassivePhysics,
    Reset,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AuthorityObservation {
    pub dead: bool,
    pub reset_pending: bool,
    pub articulated: bool,
    pub recovering: bool,
    pub vehicle_response: bool,
    pub hit_reaction: bool,
    pub protective_reach: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ControlAuthority {
    pub owner: BodyOwner,
    pub locomotion: bool,
    pub combat: bool,
}

impl ControlAuthority {
    pub fn resolve(state: AuthorityObservation) -> Self {
        let owner = if state.reset_pending {
            BodyOwner::Reset
        } else if state.dead {
            BodyOwner::PassivePhysics
        } else if state.articulated {
            BodyOwner::Endorphin
        } else if state.recovering {
            BodyOwner::Recovery
        } else if state.vehicle_response {
            BodyOwner::Impact
        } else {
            BodyOwner::Locomotion
        };
        let locomotion = owner == BodyOwner::Locomotion;
        Self {
            owner,
            locomotion,
            combat: locomotion && !state.hit_reaction && !state.protective_reach,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supported_flinch_preserves_movement_without_permitting_voluntary_combat() {
        let normal = ControlAuthority::resolve(AuthorityObservation::default());
        assert_eq!(normal.owner, BodyOwner::Locomotion);
        assert!(normal.locomotion && normal.combat);
        let flinch = ControlAuthority::resolve(AuthorityObservation {
            hit_reaction: true,
            ..Default::default()
        });
        assert_eq!(flinch.owner, BodyOwner::Locomotion);
        assert!(flinch.locomotion && !flinch.combat);
    }
    #[test]
    fn physical_recovery_and_vehicle_response_cannot_share_capsule_authority() {
        for (state, expected) in [
            (
                AuthorityObservation {
                    articulated: true,
                    recovering: true,
                    ..Default::default()
                },
                BodyOwner::Endorphin,
            ),
            (
                AuthorityObservation {
                    recovering: true,
                    ..Default::default()
                },
                BodyOwner::Recovery,
            ),
            (
                AuthorityObservation {
                    vehicle_response: true,
                    ..Default::default()
                },
                BodyOwner::Impact,
            ),
        ] {
            let authority = ControlAuthority::resolve(state);
            assert_eq!(authority.owner, expected);
            assert!(!authority.locomotion && !authority.combat);
        }
    }
    #[test]
    fn death_and_reset_override_living_behavior_without_new_time_or_health_thresholds() {
        let dead = AuthorityObservation {
            dead: true,
            articulated: true,
            recovering: true,
            vehicle_response: true,
            hit_reaction: true,
            ..Default::default()
        };
        let corpse = ControlAuthority::resolve(dead);
        assert_eq!(corpse.owner, BodyOwner::PassivePhysics);
        assert!(!corpse.locomotion && !corpse.combat);
        let reset = ControlAuthority::resolve(AuthorityObservation {
            reset_pending: true,
            ..dead
        });
        assert_eq!(reset.owner, BodyOwner::Reset);
        assert!(!reset.locomotion && !reset.combat);
    }
    #[test]
    fn recovery_releases_authority_from_current_evidence_without_a_stale_latch() {
        let mut state = AuthorityObservation {
            recovering: true,
            ..Default::default()
        };
        assert!(!ControlAuthority::resolve(state).combat);
        state.recovering = false;
        assert!(ControlAuthority::resolve(state).combat);
        state.articulated = true;
        assert!(!ControlAuthority::resolve(state).locomotion);
        state.articulated = false;
        assert!(ControlAuthority::resolve(state).locomotion);
    }
}

#[cfg(test)]
mod protective_authority_tests {
    use super::*;
    #[test]
    fn protective_arms_suspend_voluntary_combat_while_locomotion_can_continue() {
        let a = ControlAuthority::resolve(AuthorityObservation {
            protective_reach: true,
            ..Default::default()
        });
        assert!(a.locomotion && !a.combat && a.owner == BodyOwner::Locomotion);
        assert!(ControlAuthority::resolve(AuthorityObservation::default()).combat);
    }
}
