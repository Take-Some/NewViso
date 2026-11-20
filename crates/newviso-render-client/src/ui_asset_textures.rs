//! Provider-neutral bridge from authored VFS UI textures to renderer texture
//! deltas. The UI provider only emits a canonical MaterialQuad.texture_ref;
//! the render client resolves it once and submits a retained GPU texture ID.
//! This intentionally uses a small, safe RGBA8 transport format so no image
//! codec, filesystem path, or UI implementation leaks into the renderer ABI.
use newviso_assets_client::AssetClient;
use newviso_render_api as api;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, OnceLock};

const HEADER: &[u8; 4] = b"NVUI";
const MAX_SIDE: u32 = 4096;
const MAX_RAW_BYTES: usize = 12 + 4 * MAX_SIDE as usize * MAX_SIDE as usize;

#[derive(Default)]
struct Cache {
    identities: BTreeMap<String, api::UiTexId>,
    owners: BTreeMap<api::UiTexId, String>,
    uploaded: BTreeSet<String>,
    unavailable: BTreeSet<String>,
}

static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();

fn decode_rgba(bytes: &[u8]) -> Result<api::UiTexture, String> {
    if bytes.len() < 12 || bytes.len() > MAX_RAW_BYTES || bytes.get(..4) != Some(HEADER) {
        return Err("invalid NVUI header or excessive image payload".into());
    }
    let width = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let height = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    if !(1..=MAX_SIDE).contains(&width) || !(1..=MAX_SIDE).contains(&height) {
        return Err("NVUI dimensions outside supported range".into());
    }
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|size| size.checked_mul(4))
        .ok_or("NVUI dimensions overflow")?;
    if bytes.len() != 12 + expected {
        return Err(format!(
            "invalid NVUI byte count: expected={}, got={}",
            12 + expected, bytes.len()
        ));
    }
    Ok(api::UiTexture { size: [width, height], rgba8: bytes[12..].to_vec() })
}


/// Canonical asset decode service result for YTD entry texture.rgba8.
/// Magic NTRT, version u16=1, reserved u16, width u32, height u32,
/// payload length u32, then RGBA8 pixels.
fn decode_ntrt(bytes: &[u8]) -> Result<api::UiTexture, String> {
    if bytes.len() < 20 || bytes.get(..4) != Some(b"NTRT") {
        return Err("invalid NTRT texture frame header".into());
    }
    if u16::from_le_bytes(bytes[4..6].try_into().unwrap()) != 1 {
        return Err("unsupported NTRT version".into());
    }
    let width = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    let height = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    let size = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
    let pixels = (width as usize)
        .checked_mul(height as usize)
        .and_then(|value| value.checked_mul(4))
        .ok_or("NTRT texture dimensions overflow")?;
    if !(1..=MAX_SIDE).contains(&width) || !(1..=MAX_SIDE).contains(&height)
        || size != pixels || bytes.len() != 20 + size {
        return Err("invalid NTRT texture dimensions or byte count".into());
    }
    Ok(api::UiTexture { size: [width, height], rgba8: bytes[20..].to_vec() })
}

fn asset_texture_reference(reference: &str) -> bool {
    reference.ends_with(".nvui") || reference.split_once(".ytd@")
        .is_some_and(|(dict, entry)| !dict.is_empty() && !entry.is_empty())
}

fn decode_texture_reference(reference: &str) -> Result<api::UiTexture, String> {
    let assets = AssetClient::new();
    if let Some((dictionary, entry)) = reference.split_once(".ytd@") {
        let resource = format!("{dictionary}.ytd");
        return assets.decode(&resource, "texture.rgba8",
            serde_json::json!({"texture_name": entry}))
            .and_then(|bytes| decode_ntrt(&bytes));
    }
    if reference.ends_with(".nvui") {
        return assets.raw_bytes(reference).and_then(|bytes| decode_rgba(&bytes));
    }
    Err(format!("unsupported authored UI texture reference '{reference}'"))
}

fn stable_texture_seed(source: &str) -> u32 {
    // Low-overhead FNV-1a. The registry resolves rare 30-bit collisions.
    let mut hash = 0x811C_9DC5u32;
    for byte in source.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash & 0x3fff_ffff
}

