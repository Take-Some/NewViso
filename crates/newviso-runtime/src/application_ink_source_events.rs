use serde_json::{json, Value};
fn ink_event_body(event:&Value)->&Value {event.get("Data").unwrap_or(event)}
fn ink_event_time(event:&Value)->f64 {
    ink_event_body(event).get("startTime").and_then(Value::as_f64)
        .filter(|v|v.is_finite()&&*v>=0.0).unwrap_or(f64::INFINITY)
}
fn ink_event_string(value:&Value)->Option<&str> {
    value.as_str().or_else(||value.get("$value").and_then(Value::as_str))
}

fn apply_ink_visual_event(node:&mut Value,event:&Value,t:f64) {
    let body=ink_event_body(event);
    if t<ink_event_time(event) {return;}
    let Some(props)=node.get_mut("props") else {return;};
    match body.get("$type").and_then(Value::as_str) {
        Some("inkanimSetVisibilityEvent") => {
            let visible=body.get("isVisible").and_then(|v|
                v.as_bool().or_else(||v.as_i64().map(|n|n!=0)));
            if let Some(visible)=visible {props["visible"]=json!(visible);}
        }
        Some("inkanimChangeStateEvent") => {
            if let Some(name)=body.get("state").and_then(ink_event_string) {
                props["ink_state"]=json!(name);
            }
        }
        Some("inkanimChangeTexturePartEvent") => {
            if let Some(name)=body.get("imageTexturePartName").and_then(ink_event_string) {
                props["ink_part"]=json!(name);
                props["ink_part_requires_runtime_atlas_binding"]=json!(true);
            }
        }
        _ => {} // Controller/sound/video require one-shot adapters.
    }
}
