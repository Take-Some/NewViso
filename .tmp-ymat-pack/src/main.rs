use flate2::{write::DeflateEncoder, Compression};
use serde_json::Value;
use std::{env, fs, io::Write};

fn main() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let input = args.next().ok_or("INPUT required")?;
    let output = args.next().ok_or("OUTPUT required")?;
    let body = fs::read(&input).map_err(|e| e.to_string())?;
    let document: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    if document.get("schema").and_then(Value::as_str) != Some("northstar.ymat.v1") {
        return Err("schema must be northstar.ymat.v1".into());
    }
    let entry_count = document.get("materials").and_then(Value::as_array)
        .ok_or("materials array required")?.len();
    if entry_count == 0 || entry_count > u32::MAX as usize {
        return Err("invalid material count".into());
    }

    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&body).map_err(|e| e.to_string())?;
    let stored = encoder.finish().map_err(|e| e.to_string())?;
    let hash = blake3::hash(&body);

    let mut out = Vec::with_capacity(64 + stored.len());
    out.extend_from_slice(b"NEF8");
    out.push(2); // list-file version
    out.push(6); // size class -> 64-byte header
    out.extend_from_slice(&30u16.to_le_bytes()); // material_dictionary
    out.extend_from_slice(&5u16.to_le_bytes());  // deflate + raw BLAKE3
    out.extend_from_slice(&1u16.to_le_bytes());  // YMAT schema version
    out.extend_from_slice(&(entry_count as u32).to_le_bytes());
    out.extend_from_slice(&(stored.len() as u64).to_le_bytes());
    out.extend_from_slice(&(body.len() as u64).to_le_bytes());
    out.extend_from_slice(hash.as_bytes());
    if out.len() != 64 { return Err(format!("header len {}", out.len())); }
    out.extend_from_slice(&stored);
    fs::write(&output, &out).map_err(|e| e.to_string())?;
    println!("YMAT_PACK_OK entries={} bytes={} output={}", entry_count, out.len(), output);
    Ok(())
}
