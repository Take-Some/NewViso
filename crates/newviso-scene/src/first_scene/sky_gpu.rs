use super::*;

pub(super) fn upload_sky_texture(
    render: &RenderClient,
    label: &str,
    texture: &SkyTextureResources,
) -> Result<u32, String> {
    if texture.width == 0 || texture.height == 0 {
        return Err(format!("sky texture '{}' has zero extent", texture.name));
    }
    let expected = texture.width as usize * texture.height as usize * 4;
    if texture.rgba8.len() != expected {
        return Err(format!(
            "sky texture '{}' rgba8 byte length {} does not match {}x{} RGBA8 ({expected})",
            texture.name,
            texture.rgba8.len(),
            texture.width,
            texture.height
        ));
    }
    let mip = TextureMipUpload {
        level: 0,
        width: texture.width,
        height: texture.height,
        offset: 0,
        byte_len: texture.rgba8.len() as u64,
    };
    render.create_texture(
        label,
        texture.width,
        texture.height,
        if texture.srgb {
            "Rgba8Srgb"
        } else {
            "Rgba8Unorm"
        },
        &[mip],
        &texture.rgba8,
    )
}
