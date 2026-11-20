use super::decode::{
    BODY_HEADER_LEN, ENTRY_RECORD_LEN, MESH_HEADER_LEN_V2, MESH_HEADER_LEN_V3, MESH_HEADER_LEN_V5,
    SKIN_VERTEX_STRIDE_V3, SKIN_VERTEX_STRIDE_V4, VERTEX_STRIDE,
};
use super::*;

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn push_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn push_vec3(out: &mut Vec<u8>, value: [f32; 3]) {
    for component in value {
        push_f32(out, component);
    }
}
fn push_mat4(out: &mut Vec<u8>, value: [f32; 16]) {
    for component in value {
        push_f32(out, component);
    }
}

fn test_body(version: u32, skinned: bool) -> Vec<u8> {
    let strings = b"entry\0source.gltf\0props.ytyp\0mesh\0materials/test.nemat@test\0";
    let entry_name = 0u32;
    let source = 6u32;
    let properties = 18u32;
    let mesh_name = 29u32;
    let material = 34u32;
    let table_offset = BODY_HEADER_LEN;
    let string_offset = table_offset + ENTRY_RECORD_LEN;
    let payload_offset = string_offset + strings.len();
    let mesh_header_len = if version >= YDD_BINARY_SCHEMA_VERSION {
        MESH_HEADER_LEN_V5
    } else if version >= YDD_BINARY_SCHEMA_VERSION_V3 {
        MESH_HEADER_LEN_V3
    } else {
        MESH_HEADER_LEN_V2
    };
    let has_source_transform = version >= YDD_BINARY_SCHEMA_VERSION_V3 && skinned;
    let source_transform_len = if has_source_transform { 64 } else { 0 };
    let skin_stride = match version {
        YDD_BINARY_SCHEMA_VERSION_V3 => SKIN_VERTEX_STRIDE_V3,
        YDD_BINARY_SCHEMA_VERSION_V4 | YDD_BINARY_SCHEMA_VERSION => SKIN_VERTEX_STRIDE_V4,
        _ => 0,
    };
    let skin_len = if skinned { 3 * skin_stride } else { 0 };
    let payload_len =
        8 + source_transform_len + mesh_header_len + 3 * VERTEX_STRIDE + skin_len + 3 * 4;
    let mut out = Vec::new();
    push_u32(&mut out, version);
    push_u32(&mut out, 1);
    push_u64(&mut out, table_offset as u64);
    push_u64(&mut out, string_offset as u64);
    push_u64(&mut out, strings.len() as u64);
    push_u64(&mut out, payload_offset as u64);
    push_u64(&mut out, 1);
    push_u32(&mut out, entry_name);
    push_u32(&mut out, source);
    push_u32(&mut out, 1);
    push_u32(&mut out, 3);
    push_u32(&mut out, 3);
    push_u32(&mut out, 1);
    push_u32(&mut out, properties);
    push_vec3(&mut out, [0.0, 0.0, 0.0]);
    push_vec3(&mut out, [1.0, 0.0, 1.0]);
    push_u32(&mut out, 0);
    push_u64(&mut out, payload_offset as u64);
    push_u64(&mut out, payload_len as u64);
    out.extend_from_slice(strings);
    push_u32(&mut out, 1);
    push_u32(&mut out, u32::from(has_source_transform));
    if has_source_transform {
        push_mat4(
            &mut out,
            [
                2.0, 0.0, 0.0, 0.0, // column 0
                0.0, 0.0, -2.0, 0.0, // column 1
                0.0, 2.0, 0.0, 0.0, // column 2
                0.0, 1.25, 0.0, 1.0, // column 3
            ],
        );
    }
    push_u32(&mut out, mesh_name);
    push_u32(&mut out, material);
    push_u32(&mut out, 3);
    push_u32(&mut out, 3);
    push_vec3(&mut out, [0.0, 0.0, 0.0]);
    push_vec3(&mut out, [1.0, 0.0, 1.0]);
    if version >= YDD_BINARY_SCHEMA_VERSION_V3 {
        push_u32(&mut out, u32::from(skinned));
        push_u32(&mut out, if skinned { skin_stride as u32 } else { 0 });
    }
    if version >= YDD_BINARY_SCHEMA_VERSION {
        push_u32(&mut out, u32::MAX);
    }
    for (position, uv) in [
        ([0.0, 0.0, 0.0], [0.0, 0.0]),
        ([1.0, 0.0, 0.0], [1.0, 0.0]),
        ([0.0, 0.0, 1.0], [0.0, 1.0]),
    ] {
        push_vec3(&mut out, position);
        push_vec3(&mut out, [0.0, 1.0, 0.0]);
        push_f32(&mut out, uv[0]);
        push_f32(&mut out, uv[1]);
    }
    if skinned {
        for joint in [1u16, 2, 3] {
            push_u16(&mut out, joint);
            push_u16(&mut out, 0);
            push_u16(&mut out, 0);
            push_u16(&mut out, 0);
            push_f32(
                &mut out,
                if version >= YDD_BINARY_SCHEMA_VERSION_V4 {
                    0.75
                } else {
                    1.0
                },
            );
            push_f32(&mut out, 0.0);
            push_f32(&mut out, 0.0);
            push_f32(&mut out, 0.0);
            if version >= YDD_BINARY_SCHEMA_VERSION_V4 {
                push_u16(&mut out, joint + 10);
                push_u16(&mut out, 0);
                push_u16(&mut out, 0);
                push_u16(&mut out, 0);
                push_f32(&mut out, 0.25);
                push_f32(&mut out, 0.0);
                push_f32(&mut out, 0.0);
                push_f32(&mut out, 0.0);
            }
        }
    }
    push_u32(&mut out, 0);
    push_u32(&mut out, 1);
    push_u32(&mut out, 2);
    out
}

