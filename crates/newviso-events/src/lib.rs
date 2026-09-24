use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub const EVENT_SCHEMA_V1: &str = "newviso.event.v1";

pub mod topic {
    pub const RUNTIME_STARTED: &str = "engine.runtime.started";
    pub const RUNTIME_FRAME_BEGIN: &str = "engine.runtime.frame.begin";
    pub const RUNTIME_FRAME_END: &str = "engine.runtime.frame.end";
    pub const RUNTIME_SHUTDOWN: &str = "engine.runtime.shutdown";

    pub const WINDOW_FOCUS_CHANGED: &str = "engine.platform.window.focus.changed";
    pub const CURSOR_CAPTURE_CHANGED: &str = "engine.platform.cursor.capture.changed";

    pub const KEY_PRESSED: &str = "engine.input.key.pressed";
    pub const KEY_RELEASED: &str = "engine.input.key.released";
    pub const MOUSE_BUTTON_PRESSED: &str = "engine.input.mouse.button.pressed";
    pub const MOUSE_BUTTON_RELEASED: &str = "engine.input.mouse.button.released";
    pub const MOUSE_MOVED: &str = "engine.input.mouse.moved";
    pub const MOUSE_WHEEL: &str = "engine.input.mouse.wheel";
    pub const TEXT_INPUT: &str = "engine.input.text";
    pub const IME_COMMIT: &str = "engine.input.ime.commit";
    pub const GAMEPAD_BUTTON_PRESSED: &str = "engine.input.gamepad.button.pressed";
    pub const GAMEPAD_BUTTON_RELEASED: &str = "engine.input.gamepad.button.released";

    pub const SCRIPT_QUEUE_DROPPED: &str = "engine.scripting.events.dropped";
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventPhase {
    Before,
    After,
    #[default]
    Observe,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EventEnvelope {
    pub schema: String,
    pub sequence: u64,
    pub topic: String,
    pub source: String,
    #[serde(default)]
    pub phase: EventPhase,
    #[serde(default)]
    pub cancelable: bool,
    #[serde(default)]
    pub payload: Value,
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
}

impl EventEnvelope {
    pub fn new(
        sequence: u64,
        topic: impl Into<String>,
        source: impl Into<String>,
        payload: Value,
    ) -> Result<Self, String> {
        Ok(Self {
            schema: EVENT_SCHEMA_V1.to_owned(),
            sequence,
            topic: normalize_topic(&topic.into())?,
            source: normalize_source(&source.into())?,
            phase: EventPhase::Observe,
            cancelable: false,
            payload,
            metadata: BTreeMap::new(),
        })
    }

    pub fn with_phase(mut self, phase: EventPhase) -> Self {
        self.phase = phase;
        self
    }

    pub fn cancelable(mut self, cancelable: bool) -> Self {
        self.cancelable = cancelable;
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    pub fn script_echo_enabled(&self) -> bool {
        self.metadata
            .get("script_echo")
            .and_then(Value::as_bool)
            .unwrap_or(true)
    }
}

pub fn normalize_topic(topic: &str) -> Result<String, String> {
    let topic = topic.trim().to_ascii_lowercase();
    if topic.is_empty() {
        return Err("event topic is empty".to_owned());
    }
    if topic.len() > 192 {
        return Err(format!("event topic is too long: {} bytes", topic.len()));
    }
    if topic.starts_with('.') || topic.ends_with('.') || topic.contains("..") {
        return Err(format!(
            "event topic '{topic}' has an invalid segment layout"
        ));
    }
    if !topic
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
    {
        return Err(format!(
            "event topic '{topic}' contains unsupported characters"
        ));
    }
    Ok(topic)
}

pub fn normalize_source(source: &str) -> Result<String, String> {
    let source = source.trim();
    if source.is_empty() {
        return Err("event source is empty".to_owned());
    }
    if source.len() > 128 {
        return Err(format!("event source is too long: {} bytes", source.len()));
    }
    Ok(source.to_owned())
}

pub fn decode_host_event(topic: &str, bytes: &[u8]) -> EventEnvelope {
    if let Ok(envelope) = serde_json::from_slice::<EventEnvelope>(bytes) {
        if envelope.schema == EVENT_SCHEMA_V1 {
            return envelope;
        }
    }

    let payload = serde_json::from_slice::<Value>(bytes).unwrap_or_else(|_| {
        json!({
            "opaque": true,
            "byte_length": bytes.len()
        })
    });

    EventEnvelope {
        schema: EVENT_SCHEMA_V1.to_owned(),
        sequence: 0,
        topic: normalize_topic(topic).unwrap_or_else(|_| "engine.event.invalid".to_owned()),
        source: "legacy.host-event".to_owned(),
        phase: EventPhase::Observe,
        cancelable: false,
        payload,
        metadata: BTreeMap::from([("legacy_transport".to_owned(), Value::Bool(true))]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_is_canonicalized() {
        assert_eq!(
            normalize_topic(" Game.Player.Jump ").unwrap(),
            "game.player.jump"
        );
    }

    #[test]
    fn invalid_topic_is_rejected() {
        assert!(normalize_topic("game..jump").is_err());
        assert!(normalize_topic("game player jump").is_err());
    }

    #[test]
    fn event_wire_round_trips() {
        let event =
            EventEnvelope::new(42, "game.player.jump", "game.script", json!({"speed": 6.0}))
                .unwrap()
                .cancelable(true)
                .with_phase(EventPhase::Before);
        let bytes = serde_json::to_vec(&event).unwrap();
        assert_eq!(decode_host_event("ignored", &bytes), event);
    }

    #[test]
    fn legacy_json_event_is_preserved() {
        let event = decode_host_event("engine.legacy", br#"{"ok":true}"#);
        assert_eq!(event.topic, "engine.legacy");
        assert_eq!(event.payload["ok"], true);
        assert_eq!(event.sequence, 0);
    }
}
