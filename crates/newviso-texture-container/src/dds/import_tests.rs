use super::*;
use crate::PIXEL_FORMAT_RGBA8_UNORM;

struct LegacyDdsSpec {
    width: u32,
    height: u32,
    mip_count: u32,
    header_flags: u32,
    pitch: u32,
    pixel_format_flags: u32,
    bit_count: u32,
    masks: [u32; 4],
}

fn legacy_dds(spec: LegacyDdsSpec, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0u8; 128];
    bytes[0..4].copy_from_slice(b"DDS ");
    write_at(&mut bytes, 4, 124);
    write_at(&mut bytes, 8, spec.header_flags);
    write_at(&mut bytes, 12, spec.height);
    write_at(&mut bytes, 16, spec.width);
    write_at(&mut bytes, 20, spec.pitch);
    write_at(&mut bytes, 28, spec.mip_count);
    write_at(&mut bytes, 76, 32);
    write_at(&mut bytes, 80, spec.pixel_format_flags);
    write_at(&mut bytes, 88, spec.bit_count);
    write_at(&mut bytes, 92, spec.masks[0]);
    write_at(&mut bytes, 96, spec.masks[1]);
    write_at(&mut bytes, 100, spec.masks[2]);
    write_at(&mut bytes, 104, spec.masks[3]);
    bytes.extend_from_slice(payload);
    bytes
}

fn write_at(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn imports_bgra8_and_preserves_mips() {
    let bytes = legacy_dds(
        LegacyDdsSpec {
            width: 2,
            height: 1,
            mip_count: 2,
            header_flags: 0x0002_1007,
            pitch: 0,
            pixel_format_flags: DDPF_RGB | 0x0000_0001,
            bit_count: 32,
            masks: [0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0xff00_0000],
        },
        &[1, 2, 3, 4, 10, 20, 30, 40, 5, 6, 7, 8],
    );
    let texture = read_dds_runtime_texture(&bytes).expect("BGRA8 import");
    assert_eq!(texture.format, PIXEL_FORMAT_RGBA8_UNORM);
    assert_eq!(texture.mips.len(), 2);
    assert_eq!(texture.mips[0].bytes, vec![3, 2, 1, 4, 30, 20, 10, 40]);
    assert_eq!(texture.mips[1].bytes, vec![7, 6, 5, 8]);
}

#[test]
fn imports_rgb24_and_adds_opaque_alpha() {
    let bytes = legacy_dds(
        LegacyDdsSpec {
            width: 2,
            height: 1,
            mip_count: 1,
            header_flags: 0x0000_1007,
            pitch: 0,
            pixel_format_flags: DDPF_RGB,
            bit_count: 24,
            masks: [0x0000_00ff, 0x0000_ff00, 0x00ff_0000, 0],
        },
        &[1, 2, 3, 10, 20, 30],
    );
    let texture = read_dds_runtime_texture(&bytes).expect("RGB24 import");
    assert_eq!(texture.mips[0].bytes, vec![1, 2, 3, 0xff, 10, 20, 30, 0xff]);
}

#[test]
fn imports_bgr24_and_reorders_channels() {
    let bytes = legacy_dds(
        LegacyDdsSpec {
            width: 1,
            height: 1,
            mip_count: 1,
            header_flags: 0x0000_1007,
            pitch: 0,
            pixel_format_flags: DDPF_RGB,
            bit_count: 24,
            masks: [0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0],
        },
        &[1, 2, 3],
    );
    let texture = read_dds_runtime_texture(&bytes).expect("BGR24 import");
    assert_eq!(texture.mips[0].bytes, vec![3, 2, 1, 0xff]);
}

#[test]
fn imports_l8_as_rgba8_across_mips() {
    let bytes = legacy_dds(
        LegacyDdsSpec {
            width: 2,
            height: 1,
            mip_count: 2,
            header_flags: 0x0002_1007,
            pitch: 0,
            pixel_format_flags: DDPF_LUMINANCE,
            bit_count: 8,
            masks: [0x0000_00ff, 0, 0, 0],
        },
        &[7, 9, 11],
    );
    let texture = read_dds_runtime_texture(&bytes).expect("L8 import");
    assert_eq!(texture.mips[0].bytes, vec![7, 7, 7, 0xff, 9, 9, 9, 0xff]);
    assert_eq!(texture.mips[1].bytes, vec![11, 11, 11, 0xff]);
}

#[test]
fn imported_bgra8_round_trips_through_runtime_dictionary() {
    let bytes = legacy_dds(
        LegacyDdsSpec {
            width: 1,
            height: 1,
            mip_count: 1,
            header_flags: 0x0000_1007,
            pitch: 0,
            pixel_format_flags: DDPF_RGB | 0x0000_0001,
            bit_count: 32,
            masks: [0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0xff00_0000],
        },
        &[10, 20, 30, 40],
    );
    let imported = read_dds_runtime_texture(&bytes).expect("BGRA8 import");
    let netd = crate::pack_encoded(vec![crate::TextureEncodedBuildEntry {
        name: "bgra_runtime".to_owned(),
        width: imported.width,
        height: imported.height,
        format: imported.format,
        color_space: imported.color_space,
        mips: imported.mips,
    }])
    .expect("runtime dictionary pack");
    let dictionary = crate::parse(&netd).expect("runtime dictionary parse");
    let entry = dictionary.entry("bgra_runtime").expect("runtime entry");
    assert_eq!(entry.meta.format, PIXEL_FORMAT_RGBA8_UNORM);
    assert_eq!(entry.mip_bytes(0), Some(&[30, 20, 10, 40][..]));
}

#[test]
fn honors_dword_row_padding_for_rgb24_mips() {
    let bytes = legacy_dds(
        LegacyDdsSpec {
            width: 3,
            height: 1,
            mip_count: 2,
            header_flags: 0x0002_100f,
            pitch: 12,
            pixel_format_flags: DDPF_RGB,
            bit_count: 24,
            masks: [0x0000_00ff, 0x0000_ff00, 0x00ff_0000, 0],
        },
        &[1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 0, 0, 10, 20, 30, 0],
    );
    let texture = read_dds_runtime_texture(&bytes).expect("padded RGB24 import");
    assert_eq!(texture.mips[0].bytes.len(), 3 * 4);
    assert_eq!(texture.mips[1].bytes, vec![10, 20, 30, 0xff]);
}
