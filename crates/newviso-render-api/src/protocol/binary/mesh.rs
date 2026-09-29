use super::codec::*;

const MULTI_ADAPTER_MESH_REQUEST_MAGIC: &[u8; 8] = b"NEMW\x01\0\0\0";
const MULTI_ADAPTER_MESH_RESPONSE_MAGIC: &[u8; 8] = b"NEMX\x01\0\0\0";
pub const MULTI_ADAPTER_VERTEX_STRIDE_BYTES: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiAdapterMeshTranscodeRequest {
    /// Interleaved little-endian f32 records: position.xyz, normal.xyz, uv.xy.
    pub vertex_bytes: Vec<u8>,
}

impl MultiAdapterMeshTranscodeRequest {
    pub fn new(vertex_bytes: Vec<u8>) -> Result<Self, String> {
        validate_multi_adapter_vertex_bytes(&vertex_bytes)?;
        Ok(Self { vertex_bytes })
    }

    #[inline]
    pub fn vertex_count(&self) -> usize {
        self.vertex_bytes.len() / MULTI_ADAPTER_VERTEX_STRIDE_BYTES
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiAdapterMeshTranscodeResult {
    pub worker_index: u32,
    pub invalid_vertex_count: u32,
    pub gpu_elapsed_ns: u64,
    pub vertex_bytes: Vec<u8>,
}

impl MultiAdapterMeshTranscodeResult {
    #[inline]
    pub fn vertex_count(&self) -> usize {
        self.vertex_bytes.len() / MULTI_ADAPTER_VERTEX_STRIDE_BYTES
    }
}

pub fn encode_multi_adapter_mesh_transcode_request(
    request: &MultiAdapterMeshTranscodeRequest,
) -> Result<Vec<u8>, String> {
    validate_multi_adapter_vertex_bytes(&request.vertex_bytes)?;
    let vertex_count = u32::try_from(request.vertex_count())
        .map_err(|_| "multi-adapter mesh packet contains too many vertices".to_owned())?;
    let mut out = Vec::with_capacity(request.vertex_bytes.len().saturating_add(20));
    out.extend_from_slice(MULTI_ADAPTER_MESH_REQUEST_MAGIC);
    put_u32(&mut out, MULTI_ADAPTER_VERTEX_STRIDE_BYTES as u32);
    put_u32(&mut out, vertex_count);
    put_bytes(
        &mut out,
        &request.vertex_bytes,
        "multi-adapter vertex payload",
    )?;
    Ok(out)
}

pub fn decode_multi_adapter_mesh_transcode_request(
    bytes: &[u8],
) -> Result<MultiAdapterMeshTranscodeRequest, String> {
    let mut reader = BinReader::new(bytes);
    if reader.take(8)? != MULTI_ADAPTER_MESH_REQUEST_MAGIC {
        return Err("multi-adapter mesh request has invalid magic".to_owned());
    }
    let stride = reader.u32()? as usize;
    if stride != MULTI_ADAPTER_VERTEX_STRIDE_BYTES {
        return Err(format!(
            "multi-adapter mesh request has unsupported vertex stride={stride} expected={MULTI_ADAPTER_VERTEX_STRIDE_BYTES}"
        ));
    }
    let declared_count = reader.u32()? as usize;
    let vertex_bytes = reader.bytes_vec()?;
    if !reader.is_eof() {
        return Err("multi-adapter mesh request has trailing bytes".to_owned());
    }
    validate_multi_adapter_vertex_bytes(&vertex_bytes)?;
    let actual_count = vertex_bytes.len() / MULTI_ADAPTER_VERTEX_STRIDE_BYTES;
    if actual_count != declared_count {
        return Err(format!(
            "multi-adapter mesh request vertex count mismatch declared={declared_count} actual={actual_count}"
        ));
    }
    Ok(MultiAdapterMeshTranscodeRequest { vertex_bytes })
}

pub fn encode_multi_adapter_mesh_transcode_result(
    result: &MultiAdapterMeshTranscodeResult,
) -> Result<Vec<u8>, String> {
    validate_multi_adapter_vertex_bytes(&result.vertex_bytes)?;
    let vertex_count = u32::try_from(result.vertex_count())
        .map_err(|_| "multi-adapter mesh response contains too many vertices".to_owned())?;
    let mut out = Vec::with_capacity(result.vertex_bytes.len().saturating_add(32));
    out.extend_from_slice(MULTI_ADAPTER_MESH_RESPONSE_MAGIC);
    put_u32(&mut out, result.worker_index);
    put_u32(&mut out, result.invalid_vertex_count);
    put_u64(&mut out, result.gpu_elapsed_ns);
    put_u32(&mut out, MULTI_ADAPTER_VERTEX_STRIDE_BYTES as u32);
    put_u32(&mut out, vertex_count);
    put_bytes(
        &mut out,
        &result.vertex_bytes,
        "multi-adapter result payload",
    )?;
    Ok(out)
}

pub fn decode_multi_adapter_mesh_transcode_result(
    bytes: &[u8],
) -> Result<MultiAdapterMeshTranscodeResult, String> {
    let mut reader = BinReader::new(bytes);
    if reader.take(8)? != MULTI_ADAPTER_MESH_RESPONSE_MAGIC {
        return Err("multi-adapter mesh response has invalid magic".to_owned());
    }
    let worker_index = reader.u32()?;
    let invalid_vertex_count = reader.u32()?;
    let gpu_elapsed_ns = reader.u64()?;
    let stride = reader.u32()? as usize;
    if stride != MULTI_ADAPTER_VERTEX_STRIDE_BYTES {
        return Err(format!(
            "multi-adapter mesh response has unsupported vertex stride={stride} expected={MULTI_ADAPTER_VERTEX_STRIDE_BYTES}"
        ));
    }
    let declared_count = reader.u32()? as usize;
    let vertex_bytes = reader.bytes_vec()?;
    if !reader.is_eof() {
        return Err("multi-adapter mesh response has trailing bytes".to_owned());
    }
    validate_multi_adapter_vertex_bytes(&vertex_bytes)?;
    let actual_count = vertex_bytes.len() / MULTI_ADAPTER_VERTEX_STRIDE_BYTES;
    if actual_count != declared_count {
        return Err(format!(
            "multi-adapter mesh response vertex count mismatch declared={declared_count} actual={actual_count}"
        ));
    }
    Ok(MultiAdapterMeshTranscodeResult {
        worker_index,
        invalid_vertex_count,
        gpu_elapsed_ns,
        vertex_bytes,
    })
}

fn validate_multi_adapter_vertex_bytes(bytes: &[u8]) -> Result<(), String> {
    if bytes.is_empty() {
        return Err("multi-adapter mesh packet contains no vertices".to_owned());
    }
    if !bytes
        .len()
        .is_multiple_of(MULTI_ADAPTER_VERTEX_STRIDE_BYTES)
    {
        return Err(format!(
            "multi-adapter mesh packet byte length is not stride-aligned bytes={} stride={MULTI_ADAPTER_VERTEX_STRIDE_BYTES}",
            bytes.len()
        ));
    }
    const MAX_PACKET_BYTES: usize = 128 * 1024 * 1024;
    if bytes.len() > MAX_PACKET_BYTES {
        return Err(format!(
            "multi-adapter mesh packet exceeds safety limit bytes={} limit={MAX_PACKET_BYTES}",
            bytes.len()
        ));
    }
    Ok(())
}
