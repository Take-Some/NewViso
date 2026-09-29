use crate::{UiDrawCmd, UiDrawList, UiRect, UiTexId, UiTexture, UiTexturePatch, UiVertex};

const UI_DRAW_LIST_BIN_MAGIC: &[u8; 8] = b"NEUIDL1\0";

/// Encodes a UI draw list into a compact, deterministic binary payload.
///
/// The JSON shape remains useful for inspection/control calls, but frame-local
/// UI meshes are a hot path. This codec keeps the UI/provider contract stable
/// while avoiding per-frame serde_json cost for thousands of vertices.
pub fn encode_ui_draw_list_bin(list: &UiDrawList) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(64 + list.mesh.vertices.len().saturating_mul(24));
    encode_ui_draw_list_bin_into(&mut out, list)?;
    Ok(out)
}

/// Appends a binary UI draw-list payload to an existing buffer.
pub fn encode_ui_draw_list_bin_into(out: &mut Vec<u8>, list: &UiDrawList) -> Result<(), String> {
    out.extend_from_slice(UI_DRAW_LIST_BIN_MAGIC);
    put_u32(out, list.screen_size_px[0]);
    put_u32(out, list.screen_size_px[1]);
    put_f32(out, list.pixels_per_point);

    put_len(out, list.mesh.vertices.len(), "ui vertex count")?;
    for v in &list.mesh.vertices {
        put_f32(out, v.pos[0]);
        put_f32(out, v.pos[1]);
        put_f32(out, v.uv[0]);
        put_f32(out, v.uv[1]);
        put_u32(out, v.color);
    }

    put_len(out, list.mesh.indices.len(), "ui index count")?;
    for &idx in &list.mesh.indices {
        put_u32(out, idx);
    }

    put_len(out, list.mesh.cmds.len(), "ui draw command count")?;
    for cmd in &list.mesh.cmds {
        put_u32(out, cmd.texture.0);
        put_f32(out, cmd.clip_rect.min_x);
        put_f32(out, cmd.clip_rect.min_y);
        put_f32(out, cmd.clip_rect.max_x);
        put_f32(out, cmd.clip_rect.max_y);
        put_u32(out, cmd.index_range.start);
        put_u32(out, cmd.index_range.end);
    }

    // Hash-map iteration order must not leak into the packet. Sort by UiTexId
    // so identical UI frames serialize identically across runs/platforms.
    let mut texture_set = list.texture_delta.set.iter().collect::<Vec<_>>();
    texture_set.sort_by_key(|(id, _)| id.0);
    put_len(out, texture_set.len(), "ui texture set count")?;
    for (id, tex) in texture_set {
        put_u32(out, id.0);
        put_u32(out, tex.size[0]);
        put_u32(out, tex.size[1]);
        put_bytes(out, &tex.rgba8, "ui texture rgba8 payload")?;
    }

    put_len(
        out,
        list.texture_delta.patches.len(),
        "ui texture patch count",
    )?;
    for patch in &list.texture_delta.patches {
        put_u32(out, patch.id.0);
        put_u32(out, patch.origin[0]);
        put_u32(out, patch.origin[1]);
        put_u32(out, patch.size[0]);
        put_u32(out, patch.size[1]);
        put_bytes(out, &patch.rgba8, "ui texture patch rgba8 payload")?;
    }

    put_len(out, list.texture_delta.free.len(), "ui texture free count")?;
    for id in &list.texture_delta.free {
        put_u32(out, id.0);
    }

    // Optional protocol extension: renderer-neutral paint commands live at the
    // packet tail so older providers that stop after texture_delta remain
    // readable by newer hosts. Never insert extension fields in the middle of
    // the hot binary frame format.
    let paint_bytes = serde_json::to_vec(&list.paint)
        .map_err(|e| format!("encode ui paint command list failed: {e}"))?;
    put_bytes(out, &paint_bytes, "ui paint command list payload")?;

    Ok(())
}

