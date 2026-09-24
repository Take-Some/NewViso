use newviso_host as host;
use serde::{Deserialize, Serialize};

pub const ENGINE_PHYSICS_SERVICE: &str = "engine.physics";
pub const PHYSICS_INVOKE_METHOD: &str = "invoke_json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhysicsBodyKind {
    Static,
    Dynamic,
    Kinematic,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum CollisionShape {
    Box { half_extents: [f32; 3] },
    Sphere { radius: f32 },
    Capsule { radius: f32, half_height: f32 },
    Cylinder { radius: f32, half_height: f32 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PhysicsMaterial {
    pub friction: f32,
    pub restitution: f32,
    pub density: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PhysicsBodyFlags {
    pub is_trigger: bool,
    pub participates_in_queries: bool,
    pub casts_contacts: bool,
    #[serde(default)]
    pub continuous_collision: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhysicsBodySnapshot {
    pub entity: u64,
    pub kind: PhysicsBodyKind,
    pub shape: CollisionShape,
    pub flags: PhysicsBodyFlags,
    pub material: PhysicsMaterial,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub linear_velocity: [f32; 3],
    #[serde(default)]
    pub angular_velocity: [f32; 3],
    #[serde(default)]
    pub linear_damping: Option<f32>,
    #[serde(default)]
    pub angular_damping: Option<f32>,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum PhysicsCommandKind {
    SetBodyPose {
        entity: u64,
        position: [f32; 3],
        rotation: [f32; 4],
    },
    SetLinearVelocity {
        entity: u64,
        velocity: [f32; 3],
    },
    SetAngularVelocity {
        entity: u64,
        velocity: [f32; 3],
    },
    ApplyImpulse {
        entity: u64,
        impulse: [f32; 3],
        point: [f32; 3],
    },
    DestroyBody {
        entity: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhysicsCommand {
    pub seq: u64,
    pub kind: PhysicsCommandKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum PhysicsQueryKind {
    Ray {
        origin: [f32; 3],
        dir: [f32; 3],
        max_t: f32,
    },
    BallisticRay {
        origin: [f32; 3],
        dir: [f32; 3],
        max_t: f32,
        max_hits: u16,
        collide_back_faces: bool,
    },
    Sphere {
        center: [f32; 3],
        radius: f32,
    },
    Aabb {
        min: [f32; 3],
        max: [f32; 3],
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhysicsQuery {
    pub seq: u64,
    #[serde(default)]
    pub ignore_entity: Option<u64>,
    pub kind: PhysicsQueryKind,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PhysicsFrameInput {
    pub frame_index: u64,
    pub fixed_tick: u64,
    pub dt: f32,
    pub gravity: f32,
    pub contact_skin: f32,
    #[serde(default)]
    pub bodies: Vec<PhysicsBodySnapshot>,
    #[serde(default)]
    pub colliders: Vec<serde_json::Value>,
    #[serde(default)]
    pub commands: Vec<PhysicsCommand>,
    #[serde(default)]
    pub queries: Vec<PhysicsQuery>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhysicsBodyPoseUpdate {
    pub entity: u64,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhysicsBodyVelocityUpdate {
    pub entity: u64,
    pub linear_velocity: [f32; 3],
    #[serde(default)]
    pub angular_velocity: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhysicsBodyActivityUpdate {
    pub entity: u64,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhysicsQueryHit {
    pub seq: u64,
    pub entity: u64,
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub distance: f32,
    #[serde(default)]
    pub subshape_id: u32,
    #[serde(default)]
    pub hit_index: u16,
    #[serde(default)]
    pub back_face: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PhysicsContactMaterialPair {
    #[serde(default)]
    pub a: Option<PhysicsMaterial>,
    #[serde(default)]
    pub b: Option<PhysicsMaterial>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhysicsContactEvent {
    pub a: u64,
    pub b: u64,
    pub point: [f32; 3],
    pub normal: [f32; 3],
    pub impulse: f32,
    #[serde(default)]
    pub relative_velocity: Option<[f32; 3]>,
    #[serde(default)]
    pub materials: PhysicsContactMaterialPair,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum PhysicsEvent {
    ContactBegin(PhysicsContactEvent),
    ContactPersist(PhysicsContactEvent),
    ContactEnd { a: u64, b: u64 },
    BodyCreated { entity: u64 },
    BodyDestroyed { entity: u64 },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PhysicsStepReport {
    pub fixed_tick: u64,
    pub dt: f32,
    pub substeps: u32,
    pub active_bodies: usize,
    pub static_bodies: usize,
    pub dynamic_bodies: usize,
    pub contacts: usize,
    pub commands_applied: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PhysicsFrameOutput {
    pub fixed_tick: u64,
    #[serde(default)]
    pub pose_updates: Vec<PhysicsBodyPoseUpdate>,
    #[serde(default)]
    pub velocity_updates: Vec<PhysicsBodyVelocityUpdate>,
    #[serde(default)]
    pub activity_updates: Vec<PhysicsBodyActivityUpdate>,
    #[serde(default)]
    pub events: Vec<PhysicsEvent>,
    #[serde(default)]
    pub query_hits: Vec<PhysicsQueryHit>,
    pub report: PhysicsStepReport,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhysicsApiVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl Default for PhysicsApiVersion {
    fn default() -> Self {
        Self {
            major: 1,
            minor: 0,
            patch: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhysicsFeature {
    StaticColliders,
    DynamicBodies,
    KinematicBodies,
    TriggerBodies,
    Contacts,
    Queries,
    DeterministicReplay,
    NativeBackend,
    HeightfieldColliders,
    MeshColliders,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PhysicsCapabilityNegotiationRequest {
    pub preferred_version: PhysicsApiVersion,
    #[serde(default)]
    pub required_features: Vec<PhysicsFeature>,
    #[serde(default)]
    pub optional_features: Vec<PhysicsFeature>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PhysicsCapabilityNegotiationResponse {
    pub accepted_version: PhysicsApiVersion,
    pub backend_version: PhysicsApiVersion,
    pub ok: bool,
    pub enabled_features: Vec<PhysicsFeature>,
    pub missing_required_features: Vec<PhysicsFeature>,
    #[serde(default)]
    pub notices: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PhysicsProblem {
    pub code: String,
    pub title: String,
    pub detail: String,
    pub backend: Option<String>,
    pub phase: Option<String>,
    #[serde(default)]
    pub recoverable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum PhysicsServiceRequest {
    Negotiate(PhysicsCapabilityNegotiationRequest),
    StepFrame(PhysicsFrameInput),
    DiagnosticsSnapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum PhysicsServiceResponse {
    Unit,
    Negotiation(PhysicsCapabilityNegotiationResponse),
    FrameOutput(PhysicsFrameOutput),
    BackendInfo(serde_json::Value),
    DiagnosticsSnapshot(serde_json::Value),
    Problem(PhysicsProblem),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PhysicsClient;

impl PhysicsClient {
    pub const fn new() -> Self {
        Self
    }

    pub fn negotiate(
        &self,
        required_features: Vec<PhysicsFeature>,
        optional_features: Vec<PhysicsFeature>,
    ) -> Result<PhysicsCapabilityNegotiationResponse, String> {
        let response = self.invoke(PhysicsServiceRequest::Negotiate(
            PhysicsCapabilityNegotiationRequest {
                preferred_version: PhysicsApiVersion::default(),
                required_features,
                optional_features,
            },
        ))?;
        match response {
            PhysicsServiceResponse::Negotiation(result) if result.ok => Ok(result),
            PhysicsServiceResponse::Negotiation(result) => Err(format!(
                "engine.physics negotiation rejected missing_required={:?}",
                result.missing_required_features
            )),
            PhysicsServiceResponse::Problem(problem) => Err(problem_message(problem)),
            other => Err(format!(
                "engine.physics negotiate returned unexpected response: {other:?}"
            )),
        }
    }

    pub fn step_frame(&self, input: PhysicsFrameInput) -> Result<PhysicsFrameOutput, String> {
        validate_frame(&input)?;
        match self.invoke(PhysicsServiceRequest::StepFrame(input))? {
            PhysicsServiceResponse::FrameOutput(output) => Ok(output),
            PhysicsServiceResponse::Problem(problem) => Err(problem_message(problem)),
            other => Err(format!(
                "engine.physics step returned unexpected response: {other:?}"
            )),
        }
    }

    pub fn diagnostics(&self) -> Result<serde_json::Value, String> {
        match self.invoke(PhysicsServiceRequest::DiagnosticsSnapshot)? {
            PhysicsServiceResponse::DiagnosticsSnapshot(value)
            | PhysicsServiceResponse::BackendInfo(value) => Ok(value),
            PhysicsServiceResponse::Problem(problem) => Err(problem_message(problem)),
            other => Err(format!(
                "engine.physics diagnostics returned unexpected response: {other:?}"
            )),
        }
    }

    fn invoke(&self, request: PhysicsServiceRequest) -> Result<PhysicsServiceResponse, String> {
        let request = serde_json::to_value(request)
            .map_err(|error| format!("physics request encode failed: {error}"))?;
        let response = host::call_json(ENGINE_PHYSICS_SERVICE, PHYSICS_INVOKE_METHOD, &request)?;
        serde_json::from_value(response)
            .map_err(|error| format!("engine.physics response decode failed: {error}"))
    }
}

fn validate_frame(input: &PhysicsFrameInput) -> Result<(), String> {
    if !input.dt.is_finite() || input.dt <= 0.0 || input.dt > 0.25 {
        return Err(format!("physics frame dt out of range: {}", input.dt));
    }
    if !input.gravity.is_finite() || input.gravity < 0.0 {
        return Err(format!("physics gravity invalid: {}", input.gravity));
    }
    if !input.contact_skin.is_finite() || input.contact_skin < 0.0 {
        return Err(format!(
            "physics contact skin invalid: {}",
            input.contact_skin
        ));
    }
    for body in &input.bodies {
        if body
            .position
            .iter()
            .chain(&body.linear_velocity)
            .chain(&body.angular_velocity)
            .any(|value| !value.is_finite())
        {
            return Err(format!(
                "physics body {} contains non-finite state",
                body.entity
            ));
        }
    }
    Ok(())
}

fn problem_message(problem: PhysicsProblem) -> String {
    format!(
        "engine.physics problem code='{}' title='{}' detail='{}' backend={} phase={}",
        problem.code,
        problem.title,
        problem.detail,
        problem.backend.as_deref().unwrap_or("<unknown>"),
        problem.phase.as_deref().unwrap_or("<unknown>")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_frame_wire_shape_matches_replaceable_physics_contract() {
        let request = PhysicsServiceRequest::StepFrame(PhysicsFrameInput {
            frame_index: 1,
            fixed_tick: 2,
            dt: 1.0 / 60.0,
            gravity: 9.81,
            contact_skin: 0.035,
            bodies: vec![PhysicsBodySnapshot {
                entity: 7,
                kind: PhysicsBodyKind::Dynamic,
                shape: CollisionShape::Capsule {
                    radius: 0.3,
                    half_height: 0.6,
                },
                flags: PhysicsBodyFlags {
                    is_trigger: false,
                    participates_in_queries: true,
                    casts_contacts: true,
                    continuous_collision: true,
                },
                material: PhysicsMaterial {
                    friction: 0.2,
                    restitution: 0.0,
                    density: 70.0,
                },
                position: [0.0, 0.9, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                linear_velocity: [0.0, 0.0, 0.0],
                angular_velocity: [0.0, 0.0, 0.0],
                linear_damping: Some(0.0),
                angular_damping: Some(8.0),
                bounds_min: [-0.3, 0.0, -0.3],
                bounds_max: [0.3, 1.8, 0.3],
            }],
            colliders: Vec::new(),
            commands: Vec::new(),
            queries: Vec::new(),
        });
        let value = serde_json::to_value(request).unwrap();
        assert_eq!(
            value
                .pointer("/StepFrame/bodies/0/kind")
                .and_then(|v| v.as_str()),
            Some("Dynamic")
        );
        let radius = value
            .pointer("/StepFrame/bodies/0/shape/Capsule/radius")
            .and_then(|v| v.as_f64())
            .expect("capsule radius");
        assert!((radius - 0.3).abs() < 1.0e-6);
    }
}
