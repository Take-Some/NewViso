use super::*;

pub(super) fn create_frame_buffer_ring(
    render: &RenderClient,
    label: &str,
    byte_len: u64,
    usage: &str,
) -> Result<[u32; SCENE_FRAME_SLOTS], String> {
    let mut buffers = [0; SCENE_FRAME_SLOTS];
    for slot in 0..SCENE_FRAME_SLOTS {
        match render.create_frame_buffer(
            slot,
            &format!("{label}.slot.{slot}"),
            byte_len,
            usage,
            "CpuToGpu",
        ) {
            Ok(buffer) => buffers[slot] = buffer,
            Err(error) => {
                for buffer in &buffers[..slot] {
                    render.destroy_buffer(*buffer);
                }
                return Err(error);
            }
        }
    }
    Ok(buffers)
}

pub(super) fn scene_pipeline_desc<'a>(
    label: &'a str,
    cache_key: &'a str,
    vertex_shader: u32,
    fragment_shader: u32,
    attributes: &'a [VertexAttribute],
    bind_group_layouts: &'a [u32],
) -> GraphicsPipelineDesc<'a> {
    GraphicsPipelineDesc {
        label,
        vertex_shader,
        fragment_shader,
        vertex_stride: VERTEX_STRIDE,
        attributes,
        topology: "TriangleList",
        bind_group_layouts,
        color_format: "Rgba16Float",
        depth_format: Some("Depth32Float"),
        depth_test: true,
        depth_write: true,
        depth_compare: "LessOrEqual",
        cull_mode: "None",
        blend_mode: "Opaque",
        cache_key,
    }
}
