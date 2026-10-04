use newviso_assets_client::AssetClient;
use newviso_model::{ModelJoint, ModelResource, ModelSkeleton, VertexSemantic};
use newviso_resource_runtime::AssetAddress;
use quick_xml::{events::Event, Reader, XmlVersion};
use serde_json::Value;

const LIST_FILE_BODY_OUTPUT: &str = "asset.list_file_body";

#[derive(Clone, Debug)]
struct RawJoint {
    index: usize,
    tag: u32,
    parent: Option<usize>,
    name: String,
    translation: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
}

pub(crate) fn attach_character_skeleton(
    assets: &AssetClient,
    address: &AssetAddress,
    model: &mut ModelResource,
) -> Result<(), String> {
    // Native drawable skeleton metadata is authoritative when the semantic
    // model already carries it (notably weapon YDRs with Gun_GripR/Gun_Muzzle).
    if model.skeleton.is_some() {
        return Ok(());
    }
    if !model.meshes.iter().any(|mesh| {
        mesh.vertex_streams.iter().any(|stream| {
            matches!(
                stream.semantic,
                VertexSemantic::JointIndices | VertexSemantic::JointIndicesExtra
            )
        })
    }) {
        return Ok(());
    }

    let candidates = skeleton_candidates(address.logical_path());
    let mut failures = Vec::new();
    for candidate in &candidates {
        match load_skeleton_xml_bytes(assets, candidate) {
            Ok(body) => match decode_skeleton_xml(candidate, &body) {
                Ok(skeleton) => {
                    validate_joint_stream_domain(model, &skeleton, candidate)?;
                    model.skeleton = Some(skeleton);
                    return Ok(());
                }
                Err(error) => failures.push(format!("{candidate}: {error}")),
            },
            Err(error) => failures.push(format!("{candidate}: {error}")),
        }
    }

    let logical = address
        .logical_path()
        .replace('\\', "/")
        .to_ascii_lowercase();
    let canonical_character = logical.starts_with("models/characters/");
    if canonical_character {
        return Err(format!(
            "skinned character '{}' has no readable skeleton metadata; tried [{}]",
            address.canonical(),
            failures.join(" | ")
        ));
    }
    Ok(())
}

fn load_skeleton_xml_bytes(assets: &AssetClient, logical_path: &str) -> Result<Vec<u8>, String> {
    let raw = assets.raw_bytes(logical_path)?;
    if looks_like_xml(&raw) {
        return Ok(raw);
    }

    // Runtime/authored metadata may also be packaged as NEF8 while preserving
    // the same XML body contract. In that case the generic ListFile codec owns
    // envelope decoding and this layer only consumes the decoded XML payload.
    assets
        .decode(logical_path, LIST_FILE_BODY_OUTPUT, Value::Null)
        .map_err(|error| {
            format!(
                "skeleton metadata is neither authored XML nor decodable ListFile body: {error}"
            )
        })
}

fn looks_like_xml(bytes: &[u8]) -> bool {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    bytes
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|byte| byte == b'<')
}

fn skeleton_candidates(logical_path: &str) -> Vec<String> {
    let path = logical_path.replace('\\', "/");
    let mut out = Vec::new();
    push_skeleton_candidates(&mut out, &path);

    let lower = path.to_ascii_lowercase();
    if let Some(index) = lower.rfind("/variants/") {
        let character_dir = &path[..index];
        if let Some(name) = character_dir.rsplit('/').next() {
            push_unique_candidate(&mut out, format!("{character_dir}/{name}.skeleton.ymt"));
            push_unique_candidate(&mut out, format!("{character_dir}/{name}.ymt"));
        }
    }
    out
}

fn push_skeleton_candidates(out: &mut Vec<String>, model_path: &str) {
    push_unique_candidate(out, replace_extension(model_path, "skeleton.ymt"));
    push_unique_candidate(out, replace_extension(model_path, "ymt"));
}

fn push_unique_candidate(out: &mut Vec<String>, candidate: String) {
    if !out
        .iter()
        .any(|value| value.eq_ignore_ascii_case(&candidate))
    {
        out.push(candidate);
    }
}

fn replace_extension(path: &str, extension: &str) -> String {
    match path.rsplit_once('.') {
        Some((base, _)) => format!("{base}.{extension}"),
        None => format!("{path}.{extension}"),
    }
}

