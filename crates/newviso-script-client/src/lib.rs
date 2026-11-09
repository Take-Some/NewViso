use newviso_host as host;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

const SCRIPTING_SERVICE: &str = "engine.scripting";
const LOAD_MODULE_METHOD: &str = "scripting.load_module_bytes_v1";
const INVOKE_METHOD: &str = "scripting.invoke_bytes_v1";
const FRAME_METHOD: &str = "scripting.frame_bytes_v1";
const FRAME_BATCH_METHOD: &str = "scripting.frame_batch_bytes_v1";
const MAX_BATCH_ITEMS: usize = 64;
const MAX_BATCH_BYTES: usize = 16 * 1024 * 1024;

const WIRE_VERSION: u16 = 1;
const MODULE_LOAD_MAGIC: &[u8; 4] = b"NSML";
const MODULE_LOAD_RESPONSE_MAGIC: &[u8; 4] = b"NSLR";
const REQUEST_MAGIC: &[u8; 4] = b"NSCR";
const RESPONSE_MAGIC: &[u8; 4] = b"NSRS";
const BATCH_REQUEST_MAGIC: &[u8; 4] = b"NSBF";
const BATCH_RESPONSE_MAGIC: &[u8; 4] = b"NSBR";

#[derive(Clone, Debug)]
pub struct ScriptPermission {
    pub id: String,
    pub scope: String,
}

#[derive(Clone, Debug)]
pub struct ScriptModuleLoad<'a> {
    pub reference: &'a str,
    pub module_bytes: &'a [u8],
    pub permissions: &'a [ScriptPermission],
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct ScriptInvocation<'a> {
    pub request_id: &'a str,
    pub script_ref: &'a str,
    pub operation: &'a str,
    pub payload: &'a Value,
    pub context_bytes: &'a [u8],
    pub permissions: &'a [ScriptPermission],
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScriptResponseStatus {
    Ok,
    Empty,
    Rejected,
    InvalidRequest,
    ProviderError,
}

#[derive(Clone, Debug)]
pub struct ScriptResponse {
    pub request_id: String,
    pub status: ScriptResponseStatus,
    pub payload_bytes: Vec<u8>,
    pub diagnostics: Vec<String>,
    pub trace_id: String,
    pub metadata: BTreeMap<String, String>,
}

