use super::*;
use crate::application_characters::NativeCharacterBody;
use crate::application_physics::{PhysicsDamageContact, PhysicsDamageKind};

const PED_MASS_KG: f32 = 70.0;
const VEHICLE_DAMAGE_INTERVAL: f64 = 0.4;
// CPed::KillPedWithCar uses CTaskFallAndGetUp(..., 2.0f).
const VEHICLE_FALL_SECONDS: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VehiclePedHit {
    Bump,
    Rammed,
    RunOver,
}

impl VehiclePedHit {
    fn as_str(self) -> &'static str {
        match self {
            Self::Bump => "bump",
            Self::Rammed => "rammed",
            Self::RunOver => "run_over",
        }
    }
    fn weapon_type(self) -> Option<&'static str> {
        match self {
            Self::Bump => None,
            Self::Rammed => Some("RAMMEDBYVEHICLE"),
            Self::RunOver => Some("RUNOVERBYVEHICLE"),
        }
    }
}

#[derive(Clone, Debug)]
struct VehiclePedResponse {
    source: u64,
    kind: VehiclePedHit,
    elapsed: f32,
    direction: [f32; 3],
    wheel_load: f32,
}

#[derive(Clone, Debug)]
pub(crate) struct PedBody {
    pub entity: u64,
    pub center_offset: f32,
    radius: f32,
    half_height: f32,
    pub recovery_seconds: f32,
    pub death_heading: f32,
    was_dead: bool,
    reset_requested: bool,
    vehicle_response: Option<VehiclePedResponse>,
    reaction: Option<animation::PedReactionState>,
    animation_generation: u64,
    ragdoll: Option<ragdoll::RagdollState>,
    hit_reaction: Option<wounds::HitReaction>,
}

#[derive(Debug, Default)]
pub(crate) struct PedPhysicsRuntime {
    pub bodies: BTreeMap<String, PedBody>,
    weapon_sources: BTreeMap<u64, String>,
    last_collision_damage: BTreeMap<(String, u64), f64>,
    last_wheel_tick: u64,
}

fn capsule_body_command(
    entity: u64,
    center: [f32; 3],
    velocity: [f32; 3],
    radius: f32,
    half_height: f32,
) -> Value {
    let volume = std::f32::consts::PI * radius * radius * (2.0 * half_height + 4.0 * radius / 3.0);
    json!({"entity": entity, "body_kind": "dynamic", "position": center,
        "shape": {"kind": "capsule", "radius": radius, "half_height": half_height},
        "rotation": [0, 0, 0, 1], "linear_velocity": velocity,
        "density": PED_MASS_KG / volume, "friction": 0.65, "restitution": 0.0,
        "linear_damping": 0.0, "angular_damping": 12.0,
        "is_trigger": false, "continuous_collision": true,
        "participates_in_queries": true, "casts_contacts": true})
}

fn vehicle_impact_damage(impulse: f32, closing_speed: f32) -> f32 {
    let speed = (impulse.max(0.0) / PED_MASS_KG).max(closing_speed.max(0.0));
    ((speed - 2.5).max(0.0) * 30.0).min(1000.0)
}

fn vehicle_can_knock_down(impulse: f32, vehicle_speed: f32, ped_speed: f32) -> bool {
    // The original AI activation speed gate concerns the vehicle, rather than
    // a pedestrian walking into a parked car. Resting load fails the force gate.
    let minimum_speed = if ped_speed < 0.1 { 0.2 } else { 0.5 };
    vehicle_speed >= minimum_speed && impulse >= PED_MASS_KG * 0.5
}

fn vehicle_approach_speed(velocity: [f32; 3], impact_direction: [f32; 3]) -> f32 {
    (velocity[0] * impact_direction[0] + velocity[2] * impact_direction[2]).max(0.0)
}

fn classify_vehicle_hit(
    local: [f32; 3],
    min: [f32; 3],
    max: [f32; 3],
    fallen: bool,
) -> VehiclePedHit {
    let half_width = min[0].abs().max(max[0].abs());
    let half_length = min[2].abs().max(max[2].abs());
    let sideswipe = local[0].abs() > 0.99 * half_width && local[2].abs() < 0.85 * half_length;
    if fallen || local[1] <= 0.1 || sideswipe {
        VehiclePedHit::RunOver
    } else {
        VehiclePedHit::Rammed
    }
}

