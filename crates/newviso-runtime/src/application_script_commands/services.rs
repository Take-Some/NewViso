use super::*;

impl EngineApplication {
    pub(super) fn apply_services_command(
        &mut self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<(), String> {
        match op {
            "runtime.configure" => {
                let patch = command.get("settings").ok_or_else(|| {
                    format!("script command[{index}] runtime.configure requires settings")
                })?;
                self.configure_runtime(patch)?;
            }
            "console.write" => {
                let message = command
                    .get("message")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        format!("script command[{index}] console.write requires string 'message'")
                    })?;
                let level = command
                    .get("level")
                    .and_then(Value::as_str)
                    .unwrap_or("info")
                    .trim()
                    .to_ascii_lowercase();
                let raw_target = command
                    .get("target")
                    .and_then(Value::as_str)
                    .unwrap_or("script")
                    .trim()
                    .to_ascii_lowercase();
                let clean_target = raw_target
                    .chars()
                    .map(|ch| {
                        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | ':' | '-') {
                            ch
                        } else {
                            '_'
                        }
                    })
                    .take(96)
                    .collect::<String>();
                let target = if clean_target.is_empty() || clean_target == "script" {
                    "script".to_owned()
                } else {
                    format!("script.{clean_target}")
                };
                let bounded_message = message.chars().take(16_384).collect::<String>();
                match level.as_str() {
                    "trace" => host::trace(target, bounded_message),
                    "debug" => host::debug(target, bounded_message),
                    "info" => host::info(target, bounded_message),
                    "warn" | "warning" => host::warn(target, bounded_message),
                    "error" => host::error(target, bounded_message),
                    other => {
                        return Err(format!(
                            "script command[{index}] console.write has invalid level '{other}'"
                        ));
                    }
                }
            }
            "events.emit" => {
                let topic = command
                    .get("topic")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        format!("script command[{index}] events.emit requires string 'topic'")
                    })?;
                let source = command
                    .get("source")
                    .and_then(Value::as_str)
                    .unwrap_or("project.script");
                let payload = command.get("payload").cloned().unwrap_or(Value::Null);
                let cancelable = command
                    .get("cancelable")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let phase = match command
                    .get("phase")
                    .and_then(Value::as_str)
                    .unwrap_or("observe")
                {
                    "before" => EventPhase::Before,
                    "after" => EventPhase::After,
                    "observe" => EventPhase::Observe,
                    other => {
                        return Err(format!(
                            "script command[{index}] events.emit has invalid phase '{other}'"
                        ));
                    }
                };

                let mut metadata = command
                    .get("metadata")
                    .and_then(Value::as_object)
                    .map(|values| {
                        values
                            .iter()
                            .map(|(key, value)| (key.clone(), value.clone()))
                            .collect::<BTreeMap<_, _>>()
                    })
                    .unwrap_or_default();
                metadata
                    .entry("script_echo".to_owned())
                    .or_insert(Value::Bool(false));

                host::publish_event_json_with(topic, source, payload, phase, cancelable, metadata)?;
            }
            "platform.cursor.set" => {
                let captured = command
                    .get("captured")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| {
                        format!("script command[{index}] platform.cursor.set requires 'captured'")
                    })?;
                let previous = self.cursor_captured;
                self.cursor_captured = captured && self.window_focused;
                if previous != self.cursor_captured {
                    host::publish_event_json(
                        event_topic::CURSOR_CAPTURE_CHANGED,
                        "newviso.runtime",
                        json!({
                            "captured": self.cursor_captured,
                            "previous": previous
                        }),
                    )?;
                }
            }
            _ => return Err(unsupported_command(op, index)),
        }
        Ok(())
    }
}
