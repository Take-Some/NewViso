use super::*;

/// A derived index; the persisted event map remains authoritative.
#[derive(Clone, Debug)]
pub(super) struct EventDeadline {
    due_world_seconds: f64,
    id: String,
}

impl PartialEq for EventDeadline {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for EventDeadline {}
impl PartialOrd for EventDeadline {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for EventDeadline {
    fn cmp(&self, other: &Self) -> Ordering {
        self.due_world_seconds
            .total_cmp(&other.due_world_seconds)
            .then_with(|| self.id.cmp(&other.id))
    }
}

impl LivingWorldRuntime {
    fn ensure_scheduled_event_index(&mut self) {
        if !self.scheduled_event_index_ready {
            self.scheduled_event_deadlines = self
                .scheduled_events
                .iter()
                .map(|(id, event)| EventDeadline {
                    due_world_seconds: event.due_world_seconds,
                    id: id.clone(),
                })
                .collect();
            self.scheduled_event_index_ready = true;
        }
    }

    pub fn schedule_event(&mut self, mut desc: WorldScheduledEventDesc) -> Result<String, String> {
        if desc.id.trim().is_empty() {
            desc.id = format!("world.event.{:016x}", self.next_event_id);
            self.next_event_id = self.next_event_id.wrapping_add(1).max(1);
        }
        desc.validate()?;

        self.ensure_scheduled_event_index();
        let id = desc.id.clone();
        let due_world_seconds = self.clock.world_seconds + desc.delay_seconds;
        if let Some(previous) = self.scheduled_events.insert(
            id.clone(),
            ScheduledWorldEvent {
                expires_world_seconds: due_world_seconds + desc.ttl_seconds,
                due_world_seconds,
                desc,
            },
        ) {
            self.scheduled_event_deadlines.remove(&EventDeadline {
                due_world_seconds: previous.due_world_seconds,
                id: id.clone(),
            });
        }
        self.scheduled_event_deadlines.insert(EventDeadline {
            due_world_seconds,
            id: id.clone(),
        });
        Ok(id)
    }

    pub fn cancel_event(&mut self, id: &str) {
        self.ensure_scheduled_event_index();
        if let Some(event) = self.scheduled_events.remove(id.trim()) {
            self.scheduled_event_deadlines.remove(&EventDeadline {
                due_world_seconds: event.due_world_seconds,
                id: event.desc.id,
            });
        }
    }