fn wheel_crush_damage(load: f32, dt: f32) -> f32 {
    // Excess suspension load on the native body causes sustained crushing,
    // including a stationary wheel. Normal floor support is never a wheel hit.
    ((load - PED_MASS_KG * 9.81).max(0.0) / PED_MASS_KG * 2.0 * dt).min(1000.0)
}

pub(crate) mod animation;
mod damage;
mod lifecycle;
mod presentation;
pub(crate) mod ragdoll;
mod recovery;
mod vehicle_hits;
mod wounds;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ped_capsule_has_real_mass_contacts_queries_and_ccd() {
        let body = capsule_body_command(12, [1.0, 0.9, 3.0], [0.0; 3], 0.3, 0.6);
        let volume = std::f32::consts::PI * 0.3 * 0.3 * (1.2 + 4.0 * 0.3 / 3.0);
        assert!((body["density"].as_f64().unwrap() as f32 * volume - 70.0).abs() < 0.001);
        assert_eq!(body["body_kind"], "dynamic");
        assert_eq!(body["is_trigger"], false);
        assert_eq!(body["casts_contacts"], true);
        assert_eq!(body["participates_in_queries"], true);
        assert_eq!(body["continuous_collision"], true);
    }

    #[test]
    fn vehicle_damage_ignores_resting_support_and_scales_with_real_impact() {
        assert_eq!(vehicle_impact_damage(6.0, 0.0), 0.0);
        assert_eq!(vehicle_impact_damage(100.0, 0.5), 0.0);
        assert!(vehicle_impact_damage(350.0, 0.0) > 0.0);
        assert!(vehicle_impact_damage(700.0, 12.0) >= 200.0);
        assert!(vehicle_impact_damage(0.0, 8.0) > vehicle_impact_damage(0.0, 4.0));
    }

    #[test]
    fn walking_into_a_parked_car_does_not_become_a_vehicle_ram() {
        assert_eq!(
            vehicle_approach_speed([0.0, 0.0, 2.0], [0.0, 0.0, -1.0]),
            0.0
        );
        assert_eq!(
            vehicle_approach_speed([0.0, 0.0, -6.0], [0.0, 0.0, -1.0]),
            6.0
        );
        assert!(!vehicle_can_knock_down(350.0, 0.0, 2.0));
        assert!(!vehicle_can_knock_down(350.0, 0.1, 0.0));
        assert!(!vehicle_can_knock_down(8.0, 6.0, 0.0));
        assert!(vehicle_can_knock_down(350.0, 6.0, 0.0));
        assert!(vehicle_can_knock_down(350.0, 6.0, 2.0));
    }

    #[test]
    fn vehicle_ram_and_underbody_runover_have_distinct_responses() {
        let min = [-1.0, -0.5, -2.0];
        let max = [1.0, 0.8, 2.0];
        assert_eq!(
            classify_vehicle_hit([0.0, 0.4, -2.2], min, max, false),
            VehiclePedHit::Rammed
        );
        assert_eq!(
            classify_vehicle_hit([0.0, -0.1, -2.2], min, max, false),
            VehiclePedHit::RunOver
        );
        assert_eq!(
            classify_vehicle_hit([1.2, 0.4, 0.0], min, max, false),
            VehiclePedHit::RunOver
        );
        assert_eq!(
            classify_vehicle_hit([0.0, 0.4, -2.2], min, max, true),
            VehiclePedHit::RunOver
        );
        assert_ne!(
            VehiclePedHit::Rammed.weapon_type(),
            VehiclePedHit::RunOver.weapon_type()
        );
    }

    #[test]
    fn wheel_load_crushes_even_at_rest_and_damage_is_timestep_independent() {
        assert_eq!(wheel_crush_damage(100.0, 0.1), 0.0);
        assert!(wheel_crush_damage(5000.0, 0.1) > 0.0);
        assert!(
            (wheel_crush_damage(5000.0, 0.1) - wheel_crush_damage(5000.0, 0.02) * 5.0).abs()
                < 0.001
        );
    }
}
