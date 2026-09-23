use newviso_host as host;
use serde_json::{json, Value};

const INPUT_SERVICE: &str = "engine.input";

#[derive(Clone, Debug)]
pub struct InputSnapshot {
    pub state: Value,
    pub text: String,
    pub ime_commit: String,
}

impl InputSnapshot {
    pub fn sample() -> Result<Self, String> {
        let state_bytes = host::call_service(INPUT_SERVICE, "state_json", &[])?;
        let state: Value = serde_json::from_slice(&state_bytes)
            .map_err(|error| format!("engine.input/state_json returned invalid JSON: {error}"))?;

        let text_bytes = host::call_service(INPUT_SERVICE, "text_take_json", &[])?;
        let text_json: Value = serde_json::from_slice(&text_bytes).map_err(|error| {
            format!("engine.input/text_take_json returned invalid JSON: {error}")
        })?;
        let text = text_json
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();

        let ime_bytes = host::call_service(INPUT_SERVICE, "ime_commit_take_json", &[])?;
        let ime_json: Value = serde_json::from_slice(&ime_bytes).map_err(|error| {
            format!("engine.input/ime_commit_take_json returned invalid JSON: {error}")
        })?;
        let ime_commit = ime_json
            .get("ime_commit")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();

        Ok(Self {
            state,
            text,
            ime_commit,
        })
    }

    pub fn ui_input_frame(&self) -> Value {
        let keys = self.state.get("keys").cloned().unwrap_or(Value::Null);
        let mouse = self.state.get("mouse").cloned().unwrap_or(Value::Null);
        let text_state = self.state.get("text").cloned().unwrap_or(Value::Null);
        let gamepads = self
            .state
            .get("gamepads")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();

        let mut gamepad_buttons = serde_json::Map::new();
        let mut gamepad_buttons_pressed = Vec::<Value>::new();
        let mut gamepad_buttons_released = Vec::<Value>::new();
        let mut gamepad_axes = serde_json::Map::new();
        let mut gamepad_connected = 0usize;

        for (_id, pad) in gamepads {
            if pad
                .get("connected")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                gamepad_connected += 1;
            }
            if let Some(buttons) = pad.get("buttons").and_then(Value::as_object) {
                for (key, value) in buttons {
                    gamepad_buttons.insert(key.clone(), value.clone());
                }
            }
            if let Some(pressed) = pad.get("buttons_pressed").and_then(Value::as_array) {
                gamepad_buttons_pressed.extend(pressed.iter().cloned());
            }
            if let Some(released) = pad.get("buttons_released").and_then(Value::as_array) {
                gamepad_buttons_released.extend(released.iter().cloned());
            }
            if let Some(axes) = pad.get("axes").and_then(Value::as_object) {
                for (key, value) in axes {
                    gamepad_axes.insert(key.clone(), value.clone());
                }
            }
        }

        let edit_ops = text_state
            .get("edit_ops")
            .and_then(Value::as_array)
            .map(|ops| {
                ops.iter()
                    .filter_map(Value::as_str)
                    .map(|op| {
                        json!({
                            "kind": op,
                            "text": "",
                            "source": "engine.input"
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        json!({
            "keys_down": keys.get("down").cloned().unwrap_or_else(|| json!([])),
            "keys_pressed": keys.get("pressed").cloned().unwrap_or_else(|| json!([])),
            "keys_released": keys.get("released").cloned().unwrap_or_else(|| json!([])),
            "mouse_pos": [
                mouse.pointer("/pos/x").and_then(Value::as_f64).unwrap_or(0.0),
                mouse.pointer("/pos/y").and_then(Value::as_f64).unwrap_or(0.0)
            ],
            "mouse_delta": [
                mouse.pointer("/delta/x").and_then(Value::as_f64).unwrap_or(0.0),
                mouse.pointer("/delta/y").and_then(Value::as_f64).unwrap_or(0.0)
            ],
            "mouse_wheel": [
                mouse.pointer("/wheel/x").and_then(Value::as_f64).unwrap_or(0.0),
                mouse.pointer("/wheel/y").and_then(Value::as_f64).unwrap_or(0.0)
            ],
            "mouse_down": mouse.get("down").cloned().unwrap_or_else(|| json!([])),
            "mouse_pressed": mouse.get("pressed").cloned().unwrap_or_else(|| json!([])),
            "mouse_released": mouse.get("released").cloned().unwrap_or_else(|| json!([])),
            "text": self.text,
            "ime_preedit": text_state.get("ime_preedit").cloned().unwrap_or(Value::String(String::new())),
            "ime_commit": self.ime_commit,
            "text_edit_ops": edit_ops,
            "gamepad_buttons": Value::Object(gamepad_buttons),
            "gamepad_buttons_pressed": gamepad_buttons_pressed,
            "gamepad_buttons_released": gamepad_buttons_released,
            "gamepad_axes": Value::Object(gamepad_axes),
            "gamepad_connected": gamepad_connected
        })
    }

    pub fn key_down(&self, key: u64) -> bool {
        self.contains_code("/keys/down", key)
    }

    pub fn key_pressed(&self, key: u64) -> bool {
        self.contains_code("/keys/pressed", key)
    }

    pub fn mouse_button_pressed(&self, button: u64) -> bool {
        self.contains_code("/mouse/pressed", button)
    }

    pub fn mouse_delta(&self) -> [f32; 2] {
        [self.number("/mouse/delta/x"), self.number("/mouse/delta/y")]
    }

    pub fn mouse_wheel_y(&self) -> f32 {
        self.number("/mouse/wheel/y")
    }

    fn number(&self, path: &str) -> f32 {
        self.state
            .pointer(path)
            .and_then(Value::as_f64)
            .filter(|n| n.is_finite())
            .unwrap_or(0.0) as f32
    }

    fn contains_code(&self, path: &str, code: u64) -> bool {
        self.state
            .pointer(path)
            .and_then(Value::as_array)
            .is_some_and(|items| items.iter().any(|v| v.as_u64() == Some(code)))
    }

    pub fn mouse_button_down(&self, button: u64) -> bool {
        self.state
            .pointer("/mouse/down")
            .and_then(Value::as_array)
            .is_some_and(|buttons| {
                buttons
                    .iter()
                    .filter_map(Value::as_u64)
                    .any(|value| value == button)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compass_numeric_snapshot_is_decoded_without_resampling() {
        let input = InputSnapshot {
            state: json!({
                "keys":{"down":[41,60],"pressed":[114]},
                "mouse":{"down":[1],"pressed":[1],"delta":{"x":12.0,"y":-4.0}}
            }),
            text: String::new(),
            ime_commit: String::new(),
        };
        assert!(input.key_down(41));
        assert!(input.key_down(60));
        assert!(!input.key_down(37));
        assert!(input.key_pressed(114));
        assert!(input.mouse_button_pressed(1));
        assert_eq!(input.mouse_delta(), [12.0, -4.0]);
    }
}
