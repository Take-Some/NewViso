use super::*;

/// Intent only; the host refines movement against its navigation and collision world.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentTaskKind {
    Idle,
    Wait { duration_seconds: f32 },
    TravelToNode {
        #[serde(default)] start_node: Option<String>,
        destination_node: String,
        speed: f32,
        #[serde(default = "default_travel_mode")] mode: String,
        #[serde(default)] payload: Value,
    },
    GoToPosition {
        position: [f32; 3], speed: f32,
        #[serde(default = "default_stop_distance")] stop_distance: f32,
    },
    FollowActor {
        target_actor: String, speed: f32,
        #[serde(default = "default_follow_distance")] distance: f32,
        #[serde(default = "default_repath_seconds")] repath_seconds: f32,
    },
    Flee {
        threat_position: [f32; 3],
        #[serde(default)] threat_actor: Option<String>,
        speed: f32, safe_distance: f32, duration_seconds: f32,
    },
    Wander {
        #[serde(default)] center: Option<[f32; 3]>,
        radius: f32, speed: f32,
        #[serde(default = "default_pause_seconds")] pause_seconds: f32,
    },
    LookAt { position: [f32; 3], duration_seconds: f32 },
    PlayAnimation { clip_ref: String, duration_seconds: f32 },
    /// Complex tasks execute one child at a time and resume the interrupted child.
    Sequence { steps: Vec<AgentTaskKind>, #[serde(default)] repeat: bool },
    Combat {
        target_actor: String, speed: f32, range: f32,
        cooldown_seconds: f32, damage: f32,
        #[serde(default = "default_combat_duration")] duration_seconds: f32,
    },
}

pub(super) fn default_travel_mode() -> String { "walk".to_owned() }
fn default_stop_distance() -> f32 { 0.25 }
fn default_follow_distance() -> f32 { 2.0 }
fn default_repath_seconds() -> f32 { 0.5 }
fn default_pause_seconds() -> f32 { 1.0 }
fn default_combat_duration() -> f32 { 20.0 }

impl AgentTaskKind {
    pub(super) fn validate_at_depth(&self, depth: usize) -> Result<(), String> {
        if depth > 8 { return Err("agent sequence nesting exceeds 8 levels".into()); }
        let duration = |n: f32| finite_range("task duration", n, 0.0, 86_400.0);
        let speed = |n: f32| finite_range("task speed", n, 0.001, 10_000.0);
        match self {
            Self::Idle => Ok(()),
            Self::Wait { duration_seconds } => duration(*duration_seconds),
            Self::TravelToNode { start_node, destination_node, speed: s, mode, payload } => {
                if let Some(start) = start_node { validate_id("agent travel start", start)?; }
                validate_id("agent travel destination", destination_node)?;
                speed(*s)?; validate_id("agent travel mode", mode)?; validate_payload(payload)
            }
            Self::GoToPosition { position, speed: s, stop_distance } => {
                validate_position(*position)?; speed(*s)?;
                finite_range("stop distance", *stop_distance, 0.01, 1000.0)
            }
            Self::FollowActor { target_actor, speed: s, distance, repath_seconds } => {
                validate_id("follow target", target_actor)?; speed(*s)?;
                finite_range("follow distance", *distance, 0.1, 100_000.0)?;
                finite_range("repath interval", *repath_seconds, 0.05, 3600.0)
            }
            Self::Flee { threat_position, threat_actor, speed: s, safe_distance, duration_seconds } => {
                validate_position(*threat_position)?;
                if let Some(id) = threat_actor { validate_id("flee threat", id)?; }
                speed(*s)?; finite_range("flee distance", *safe_distance, 0.1, 100_000.0)?;
                duration(*duration_seconds)
            }
            Self::Wander { center, radius, speed: s, pause_seconds } => {
                if let Some(center) = center { validate_position(*center)?; }
                finite_range("wander radius", *radius, 0.1, 100_000.0)?;
                speed(*s)?; duration(*pause_seconds)
            }
            Self::LookAt { position, duration_seconds } => {
                validate_position(*position)?; duration(*duration_seconds)
            }
            Self::PlayAnimation { clip_ref, duration_seconds } => {
                if clip_ref.trim().is_empty() || clip_ref.len() > 512 {
                    return Err("animation clip reference must contain 1..512 bytes".into());
                }
                duration(*duration_seconds)
            }
            Self::Sequence { steps, .. } => {
                if steps.is_empty() || steps.len() > 64 { return Err("sequence requires 1..64 tasks".into()); }
                for step in steps { step.validate_at_depth(depth + 1)?; }
                Ok(())
            }
            Self::Combat { target_actor, speed: s, range, cooldown_seconds, damage, duration_seconds } => {
                validate_id("combat target", target_actor)?; speed(*s)?;
                finite_range("combat range", *range, 0.25, 10_000.0)?;
                finite_range("combat cooldown", *cooldown_seconds, 0.05, 3600.0)?;
                finite_range("combat damage", *damage, 0.0, 100_000.0)?;
                duration(*duration_seconds)
            }
        }
    }

    pub(super) fn owns_movement(&self) -> bool {
        match self {
            Self::TravelToNode { .. } | Self::GoToPosition { .. } | Self::FollowActor { .. }
            | Self::Flee { .. } | Self::Wander { .. } | Self::Combat { .. } => true,
            Self::Sequence { steps, .. } => steps.iter().any(Self::owns_movement),
            _ => false,
        }
    }
}

pub(super) fn finite_range(name: &str, n: f32, min: f32, max: f32) -> Result<(), String> {
    if n.is_finite() && (min..=max).contains(&n) { Ok(()) }
    else { Err(format!("{name} must be finite and in {min}..={max}")) }
}
pub(super) fn validate_position(p: [f32; 3]) -> Result<(), String> {
    if p.iter().all(|x| x.is_finite() && x.abs() <= 1.0e7) { Ok(()) }
    else { Err("task position is invalid".into()) }
}