pub fn decode_ui_draw_list_bin(bytes: &[u8]) -> Result<UiDrawList, String> {
    let mut r = BinReader::new(bytes, "ui binary packet");
    let magic = r.take(8)?;
    if magic != UI_DRAW_LIST_BIN_MAGIC {
        return Err("ui draw-list binary packet has invalid magic".to_owned());
    }

    let mut list = UiDrawList::new();
    list.screen_size_px = [r.u32()?, r.u32()?];
    list.pixels_per_point = r.f32()?.max(0.0001);

    let vertex_count = r.u32()? as usize;
    list.mesh.vertices.reserve(vertex_count);
    for _ in 0..vertex_count {
        list.mesh.vertices.push(UiVertex {
            pos: [r.f32()?, r.f32()?],
            uv: [r.f32()?, r.f32()?],
            color: r.u32()?,
        });
    }

    let index_count = r.u32()? as usize;
    list.mesh.indices.reserve(index_count);
    for _ in 0..index_count {
        list.mesh.indices.push(r.u32()?);
    }

    let cmd_count = r.u32()? as usize;
    list.mesh.cmds.reserve(cmd_count);
    for _ in 0..cmd_count {
        let texture = UiTexId(r.u32()?);
        let clip_rect = UiRect {
            min_x: r.f32()?,
            min_y: r.f32()?,
            max_x: r.f32()?,
            max_y: r.f32()?,
        };
        let start = r.u32()?;
        let end = r.u32()?;
        list.mesh.cmds.push(UiDrawCmd {
            texture,
            clip_rect,
            index_range: start..end,
        });
    }

    let set_count = r.u32()? as usize;
    for _ in 0..set_count {
        let id = UiTexId(r.u32()?);
        let size = [r.u32()?, r.u32()?];
        let rgba8 = r.bytes_vec()?;
        list.texture_delta.set.insert(id, UiTexture { size, rgba8 });
    }

    let patch_count = r.u32()? as usize;
    list.texture_delta.patches.reserve(patch_count);
    for _ in 0..patch_count {
        list.texture_delta.patches.push(UiTexturePatch {
            id: UiTexId(r.u32()?),
            origin: [r.u32()?, r.u32()?],
            size: [r.u32()?, r.u32()?],
            rgba8: r.bytes_vec()?,
        });
    }

    let free_count = r.u32()? as usize;
    list.texture_delta.free.reserve(free_count);
    for _ in 0..free_count {
        list.texture_delta.free.push(UiTexId(r.u32()?));
    }

    if !r.is_eof() {
        let paint_bytes = r.bytes_vec()?;
        list.paint = serde_json::from_slice(&paint_bytes)
            .map_err(|e| format!("decode ui paint command list failed: {e}"))?;
    }

    if !r.is_eof() {
        return Err("ui draw-list binary packet has trailing bytes".to_owned());
    }

    Ok(list)
}

#[inline]
fn put_len(out: &mut Vec<u8>, len: usize, what: &str) -> Result<(), String> {
    let len =
        u32::try_from(len).map_err(|_| format!("{what} is too large for ui binary packet"))?;
    put_u32(out, len);
    Ok(())
}

#[inline]
fn put_bytes(out: &mut Vec<u8>, bytes: &[u8], what: &str) -> Result<(), String> {
    put_len(out, bytes.len(), what)?;
    out.extend_from_slice(bytes);
    Ok(())
}

#[inline]
fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}
#[inline]
fn put_f32(out: &mut Vec<u8>, v: f32) {
    out.extend_from_slice(&v.to_le_bytes());
}

type BinReader<'a> = crate::binary_codec::ReadCursor<'a>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{reserved, UiTexture};

    #[test]
    fn ui_draw_list_binary_roundtrips() {
        let mut list = UiDrawList::new();
        list.screen_size_px = [1600, 900];
        list.pixels_per_point = 1.25;
        list.mesh.vertices.push(UiVertex {
            pos: [1.0, 2.0],
            uv: [0.1, 0.2],
            color: 0xff00ff00,
        });
        list.mesh.indices.push(0);
        list.mesh.cmds.push(UiDrawCmd {
            texture: reserved::FONT_ATLAS,
            clip_rect: UiRect {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 1600.0,
                max_y: 900.0,
            },
            index_range: 0..1,
        });
        list.paint
            .push(crate::UiPaintCommand::Rect(crate::UiRectPaintCommand {
                rect: [0.0, 0.0, 16.0, 16.0],
                color: 0xff00ff00,
                ..Default::default()
            }));
        list.texture_delta.set.insert(
            reserved::FONT_ATLAS,
            UiTexture {
                size: [1, 1],
                rgba8: vec![255, 255, 255, 255],
            },
        );

        let bytes = encode_ui_draw_list_bin(&list).unwrap();
        let decoded = decode_ui_draw_list_bin(&bytes).unwrap();
        assert_eq!(decoded.screen_size_px, list.screen_size_px);
        assert_eq!(decoded.mesh.vertices.len(), 1);
        assert_eq!(decoded.mesh.indices, vec![0]);
        assert_eq!(decoded.mesh.cmds[0].index_range, 0..1);
        assert_eq!(decoded.paint.commands.len(), 1);
        assert!(decoded
            .texture_delta
            .set
            .contains_key(&reserved::FONT_ATLAS));
    }

    #[test]
    fn ui_draw_list_binary_rejects_bad_magic() {
        let err = decode_ui_draw_list_bin(b"not-a-packet").unwrap_err();
        assert!(err.contains("invalid magic"));
    }
}