fn id_for(cache: &mut Cache, reference: &str) -> api::UiTexId {
    if let Some(id) = cache.identities.get(reference) {
        return *id;
    }
    let mut seed = stable_texture_seed(reference);
    loop {
        let id = api::reserved_textures::asset_ref_from_u32(seed);
        if !cache.owners.contains_key(&id) {
            cache.owners.insert(id, reference.to_owned());
            cache.identities.insert(reference.to_owned(), id);
            return id;
        }
        seed = seed.wrapping_add(1) & 0x3fff_ffff;
    }
}

/// Called for every submitted draw list, but each VFS image is decoded and
/// sent to Vulkan once. UI materials are authored, not provider-native.
pub(super) fn attach(draw: &mut api::UiDrawList) {
    let references: BTreeSet<_> = draw.paint.commands.iter().filter_map(|cmd| {
        if let api::UiPaintCommand::MaterialQuad(quad) = cmd {
            quad.texture_ref.as_ref()
                .filter(|name| asset_texture_reference(name))
                .cloned()
        } else {
            None
        }
    }).collect();
    if references.is_empty() { return; }
    let cache = CACHE.get_or_init(|| Mutex::new(Cache::default()));
    let Ok(mut cache) = cache.lock() else {
        newviso_host::warn("newviso.ui", "asset UI texture cache is poisoned");
        return;
    };

    for reference in references {
        let id = id_for(&mut cache, &reference);
        if !cache.uploaded.contains(&reference) && !cache.unavailable.contains(&reference) {
            let loaded = decode_texture_reference(&reference);
            match loaded {
                Ok(texture) => {
                    draw.texture_delta.set.insert(id, texture);
                    cache.uploaded.insert(reference.clone());
                    newviso_host::info("newviso.ui", format!(
                        "registered engine-owned UI asset image reference='{}' gpu_id={}",
                        reference, id.0
                    ));
                }
                Err(error) => {
                    cache.unavailable.insert(reference.clone());
                    newviso_host::warn("newviso.ui", format!(
                        "UI asset image unavailable reference='{}': {}", reference, error
                    ));
                    continue;
                }
            }
        }
        if cache.uploaded.contains(&reference) {
            for command in &mut draw.paint.commands {
                if let api::UiPaintCommand::MaterialQuad(quad) = command {
                    if quad.texture_ref.as_deref() == Some(reference.as_str()) {
                        quad.texture_id = Some(id);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_ui_image_is_bounded_and_exact() {
        let mut data = Vec::from(*HEADER);
        data.extend_from_slice(&2_u32.to_le_bytes());
        data.extend_from_slice(&1_u32.to_le_bytes());
        data.extend_from_slice(&[255, 0, 0, 255, 0, 255, 0, 127]);
        let decoded = decode_rgba(&data).unwrap();
        assert_eq!(decoded.size, [2, 1]);
        assert_eq!(decoded.rgba8.len(), 8);
        assert!(decode_rgba(&data[..data.len()-1]).is_err());
        data[4..8].copy_from_slice(&5000_u32.to_le_bytes());
        assert!(decode_rgba(&data).is_err());
    }

    #[test]
    fn native_ytd_entry_frame_decodes_rgba_exactly() {
        let mut data = Vec::from(*b"NTRT");
        data.extend_from_slice(&1_u16.to_le_bytes());
        data.extend_from_slice(&0_u16.to_le_bytes());
        data.extend_from_slice(&2_u32.to_le_bytes());
        data.extend_from_slice(&1_u32.to_le_bytes());
        data.extend_from_slice(&8_u32.to_le_bytes());
        data.extend_from_slice(&[1, 2, 3, 255, 4, 5, 6, 200]);
        let texture = decode_ntrt(&data).unwrap();
        assert_eq!(texture.size, [2, 1]);
        assert_eq!(texture.rgba8[4..], [4, 5, 6, 200]);
        assert!(decode_ntrt(&data[..data.len() - 1]).is_err());
        assert!(asset_texture_reference("assets/textures/ui/menu.ytd@logo"));
        assert!(!asset_texture_reference("assets/textures/ui/menu.ytd@"));
    }

    #[test]
    fn id_registry_has_stable_ids_and_avoids_reassignment() {
        let mut cache = Cache::default();
        let a = id_for(&mut cache, "assets/ui/ink/large.nvui");
        assert_eq!(id_for(&mut cache, "assets/ui/ink/large.nvui"), a);
        let b = id_for(&mut cache, "assets/ui/ink/other.nvui");
        assert_ne!(a, b);
        assert_eq!(a.0 & 0xc000_0000, 0x4000_0000);
    }
}