impl ScriptResponse {
    pub fn payload_json(&self) -> Result<Option<Value>, String> {
        if self.payload_bytes.is_empty() {
            return Ok(None);
        }
        serde_json::from_slice(&self.payload_bytes)
            .map(Some)
            .map_err(|error| format!("script response is not valid JSON: {error}"))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ScriptClient;

impl ScriptClient {
    pub const fn new() -> Self {
        Self
    }

    pub fn load_module(&self, request: &ScriptModuleLoad<'_>) -> Result<(), String> {
        let bytes = encode_module_load(request);
        let response = host::call_service(SCRIPTING_SERVICE, LOAD_MODULE_METHOD, &bytes)?;
        decode_module_load_ok(&response)
    }

    pub fn invoke(&self, request: &ScriptInvocation<'_>) -> Result<ScriptResponse, String> {
        self.invoke_method(INVOKE_METHOD, request)
    }

    pub fn frame(&self, request: &ScriptInvocation<'_>) -> Result<ScriptResponse, String> {
        self.invoke_method(FRAME_METHOD, request)
    }

    /// Encode a frame request before batching. The lifetime of borrowed
    /// project/runtime JSON does not need to survive until dispatch.
    pub fn prepare_frame_request(&self, request: &ScriptInvocation<'_>) -> Result<Vec<u8>, String> {
        encode_request(request)
    }

    /// Accept a JSON snapshot encoded once per frame and reused by all scripts
    /// consuming that snapshot. Only the wire request headers remain per script.
    pub fn prepare_frame_request_with_payload(
        &self,
        request: &ScriptInvocation<'_>,
        payload_bytes: &[u8],
    ) -> Vec<u8> {
        encode_request_payload(request, payload_bytes)
    }

    /// Dispatch many frame scripts in one provider and V8-worker round trip.
    /// Older scripting providers are automatically handled by the V1 fallback.
    pub fn frame_batch_encoded(&self, requests: &[Vec<u8>]) -> Result<Vec<ScriptResponse>, String> {
        static BATCH_SUPPORT: AtomicU8 = AtomicU8::new(0); // unknown, yes, no
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        if requests.len() > MAX_BATCH_ITEMS {
            return Err("script frame batch exceeds 64 requests".into());
        }
        let fallback = || -> Result<Vec<ScriptResponse>, String> {
            let mut responses = Vec::with_capacity(requests.len());
            for bytes in requests {
                let response =
                    decode_response(&host::call_service(SCRIPTING_SERVICE, FRAME_METHOD, bytes)?)?;
                let success = matches!(
                    response.status,
                    ScriptResponseStatus::Ok | ScriptResponseStatus::Empty
                );
                responses.push(response);
                if !success {
                    break;
                }
            }
            Ok(responses)
        };
        if requests.len() == 1 || BATCH_SUPPORT.load(Ordering::Relaxed) == 2 {
            return fallback();
        }
        let size = requests
            .iter()
            .try_fold(12usize, |size, request| {
                size.checked_add(4)?.checked_add(request.len())
            })
            .ok_or("script frame batch byte-size overflow")?;
        if size > MAX_BATCH_BYTES {
            // Large frame payloads still work through the established API.
            return fallback();
        }
        let mut bytes = Vec::with_capacity(size);
        write_header(&mut bytes, BATCH_REQUEST_MAGIC);
        bytes.extend_from_slice(&(requests.len() as u32).to_le_bytes());
        for request in requests {
            write_bytes(&mut bytes, request);
        }

        let start = Instant::now();
        match host::call_service(SCRIPTING_SERVICE, FRAME_BATCH_METHOD, &bytes) {
            Ok(answer) => {
                let elapsed_us = start.elapsed().as_micros() as u64;
                let mut responses = decode_batch_responses(&answer, requests.len())?;
                BATCH_SUPPORT.store(1, Ordering::Relaxed);
                if let Some(first) = responses.first_mut() {
                    first
                        .metadata
                        .insert("perf.host_gateway_us".into(), elapsed_us.to_string());
                }
                Ok(responses)
            }
            Err(error)
                if error.contains("unknown method")
                    || error.contains("unsupported control method") =>
            {
                BATCH_SUPPORT.store(2, Ordering::Relaxed);
                fallback()
            }
            Err(error) => Err(error),
        }
    }

    fn invoke_method(
        &self,
        method: &str,
        request: &ScriptInvocation<'_>,
    ) -> Result<ScriptResponse, String> {
        static TRACE: OnceLock<bool> = OnceLock::new();
        let trace =
            *TRACE.get_or_init(|| std::env::var("NEWVISO_PERF_TRACE").as_deref() == Ok("1"));
        let mark = trace.then(Instant::now);
        let bytes = encode_request(request)?;
        let encode_us = mark.map_or(0, |start| start.elapsed().as_micros() as u64);
        let mark = trace.then(Instant::now);
        let response = host::call_service(SCRIPTING_SERVICE, method, &bytes)?;
        let gateway_us = mark.map_or(0, |start| start.elapsed().as_micros() as u64);
        let mark = trace.then(Instant::now);
        let mut decoded = decode_response(&response)?;
        let decode_us = mark.map_or(0, |start| start.elapsed().as_micros() as u64);
        if trace {
            decoded
                .metadata
                .insert("perf.host_encode_us".into(), encode_us.to_string());
            decoded
                .metadata
                .insert("perf.host_gateway_us".into(), gateway_us.to_string());
            decoded
                .metadata
                .insert("perf.host_decode_us".into(), decode_us.to_string());
        }
        Ok(decoded)
    }
}

fn decode_batch_responses(bytes: &[u8], expected: usize) -> Result<Vec<ScriptResponse>, String> {
    let mut reader = Reader::new(bytes, BATCH_RESPONSE_MAGIC)?;
    if bytes.get(6..8) != Some(&[0u8, 0u8][..]) {
        return Err("script frame batch response has unsupported flags".into());
    }
    let count = reader.read_u32()? as usize;
    if count == 0 || count > expected || count > MAX_BATCH_ITEMS {
        return Err(format!(
            "invalid script batch response count {count}, expected at most {expected}"
        ));
    }
    let mut responses = Vec::with_capacity(count);
    for _ in 0..count {
        let message = reader.read_bytes()?;
        responses.push(decode_response(&message)?);
    }
    reader.finish()?;
    if count != expected
        && responses.last().is_none_or(|response| {
            matches!(
                response.status,
                ScriptResponseStatus::Ok | ScriptResponseStatus::Empty
            )
        })
    {
        return Err("script batch truncated without an error response".into());
    }
    Ok(responses)
}

fn encode_module_load(request: &ScriptModuleLoad<'_>) -> Vec<u8> {
    let mut out = Vec::new();
    write_header(&mut out, MODULE_LOAD_MAGIC);
    write_string(&mut out, request.reference);
    write_string(&mut out, &default_module_id(request.reference));
    write_bytes(&mut out, request.module_bytes);
    write_permissions(&mut out, request.permissions);
    write_string_map(&mut out, &request.metadata);
    out
}

fn encode_request(request: &ScriptInvocation<'_>) -> Result<Vec<u8>, String> {
    let payload = serde_json::to_vec(request.payload).map_err(|error| error.to_string())?;
    Ok(encode_request_payload(request, &payload))
}

fn encode_request_payload(request: &ScriptInvocation<'_>, payload: &[u8]) -> Vec<u8> {
    let mut metadata = request.metadata.clone();
    metadata
        .entry("payload_format".to_owned())
        .or_insert_with(|| "json".to_owned());
    let mut out = Vec::with_capacity(payload.len().saturating_add(256));
    write_header(&mut out, REQUEST_MAGIC);
    write_string(&mut out, request.request_id);
    write_string(&mut out, request.script_ref);
    write_string(&mut out, request.operation);
    write_bytes(&mut out, payload);
    write_bytes(&mut out, request.context_bytes);
    write_permissions(&mut out, request.permissions);
    write_string_map(&mut out, &metadata);
    out
}

fn decode_module_load_ok(bytes: &[u8]) -> Result<(), String> {
    let mut reader = Reader::new(bytes, MODULE_LOAD_RESPONSE_MAGIC)?;
    let ok = reader.read_u8()?;
    if ok > 1 {
        return Err(format!("invalid module-load response bool {ok}"));
    }

    let mut diagnostics = reader.skip_module_record_collect_diagnostics()?;
    diagnostics.extend(reader.read_diagnostic_messages()?);
    reader.finish()?;

    if ok == 1 {
        return Ok(());
    }

    if diagnostics.is_empty() {
        Err("scripting provider rejected module load".to_owned())
    } else {
        Err(format!(
            "scripting provider rejected module load: {}",
            diagnostics.join(" | ")
        ))
    }
}

fn decode_response(bytes: &[u8]) -> Result<ScriptResponse, String> {
    let mut reader = Reader::new(bytes, RESPONSE_MAGIC)?;
    let request_id = reader.read_string()?;
    let status = match reader.read_u8()? {
        0 => ScriptResponseStatus::Ok,
        1 => ScriptResponseStatus::Empty,
        2 => ScriptResponseStatus::Rejected,
        3 => ScriptResponseStatus::InvalidRequest,
        4 => ScriptResponseStatus::ProviderError,
        other => return Err(format!("invalid scripting response status {other}")),
    };
    let payload_bytes = reader.read_bytes()?;
    let diagnostics = reader.read_diagnostic_messages()?;
    let trace_id = reader.read_string()?;
    let metadata = reader.read_string_map()?;
    reader.finish()?;

    Ok(ScriptResponse {
        request_id,
        status,
        payload_bytes,
        diagnostics,
        trace_id,
        metadata,
    })
}

fn write_header(out: &mut Vec<u8>, magic: &[u8; 4]) {
    out.extend_from_slice(magic);
    out.extend_from_slice(&WIRE_VERSION.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
}

fn write_string(out: &mut Vec<u8>, value: &str) {
    write_bytes(out, value.as_bytes());
}

fn write_bytes(out: &mut Vec<u8>, value: &[u8]) {
    out.extend_from_slice(&(value.len() as u32).to_le_bytes());
    out.extend_from_slice(value);
}

fn write_permissions(out: &mut Vec<u8>, values: &[ScriptPermission]) {
    out.extend_from_slice(&(values.len() as u32).to_le_bytes());
    for value in values {
        write_string(out, &value.id);
        write_string(out, &value.scope);
    }
}

fn write_string_map(out: &mut Vec<u8>, values: &BTreeMap<String, String>) {
    out.extend_from_slice(&(values.len() as u32).to_le_bytes());
    for (key, value) in values {
        write_string(out, key);
        write_string(out, value);
    }
}

fn default_module_id(reference: &str) -> String {
    reference
        .trim()
        .trim_start_matches('/')
        .replace('\\', "/")
        .to_ascii_lowercase()
        .chars()
        .map(|ch| {
            if matches!(ch, '/' | '@' | '.') {
                '_'
            } else {
                ch
            }
        })
        .collect()
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8], magic: &[u8; 4]) -> Result<Self, String> {
        if bytes.len() < 8 || bytes.get(0..4) != Some(&magic[..]) {
            return Err("scripting wire header mismatch".to_owned());
        }
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != WIRE_VERSION {
            return Err(format!("unsupported scripting wire version {version}"));
        }
        Ok(Self { bytes, offset: 8 })
    }

    fn finish(&self) -> Result<(), String> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err("scripting response has trailing bytes".to_owned())
        }
    }

    fn read_exact(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| "scripting wire overflow".to_owned())?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| "scripting wire truncated".to_owned())?;
        self.offset = end;
        Ok(value)
    }

    fn read_u8(&mut self) -> Result<u8, String> {
        Ok(self.read_exact(1)?[0])
    }

    fn read_u32(&mut self) -> Result<u32, String> {
        let value = self.read_exact(4)?;
        Ok(u32::from_le_bytes(value.try_into().expect("4-byte slice")))
    }

    fn read_u64(&mut self) -> Result<u64, String> {
        let value = self.read_exact(8)?;
        Ok(u64::from_le_bytes(value.try_into().expect("8-byte slice")))
    }

    fn read_bytes(&mut self) -> Result<Vec<u8>, String> {
        let len = self.read_u32()? as usize;
        Ok(self.read_exact(len)?.to_vec())
    }

    fn read_string(&mut self) -> Result<String, String> {
        String::from_utf8(self.read_bytes()?)
            .map_err(|error| format!("invalid scripting wire UTF-8: {error}"))
    }

    fn read_string_map(&mut self) -> Result<BTreeMap<String, String>, String> {
        let count = self.read_u32()? as usize;
        let mut values = BTreeMap::new();
        for _ in 0..count {
            values.insert(self.read_string()?, self.read_string()?);
        }
        Ok(values)
    }

    fn skip_permissions(&mut self) -> Result<(), String> {
        let count = self.read_u32()? as usize;
        for _ in 0..count {
            self.read_string()?;
            self.read_string()?;
        }
        Ok(())
    }

    fn read_diagnostic_messages(&mut self) -> Result<Vec<String>, String> {
        let count = self.read_u32()? as usize;
        let mut messages = Vec::with_capacity(count);
        for _ in 0..count {
            let severity = self.read_u8()?;
            let code = self.read_string()?;
            let message = self.read_string()?;
            let script_ref = self.read_string()?;
            self.read_bytes()?;
            messages.push(format!(
                "severity={severity} code='{code}' script='{}' message={message}",
                if script_ref.is_empty() {
                    "<unknown>"
                } else {
                    &script_ref
                }
            ));
        }
        Ok(messages)
    }

    fn skip_module_record_collect_diagnostics(&mut self) -> Result<Vec<String>, String> {
        self.read_string()?; // schema
        self.read_string()?; // module reference
        self.read_string()?; // module id
        self.read_u8()?; // module state
        self.skip_permissions()?;
        self.read_u64()?; // module bytes len
        self.read_string_map()?;
        self.read_diagnostic_messages()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preencoded_frame_snapshot_is_wire_compatible_with_legacy_request() {
        let payload =
            serde_json::json!({"project":{"scene":{"agents":[1,2,3]}},"runtime":{"tick":123}});
        let permissions = vec![ScriptPermission {
            id: "scene.write".into(),
            scope: "game".into(),
        }];
        let request = ScriptInvocation {
            request_id: "frame-123",
            script_ref: "scripts/main.ysc",
            operation: "on_frame",
            payload: &payload,
            context_bytes: &[],
            permissions: &permissions,
            metadata: BTreeMap::from([("phase".into(), "frame".into())]),
        };
        let client = ScriptClient::new();
        let old = client.prepare_frame_request(&request).unwrap();
        let encoded = serde_json::to_vec(&payload).unwrap();
        let new = client.prepare_frame_request_with_payload(&request, &encoded);
        assert_eq!(
            old, new,
            "shared frame JSON must preserve v1 request compatibility"
        );
    }

    #[test]
    #[ignore = "manual 8-script frame serialization benchmark"]
    fn benchmark_reused_frame_snapshot_serialization() {
        const FRAMES: usize = 1500;
        const SCRIPTS_PER_FRAME: usize = 8;
        let payload = serde_json::json!({
            "project": {
                "entities": (0..256).map(|id| serde_json::json!({
                    "id": id, "position": [id as f64, 2.0, 3.0],
                    "name": format!("entity-{id:04}"),
                    "velocity": [0.2, 0.0, -0.6],
                })).collect::<Vec<_>>()
            },
            "frame": { "delta_seconds": 0.01666667 }
        });
        let permissions: Vec<ScriptPermission> = Vec::new();
        let client = ScriptClient::new();
        let request = ScriptInvocation {
            request_id: "bench-frame",
            script_ref: "scripts/benchmark.ysc",
            operation: "on_frame",
            payload: &payload,
            context_bytes: &[],
            permissions: &permissions,
            metadata: BTreeMap::new(),
        };
        let started = Instant::now();
        let mut processed = 0usize;
        for _ in 0..FRAMES {
            for _ in 0..SCRIPTS_PER_FRAME {
                processed += client.prepare_frame_request(&request).unwrap().len();
            }
        }
        let sequential = started.elapsed();
        let started = Instant::now();
        for _ in 0..FRAMES {
            let json = serde_json::to_vec(&payload).unwrap();
            for _ in 0..SCRIPTS_PER_FRAME {
                processed += client
                    .prepare_frame_request_with_payload(&request, &json)
                    .len();
            }
        }
        let shared = started.elapsed();
        eprintln!("NewViso frame JSON pack: payload_bytes={} frames={} scripts_per_frame={} sequential_ms={:.3} cached_ms={:.3} speedup={:.2}x total_wire_bytes={}",
            serde_json::to_vec(&payload).unwrap().len(),
            FRAMES, SCRIPTS_PER_FRAME,
            sequential.as_secs_f64() * 1000.0,
            shared.as_secs_f64() * 1000.0,
            sequential.as_secs_f64() / shared.as_secs_f64(), processed);
    }

    fn mock_binary_response(id: &str, status: u8) -> Vec<u8> {
        let mut out = Vec::new();
        write_header(&mut out, RESPONSE_MAGIC);
        write_string(&mut out, id);
        out.push(status);
        write_bytes(&mut out, &[]);
        out.extend_from_slice(&0u32.to_le_bytes()); // zero diagnostics
        write_string(&mut out, ""); // trace
        write_string_map(&mut out, &BTreeMap::new());
        out
    }

    fn mock_batch(responses: &[Vec<u8>]) -> Vec<u8> {
        let mut out = Vec::new();
        write_header(&mut out, BATCH_RESPONSE_MAGIC);
        out.extend_from_slice(&(responses.len() as u32).to_le_bytes());
        for response in responses {
            write_bytes(&mut out, response);
        }
        out
    }

    #[test]
    fn batch_response_preserves_order_and_allows_error_prefix() {
        let first = mock_binary_response("first", 0);
        let second = mock_binary_response("second", 4);
        let batch = mock_batch(&[first, second]);
        let result = decode_batch_responses(&batch, 3).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].request_id, "first");
        assert_eq!(result[1].request_id, "second");
        assert_eq!(result[1].status, ScriptResponseStatus::ProviderError);
    }

    #[test]
    fn batch_response_rejects_missing_success_tail_and_truncation() {
        let success = mock_batch(&[mock_binary_response("first", 0)]);
        assert!(decode_batch_responses(&success, 2).is_err());
        assert!(decode_batch_responses(&success[..success.len() - 1], 1).is_err());
        assert!(decode_batch_responses(&success, 1).is_ok());
    }
    #[test]
    fn module_id_is_canonical() {
        assert_eq!(default_module_id("Scripts/Foo.ysc"), "scripts_foo_ysc");
    }
}