    pub(super) fn tick_scheduled_events(&mut self, step: WorldStep) {
        self.ensure_scheduled_event_index();
        let mut due_ids = Vec::new();
        while self
            .scheduled_event_deadlines
            .first()
            .is_some_and(|deadline| deadline.due_world_seconds <= step.world_seconds + f64::EPSILON)
        {
            due_ids.push(
                self.scheduled_event_deadlines
                    .pop_first()
                    .expect("deadline checked above")
                    .id,
            );
        }
        // Preserve the original ID order of reality-history creation within a
        // fixed step. Frame deliveries are sorted by priority afterwards.
        due_ids.sort_unstable();

        for id in due_ids {
            let Some(event) = self.scheduled_events.remove(&id) else {
                continue;
            };
            if step.world_seconds > event.expires_world_seconds {
                self.dropped_expired_events = self.dropped_expired_events.wrapping_add(1);
                continue;
            }
            let delivered_desc = event.desc.clone();
            let reality_event_id = self.push_reality_event(
                WorldRealityEventDesc {
                    id: String::new(),
                    kind: delivered_desc.kind,
                    source: delivered_desc.source,
                    cause: delivered_desc.cause.or(Some(delivered_desc.id)),
                    participants: Vec::new(),
                    position: delivered_desc.position,
                    importance: (delivered_desc.priority.max(0) as f32) + 1.0,
                    tags: delivered_desc.tags,
                    payload: delivered_desc.payload,
                },
                step.world_seconds,
            );
            self.frame_events.push(WorldEventDelivery {
                due_world_seconds: event.due_world_seconds,
                delivered_world_seconds: step.world_seconds,
                reality_event_id,
                desc: event.desc,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(id: &str, delay_seconds: f64) -> WorldScheduledEventDesc {
        WorldScheduledEventDesc {
            id: id.to_owned(),
            kind: "test.delivery".to_owned(),
            source: "test.scheduler".to_owned(),
            cause: None,
            delay_seconds,
            ttl_seconds: 10.0,
            priority: 1,
            position: None,
            tags: Vec::new(),
            payload: json!({"id":id}),
        }
    }

    #[test]
    fn replacement_and_cancellation_remove_stale_deadlines() {
        let mut world = LivingWorldRuntime::default();
        world.schedule_event(event("replace", 0.0)).unwrap();
        world.schedule_event(event("replace", 1.0)).unwrap();
        world.schedule_event(event("cancel", 0.0)).unwrap();
        world.cancel_event("cancel");
        assert_eq!(world.scheduled_event_deadlines.len(), 1);
        world.tick_frame(0.1, &[]);
        assert!(world.frame_events.is_empty());
        world.tick_frame(1.0, &[]);
        // The world clock processes at most eight 50 ms steps per frame.
        // Preserve and drain its backlog before expecting a one-second event.
        assert!(world.frame_events.is_empty());
        world.tick_frame(0.0, &[]);
        assert!(world.frame_events.is_empty());
        world.tick_frame(0.0, &[]);
        assert_eq!(world.frame_events.len(), 1);
        assert_eq!(world.frame_events[0].desc.id, "replace");
        assert!(world.scheduled_event_deadlines.is_empty());
    }

    #[test]
    fn deadlines_rebuild_after_checkpoint_and_direct_deserialization() {
        let mut world = LivingWorldRuntime::default();
        world.schedule_event(event("pending", 0.1)).unwrap();
        let checkpoint = world.checkpoint().unwrap();
        assert!(checkpoint["state"]
            .get("scheduled_event_deadlines")
            .is_none());
        let mut restored = LivingWorldRuntime::from_checkpoint(checkpoint.clone()).unwrap();
        let mut deserialized: LivingWorldRuntime =
            serde_json::from_value(checkpoint["state"].clone()).unwrap();
        for candidate in [&mut restored, &mut deserialized] {
            candidate.tick_frame(0.2, &[]);
            assert_eq!(candidate.frame_events.len(), 1);
            assert_eq!(candidate.frame_events[0].desc.id, "pending");
            candidate.tick_frame(0.2, &[]);
            assert!(candidate.frame_events.is_empty());
        }
    }

    #[test]
    fn deadline_order_preserves_reality_sequence_and_delivery_priority() {
        let mut world = LivingWorldRuntime::default();
        world.schedule_event(event("a.later", 0.02)).unwrap();
        let mut earlier = event("z.earlier", 0.01);
        earlier.priority = 10;
        world.schedule_event(earlier).unwrap();
        world.tick_frame(0.1, &[]);
        assert_eq!(world.frame_reality_events.len(), 2);
        assert_eq!(
            world.frame_reality_events[0].desc.cause.as_deref(),
            Some("a.later")
        );
        assert_eq!(
            world.frame_reality_events[1].desc.cause.as_deref(),
            Some("z.earlier")
        );
        assert_eq!(world.frame_events[0].desc.id, "z.earlier");
        assert_eq!(world.frame_events[1].desc.id, "a.later");
    }

    #[test]
    fn expired_deadlines_are_dropped_and_clock_rewind_keeps_pending_events() {
        let mut world = LivingWorldRuntime::default();
        let mut expired = event("expired", 0.0);
        expired.ttl_seconds = 0.001;
        world.schedule_event(expired).unwrap();
        world.schedule_event(event("future", 1.0)).unwrap();
        world.tick_frame(0.1, &[]);
        assert_eq!(world.dropped_expired_events, 1);
        world.set_world_seconds(0.0).unwrap();
        world.tick_frame(0.1, &[]);
        assert!(world.frame_events.is_empty());
        world.set_world_seconds(1.0).unwrap();
        world.tick_frame(0.1, &[]);
        assert_eq!(world.frame_events[0].desc.id, "future");
    }
}
