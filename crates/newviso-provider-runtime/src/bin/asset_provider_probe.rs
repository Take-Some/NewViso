//! Release gating probe for a current ABI provider DLL.
//! This binary is a dependency of the atomic AssetManager publisher; it runs
//! in a separate process so that provider DLLs can be unloaded before replacement.
use newviso_provider_runtime::{probe_file, probe_lifecycle, RootSymbol};
use std::{env, path::PathBuf, process};

fn run() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let path: PathBuf = args.next().ok_or("expected provider DLL path")?.into();
    let expected_id = args
        .next()
        .ok_or("expected provider id (e.g. engine.assets.starvault)")?
        .to_string_lossy()
        .into_owned();
    let expected_version = args
        .next()
        .ok_or("expected version (e.g. 3.8.1)")?
        .to_string_lossy()
        .into_owned();
    if args.next().is_some() {
        return Err("unexpected arguments".into());
    }
    let info = probe_file(&path).map_err(|error| error.to_string())?;
    if info.root_symbol != RootSymbol::Current || !info.has_descriptor_v2 {
        return Err(format!(
            "provider '{}' does not expose current root + descriptor v2",
            path.display()
        ));
    }
    if info.id != expected_id || info.version != expected_version {
        return Err(format!(
            "provider identity mismatch: discovered '{}' version '{}' but expected '{}' version '{}'",
            info.id, info.version, expected_id, expected_version
        ));
    }
    let lifecycle = probe_lifecycle(&path)?;
    if lifecycle.descriptor_id != info.id || lifecycle.descriptor_version != info.version {
        return Err(format!(
            "provider signature/descriptor mismatch: signature='{}@{}' descriptor='{}@{}'",
            info.id, info.version, lifecycle.descriptor_id, lifecycle.descriptor_version
        ));
    }
    let capabilities: Vec<_> = lifecycle
        .capabilities
        .iter()
        .map(|capability| {
            serde_json::json!({
                "id": capability.id,
                "role": format!("{:?}", capability.role),
                "kind": format!("{:?}", capability.kind),
                "version": capability.version,
                "description": serde_json::from_str::<serde_json::Value>(&capability.describe_json)
                    .unwrap_or(serde_json::Value::String(capability.describe_json.clone()))
            })
        })
        .collect();
    let result = serde_json::json!({
        "abi_current": true,
        "provider_id": info.id,
        "provider_version": info.version,
        "descriptor_v2": true,
        "config_format_version": lifecycle.default_format_version,
        "capabilities": capabilities
    });
    println!("{}", result);
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("ASSET_PROVIDER_PROBE_FAILED: {error}");
        process::exit(2);
    }
}
