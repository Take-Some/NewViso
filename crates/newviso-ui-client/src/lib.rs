use newviso_host as host;
use serde_json::{json, Value};

const UI_SERVICE: &str = "engine.ui";
const SURFACE_NODE_METHOD: &str = "ui.surface_node_v1";
const DRAW_FRAME_METHOD: &str = "draw_frame_v1";
const DISPATCH_INPUT_METHOD: &str = "ui.dispatch_input_v1";

#[derive(Clone, Debug)]
pub struct UiFrameOutput {
    pub draw_list: Value,
    pub input_capture: Value,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UiClient;

impl UiClient {
    pub const fn new() -> Self {
        Self
    }

    pub fn publish_surface(&self, surface: &Value) -> Result<(), String> {
        let payload = serde_json::to_vec(surface)
            .map_err(|error| format!("encode UI surface failed: {error}"))?;
        host::call_service(UI_SERVICE, SURFACE_NODE_METHOD, &payload)?;
        Ok(())
    }

    pub fn dispatch_input(
        &self,
        frame_index: u64,
        input: &Value,
        surface_size_px: [u32; 2],
        pixels_per_point: f32,
    ) -> Result<Value, String> {
        let request = json!({
            "version": 1,
            "frame_index": frame_index,
            "surface_id": "",
            "input": input,
            "surface_size_px": surface_size_px,
            "pixels_per_point": pixels_per_point.max(0.0001),
            "event": "",
            "payload": null
        });

        host::call_json(UI_SERVICE, DISPATCH_INPUT_METHOD, &request)
    }

    pub fn frame(
        &self,
        frame_index: u64,
        dt_sec: f32,
        surface_size_px: [u32; 2],
        pixels_per_point: f32,
    ) -> Result<UiFrameOutput, String> {
        let pixels_per_point = pixels_per_point.max(0.0001);
        let dt_sec = dt_sec.max(0.0);
        let request = json!({
            "version": 1,
            "frame_index": frame_index,
            "dt_sec": dt_sec,
            "surface_size_px": surface_size_px,
            "pixels_per_point": pixels_per_point,
            "frame_input": {
                "version": 1,
                "frame_index": frame_index,
                "now_ms": 0,
                "dt_sec": dt_sec,
                "viewport_px": surface_size_px,
                "pixels_per_point": pixels_per_point,
                "render_surface_ids": [],
                "diagnostics_flags": []
            },
            "diagnostics_flags": [],
            "render_surface_ids": []
        });

        let response = host::call_json(UI_SERVICE, DRAW_FRAME_METHOD, &request)?;
        let draw_list = response
            .get("draw_list")
            .cloned()
            .ok_or_else(|| format!("UI frame response has no draw_list: {response}"))?;
        let input_capture = response
            .get("input_capture")
            .cloned()
            .unwrap_or(Value::Null);

        Ok(UiFrameOutput {
            draw_list,
            input_capture,
        })
    }
}
