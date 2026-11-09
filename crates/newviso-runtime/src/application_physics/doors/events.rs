use super::*;
#[derive(Clone, Copy, Debug)]
pub(super) struct PublishedDoor {
    locked: bool,
    latched: bool,
    bolt_broken: bool,
    latch_broken: bool,
    closed: bool,
    jammed: bool,
}
impl PhysicalDoors {
    pub(super) fn action(
        &mut self,
        entity: u64,
        actor: &str,
        action: &str,
        result: dm::DoorResult,
        position: [f32; 3],
    ) {
        self.event(entity, Some(actor), "action_result", Some(result), position);
        self.events.back_mut().unwrap()["action"] = json!(action);
    }
}
impl PhysicsRuntime {
    pub(super) fn publish_door_state(&mut self, id: u64, b: &mut DoorBinding, o: dm::Observation) {
        let s = &b.mechanism.state;
        let next = PublishedDoor {
            locked: s.locked,
            latched: s.latch_engaged,
            bolt_broken: s.bolt_broken,
            latch_broken: s.latch_broken,
            closed: b.mechanism.closed(o),
            jammed: o.seconds < s.jammed_until,
        };
        let p = b.geometry.as_ref().unwrap().center;
        if let Some(old) = b.published {
            for (changed, kind) in [
                (
                    old.locked != next.locked,
                    if next.locked { "locked" } else { "unlocked" },
                ),
                (
                    old.latched != next.latched,
                    if next.latched { "latched" } else { "unlatched" },
                ),
                (!old.bolt_broken && next.bolt_broken, "bolt_broken"),
                (!old.latch_broken && next.latch_broken, "latch_broken"),
                (
                    old.closed != next.closed,
                    if next.closed { "closed" } else { "opened" },
                ),
                (!old.jammed && next.jammed, "obstructed"),
            ] {
                if changed {
                    self.doors.event(id, None, kind, None, p);
                }
            }
        } else {
            self.doors.event(id, None, "state_initialized", None, p);
        }
        b.published = Some(next);
    }
}
