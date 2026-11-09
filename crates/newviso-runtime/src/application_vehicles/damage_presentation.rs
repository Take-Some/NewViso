use super::*;

mod detach;
mod explosion;
mod fire;
mod transaction;

fn vehicle_role_matches_damage_component(
    role: ModelFragmentPartRole,
    component: VehicleDamageComponent,
) -> bool {
    match component {
        VehicleDamageComponent::Glass => role == ModelFragmentPartRole::Glass,
        VehicleDamageComponent::Light => role == ModelFragmentPartRole::Light,
        VehicleDamageComponent::Door => role == ModelFragmentPartRole::Door,
        VehicleDamageComponent::Bonnet => role == ModelFragmentPartRole::Bonnet,
        VehicleDamageComponent::Boot => role == ModelFragmentPartRole::Boot,
        VehicleDamageComponent::Wheel(_) => role == ModelFragmentPartRole::Wheel,
        VehicleDamageComponent::Body => role == ModelFragmentPartRole::Body,
        _ => true,
    }
}

fn vehicle_damage_surface_priority(role: ModelFragmentPartRole) -> u8 {
    match role {
        ModelFragmentPartRole::Glass => 4,
        ModelFragmentPartRole::Light => 3,
        ModelFragmentPartRole::Body => 0,
        _ => 1,
    }
}

/// Persisting manifolds report support impulses each frame. Only a rising impact
/// within a contact episode adds damage; separation opens a new episode.
fn collision_episode_damage(previous: Option<(f64, f32)>, seconds: f64, impulse: f32) -> f32 {
    previous
        .filter(|(time, _)| seconds - *time <= 0.18)
        .map_or(impulse, |(_, peak)| (impulse - peak).max(0.0))
}

#[cfg(test)]
mod collision_episode_tests {
    use super::*;
    #[test]
    fn sustained_contact_is_not_repeated_impact_damage() {
        assert_eq!(
            collision_episode_damage(Some((1.0, 1000.0)), 1.01, 1000.0),
            0.0
        );
        assert_eq!(
            collision_episode_damage(Some((1.0, 1000.0)), 1.01, 1200.0),
            200.0
        );
        assert_eq!(
            collision_episode_damage(Some((1.0, 1000.0)), 1.3, 1200.0),
            1200.0
        );
    }
    #[test]
    fn glass_wins_only_a_surface_tie_and_explicit_body_damage_stays_on_body() {
        assert!(
            vehicle_damage_surface_priority(ModelFragmentPartRole::Glass)
                > vehicle_damage_surface_priority(ModelFragmentPartRole::Door)
        );
        assert!(!vehicle_role_matches_damage_component(
            ModelFragmentPartRole::Door,
            VehicleDamageComponent::Glass
        ));
        assert!(vehicle_role_matches_damage_component(
            ModelFragmentPartRole::Body,
            VehicleDamageComponent::Body
        ));
    }
}
