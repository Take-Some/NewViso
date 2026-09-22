use newviso_host as host;
use serde_json::Value;
use std::collections::BTreeMap;

const SCRIPTING_SERVICE: &str = "engine.scripting";
const LOAD_MODULE_METHOD: &str = "scripting.load_module_bytes_v1";
const INVOKE_METHOD: &str = "scripting.invoke_bytes_v1";
const FRAME_METHOD: &str = "scripting.frame_bytes_v1";

const WIRE_VERSION: u16 = 1;
const MODULE_LOAD_MAGIC: &[u8; 4] = b"NSML";
const MODULE_LOAD_RESPONSE_MAGIC: &[u8; 4] = b"NSLR";
const REQUEST_MAGIC: &[u8; 4] = b"NSCR";
const RESPONSE_MAGIC: &[u8; 4] = b"NSRS";

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
        let bytes = encode_request(request)?;
        let response = host::call_service(SCRIPTING_SERVICE, INVOKE_METHOD, &bytes)?;
        decode_response(&response)
    }

    pub fn frame(&self, request: &ScriptInvocation<'_>) -> Result<ScriptResponse, String> {
        let bytes = encode_request(request)?;
        let response = host::call_service(SCRIPTING_SERVICE, FRAME_METHOD, &bytes)?;
        decode_response(&response)
    }
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
    let mut metadata = request.metadata.clone();
    metadata
        .entry("payload_format".to_owned())
        .or_insert_with(|| "json".to_owned());
    let payload = serde_json::to_vec(request.payload).map_err(|error| error.to_string())?;

    let mut out = Vec::new();
    write_header(&mut out, REQUEST_MAGIC);
    write_string(&mut out, request.request_id);
    write_string(&mut out, request.script_ref);
    write_string(&mut out, request.operation);
    write_bytes(&mut out, &payload);
    write_bytes(&mut out, request.context_bytes);
    write_permissions(&mut out, request.permissions);
    write_string_map(&mut out, &metadata);
    Ok(out)
}

fn decode_module_load_ok(bytes: &[u8]) -> Result<(), String> {
    let mut reader = Reader::new(bytes, MODULE_LOAD_RESPONSE_MAGIC)?;
    let ok = reader.read_u8()?;
    match ok {
        1 => Ok(()),
        0 => Err("scripting provider rejected module load".to_owned()),
        other => Err(format!("invalid module-load response bool {other}")),
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
    reader.skip_diagnostics()?;
    let trace_id = reader.read_string()?;
    let metadata = reader.read_string_map()?;
    reader.finish()?;

    Ok(ScriptResponse {
        request_id,
        status,
        payload_bytes,
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

    fn skip_diagnostics(&mut self) -> Result<(), String> {
        let count = self.read_u32()? as usize;
        for _ in 0..count {
            self.read_u8()?;
            self.read_string()?;
            self.read_string()?;
            self.read_string()?;
            self.read_bytes()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_id_is_canonical() {
        assert_eq!(default_module_id("Scripts/Foo.ysc"), "scripts_foo_ysc");
    }
}
