pub use newviso_physics_api::*;

use newviso_host as host;

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

    pub fn diagnostics(&self) -> Result<PhysicsBackendInfo, String> {
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
        let response = host::call_json(
            ENGINE_PHYSICS_SERVICE_ID,
            PHYSICS_SERVICE_METHOD_INVOKE,
            &request,
        )?;
        serde_json::from_value(response)
            .map_err(|error| format!("engine.physics response decode failed: {error}"))
    }
}

fn problem_message(problem: PhysicsProblemDetails) -> String {
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
