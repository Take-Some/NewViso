//! Shared bounded little-endian cursor for engine binary protocols.
//!
//! The crate deliberately does not own any protocol schema. It only centralizes
//! cursor advancement, overflow checks, length limits and primitive decoding.

/// Conservative default for one length-prefixed payload.
pub const DEFAULT_MAX_BLOB_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct ReadCursor<'a> {
    bytes: &'a [u8],
    cursor: usize,
    context: &'static str,
    max_blob_bytes: usize,
}

impl<'a> ReadCursor<'a> {
    #[inline]
    pub const fn new(bytes: &'a [u8], context: &'static str) -> Self {
        Self {
            bytes,
            cursor: 0,
            context,
            max_blob_bytes: DEFAULT_MAX_BLOB_BYTES,
        }
    }

    #[inline]
    pub const fn with_max_blob_bytes(mut self, max_blob_bytes: usize) -> Self {
        self.max_blob_bytes = max_blob_bytes;
        self
    }

    #[inline]
    pub const fn position(&self) -> usize {
        self.cursor
    }

    #[inline]
    pub const fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.cursor)
    }

    #[inline]
    pub const fn is_eof(&self) -> bool {
        self.cursor == self.bytes.len()
    }

    pub fn take(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self.cursor.checked_add(len).ok_or_else(|| {
            format!(
                "{} cursor overflow at offset={} requested={len}",
                self.context, self.cursor
            )
        })?;
        if end > self.bytes.len() {
            return Err(format!(
                "{} ended early at offset={} requested={} remaining={}",
                self.context,
                self.cursor,
                len,
                self.remaining()
            ));
        }
        let out = &self.bytes[self.cursor..end];
        self.cursor = end;
        Ok(out)
    }

    #[inline]
    pub fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    #[inline]
    pub fn u16(&mut self) -> Result<u16, String> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    #[inline]
    pub fn u32(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    #[inline]
    pub fn i32(&mut self) -> Result<i32, String> {
        let b = self.take(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    #[inline]
    pub fn u64(&mut self) -> Result<u64, String> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    #[inline]
    pub fn f32(&mut self) -> Result<f32, String> {
        let b = self.take(4)?;
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn bytes_vec(&mut self) -> Result<Vec<u8>, String> {
        let len = self.u32()? as usize;
        if len > self.max_blob_bytes {
            return Err(format!(
                "{} length-prefixed blob exceeds limit length={} limit={}",
                self.context, len, self.max_blob_bytes
            ));
        }
        Ok(self.take(len)?.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_little_endian_values_and_tracks_eof() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&7u32.to_le_bytes());
        bytes.extend_from_slice(&1.5f32.to_le_bytes());
        let mut cursor = ReadCursor::new(&bytes, "test packet");
        assert_eq!(cursor.u32().unwrap(), 7);
        assert!((cursor.f32().unwrap() - 1.5).abs() < f32::EPSILON);
        assert!(cursor.is_eof());
    }

    #[test]
    fn rejects_oversized_length_prefix_before_allocation() {
        let bytes = 1024u32.to_le_bytes();
        let mut cursor = ReadCursor::new(&bytes, "test packet").with_max_blob_bytes(16);
        let error = cursor.bytes_vec().unwrap_err();
        assert!(error.contains("exceeds limit"));
    }

    #[test]
    fn reports_offset_on_truncated_input() {
        let bytes = [1u8, 2, 3];
        let mut cursor = ReadCursor::new(&bytes, "test packet");
        let error = cursor.u32().unwrap_err();
        assert!(error.contains("offset=0"));
        assert!(error.contains("remaining=3"));
    }
}
