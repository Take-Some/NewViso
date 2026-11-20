use super::DdsImportError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DdsSourceLayout {
    Native,
    Rgba8,
    Bgra8,
    Rgb24,
    Bgr24,
    L8,
}

impl DdsSourceLayout {
    #[inline]
    pub(super) const fn is_native(self) -> bool {
        matches!(self, Self::Native)
    }

    pub(super) fn packed_row_len(self, width: u32) -> Result<usize, DdsImportError> {
        let width = usize::try_from(width).map_err(|_| DdsImportError::Overflow("row width"))?;
        match self {
            Self::Native => Err(DdsImportError::InvalidPayload(
                "native block payload does not have a legacy source row layout".to_owned(),
            )),
            Self::Rgba8 | Self::Bgra8 => width
                .checked_mul(4)
                .ok_or(DdsImportError::Overflow("RGBA8 row length")),
            Self::Rgb24 | Self::Bgr24 => width
                .checked_mul(3)
                .ok_or(DdsImportError::Overflow("RGB24 row length")),
            Self::L8 => Ok(width),
        }
    }

    pub(super) fn decode_rows(
        self,
        payload: &[u8],
        width: u32,
        height: u32,
        row_pitch: usize,
    ) -> Result<Vec<u8>, DdsImportError> {
        let packed_row = self.packed_row_len(width)?;
        if row_pitch < packed_row {
            return Err(DdsImportError::InvalidPayload(format!(
                "row pitch {row_pitch} is smaller than packed row {packed_row}"
            )));
        }
        let pixel_count = (width as usize)
            .checked_mul(height as usize)
            .ok_or(DdsImportError::Overflow("decoded pixel count"))?;
        let output_len = pixel_count
            .checked_mul(4)
            .ok_or(DdsImportError::Overflow("decoded RGBA8 payload length"))?;
        let mut rgba = Vec::with_capacity(output_len);

        for row in 0..height as usize {
            let start = row
                .checked_mul(row_pitch)
                .ok_or(DdsImportError::Overflow("row offset"))?;
            let end = start
                .checked_add(packed_row)
                .ok_or(DdsImportError::Overflow("row end"))?;
            let row_bytes = payload.get(start..end).ok_or_else(|| {
                DdsImportError::InvalidPayload(format!(
                    "row {row} is truncated start={start} end={end} bytes={}",
                    payload.len()
                ))
            })?;
            match self {
                Self::Rgba8 => rgba.extend_from_slice(row_bytes),
                Self::Bgra8 => {
                    for pixel in row_bytes.as_chunks::<4>().0 {
                        rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
                    }
                }
                Self::Rgb24 => {
                    for pixel in row_bytes.as_chunks::<3>().0 {
                        rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 0xff]);
                    }
                }
                Self::Bgr24 => {
                    for pixel in row_bytes.as_chunks::<3>().0 {
                        rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 0xff]);
                    }
                }
                Self::L8 => {
                    for &luma in row_bytes {
                        rgba.extend_from_slice(&[luma, luma, luma, 0xff]);
                    }
                }
                Self::Native => unreachable!("native DDS payload is not row-decoded"),
            }
        }
        debug_assert_eq!(rgba.len(), output_len);
        Ok(rgba)
    }
}
