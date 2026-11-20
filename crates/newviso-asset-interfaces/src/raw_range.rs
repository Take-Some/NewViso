use serde::{Deserialize, Serialize};

pub const ASSET_RAW_RANGE_WIRE_MAGIC: [u8; 4] = *b"NARR";
pub const ASSET_RAW_RANGE_WIRE_VERSION: u16 = 1;
pub const ASSET_RAW_RANGE_MAX_BYTES: u32 = 4 * 1024 * 1024;
const HEADER_BYTES: usize = 4 + 2 + 2 + 8 + 8 + 4;
const FLAG_EOF: u16 = 1 << 0;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetRawRangeRequest {
    pub logical_path: String,
    pub offset: u64,
    pub length: u32,
}

impl AssetRawRangeRequest {
    pub fn new(logical_path: impl Into<String>, offset: u64, length: u32) -> Self {
        Self {
            logical_path: logical_path.into(),
            offset,
            length,
        }
    }

    pub fn sanitized(mut self) -> Result<Self, String> {
        self.logical_path = self
            .logical_path
            .trim()
            .replace('\\', "/")
            .trim_matches('/')
            .to_owned();
        if self.logical_path.is_empty() {
            return Err("asset raw range logical_path is empty".to_owned());
        }
        if self.length == 0 {
            return Err("asset raw range length must be greater than zero".to_owned());
        }
        if self.length > ASSET_RAW_RANGE_MAX_BYTES {
            return Err(format!(
                "asset raw range length {} exceeds hard cap {}",
                self.length, ASSET_RAW_RANGE_MAX_BYTES
            ));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetRawRangeResponse {
    pub offset: u64,
    pub total_len: u64,
    pub eof: bool,
    pub bytes: Vec<u8>,
}

impl AssetRawRangeResponse {
    #[inline]
    pub fn end_offset(&self) -> u64 {
        self.offset.saturating_add(self.bytes.len() as u64)
    }
}

pub fn encode_asset_raw_range_response(
    response: &AssetRawRangeResponse,
) -> Result<Vec<u8>, String> {
    let payload_len = u32::try_from(response.bytes.len())
        .map_err(|_| "asset raw range payload exceeds u32 wire length".to_owned())?;
    if payload_len > ASSET_RAW_RANGE_MAX_BYTES {
        return Err(format!(
            "asset raw range payload {} exceeds hard cap {}",
            payload_len, ASSET_RAW_RANGE_MAX_BYTES
        ));
    }
    if response.offset > response.total_len {
        return Err("asset raw range response offset exceeds total_len".to_owned());
    }
    if response.end_offset() > response.total_len {
        return Err("asset raw range response bytes exceed total_len".to_owned());
    }

    let mut out = Vec::with_capacity(HEADER_BYTES + response.bytes.len());
    out.extend_from_slice(&ASSET_RAW_RANGE_WIRE_MAGIC);
    out.extend_from_slice(&ASSET_RAW_RANGE_WIRE_VERSION.to_le_bytes());
    let flags = if response.eof { FLAG_EOF } else { 0 };
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&response.offset.to_le_bytes());
    out.extend_from_slice(&response.total_len.to_le_bytes());
    out.extend_from_slice(&payload_len.to_le_bytes());
    out.extend_from_slice(&response.bytes);
    Ok(out)
}

pub fn decode_asset_raw_range_response(bytes: &[u8]) -> Result<AssetRawRangeResponse, String> {
    if bytes.len() < HEADER_BYTES {
        return Err(format!(
            "asset raw range wire too short: {} < {}",
            bytes.len(),
            HEADER_BYTES
        ));
    }
    if bytes[0..4] != ASSET_RAW_RANGE_WIRE_MAGIC {
        return Err("asset raw range wire magic mismatch".to_owned());
    }
    let version = u16::from_le_bytes(bytes[4..6].try_into().expect("fixed range"));
    if version != ASSET_RAW_RANGE_WIRE_VERSION {
        return Err(format!(
            "unsupported asset raw range wire version {version}"
        ));
    }
    let flags = u16::from_le_bytes(bytes[6..8].try_into().expect("fixed range"));
    let offset = u64::from_le_bytes(bytes[8..16].try_into().expect("fixed range"));
    let total_len = u64::from_le_bytes(bytes[16..24].try_into().expect("fixed range"));
    let payload_len = u32::from_le_bytes(bytes[24..28].try_into().expect("fixed range"));
    if payload_len > ASSET_RAW_RANGE_MAX_BYTES {
        return Err(format!(
            "asset raw range wire payload {} exceeds hard cap {}",
            payload_len, ASSET_RAW_RANGE_MAX_BYTES
        ));
    }
    let expected = HEADER_BYTES
        .checked_add(payload_len as usize)
        .ok_or_else(|| "asset raw range wire length overflow".to_owned())?;
    if bytes.len() != expected {
        return Err(format!(
            "asset raw range wire length mismatch: got {} expected {}",
            bytes.len(),
            expected
        ));
    }
    let payload = bytes[HEADER_BYTES..].to_vec();
    let end = offset
        .checked_add(payload.len() as u64)
        .ok_or_else(|| "asset raw range response end overflow".to_owned())?;
    if offset > total_len || end > total_len {
        return Err("asset raw range wire range exceeds total_len".to_owned());
    }
    let eof = flags & FLAG_EOF != 0;
    if eof != (end >= total_len) {
        return Err("asset raw range wire EOF flag is inconsistent with total_len".to_owned());
    }
    Ok(AssetRawRangeResponse {
        offset,
        total_len,
        eof,
        bytes: payload,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_range_wire_round_trips_metadata_and_payload() {
        let response = AssetRawRangeResponse {
            offset: 64,
            total_len: 69,
            eof: true,
            bytes: vec![1, 2, 3, 4, 5],
        };
        let encoded = encode_asset_raw_range_response(&response).expect("encode");
        let decoded = decode_asset_raw_range_response(&encoded).expect("decode");
        assert_eq!(decoded, response);
    }

    #[test]
    fn raw_range_request_is_bounded() {
        assert!(
            AssetRawRangeRequest::new("shared/music.ogg", 0, ASSET_RAW_RANGE_MAX_BYTES)
                .sanitized()
                .is_ok()
        );
        assert!(
            AssetRawRangeRequest::new("shared/music.ogg", 0, ASSET_RAW_RANGE_MAX_BYTES + 1)
                .sanitized()
                .is_err()
        );
    }
}