#[test]
fn binary_ydd_decodes_strict_v2_body() {
    let document = decode_ydd_binary_body(&test_body(YDD_BINARY_SCHEMA_VERSION_V2, false))
        .expect("decode binary YDD v2");
    assert_eq!(document.entries.len(), 1);
    let entry = &document.entries[0];
    assert_eq!(entry.name, "entry");
    assert_eq!(entry.properties_ref.as_deref(), Some("props.ytyp"));
    assert_eq!(entry.skin_source_to_model, None);
    assert_eq!(entry.meshes.len(), 1);
    assert_eq!(entry.meshes[0].vertices.len(), 3);
    assert_eq!(entry.meshes[0].skin, None);
    assert_eq!(entry.meshes[0].indices, vec![0, 1, 2]);
    assert_eq!(entry.meshes[0].material_slot(), "test");
}

#[test]
fn binary_ydd_subset_decoder_materializes_only_requested_entries() {
    let mut base = decode_ydd_binary_body(&test_body(YDD_BINARY_SCHEMA_VERSION_V2, false))
        .expect("decode base entry")
        .entries
        .remove(0);
    base.name = "entry_a".to_owned();
    let mut other = base.clone();
    other.name = "entry_b".to_owned();
    other.source_path = "source_b.gltf".to_owned();
    other.meshes[0].name = "mesh_b".to_owned();
    let encoded = encode_ydd_binary_body(&YddBinaryDocument {
        entries: vec![base, other.clone()],
    })
    .expect("encode two-entry dictionary");

    let selected = decode_ydd_binary_entries(&encoded, &["entry_b".to_owned()])
        .expect("decode selected entry");
    assert_eq!(selected.entries, vec![other]);
}

#[test]
fn binary_ydd_subset_decoder_rejects_unknown_selector() {
    let encoded = test_body(YDD_BINARY_SCHEMA_VERSION_V2, false);
    let error = decode_ydd_binary_entries(&encoded, &["missing".to_owned()])
        .expect_err("missing selector must fail");
    assert!(error.contains("selector 'missing' was not found"));
}

#[test]
fn binary_ydd_decodes_v3_skin_stream_and_source_space_transform() {
    let document = decode_ydd_binary_body(&test_body(YDD_BINARY_SCHEMA_VERSION_V3, true))
        .expect("decode binary YDD v3");
    let entry = &document.entries[0];
    assert_eq!(
        entry.skin_source_to_model,
        Some([2.0, 0.0, 0.0, 0.0, 0.0, 0.0, -2.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 1.25, 0.0, 1.0,])
    );
    let mesh = &entry.meshes[0];
    assert!(mesh.is_skinned());
    let skin = mesh.skin.as_ref().expect("v3 skin stream");
    assert_eq!(skin.len(), 3);
    assert_eq!(skin[0].joints, [1, 0, 0, 0]);
    assert_eq!(skin[1].joints, [2, 0, 0, 0]);
    assert_eq!(skin[2].joints, [3, 0, 0, 0]);
    assert_eq!(skin[0].weights, [1.0, 0.0, 0.0, 0.0]);
    assert_eq!(skin[0].weights_extra, [0.0; 4]);
}

#[test]
fn binary_ydd_decodes_v4_eight_influence_skin_stream() {
    let document = decode_ydd_binary_body(&test_body(YDD_BINARY_SCHEMA_VERSION_V4, true))
        .expect("decode binary YDD v4");
    let skin = document.entries[0].meshes[0]
        .skin
        .as_ref()
        .expect("v4 skin stream");
    assert_eq!(skin[0].joints, [1, 0, 0, 0]);
    assert_eq!(skin[0].weights, [0.75, 0.0, 0.0, 0.0]);
    assert_eq!(skin[0].joints_extra, [11, 0, 0, 0]);
    assert_eq!(skin[0].weights_extra, [0.25, 0.0, 0.0, 0.0]);
}

