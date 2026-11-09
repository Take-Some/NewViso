//! Native mechanism ownership. Scripts request actions; joints carry all effort.
use super::*;
use newviso_doors as dm;
mod control;
mod events;
mod geometry;
mod impact;
mod interaction;
mod persistence;
mod planning;
mod retention;
#[cfg(test)]
mod tests;
use geometry::DoorGeometry;
use newviso_navigation::TraversalGate;
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct DoorDefinition {
    pub entity: u64,
    pub joint_ids: [u64; 2],
    pub mechanism: dm::MechanismConfig,
    #[serde(default)]
    pub handle_local: Option<[f32; 3]>,
    #[serde(default)]
    pub latch_local: Option<[f32; 3]>,
    #[serde(default)]
    pub persistent_id: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default = "navigation_enabled")]
    pub navigation: bool,
}
fn navigation_enabled() -> bool {
    true
}
#[derive(Clone, Debug)]
pub(crate) struct DoorActor {
    pub id: String,
    pub body: u64,
    pub radius: f32,
    pub height: f32,
    pub access: BTreeSet<u64>,
}
#[derive(Clone, Debug, serde::Serialize)]
struct PassageRequest {
    actor: String,
    body: u64,
    radius: f32,
    side: f32,
    ticket: u64,
    seen: f64,
    started: f64,
    active_since: Option<f64>,
    progressed: f64,
    last_position: [f32; 3],
}
impl PassageRequest {
    fn new(actor: &DoorActor, side: f32, ticket: u64, now: f64, position: [f32; 3]) -> Self {
        Self {
            actor: actor.id.clone(),
            body: actor.body,
            radius: actor.radius,
            side,
            ticket,
            seen: now,
            started: now,
            active_since: None,
            progressed: now,
            last_position: position,
        }
    }
}
#[derive(Debug)]
struct DoorBinding {
    definition: DoorDefinition,
    mechanism: dm::Mechanism,
    geometry: Option<DoorGeometry>,
    baseline: Option<[PhysicsJointSnapshot; 2]>,
    queue: std::collections::VecDeque<PassageRequest>,
    angle: f32,
    angular_speed: f32,
    occupied: bool,
    ready: bool,
    published: Option<events::PublishedDoor>,
    retention_joint: u64,
    contact: impact::DoorContactWork,
}
#[derive(Debug, Default)]
pub(crate) struct PhysicalDoors {
    entries: BTreeMap<u64, DoorBinding>,
    events: std::collections::VecDeque<Value>,
    sequence: u64,
    next_ticket: u64,
    saved: BTreeMap<String, persistence::SavedDoor>,
}
impl PhysicalDoors {
    fn event(
        &mut self,
        entity: u64,
        actor: Option<&str>,
        kind: &str,
        result: Option<dm::DoorResult>,
        position: [f32; 3],
    ) {
        self.sequence = self.sequence.wrapping_add(1).max(1);
        self.events.push_back(
            json!({"sequence":self.sequence,"entity":entity,"entity_key":entity.to_string(),
   "actor_id":actor,"kind":kind,"result":result,"position":position}),
        );
        while self.events.len() > 128 {
            self.events.pop_front();
        }
    }
}
impl PhysicsRuntime {
    pub(crate) fn configure_door(&mut self, value: &Value) -> Result<(), String> {
        if !self.supports_articulated_drives() {
            return Err("door mechanisms require native articulation".into());
        }
        let mut spec: DoorDefinition = serde_json::from_value(value.clone())
            .map_err(|e| format!("invalid door definition: {e}"))?;
        spec.mechanism.validate()?;
        if spec
            .handle_local
            .iter()
            .chain(&spec.latch_local)
            .flatten()
            .any(|v| !v.is_finite())
        {
            return Err("invalid authored door hardware position".into());
        }
        if spec.display_name.is_empty() {
            spec.display_name = "Door".into();
        }
        if spec.persistent_id.is_empty() {
            spec.persistent_id = spec.entity.to_string();
        }
        if spec.persistent_id.len() > 256 || spec.display_name.len() > 256 {
            return Err("door names are too long".into());
        }
        if spec.joint_ids[0] == spec.joint_ids[1] || spec.joint_ids.contains(&0) {
            return Err("door requires two distinct hinge joints".into());
        }
        if let Some(old) = self.doors.entries.get(&spec.entity) {
            return if old.definition == spec {
                Ok(())
            } else {
                Err("remove the existing mechanism before reconfiguration".into())
            };
        }
        if self.doors.entries.values().any(|d| {
            d.definition
                .joint_ids
                .iter()
                .any(|j| spec.joint_ids.contains(j))
        }) {
            return Err("hinge joints already belong to another door mechanism".into());
        }
        if self
            .doors
            .entries
            .values()
            .any(|b| b.definition.persistent_id == spec.persistent_id)
        {
            return Err("duplicate persistent door identity".into());
        }
        let retention_joint =
            (1..=u64::MAX)
                .rev()
                .find(|id| {
                    !self.joints.contains_key(id)
                        && !spec.joint_ids.contains(id)
                        && !self.doors.entries.values().any(|d| {
                            d.retention_joint == *id || d.definition.joint_ids.contains(id)
                        })
                })
                .ok_or("no free joint identity for door hardware")?;
        let mechanism = dm::Mechanism::new(spec.mechanism.clone())?;
        self.doors.entries.insert(
            spec.entity,
            DoorBinding {
                definition: spec,
                mechanism,
                geometry: None,
                baseline: None,
                queue: Default::default(),
                angle: 0.0,
                angular_speed: 0.0,
                occupied: false,
                ready: false,
                published: None,
                retention_joint,
                contact: Default::default(),
            },
        );
        Ok(())
    }
    pub(crate) fn remove_door(&mut self, entity: u64) {
        if let Some(b) = self.doors.entries.remove(&entity) {
            self.joints.remove(&b.retention_joint);
            self.stash_door(&b);
            if let Some(base) = b.baseline {
                for joint in base {
                    if self.joints.contains_key(&joint.id) {
                        self.joints.insert(joint.id, joint);
                    }
                }
            }
            self.doors.event(
                entity,
                None,
                "removed",
                None,
                b.geometry.map_or([0.0; 3], |g| g.center),
            );
        }
    }
    pub(crate) fn deny_door_interaction(&mut self, entity: u64, actor: &str, action: &str) {
        let p = self
            .doors
            .entries
            .get(&entity)
            .and_then(|d| d.geometry.as_ref())
            .map_or([0.0; 3], |g| g.center);
        self.doors
            .action(entity, actor, action, dm::DoorResult::Denied, p);
    }
    pub(crate) fn door_key_items(&self) -> Vec<(u64, Option<String>)> {
        self.doors
            .entries
            .iter()
            .map(|(id, b)| (*id, b.mechanism.config.key_item.clone()))
            .collect()
    }
    pub(crate) fn doors_snapshot(&self) -> Value {
        json!({"revision":self.doors.sequence,"events":self.doors.events,"doors":self.doors.entries.iter().map(|(id,b)|{
   let handle=b.geometry.as_ref().and_then(|g|self.bodies.get(id).map(|p|g.handle(p.position,p.rotation)));
   json!({"entity":id,"entity_key":id.to_string(),"display_name":b.definition.display_name,"ready":b.ready,"state":b.mechanism.state,
    "config":b.mechanism.config,"angle":b.angle,"angular_speed":b.angular_speed,"occupied":b.occupied,
    "geometry":b.geometry,"handle_position":handle,"queue":b.queue})
  }).collect::<Vec<_>>()})
    }
    pub(crate) fn door_restore(
        &mut self,
        entity: u64,
        state: dm::MechanismState,
    ) -> Result<(), String> {
        let b = self
            .doors
            .entries
            .get_mut(&entity)
            .ok_or("unknown door mechanism")?;
        if !b.ready {
            return Err("door restore requires resident hinge geometry".into());
        }
        let o = dm::Observation {
            seconds: self.fixed_seconds,
            angle: b.angle,
            angular_speed: b.angular_speed,
            edge_radius: b.geometry.as_ref().unwrap().width(),
            occupied: b.occupied,
            ..Default::default()
        };
        b.mechanism.restore(state, o)
    }
}
