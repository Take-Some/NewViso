use super::*;

impl EngineApplication {
    pub(super) fn publish_vehicle_script_event(&self, event: &VehicleEvent) -> Result<(), String> {
        let mut payload =
            vehicle_event_payload(event, self.scene.entity_transform_values(event.entity));
        if let Some(part_index) = event.part_index {
            if let Some(part) = self
                .vehicle_presentations
                .get(&event.entity)
                .and_then(|binding| binding.parts.iter().find(|part| part.index == part_index))
            {
                payload["part"] = Value::String(part.name.clone());
            }
        }
        host::publish_event_json_with(
            event.kind.topic(),
            "engine.vehicle",
            payload,
            EventPhase::After,
            false,
            BTreeMap::new(),
        )
        .map(|_| ())
    }
}

pub(super) fn vehicle_event_payload(
    event: &VehicleEvent,
    transform: Option<([f32; 3], [f32; 3], [f32; 3])>,
) -> Value {
    let mut payload = serde_json::to_value(event).expect("vehicle event must serialize");
    // The vehicle sequence is a u64 namespace, above JS's exact integer range.
    payload["sequence"] = Value::String(event.sequence.to_string());
    payload["entity_key"] = Value::String(event.entity.to_string());
    payload["actor_entity_key"] = event
        .actor_entity
        .map(|id| Value::String(id.to_string()))
        .unwrap_or(Value::Null);
    payload["other_entity_key"] = event
        .other_entity
        .map(|id| Value::String(id.to_string()))
        .unwrap_or(Value::Null);
    payload["position_space"] = Value::String(
        if event.local_space {
            "vehicle_local"
        } else {
            "world"
        }
        .into(),
    );
    if event.local_space {
        if let Some((position, rotation, scale)) = transform {
            if let Some(point) = event.position {
                payload["local_position"] = json!(point);
                payload["position"] = json!(vehicle_local_point(position, rotation, scale, point));
            }
            if let Some(normal) = event.normal {
                payload["local_normal"] = json!(normal);
                let local = std::array::from_fn(|axis| {
                    normal[axis]
                        / if scale[axis].abs() > 1.0e-5 {
                            scale[axis]
                        } else {
                            1.0
                        }
                });
                let world = rotate_euler_xyz(local, rotation);
                let length = world.iter().map(|v| v * v).sum::<f32>().sqrt();
                payload["normal"] =
                    json!(world.map(|v| if length > 1.0e-5 { v / length } else { 0.0 }));
            }
            payload["position_space"] = Value::String("world".into());
            payload["local_space"] = Value::Bool(false);
        }
    }
    payload
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vehicle_events_preserve_exact_sequence_and_transform_damage_points() {
        let mut event =
            VehicleEvent::new(u64::MAX - 2, u64::MAX - 3, VehicleEventKind::GlassBroken);
        event.actor_entity = Some(u64::MAX - 4);
        event.other_entity = Some(u64::MAX - 5);
        event.local_space = true;
        event.position = Some([1.0, 2.0, 3.0]);
        event.normal = Some([1.0, 1.0, 0.0]);
        let payload =
            vehicle_event_payload(&event, Some(([10.0, 0.0, 0.0], [0.0; 3], [2.0, 1.0, 3.0])));
        assert_eq!(payload["sequence"], (u64::MAX - 2).to_string());
        assert_eq!(payload["entity_key"], (u64::MAX - 3).to_string());
        assert_eq!(payload["actor_entity_key"], (u64::MAX - 4).to_string());
        assert_eq!(payload["other_entity_key"], (u64::MAX - 5).to_string());
        assert_eq!(payload["position"], json!([12.0, 2.0, 9.0]));
        assert_eq!(payload["local_position"], json!([1.0, 2.0, 3.0]));
        assert_eq!(payload["position_space"], "world");
        let normal = payload["normal"].as_array().unwrap();
        assert!((normal[0].as_f64().unwrap() - 0.4472136).abs() < 1.0e-5);
        assert!((normal[1].as_f64().unwrap() - 0.8944272).abs() < 1.0e-5);
    }
}
