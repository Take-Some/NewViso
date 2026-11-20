//! Opt-in NewViso/Compass cross-toolchain ABI integration smoke.
//! Run with NORTHSTAR_COMPASS_SMOKE_DLL pointing to the built MSVC DLL.
use abi_stable::std_types::{RString, RVec};
use newviso_host as host;
use newviso_provider_runtime::RunningProvider;
use serde_json::{json, Value};

fn input_state() -> Value {
    let bytes = host::call_service("engine.input", "state_json", &[])
        .expect("NewViso engine.input state_json must be routed to Compass");
    serde_json::from_slice(&bytes).expect("Compass must return structured JSON")
}

fn emit(topic: &str, payload: Value) {
    let bytes = serde_json::to_vec(&payload).unwrap();
    (host::host_api().emit_event_v1)(RString::from(topic), RVec::from(bytes))
        .into_result()
        .unwrap_or_else(|error| panic!("host.events dispatch failed: {error}"));
}

#[test]
#[ignore = "requires NORTHSTAR_COMPASS_SMOKE_DLL, a verified Windows MSVC Compass DLL"]
fn compass_plugin_newviso_host_smoke() {
    let path = std::env::var("NORTHSTAR_COMPASS_SMOKE_DLL")
        .expect("NORTHSTAR_COMPASS_SMOKE_DLL must point to built Compass DLL");
    let mut provider = RunningProvider::load_current(&path)
        .expect("NewViso must instantiate the Compass plugin root and ABI trait");
    assert_eq!(provider.id(), "engine.input.compass");
    assert_eq!(provider.descriptor().version.as_str(), "0.6.0");
    provider.initialize().expect("Compass plugin initialization failed");

    emit("winit.focused", json!({"focused":true}));
    emit("winit.key", json!({"key":41, "state":"pressed"}));
    let first = input_state();
    assert_eq!(first["keys"]["down"], json!([41]));
    assert_eq!(first["keys"]["pressed"], json!([41]));
    let consumed = input_state();
    assert_eq!(consumed["keys"]["pressed"], json!([]));

    emit("winit.text_char", json!({"cp":65}));
    emit("winit.ime_commit", json!({"text":"B"}));
    let text = input_state();
    assert_eq!(text["text"]["buffer"], "A");
    assert_eq!(text["text"]["ime_commit"], "B");

    emit("winit.focused", json!({"focused":false}));
    let focus = input_state();
    assert_eq!(focus["keys"]["down"], json!([]));
    assert_eq!(focus["keys"]["released"], json!([41]));

    provider.shutdown();
    // The current host registry intentionally retains service/event sink ABI
    // trait objects for its process lifetime, so keep the provider DLL mapped.
    // The OS releases the test process resources after this test exits.
    std::mem::forget(provider);
}
