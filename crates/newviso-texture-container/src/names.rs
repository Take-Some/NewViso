use crate::{COLOR_SPACE_LINEAR, COLOR_SPACE_SRGB};

pub fn normalize_texture_name(name: &str) -> String {
    let trimmed = name.trim().replace('\\', "/");
    let file_name = trimmed
        .split('/')
        .next_back()
        .unwrap_or(trimmed.as_str())
        .trim();
    let lower = file_name.to_ascii_lowercase();
    let Some((stem, ext)) = lower.rsplit_once('.') else {
        return lower;
    };

    // Strip only real source/runtime texture extensions. Plugin ids and logical
    // entry names such as `engine.render.vulkan` must remain intact; the
    // old split-on-any-dot rule collapsed them all to `newengine` and produced
    // duplicate dictionary hashes.
    match ext {
        "png" | "jpg" | "jpeg" | "bmp" | "tga" | "webp" | "dds" | "ktx" | "ktx2" | "ytd" => {
            stem.to_owned()
        }
        _ => lower,
    }
}

pub fn normalize_color_space(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "srgb" | "s_rgb" | "color" | "albedo" | "basecolor" | "base_color" | "diffuse" => {
            COLOR_SPACE_SRGB.to_owned()
        }
        _ => COLOR_SPACE_LINEAR.to_owned(),
    }
}

pub fn infer_color_space_from_name(name: &str) -> String {
    let n = name.to_ascii_lowercase();
    if n.contains("normal")
        || n.contains("roughness")
        || n.contains("metallic")
        || n.contains("occlusion")
        || n.contains("_ao")
    {
        COLOR_SPACE_LINEAR.to_owned()
    } else {
        COLOR_SPACE_SRGB.to_owned()
    }
}

pub fn stable_name_hash64(name: &str) -> u64 {
    // FNV-1a 64-bit. Deterministic, tiny and sufficient for dictionary lookup/read-models.
    let mut hash = 0xcbf29ce484222325u64;
    for b in normalize_texture_name(name).as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
