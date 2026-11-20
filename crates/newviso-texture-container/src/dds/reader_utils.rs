use super::{source_layout::DdsSourceLayout, DdsImportError, DDSD_PITCH};

pub(super) fn choose_legacy_row_pitches(
    layout: DdsSourceLayout,
    width: u32,
    height: u32,
    mip_count: u32,
    header_flags: u32,
    header_pitch: usize,
    available_payload: usize,
) -> Result<Vec<usize>, DdsImportError> {
    let mut widths = Vec::with_capacity(mip_count as usize);
    let mut heights = Vec::with_capacity(mip_count as usize);
    let mut mip_width = width;
    let mut mip_height = height;
    for _ in 0..mip_count {
        widths.push(mip_width);
        heights.push(mip_height);
        mip_width = (mip_width / 2).max(1);
        mip_height = (mip_height / 2).max(1);
    }

    let tight = widths
        .iter()
        .map(|&w| layout.packed_row_len(w))
        .collect::<Result<Vec<_>, _>>()?;
    let aligned = tight
        .iter()
        .map(|&row| align_up(row, 4))
        .collect::<Result<Vec<_>, _>>()?;

    let mut candidates = Vec::<Vec<usize>>::new();
    if (header_flags & DDSD_PITCH) != 0 && header_pitch > 0 {
        if header_pitch < tight[0] {
            return Err(DdsImportError::InvalidPayload(format!(
                "header pitch {header_pitch} is smaller than packed top row {}",
                tight[0]
            )));
        }
        let mut header_tight = tight.clone();
        header_tight[0] = header_pitch;
        candidates.push(header_tight);
        let mut header_aligned = aligned.clone();
        header_aligned[0] = header_pitch;
        candidates.push(header_aligned);
    }
    candidates.push(tight);
    candidates.push(aligned);
    candidates.dedup();

    let mut exact = Vec::new();
    for candidate in candidates {
        let total = candidate_payload_len(&candidate, &heights)?;
        if total == available_payload {
            exact.push(candidate);
        }
    }
    if let Some(candidate) = exact.into_iter().next() {
        return Ok(candidate);
    }

    Err(DdsImportError::InvalidPayload(format!(
        "no tight/aligned row layout matches payload bytes={available_payload} extent={width}x{height} mips={mip_count} header_pitch={header_pitch}"
    )))
}

fn candidate_payload_len(row_pitches: &[usize], heights: &[u32]) -> Result<usize, DdsImportError> {
    row_pitches
        .iter()
        .zip(heights)
        .try_fold(0usize, |total, (&row_pitch, &height)| {
            let mip_len = row_pitch
                .checked_mul(height as usize)
                .ok_or(DdsImportError::Overflow("candidate mip length"))?;
            total
                .checked_add(mip_len)
                .ok_or(DdsImportError::Overflow("candidate payload length"))
        })
}

pub(super) fn checked_mip_slice(
    bytes: &[u8],
    offset: usize,
    len: usize,
    level: u32,
) -> Result<&[u8], DdsImportError> {
    let end = offset
        .checked_add(len)
        .ok_or(DdsImportError::Overflow("mip end offset"))?;
    bytes.get(offset..end).ok_or(DdsImportError::TruncatedMip {
        level,
        offset,
        needed: len,
        available: bytes.len().saturating_sub(offset),
    })
}

fn align_up(value: usize, alignment: usize) -> Result<usize, DdsImportError> {
    let mask = alignment
        .checked_sub(1)
        .ok_or_else(|| DdsImportError::InvalidPayload("alignment must be non-zero".to_owned()))?;
    value
        .checked_add(mask)
        .map(|v| v & !mask)
        .ok_or(DdsImportError::Overflow("aligned row length"))
}

pub(super) fn read_u32_at(bytes: &[u8], offset: usize) -> Result<u32, DdsImportError> {
    let slice = bytes.get(offset..offset + 4).ok_or_else(|| {
        DdsImportError::InvalidHeader(format!("truncated u32 at offset {offset}"))
    })?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

pub(super) fn read_fourcc_at(bytes: &[u8], offset: usize) -> Result<[u8; 4], DdsImportError> {
    let slice = bytes.get(offset..offset + 4).ok_or_else(|| {
        DdsImportError::InvalidHeader(format!("truncated FourCC at offset {offset}"))
    })?;
    Ok([slice[0], slice[1], slice[2], slice[3]])
}
