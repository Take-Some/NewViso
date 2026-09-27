use newviso_assets_client::AssetClient;
use newviso_model::{
    AnimationInterpolation, AnimationQuatKey, AnimationVec3Key, JointAnimationTrack,
    ModelAnimationClip, ModelSkeleton,
};
use newviso_resource_runtime::AssetAddress;
use serde_json::Value;
use std::collections::BTreeMap;

const LIST_FILE_BODY_OUTPUT: &str = "asset.list_file_body";
const YCD_BODY_SCHEMA_V1: u32 = 1;
const YCD_BODY_SCHEMA_V2: u32 = 2;
const YCD_BODY_SCHEMA_V3: u32 = 3;
const YCD_BODY_HEADER_LEN: usize = 48;
const YCD_CLIP_RECORD_LEN: usize = 64;
const LOCAL_POSE_STRIDE_V1: usize = 28;
const LOCAL_POSE_STRIDE_V2: usize = 40;
const YCD_CLIP_FLAG_LOOP: u32 = 0x1;

#[derive(Clone, Copy, Debug)]
struct RawPose {
    translation: [f32; 3],
    rotation: [f32; 4],
    scale: Option<[f32; 3]>,
}

pub fn load_model_animation_clip(
    reference: &str,
    skeleton: &ModelSkeleton,
) -> Result<ModelAnimationClip, String> {
    let address = AssetAddress::parse(reference)
        .map_err(|error| format!("invalid animation asset reference '{reference}': {error}"))?;
    let selector = address.entry().ok_or_else(|| {
        format!("animation asset reference '{reference}' requires an @clip selector")
    })?;
    let body = AssetClient::new().decode(
        address.logical_path(),
        LIST_FILE_BODY_OUTPUT,
        Value::Null,
    )?;
    decode_selected_clip(&body, selector, skeleton)
}

fn decode_selected_clip(
    body: &[u8],
    selector: &str,
    skeleton: &ModelSkeleton,
) -> Result<ModelAnimationClip, String> {
    if body.len() < YCD_BODY_HEADER_LEN {
        return Err(format!(
            "YCD body too small bytes={} expected>={YCD_BODY_HEADER_LEN}",
            body.len()
        ));
    }
    let schema = read_u32(body, 0)?;
    let pose_stride = match schema {
        YCD_BODY_SCHEMA_V1 => LOCAL_POSE_STRIDE_V1,
        YCD_BODY_SCHEMA_V2 | YCD_BODY_SCHEMA_V3 => LOCAL_POSE_STRIDE_V2,
        _ => return Err(format!("unsupported YCD body schema={schema}")),
    };
    let clip_count = read_u32(body, 4)? as usize;
    let table_offset = usize_from_u64(read_u64(body, 8)?, "clip table")?;
    let string_offset = usize_from_u64(read_u64(body, 16)?, "string table")?;
    let string_len = usize_from_u64(read_u64(body, 24)?, "string length")?;
    let payload_floor = usize_from_u64(read_u64(body, 32)?, "payload floor")?;
    checked_slice(
        body,
        table_offset,
        clip_count
            .checked_mul(YCD_CLIP_RECORD_LEN)
            .ok_or("YCD clip table overflow")?,
        "clip table",
    )?;
    let strings = checked_slice(body, string_offset, string_len, "string table")?;

    let record = (0..clip_count)
        .find_map(|index| {
            let record = table_offset + index * YCD_CLIP_RECORD_LEN;
            let name_offset = read_u32(body, record + 8).ok()?;
            let name = read_string(strings, name_offset).ok()?;
            name.eq_ignore_ascii_case(selector).then_some(record)
        })
        .ok_or_else(|| format!("YCD selector '{selector}' was not found"))?;

    let name = read_string(strings, read_u32(body, record + 8)?)?;
    let joint_count = read_u32(body, record + 16)? as usize;
    let frame_count = read_u32(body, record + 20)? as usize;
    let duration_seconds = read_f32(body, record + 24)?;
    let sample_rate_hz = read_f32(body, record + 28)?;
    let flags = read_u32(body, record + 32)?;
    if flags & !YCD_CLIP_FLAG_LOOP != 0 {
        return Err(format!(
            "YCD clip '{name}' has unsupported flags=0x{flags:08x}"
        ));
    }
    if joint_count == 0 || frame_count == 0 || joint_count > 4096 || frame_count > 1_000_000 {
        return Err(format!(
            "YCD clip '{name}' invalid dimensions joints={joint_count} frames={frame_count}"
        ));
    }
    if duration_seconds <= 0.0
        || sample_rate_hz <= 0.0
        || !duration_seconds.is_finite()
        || !sample_rate_hz.is_finite()
    {
        return Err(format!(
            "YCD clip '{name}' invalid timing duration={duration_seconds} rate={sample_rate_hz}"
        ));
    }

    let payload_offset = usize_from_u64(read_u64(body, record + 40)?, "clip payload")?;
    let payload_len = usize_from_u64(read_u64(body, record + 48)?, "clip payload length")?;
    if payload_offset < payload_floor {
        return Err(format!("YCD clip '{name}' payload precedes payload floor"));
    }
    let payload = checked_slice(body, payload_offset, payload_len, "clip payload")?;
    let tag_bytes = joint_count.checked_mul(4).ok_or("YCD tag bytes overflow")?;
    let pose_count = joint_count
        .checked_mul(frame_count)
        .ok_or("YCD pose count overflow")?;
    let pose_bytes = pose_count
        .checked_mul(pose_stride)
        .ok_or("YCD pose bytes overflow")?;
    if tag_bytes.checked_add(pose_bytes) != Some(payload.len()) {
        return Err(format!(
            "YCD clip '{name}' payload size mismatch actual={} expected={}",
            payload.len(),
            tag_bytes + pose_bytes
        ));
    }

    let mut tags = Vec::with_capacity(joint_count);
    for joint in 0..joint_count {
        tags.push(read_u32(payload, joint * 4)?);
    }

    let mut poses = Vec::with_capacity(pose_count);
    let mut cursor = tag_bytes;
    for _ in 0..pose_count {
        let translation = [
            read_f32(payload, cursor)?,
            read_f32(payload, cursor + 4)?,
            read_f32(payload, cursor + 8)?,
        ];
        let rotation = normalize_quat([
            read_f32(payload, cursor + 12)?,
            read_f32(payload, cursor + 16)?,
            read_f32(payload, cursor + 20)?,
            read_f32(payload, cursor + 24)?,
        ])?;
        let scale = if schema == YCD_BODY_SCHEMA_V1 {
            None
        } else {
            Some([
                read_f32(payload, cursor + 28)?,
                read_f32(payload, cursor + 32)?,
                read_f32(payload, cursor + 36)?,
            ])
        };
        poses.push(RawPose {
            translation,
            rotation,
            scale,
        });
        cursor += pose_stride;
    }

    let tag_to_joint = skeleton
        .joints
        .iter()
        .enumerate()
        .map(|(index, joint)| (joint.tag, index))
        .collect::<BTreeMap<_, _>>();
    let mut tracks = Vec::with_capacity(joint_count);
    for (clip_joint, tag) in tags.into_iter().enumerate() {
        let joint_index = *tag_to_joint.get(&tag).ok_or_else(|| {
            format!(
                "YCD clip '{name}' references skeleton tag={tag} not present in '{}'",
                skeleton.name
            )
        })?;
        let joint = u16::try_from(joint_index)
            .map_err(|_| format!("skeleton joint index {joint_index} exceeds u16"))?;
        let mut translations = Vec::with_capacity(frame_count);
        let mut rotations = Vec::with_capacity(frame_count);
        let mut scales = Vec::with_capacity(frame_count);
        for frame in 0..frame_count {
            let pose = poses[frame * joint_count + clip_joint];
            let time_seconds = ((frame as f32) / sample_rate_hz).min(duration_seconds);
            translations.push(AnimationVec3Key {
                time_seconds,
                value: pose.translation,
            });
            rotations.push(AnimationQuatKey {
                time_seconds,
                value: pose.rotation,
            });
            if let Some(scale) = pose.scale {
                scales.push(AnimationVec3Key {
                    time_seconds,
                    value: scale,
                });
            }
        }
        tracks.push(JointAnimationTrack {
            joint,
            translation_interpolation: AnimationInterpolation::Linear,
            rotation_interpolation: AnimationInterpolation::Linear,
            scale_interpolation: AnimationInterpolation::Linear,
            translations,
            rotations,
            scales,
        });
    }

    Ok(ModelAnimationClip {
        name,
        duration_seconds,
        looping: flags & YCD_CLIP_FLAG_LOOP != 0,
        tracks,
    })
}