fn decode_skeleton_xml(logical_path: &str, bytes: &[u8]) -> Result<ModelSkeleton, String> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut skeleton_name = None::<String>;
    let mut joints = Vec::<RawJoint>::new();

    loop {
        match reader.read_event_into(&mut buffer).map_err(|error| {
            format!("YMT skeleton XML read failed path='{logical_path}': {error}")
        })? {
            Event::Start(event) | Event::Empty(event) => {
                if event.name().into_inner() == "Skeleton" {
                    skeleton_name = attribute(&reader, &event, "name")?;
                } else if event.name().into_inner() == "Joint" {
                    joints.push(parse_joint(&reader, &event, logical_path)?);
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }

    if joints.is_empty() {
        return Err(format!(
            "YMT skeleton metadata contains no joints path='{logical_path}'"
        ));
    }
    joints.sort_by_key(|joint| joint.index);
    for (dense, joint) in joints.iter().enumerate() {
        if joint.index != dense {
            return Err(format!(
                "YMT skeleton joints are not dense path='{logical_path}' expected={dense} actual={}",
                joint.index
            ));
        }
        if let Some(parent) = joint.parent {
            if parent >= joints.len() || parent >= dense {
                return Err(format!(
                    "YMT skeleton parent must reference an earlier dense joint path='{logical_path}' joint={} parent={} joints={}",
                    joint.name,
                    parent,
                    joints.len()
                ));
            }
        }
    }

    let mut globals = Vec::<[f32; 16]>::with_capacity(joints.len());
    let mut output = Vec::<ModelJoint>::with_capacity(joints.len());
    for joint in joints {
        validate_trs(&joint, logical_path)?;
        let local = trs_matrix(joint.translation, joint.rotation, joint.scale);
        let global = match joint.parent {
            Some(parent) => mul_mat4(globals[parent], local),
            None => local,
        };
        let inverse_bind_matrix = inverse_affine(global).ok_or_else(|| {
            format!(
                "YMT skeleton bind transform is singular path='{logical_path}' joint='{}'",
                joint.name
            )
        })?;
        globals.push(global);
        output.push(ModelJoint {
            name: joint.name,
            tag: joint.tag,
            parent: joint
                .parent
                .map(|value| u16::try_from(value))
                .transpose()
                .map_err(|_| "YMT skeleton parent exceeds u16".to_owned())?,
            inverse_bind_matrix,
            bind_translation: joint.translation,
            bind_rotation: joint.rotation,
            bind_scale: joint.scale,
        });
    }

    Ok(ModelSkeleton {
        name: skeleton_name.unwrap_or_else(|| replace_extension(logical_path, "skeleton")),
        joints: output,
    })
}

fn parse_joint(
    reader: &Reader<&[u8]>,
    event: &quick_xml::events::BytesStart<'_>,
    logical_path: &str,
) -> Result<RawJoint, String> {
    let index = parse_usize_attr(reader, event, "index", logical_path)?;
    let parent_raw = attribute(reader, event, "parent_index")?.unwrap_or_else(|| "-1".to_owned());
    let parent_value = parent_raw.parse::<i64>().map_err(|error| {
        format!("YMT joint parent_index is invalid path='{logical_path}' index={index}: {error}")
    })?;
    let parent = if parent_value < 0 {
        None
    } else {
        Some(usize::try_from(parent_value).map_err(|_| "YMT parent index overflow".to_owned())?)
    };
    let name = attribute(reader, event, "name")?
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("YMT joint has empty name path='{logical_path}' index={index}"))?;

    Ok(RawJoint {
        index,
        tag: parse_u32_attr(reader, event, "tag", logical_path)?,
        parent,
        name,
        translation: [
            parse_f32_attr(reader, event, "tx", logical_path)?,
            parse_f32_attr(reader, event, "ty", logical_path)?,
            parse_f32_attr(reader, event, "tz", logical_path)?,
        ],
        rotation: [
            parse_f32_attr(reader, event, "qx", logical_path)?,
            parse_f32_attr(reader, event, "qy", logical_path)?,
            parse_f32_attr(reader, event, "qz", logical_path)?,
            parse_f32_attr(reader, event, "qw", logical_path)?,
        ],
        scale: [
            parse_f32_attr(reader, event, "sx", logical_path)?,
            parse_f32_attr(reader, event, "sy", logical_path)?,
            parse_f32_attr(reader, event, "sz", logical_path)?,
        ],
    })
}

fn attribute(
    _reader: &Reader<&[u8]>,
    event: &quick_xml::events::BytesStart<'_>,
    key: &str,
) -> Result<Option<String>, String> {
    for attribute in event.attributes().with_checks(false) {
        let attribute = attribute.map_err(|error| format!("YMT XML attribute error: {error}"))?;
        if attribute.key.into_inner() == key {
            return attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .map(|value| Some(value.into_owned()))
                .map_err(|error| format!("YMT XML attribute decode failed: {error}"));
        }
    }
    Ok(None)
}

fn parse_usize_attr(
    reader: &Reader<&[u8]>,
    event: &quick_xml::events::BytesStart<'_>,
    key: &str,
    logical_path: &str,
) -> Result<usize, String> {
    let value = attribute(reader, event, key)?
        .ok_or_else(|| format!("YMT joint missing '{}' path='{logical_path}'", key))?;
    value.parse::<usize>().map_err(|error| {
        format!(
            "YMT joint '{}' is invalid path='{logical_path}' value='{value}': {error}",
            key
        )
    })
}

fn parse_u32_attr(
    reader: &Reader<&[u8]>,
    event: &quick_xml::events::BytesStart<'_>,
    key: &str,
    logical_path: &str,
) -> Result<u32, String> {
    let value = attribute(reader, event, key)?
        .ok_or_else(|| format!("YMT joint missing '{}' path='{logical_path}'", key))?;
    value.parse::<u32>().map_err(|error| {
        format!(
            "YMT joint '{}' is invalid path='{logical_path}' value='{value}': {error}",
            key
        )
    })
}

fn parse_f32_attr(
    reader: &Reader<&[u8]>,
    event: &quick_xml::events::BytesStart<'_>,
    key: &str,
    logical_path: &str,
) -> Result<f32, String> {
    let value = attribute(reader, event, key)?
        .ok_or_else(|| format!("YMT joint missing '{}' path='{logical_path}'", key))?;
    let value = value.parse::<f32>().map_err(|error| {
        format!(
            "YMT joint '{}' is invalid path='{logical_path}': {error}",
            key
        )
    })?;
    if !value.is_finite() {
        return Err(format!(
            "YMT joint '{}' is non-finite path='{logical_path}'",
            key
        ));
    }
    Ok(value)
}

fn validate_trs(joint: &RawJoint, logical_path: &str) -> Result<(), String> {
    if joint
        .translation
        .iter()
        .chain(joint.rotation.iter())
        .chain(joint.scale.iter())
        .any(|value| !value.is_finite())
    {
        return Err(format!(
            "YMT joint contains non-finite bind pose path='{logical_path}' joint='{}'",
            joint.name
        ));
    }
    if joint.scale.iter().any(|value| value.abs() < 1.0e-8) {
        return Err(format!(
            "YMT joint contains zero bind scale path='{logical_path}' joint='{}'",
            joint.name
        ));
    }
    let q2 = joint
        .rotation
        .iter()
        .map(|value| value * value)
        .sum::<f32>();
    if !(0.5..=1.5).contains(&q2) {
        return Err(format!(
            "YMT joint quaternion has invalid norm path='{logical_path}' joint='{}' norm2={q2}",
            joint.name
        ));
    }
    Ok(())
}

fn validate_joint_stream_domain(
    model: &ModelResource,
    skeleton: &ModelSkeleton,
    logical_path: &str,
) -> Result<(), String> {
    for mesh in &model.meshes {
        for stream in &mesh.vertex_streams {
            if !matches!(
                stream.semantic,
                VertexSemantic::JointIndices | VertexSemantic::JointIndicesExtra
            ) {
                continue;
            }
            if stream.stride != 16 || stream.data.len() % 4 != 0 {
                return Err(format!(
                    "character joint stream has unsupported layout model='{}' mesh='{}' stride={} bytes={}",
                    model.name,
                    mesh.name,
                    stream.stride,
                    stream.data.len()
                ));
            }
            for chunk in stream.data.chunks_exact(4) {
                let value = f32::from_le_bytes(chunk.try_into().expect("exact f32 chunk"));
                if !value.is_finite()
                    || value < 0.0
                    || value.fract().abs() > 1.0e-4
                    || value as usize >= skeleton.joints.len()
                {
                    return Err(format!(
                        "character skin joint index outside skeleton path='{logical_path}' model='{}' mesh='{}' value={} joints={}",
                        model.name,
                        mesh.name,
                        value,
                        skeleton.joints.len()
                    ));
                }
            }
        }
    }
    Ok(())
}

fn trs_matrix(translation: [f32; 3], rotation: [f32; 4], scale: [f32; 3]) -> [f32; 16] {
    let [x, y, z, w] = rotation;
    let length = (x * x + y * y + z * z + w * w).sqrt().max(1.0e-12);
    let (x, y, z, w) = (x / length, y / length, z / length, w / length);
    let xx = x * x;
    let yy = y * y;
    let zz = z * z;
    let xy = x * y;
    let xz = x * z;
    let yz = y * z;
    let wx = w * x;
    let wy = w * y;
    let wz = w * z;

    [
        (1.0 - 2.0 * (yy + zz)) * scale[0],
        (2.0 * (xy + wz)) * scale[0],
        (2.0 * (xz - wy)) * scale[0],
        0.0,
        (2.0 * (xy - wz)) * scale[1],
        (1.0 - 2.0 * (xx + zz)) * scale[1],
        (2.0 * (yz + wx)) * scale[1],
        0.0,
        (2.0 * (xz + wy)) * scale[2],
        (2.0 * (yz - wx)) * scale[2],
        (1.0 - 2.0 * (xx + yy)) * scale[2],
        0.0,
        translation[0],
        translation[1],
        translation[2],
        1.0,
    ]
}

fn mul_mat4(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    let mut out = [0.0; 16];
    for column in 0..4 {
        for row in 0..4 {
            out[column * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[column * 4 + k]).sum();
        }
    }
    out
}

fn inverse_affine(matrix: [f32; 16]) -> Option<[f32; 16]> {
    let a00 = matrix[0];
    let a01 = matrix[4];
    let a02 = matrix[8];
    let a10 = matrix[1];
    let a11 = matrix[5];
    let a12 = matrix[9];
    let a20 = matrix[2];
    let a21 = matrix[6];
    let a22 = matrix[10];

    let c00 = a11 * a22 - a12 * a21;
    let c01 = a12 * a20 - a10 * a22;
    let c02 = a10 * a21 - a11 * a20;
    let det = a00 * c00 + a01 * c01 + a02 * c02;
    if !det.is_finite() || det.abs() < 1.0e-10 {
        return None;
    }
    let inv_det = 1.0 / det;
    let i00 = c00 * inv_det;
    let i01 = (a02 * a21 - a01 * a22) * inv_det;
    let i02 = (a01 * a12 - a02 * a11) * inv_det;
    let i10 = c01 * inv_det;
    let i11 = (a00 * a22 - a02 * a20) * inv_det;
    let i12 = (a02 * a10 - a00 * a12) * inv_det;
    let i20 = c02 * inv_det;
    let i21 = (a01 * a20 - a00 * a21) * inv_det;
    let i22 = (a00 * a11 - a01 * a10) * inv_det;
    let t = [matrix[12], matrix[13], matrix[14]];
    let it = [
        -(i00 * t[0] + i01 * t[1] + i02 * t[2]),
        -(i10 * t[0] + i11 * t[1] + i12 * t[2]),
        -(i20 * t[0] + i21 * t[1] + i22 * t[2]),
    ];

    Some([
        i00, i10, i20, 0.0, i01, i11, i21, 0.0, i02, i12, i22, 0.0, it[0], it[1], it[2], 1.0,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_candidates_prefer_exact_then_character_base() {
        assert_eq!(
            skeleton_candidates("models/characters/abby/variants/abby_seattle.ydd"),
            vec![
                "models/characters/abby/variants/abby_seattle.skeleton.ymt",
                "models/characters/abby/variants/abby_seattle.ymt",
                "models/characters/abby/abby.skeleton.ymt",
                "models/characters/abby/abby.ymt"
            ]
        );
    }

    #[test]
    fn skeleton_source_detection_distinguishes_xml_from_nef8() {
        assert!(looks_like_xml(b"  \n<?xml version=\"1.0\"?><Skeleton/>"));
        assert!(looks_like_xml(b"\xEF\xBB\xBF<Skeleton/>"));
        assert!(!looks_like_xml(b"NEF8\x02\x05\x00\x00"));
    }

    #[test]
    fn engine_authored_skeleton_xml_decodes_without_rsc7_codec() {
        let xml = br#"<Skeleton name="ped">
  <Joint index="0" tag="0" parent_index="-1" name="SKEL_ROOT" tx="0" ty="0" tz="0" qx="0" qy="0" qz="0" qw="1" sx="1" sy="1" sz="1" />
  <Joint index="1" tag="11816" parent_index="0" name="SKEL_Pelvis" tx="0" ty="0.9" tz="0" qx="0" qy="0" qz="0" qw="1" sx="1" sy="1" sz="1" />
</Skeleton>"#;
        let skeleton =
            decode_skeleton_xml("models/characters/gta/test/test.skeleton.ymt", xml).unwrap();
        assert_eq!(skeleton.name, "ped");
        assert_eq!(skeleton.joints.len(), 2);
        assert_eq!(skeleton.joints[1].tag, 11816);
        assert_eq!(skeleton.joints[1].parent, Some(0));
    }

    #[test]
    fn inverse_bind_roundtrip_is_identity() {
        let local = trs_matrix(
            [1.0, -2.0, 3.0],
            [0.0, 0.38268343, 0.0, 0.9238795],
            [1.2, 0.8, 1.1],
        );
        let inverse = inverse_affine(local).unwrap();
        let product = mul_mat4(local, inverse);
        for (index, value) in product.iter().enumerate() {
            let expected = if index % 5 == 0 { 1.0 } else { 0.0 };
            assert!(
                (value - expected).abs() < 1.0e-4,
                "index={index} value={value}"
            );
        }
    }
}
