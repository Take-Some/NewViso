use super::*;

impl EngineApplication {
    pub(super) fn publish_input_events(&self, input: &InputSnapshot) -> Result<(), String> {
        for code in event_codes(&input.state, "/keys/pressed") {
            host::publish_event_json(
                event_topic::KEY_PRESSED,
                "engine.input",
                json!({"code": code}),
            )?;
        }
        for code in event_codes(&input.state, "/keys/released") {
            host::publish_event_json(
                event_topic::KEY_RELEASED,
                "engine.input",
                json!({"code": code}),
            )?;
        }
        for button in event_codes(&input.state, "/mouse/pressed") {
            host::publish_event_json(
                event_topic::MOUSE_BUTTON_PRESSED,
                "engine.input",
                json!({"button": button}),
            )?;
        }
        for button in event_codes(&input.state, "/mouse/released") {
            host::publish_event_json(
                event_topic::MOUSE_BUTTON_RELEASED,
                "engine.input",
                json!({"button": button}),
            )?;
        }

        let delta_x = event_number(&input.state, "/mouse/delta/x");
        let delta_y = event_number(&input.state, "/mouse/delta/y");
        if delta_x != 0.0 || delta_y != 0.0 {
            host::publish_event_json(
                event_topic::MOUSE_MOVED,
                "engine.input",
                json!({
                    "delta": [delta_x, delta_y],
                    "position": [
                        event_number(&input.state, "/mouse/pos/x"),
                        event_number(&input.state, "/mouse/pos/y")
                    ]
                }),
            )?;
        }

        let wheel_x = event_number(&input.state, "/mouse/wheel/x");
        let wheel_y = event_number(&input.state, "/mouse/wheel/y");
        if wheel_x != 0.0 || wheel_y != 0.0 {
            host::publish_event_json(
                event_topic::MOUSE_WHEEL,
                "engine.input",
                json!({"delta": [wheel_x, wheel_y]}),
            )?;
        }

        if !input.text.is_empty() {
            host::publish_event_json(
                event_topic::TEXT_INPUT,
                "engine.input",
                json!({"text": input.text}),
            )?;
        }
        if !input.ime_commit.is_empty() {
            host::publish_event_json(
                event_topic::IME_COMMIT,
                "engine.input",
                json!({"text": input.ime_commit}),
            )?;
        }

        if let Some(gamepads) = input.state.get("gamepads").and_then(Value::as_object) {
            for (gamepad_id, gamepad) in gamepads {
                for button in gamepad
                    .get("buttons_pressed")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_u64)
                {
                    host::publish_event_json(
                        event_topic::GAMEPAD_BUTTON_PRESSED,
                        "engine.input",
                        json!({
                            "gamepad_id": gamepad_id,
                            "button": button
                        }),
                    )?;
                }
                for button in gamepad
                    .get("buttons_released")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_u64)
                {
                    host::publish_event_json(
                        event_topic::GAMEPAD_BUTTON_RELEASED,
                        "engine.input",
                        json!({
                            "gamepad_id": gamepad_id,
                            "button": button
                        }),
                    )?;
                }
            }
        }

        Ok(())
    }
}

fn event_codes(state: &Value, pointer: &str) -> Vec<u64> {
    state
        .pointer(pointer)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_u64)
        .collect()
}

fn event_number(state: &Value, pointer: &str) -> f64 {
    state
        .pointer(pointer)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_codes_extract_transition_codes() {
        let state = json!({"keys":{"pressed":[17, 32, "bad"]}});
        assert_eq!(event_codes(&state, "/keys/pressed"), vec![17, 32]);
    }

    #[test]
    fn event_number_is_fail_closed_for_non_finite_or_missing_values() {
        let state = json!({"mouse":{"delta":{"x":12.5}}});
        assert_eq!(event_number(&state, "/mouse/delta/x"), 12.5);
        assert_eq!(event_number(&state, "/mouse/delta/y"), 0.0);
    }
}