fn normalize_quat(mut q: [f32; 4]) -> Result<[f32; 4], String> {
    let length_sq = q.iter().map(|v| v * v).sum::<f32>();
    if !length_sq.is_finite() || length_sq <= 1.0e-8 {
        return Err("YCD clip contains invalid quaternion".to_owned());
    }
    let inv = length_sq.sqrt().recip();
    for value in &mut q {
        *value *= inv;
    }
    Ok(q)
}

fn checked_slice<'a>(
    bytes: &'a [u8],
    offset: usize,
    len: usize,
    label: &str,
) -> Result<&'a [u8], String> {
    let end = offset
        .checked_add(len)
        .ok_or_else(|| format!("YCD {label} range overflow"))?;
    bytes
        .get(offset..end)
        .ok_or_else(|| format!("YCD {label} outside body offset={offset} len={len}"))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        checked_slice(bytes, offset, 4, "u32")?
            .try_into()
            .expect("u32 slice"),
    ))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, String> {
    Ok(u64::from_le_bytes(
        checked_slice(bytes, offset, 8, "u64")?
            .try_into()
            .expect("u64 slice"),
    ))
}

fn read_f32(bytes: &[u8], offset: usize) -> Result<f32, String> {
    let value = f32::from_le_bytes(
        checked_slice(bytes, offset, 4, "f32")?
            .try_into()
            .expect("f32 slice"),
    );
    value
        .is_finite()
        .then_some(value)
        .ok_or_else(|| format!("YCD contains non-finite f32 at {offset}"))
}

fn read_string(strings: &[u8], offset: u32) -> Result<String, String> {
    let start = offset as usize;
    let tail = strings
        .get(start..)
        .ok_or_else(|| format!("YCD string offset outside table offset={offset}"))?;
    let len = tail
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(|| format!("YCD string not terminated offset={offset}"))?;
    String::from_utf8(tail[..len].to_vec())
        .map_err(|error| format!("YCD string is not UTF-8: {error}"))
}

fn usize_from_u64(value: u64, label: &str) -> Result<usize, String> {
    usize::try_from(value).map_err(|_| format!("YCD {label} exceeds usize"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quaternion_normalization_rejects_zero_and_normalizes_unit() {
        assert!(normalize_quat([0.0; 4]).is_err());
        let q = normalize_quat([0.0, 0.0, 0.0, 2.0]).unwrap();
        assert_eq!(q, [0.0, 0.0, 0.0, 1.0]);
    }
}
