use serde::{Deserialize, Serialize};

pub const ENGINE_PHYSICS_SERVICE_ID: &str = "engine.physics";
pub const PHYSICS_SERVICE_ID: &str = "physics.api";
pub const PHYSICS_BACKEND_CAPABILITY_ID: &str = "physics.backend";
pub const PHYSICS_PROVIDER_ABI_ID: &str = "newviso.physics.provider.v1";

pub const PHYSICS_SERVICE_METHOD_INFO: &str = "info_json";
pub const PHYSICS_SERVICE_METHOD_INVOKE: &str = "invoke_json";
pub const PHYSICS_SERVICE_METHOD_SHUTDOWN_V1: &str = "shutdown_v1";

pub type PhysicsEntityKey = u64;
pub type PhysicsVec3 = [f32; 3];
pub type PhysicsQuat = [f32; 4];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhysicsBodyKind {
    Static,
    Dynamic,
    Kinematic,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum CollisionShape {
    Box { half_extents: PhysicsVec3 },
    Sphere { radius: f32 },
    Capsule { radius: f32, half_height: f32 },
    Cylinder { radius: f32, half_height: f32 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeshCollider {
    #[serde(default)]
    pub vertices: Vec<PhysicsVec3>,
    #[serde(default)]
    pub triangles: Vec<[u32; 3]>,
    #[serde(default)]
    pub material_indices: Vec<u32>,
}

impl MeshCollider {
    pub fn validate(&self) -> Result<(), String> {
        if self.vertices.is_empty() || self.triangles.is_empty() {
            return Err("physics mesh collider is empty".to_owned());
        }
        if self
            .vertices
            .iter()
            .flatten()
            .any(|value| !value.is_finite())
        {
            return Err("physics mesh collider contains non-finite vertex".to_owned());
        }
        let vertex_count = self.vertices.len() as u64;
        if self
            .triangles
            .iter()
            .flatten()
            .any(|index| u64::from(*index) >= vertex_count)
        {
            return Err(format!(
                "physics mesh collider index exceeds vertex count {}",
                self.vertices.len()
            ));
        }
        if !self.material_indices.is_empty() && self.material_indices.len() != self.triangles.len()
        {
            return Err(format!(
                "physics mesh collider material count {} does not match triangle count {}",
                self.material_indices.len(),
                self.triangles.len()
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HeightfieldCollider {
    pub sample_count_x: u32,
    pub sample_count_z: u32,
    pub spacing: [f32; 2],
    pub local_origin: PhysicsVec3,
    #[serde(default)]
    pub heights: Vec<f32>,
    pub min_height: f32,
    pub max_height: f32,
}

impl HeightfieldCollider {
    pub fn validate(&self) -> Result<(), String> {
        let nx = self.sample_count_x as usize;
        let nz = self.sample_count_z as usize;
        let expected = nx
            .checked_mul(nz)
            .ok_or_else(|| "physics heightfield dimensions overflow".to_owned())?;
        if nx < 2 || nz < 2 || self.heights.len() != expected {
            return Err(format!(
                "physics heightfield dimensions/data mismatch: {}x{} with {} heights",
                nx,
                nz,
                self.heights.len()
            ));
        }
        if self
            .spacing
            .iter()
            .chain(self.local_origin.iter())
            .chain(self.heights.iter())
            .chain([&self.min_height, &self.max_height])
            .any(|value| !value.is_finite())
        {
            return Err("physics heightfield contains non-finite values".to_owned());
        }
        if self.spacing[0] <= 0.0 || self.spacing[1] <= 0.0 {
            return Err("physics heightfield spacing must be positive".to_owned());
        }
        if self.min_height > self.max_height {
            return Err("physics heightfield min_height exceeds max_height".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PhysicsCollider {
    Mesh(MeshCollider),
    Heightfield(HeightfieldCollider),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhysicsFrameColliderSnapshot {
    pub entity: PhysicsEntityKey,
    pub collider: PhysicsCollider,
    pub flags: PhysicsBodyFlags,
    pub material: PhysicsMaterial,
    pub position: PhysicsVec3,
    pub rotation: PhysicsQuat,
    pub bounds_min: PhysicsVec3,
    pub bounds_max: PhysicsVec3,
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
    pub entity: PhysicsEntityKey,
    pub kind: PhysicsBodyKind,
    pub shape: CollisionShape,
    pub flags: PhysicsBodyFlags,
    pub material: PhysicsMaterial,
    pub position: PhysicsVec3,
    pub rotation: PhysicsQuat,
    pub linear_velocity: PhysicsVec3,
    #[serde(default)]
    pub angular_velocity: PhysicsVec3,
    #[serde(default)]
    pub linear_damping: Option<f32>,
    #[serde(default)]
    pub angular_damping: Option<f32>,
    pub bounds_min: PhysicsVec3,
    pub bounds_max: PhysicsVec3,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum PhysicsCommandKind {
    SetBodyPose {
        entity: PhysicsEntityKey,
        position: PhysicsVec3,
        rotation: PhysicsQuat,
    },
    SetLinearVelocity {
        entity: PhysicsEntityKey,
        velocity: PhysicsVec3,
    },
    SetAngularVelocity {
        entity: PhysicsEntityKey,
        velocity: PhysicsVec3,
    },
    ApplyImpulse {
        entity: PhysicsEntityKey,
        impulse: PhysicsVec3,
        point: PhysicsVec3,
    },
    DestroyBody {
        entity: PhysicsEntityKey,
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
        origin: PhysicsVec3,
        dir: PhysicsVec3,
        max_t: f32,
    },
    BallisticRay {
        origin: PhysicsVec3,
        dir: PhysicsVec3,
        max_t: f32,
        max_hits: u16,
        collide_back_faces: bool,
    },
    Sphere {
        center: PhysicsVec3,
        radius: f32,
    },
    Aabb {
        min: PhysicsVec3,
        max: PhysicsVec3,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhysicsQuery {
    pub seq: u64,
    #[serde(default)]
    pub ignore_entity: Option<PhysicsEntityKey>,
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
    pub colliders: Vec<PhysicsFrameColliderSnapshot>,
    #[serde(default)]
    pub commands: Vec<PhysicsCommand>,
    #[serde(default)]
    pub queries: Vec<PhysicsQuery>,
}

impl PhysicsFrameInput {
    pub fn empty(frame_index: u64, fixed_tick: u64, dt: f32) -> Self {
        Self {
            frame_index,
            fixed_tick,
            dt,
            gravity: 9.81,
            contact_skin: 0.035,
            bodies: Vec::new(),
            colliders: Vec::new(),
            commands: Vec::new(),
            queries: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhysicsBodyPoseUpdate {
    pub entity: PhysicsEntityKey,
    pub position: PhysicsVec3,
    pub rotation: PhysicsQuat,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhysicsBodyVelocityUpdate {
    pub entity: PhysicsEntityKey,
    pub linear_velocity: PhysicsVec3,
    #[serde(default)]
    pub angular_velocity: PhysicsVec3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhysicsBodyActivityUpdate {
    pub entity: PhysicsEntityKey,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhysicsQueryHit {
    pub seq: u64,
    pub entity: PhysicsEntityKey,
    pub position: PhysicsVec3,
    pub normal: PhysicsVec3,
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
    pub a: PhysicsEntityKey,
    pub b: PhysicsEntityKey,
    pub point: PhysicsVec3,
    pub normal: PhysicsVec3,
    pub impulse: f32,
    #[serde(default)]
    pub relative_velocity: Option<PhysicsVec3>,
    #[serde(default)]
    pub materials: PhysicsContactMaterialPair,
    #[serde(default)]
    pub surface_id: Option<u32>,
    #[serde(default)]
    pub surface_entity: Option<PhysicsEntityKey>,
    #[serde(default)]
    pub surface_triangle: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum PhysicsEvent {
    ContactBegin(PhysicsContactEvent),
    ContactPersist(PhysicsContactEvent),
    ContactEnd {
        a: PhysicsEntityKey,
        b: PhysicsEntityKey,
    },
    BodyCreated {
        entity: PhysicsEntityKey,
    },
    BodyDestroyed {
        entity: PhysicsEntityKey,
    },
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
            minor: 1,
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
    ContinuousCollision,
    AngularVelocity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhysicsBackendClass {
    Native,
    Software,
    Other,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhysicsLimits {
    pub max_bodies: u32,
    pub max_queries_per_frame: u32,
    pub max_substeps: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhysicsBackendCapabilities {
    pub backend_class: PhysicsBackendClass,
    #[serde(default)]
    pub features: Vec<PhysicsFeature>,
    pub limits: PhysicsLimits,
}

impl PhysicsBackendCapabilities {
    pub fn supports(&self, feature: PhysicsFeature) -> bool {
        self.features.contains(&feature)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhysicsBackendInfo {
    pub backend_id: String,
    pub backend_name: String,
    pub backend_version: String,
    #[serde(default)]
    pub debug_text: String,
    pub capabilities: PhysicsBackendCapabilities,
    #[serde(default)]
    pub protocol_version: PhysicsApiVersion,
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
pub struct PhysicsProblemDetails {
    pub code: String,
    pub title: String,
    pub detail: String,
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub phase: Option<String>,
    #[serde(default)]
    pub recoverable: bool,
}

impl PhysicsProblemDetails {
    pub fn new(
        code: impl Into<String>,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            title: title.into(),
            detail: detail.into(),
            backend: None,
            phase: None,
            recoverable: false,
        }
    }

    pub fn with_backend(mut self, backend: impl Into<String>) -> Self {
        self.backend = Some(backend.into());
        self
    }

    pub fn with_phase(mut self, phase: impl Into<String>) -> Self {
        self.phase = Some(phase.into());
        self
    }

    pub fn recoverable(mut self, recoverable: bool) -> Self {
        self.recoverable = recoverable;
        self
    }
}

pub type PhysicsProblem = PhysicsProblemDetails;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PhysicsServiceRequest {
    Negotiate(PhysicsCapabilityNegotiationRequest),
    StepFrame(PhysicsFrameInput),
    DiagnosticsSnapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PhysicsServiceResponse {
    Unit,
    Negotiation(PhysicsCapabilityNegotiationResponse),
    FrameOutput(PhysicsFrameOutput),
    BackendInfo(PhysicsBackendInfo),
    DiagnosticsSnapshot(PhysicsBackendInfo),
    Problem(PhysicsProblemDetails),
}

pub fn encode_json<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    serde_json::to_vec(value).map_err(|error| error.to_string())
}

pub fn decode_json<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, String> {
    serde_json::from_slice(bytes).map_err(|error| error.to_string())
}

pub fn validate_frame(input: &PhysicsFrameInput) -> Result<(), String> {
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
            .chain(body.rotation.iter())
            .chain(body.linear_velocity.iter())
            .chain(body.angular_velocity.iter())
            .chain(body.bounds_min.iter())
            .chain(body.bounds_max.iter())
            .any(|value| !value.is_finite())
        {
            return Err(format!(
                "physics body {} contains non-finite state",
                body.entity
            ));
        }
    }

    for snapshot in &input.colliders {
        if snapshot
            .position
            .iter()
            .chain(snapshot.rotation.iter())
            .chain(snapshot.bounds_min.iter())
            .chain(snapshot.bounds_max.iter())
            .any(|value| !value.is_finite())
        {
            return Err(format!(
                "physics collider {} contains non-finite transform/bounds",
                snapshot.entity
            ));
        }
        match &snapshot.collider {
            PhysicsCollider::Mesh(mesh) => mesh.validate()?,
            PhysicsCollider::Heightfield(heightfield) => heightfield.validate()?,
        }
    }
    Ok(())
}

// Compatibility aliases for providers that used the pre-NewViso DTO suffixes.
pub type PhysicsBodyKindDto = PhysicsBodyKind;
pub type CollisionShapeDto = CollisionShape;
pub type MeshColliderDto = MeshCollider;
pub type HeightfieldColliderDto = HeightfieldCollider;
pub type PhysicsColliderDto = PhysicsCollider;
pub type PhysicsMaterialDto = PhysicsMaterial;
pub type PhysicsBodyFlagsDto = PhysicsBodyFlags;
pub type PhysicsFrameBodySnapshot = PhysicsBodySnapshot;
pub type PhysicsCommandDto = PhysicsCommand;
pub type PhysicsCommandKindDto = PhysicsCommandKind;
pub type PhysicsQueryKindDto = PhysicsQueryKind;
pub type PhysicsQueryHitDto = PhysicsQueryHit;
pub type PhysicsContactMaterialPairDto = PhysicsContactMaterialPair;
pub type PhysicsContactEventDto = PhysicsContactEvent;
pub type PhysicsEventDto = PhysicsEvent;
pub type PhysicsStepReportDto = PhysicsStepReport;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_wire_contract_preserves_extended_body_state() {
        let input = PhysicsFrameInput {
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
                linear_velocity: [0.0; 3],
                angular_velocity: [1.0, 2.0, 3.0],
                linear_damping: Some(0.0),
                angular_damping: Some(8.0),
                bounds_min: [-0.3, 0.0, -0.3],
                bounds_max: [0.3, 1.8, 0.3],
            }],
            colliders: Vec::new(),
            commands: Vec::new(),
            queries: Vec::new(),
        };
        validate_frame(&input).unwrap();
        let encoded = encode_json(&PhysicsServiceRequest::StepFrame(input)).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(
            value
                .pointer("/StepFrame/bodies/0/kind")
                .and_then(|v| v.as_str()),
            Some("Dynamic")
        );
        assert_eq!(
            value
                .pointer("/StepFrame/bodies/0/angular_velocity/2")
                .and_then(|v| v.as_f64()),
            Some(3.0)
        );
    }

    #[test]
    fn heightfield_validation_matches_provider_expectations() {
        HeightfieldCollider {
            sample_count_x: 2,
            sample_count_z: 2,
            spacing: [1.0, 1.0],
            local_origin: [0.0; 3],
            heights: vec![0.0, 0.0, 1.0, 1.0],
            min_height: 0.0,
            max_height: 1.0,
        }
        .validate()
        .unwrap();
    }
}