#[test]
fn binary_ydd_v4_encoder_round_trips_eight_influence_document() {
    let document = YddBinaryDocument {
        entries: vec![YddBinaryEntry {
            name: "abby".to_owned(),
            source_path: "source/abby.pak".to_owned(),
            properties_ref: None,
            bounds_min: [0.0, 0.0, 0.0],
            bounds_max: [1.0, 1.0, 0.0],
            skin_source_to_model: Some([
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ]),
            meshes: vec![YddBinaryMesh {
                name: "body".to_owned(),
                inline_material: None,
                material_ref: Some("materials/abby.nemat@body".to_owned()),
                bounds_min: [0.0, 0.0, 0.0],
                bounds_max: [1.0, 1.0, 0.0],
                vertices: vec![
                    YddBinaryVertex {
                        position: [0.0, 0.0, 0.0],
                        normal: [0.0, 0.0, 1.0],
                        uv0: [0.0, 0.0],
                    },
                    YddBinaryVertex {
                        position: [1.0, 0.0, 0.0],
                        normal: [0.0, 0.0, 1.0],
                        uv0: [1.0, 0.0],
                    },
                    YddBinaryVertex {
                        position: [0.0, 1.0, 0.0],
                        normal: [0.0, 0.0, 1.0],
                        uv0: [0.0, 1.0],
                    },
                ],
                skin: Some(vec![
                    YddBinarySkinVertex {
                        joints: [1, 2, 3, 4],
                        weights: [0.30, 0.20, 0.15, 0.10],
                        joints_extra: [5, 6, 7, 8],
                        weights_extra: [0.08, 0.07, 0.06, 0.04],
                    };
                    3
                ]),
                indices: vec![0, 1, 2],
            }],
        }],
    };
    let encoded = encode_ydd_binary_body(&document).expect("encode binary YDD v4");
    let decoded = decode_ydd_binary_body(&encoded).expect("decode encoded binary YDD v4");
    assert_eq!(decoded, document);
}

#[test]
fn binary_ydd_rejects_json_geometry() {
    let error = decode_ydd_binary_body(br#"{\"runtime_mesh_parts\":[]}"#)
        .expect_err("JSON must be rejected");
    assert!(error.contains("JSON YDD geometry is unsupported"));
}

#[test]
fn binary_ydd_v5_inline_material_roundtrip_preserves_pbr() {
    let mut document = decode_ydd_binary_body(&test_body(YDD_BINARY_SCHEMA_VERSION_V4, false))
        .expect("v4 base body");
    let expected = serde_json::json!({
        "base_color": [0.12, 0.34, 0.56, 1.0],
        "roughness": 0.42,
        "metallic": 0.8,
        "normal_texture": "textures/vehicle/seat_normal.ytd@normal"
    });
    document.entries[0].meshes[0].material_ref = None;
    document.entries[0].meshes[0].inline_material = Some(expected.clone());
    let wire = encode_ydd_binary_body(&document).expect("v5 encode");
    assert_eq!(
        u32::from_le_bytes(wire[0..4].try_into().unwrap()),
        YDD_BINARY_SCHEMA_VERSION
    );
    let decoded = decode_ydd_binary_body(&wire).expect("v5 decode");
    assert_eq!(
        decoded.entries[0].meshes[0].inline_material.as_ref(),
        Some(&expected)
    );
    assert!(decoded.entries[0].meshes[0].material_ref.is_none());
    // Entry-subset decode must preserve the same material metadata.
    let subset = decode_ydd_binary_entries(&wire, &["entry".to_owned()]).expect("v5 subset decode");
    assert_eq!(
        subset.entries[0].meshes[0].inline_material.as_ref(),
        Some(&expected)
    );

    document.entries[0].meshes[0].inline_material = None;
    let v4 = encode_ydd_binary_body(&document).expect("v4 compatible encode");
    assert_eq!(
        u32::from_le_bytes(v4[0..4].try_into().unwrap()),
        YDD_BINARY_SCHEMA_VERSION_V4
    );
    assert!(decode_ydd_binary_body(&v4).unwrap().entries[0].meshes[0]
        .inline_material
        .is_none());
}

#[test]
fn binary_ydd_v5_rejects_invalid_inline_material_shape() {
    let mut document = decode_ydd_binary_body(&test_body(YDD_BINARY_SCHEMA_VERSION_V4, false))
        .expect("v4 base body");
    document.entries[0].meshes[0].inline_material = Some(serde_json::json!(["not-an-object"]));
    assert!(encode_ydd_binary_body(&document)
        .unwrap_err()
        .contains("must be object"));
}
