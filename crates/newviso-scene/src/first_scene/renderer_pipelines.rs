use super::*;

pub(super) struct ScenePipelines {
    pub(super) vertex_shader: u32,
    pub(super) fragment_shader: u32,
    pub(super) gbuffer_fragment_shader: u32,
    pub(super) asset_vertex_shader: u32,
    pub(super) shadow_vertex_shader: u32,
    pub(super) shadow_fragment_shader: u32,
    pub(super) asset_shadow_vertex_shader: u32,
    pub(super) pipeline: u32,
    pub(super) gbuffer_pipeline: u32,
    pub(super) alpha_pipeline: u32,
    pub(super) particle_additive_pipeline: u32,
    pub(super) asset_pipeline: u32,
    pub(super) asset_gbuffer_pipeline: u32,
    pub(super) asset_alpha_pipeline: u32,
    pub(super) shadow_pipeline: u32,
    pub(super) asset_shadow_pipeline: u32,
    pub(super) flare_vertex_shader: u32,
    pub(super) flare_fragment_shader: u32,
    pub(super) flare_pipeline: u32,
}

pub(super) fn create_scene_pipelines(
    render: &RenderClient,
    bind_group_layout: u32,
    material_bind_group_layout: u32,
    weather_material_bind_group_layout: u32,
    shadow_bind_group_layout: u32,
) -> Result<ScenePipelines, String> {
    let vertex_shader = render.create_shader_spirv(
        "newviso.first_scene.vertex",
        ShaderStage::Vertex,
        VERTEX_SHADER,
    )?;
    let fragment_shader = render.create_shader_spirv(
        "newviso.first_scene.fragment",
        ShaderStage::Fragment,
        FRAGMENT_SHADER,
    )?;
    let gbuffer_fragment_shader = render.create_shader_spirv(
        "newviso.first_scene.gbuffer.fragment",
        ShaderStage::Fragment,
        GBUFFER_FRAGMENT_SHADER,
    )?;
    let asset_vertex_shader = render.create_shader_spirv(
        "newviso.scene.asset_instanced.vertex",
        ShaderStage::Vertex,
        ASSET_INSTANCED_VERTEX_SHADER,
    )?;
    let shadow_vertex_shader = render.create_shader_spirv(
        "newviso.scene.shadow.vertex",
        ShaderStage::Vertex,
        SHADOW_VERTEX_SHADER,
    )?;
    let shadow_fragment_shader = render.create_shader_spirv(
        "newviso.scene.shadow.fragment",
        ShaderStage::Fragment,
        SHADOW_FRAGMENT_SHADER,
    )?;
    let asset_shadow_vertex_shader = render.create_shader_spirv(
        "newviso.scene.asset_instanced.shadow.vertex",
        ShaderStage::Vertex,
        ASSET_INSTANCED_SHADOW_VERTEX_SHADER,
    )?;

    let attributes = [
        VertexAttribute {
            location: 0,
            offset: 0,
            format: VertexFormat::Float32x4,
        },
        VertexAttribute {
            location: 1,
            offset: 16,
            format: VertexFormat::Float32x3,
        },
        VertexAttribute {
            location: 2,
            offset: 28,
            format: VertexFormat::Float32x4,
        },
        VertexAttribute {
            location: 3,
            offset: 44,
            format: VertexFormat::Float32x2,
        },
        VertexAttribute {
            location: 4,
            offset: 52,
            format: VertexFormat::Float32x4,
        },
    ];

    let instance_attributes = [
        VertexAttribute {
            location: 5,
            offset: 0,
            format: VertexFormat::Float32x4,
        },
        VertexAttribute {
            location: 6,
            offset: 16,
            format: VertexFormat::Float32x4,
        },
        VertexAttribute {
            location: 7,
            offset: 32,
            format: VertexFormat::Float32x4,
        },
        VertexAttribute {
            location: 8,
            offset: 48,
            format: VertexFormat::Float32x4,
        },
    ];

    let pipeline = render.create_pipeline(scene_pipeline_desc(
        "newviso.first_scene.pipeline",
        "newviso.scene.rsc7_material.opaque.weather.v6",
        vertex_shader,
        fragment_shader,
        &attributes,
        &[
            bind_group_layout,
            material_bind_group_layout,
            weather_material_bind_group_layout,
        ],
    ))?;
    let scene_vertex_layouts = [VertexLayoutDesc {
        stride: VERTEX_STRIDE,
        attributes: &attributes,
        step_mode: VertexStepMode::Vertex,
    }];
    let gbuffer_pipeline = render.create_pipeline_mrt_with_layouts(
        GraphicsPipelineDesc {
            color_format: "Rgba8Unorm",
            ..scene_pipeline_desc(
                "newviso.first_scene.gbuffer.pipeline",
                "newviso.scene.gbuffer.rsc7.weather.v1",
                vertex_shader,
                gbuffer_fragment_shader,
                &attributes,
                &[
                    bind_group_layout,
                    material_bind_group_layout,
                    weather_material_bind_group_layout,
                ],
            )
        },
        &scene_vertex_layouts,
        &["Rgba8Unorm", "Rgba16Float", "Rgba8Unorm"],
    )?;
    let alpha_pipeline = render.create_pipeline(GraphicsPipelineDesc {
        depth_write: false,
        blend_mode: "Alpha",
        ..scene_pipeline_desc(
            "newviso.first_scene.alpha_pipeline",
            "newviso.scene.rsc7_material.alpha.weather.v6",
            vertex_shader,
            fragment_shader,
            &attributes,
            &[
                bind_group_layout,
                material_bind_group_layout,
                weather_material_bind_group_layout,
            ],
        )
    })?;

    let particle_additive_pipeline = render.create_pipeline(GraphicsPipelineDesc {
        depth_write: false,
        blend_mode: "Additive",
        ..scene_pipeline_desc(
            "newviso.scene.particles.additive_pipeline",
            "newviso.scene.particles.additive.weather.v4",
            vertex_shader,
            fragment_shader,
            &attributes,
            &[
                bind_group_layout,
                material_bind_group_layout,
                weather_material_bind_group_layout,
            ],
        )
    })?;

    let asset_vertex_layouts = [
        VertexLayoutDesc {
            stride: VERTEX_STRIDE,
            attributes: &attributes,
            step_mode: VertexStepMode::Vertex,
        },
        VertexLayoutDesc {
            stride: INSTANCE_STRIDE,
            attributes: &instance_attributes,
            step_mode: VertexStepMode::Instance,
        },
    ];
    let asset_pipeline = render.create_pipeline_with_layouts(
        scene_pipeline_desc(
            "newviso.scene.asset_instanced.pipeline",
            "newviso.scene.asset_instanced.opaque.weather.v2",
            asset_vertex_shader,
            fragment_shader,
            &attributes,
            &[
                bind_group_layout,
                material_bind_group_layout,
                weather_material_bind_group_layout,
            ],
        ),
        &asset_vertex_layouts,
    )?;
    let asset_gbuffer_pipeline = render.create_pipeline_mrt_with_layouts(
        GraphicsPipelineDesc {
            color_format: "Rgba8Unorm",
            ..scene_pipeline_desc(
                "newviso.scene.asset_instanced.gbuffer.pipeline",
                "newviso.scene.asset_instanced.gbuffer.v1",
                asset_vertex_shader,
                gbuffer_fragment_shader,
                &attributes,
                &[
                    bind_group_layout,
                    material_bind_group_layout,
                    weather_material_bind_group_layout,
                ],
            )
        },
        &asset_vertex_layouts,
        &["Rgba8Unorm", "Rgba16Float", "Rgba8Unorm"],
    )?;
    let asset_alpha_pipeline = render.create_pipeline_with_layouts(
        GraphicsPipelineDesc {
            depth_write: false,
            blend_mode: "Alpha",
            ..scene_pipeline_desc(
                "newviso.scene.asset_instanced.alpha_pipeline",
                "newviso.scene.asset_instanced.alpha.weather.v2",
                asset_vertex_shader,
                fragment_shader,
                &attributes,
                &[
                    bind_group_layout,
                    material_bind_group_layout,
                    weather_material_bind_group_layout,
                ],
            )
        },
        &asset_vertex_layouts,
    )?;

    let shadow_attributes = [
        VertexAttribute {
            location: 0,
            offset: 0,
            format: VertexFormat::Float32x4,
        },
        VertexAttribute {
            location: 3,
            offset: 44,
            format: VertexFormat::Float32x2,
        },
    ];
    let shadow_pipeline = render.create_pipeline(GraphicsPipelineDesc {
        color_format: "R32Float",
        ..scene_pipeline_desc(
            "newviso.scene.shadow.pipeline",
            "newviso.scene.shadow.material_alpha.v2",
            shadow_vertex_shader,
            shadow_fragment_shader,
            &shadow_attributes,
            &[shadow_bind_group_layout, material_bind_group_layout],
        )
    })?;

    let asset_shadow_layouts = [
        VertexLayoutDesc {
            stride: VERTEX_STRIDE,
            attributes: &shadow_attributes,
            step_mode: VertexStepMode::Vertex,
        },
        VertexLayoutDesc {
            stride: INSTANCE_STRIDE,
            attributes: &instance_attributes,
            step_mode: VertexStepMode::Instance,
        },
    ];
    let asset_shadow_pipeline = render.create_pipeline_with_layouts(
        GraphicsPipelineDesc {
            color_format: "R32Float",
            ..scene_pipeline_desc(
                "newviso.scene.asset_instanced.shadow.pipeline",
                "newviso.scene.asset_instanced.shadow.v1",
                asset_shadow_vertex_shader,
                shadow_fragment_shader,
                &shadow_attributes,
                &[shadow_bind_group_layout, material_bind_group_layout],
            )
        },
        &asset_shadow_layouts,
    )?;

    let flare_vertex_shader = render.create_shader_spirv(
        "newviso.scene.lens_flare.vertex",
        ShaderStage::Vertex,
        FLARE_VERTEX_SHADER,
    )?;
    let flare_fragment_shader = render.create_shader_spirv(
        "newviso.scene.lens_flare.fragment",
        ShaderStage::Fragment,
        FLARE_FRAGMENT_SHADER,
    )?;
    let flare_attributes = [
        VertexAttribute {
            location: 0,
            offset: 0,
            format: VertexFormat::Float32x4,
        },
        VertexAttribute {
            location: 1,
            offset: 16,
            format: VertexFormat::Float32x4,
        },
        VertexAttribute {
            location: 2,
            offset: 32,
            format: VertexFormat::Float32x4,
        },
    ];
    let flare_pipeline = render.create_pipeline(GraphicsPipelineDesc {
        vertex_stride: FLARE_VERTEX_STRIDE,
        depth_test: false,
        depth_write: false,
        depth_compare: "Always",
        blend_mode: "Additive",
        ..scene_pipeline_desc(
            "newviso.scene.lens_flare.pipeline",
            "newviso.scene.lens_flare.v1",
            flare_vertex_shader,
            flare_fragment_shader,
            &flare_attributes,
            &[],
        )
    })?;

    Ok(ScenePipelines {
        vertex_shader,
        fragment_shader,
        gbuffer_fragment_shader,
        asset_vertex_shader,
        shadow_vertex_shader,
        shadow_fragment_shader,
        asset_shadow_vertex_shader,
        pipeline,
        gbuffer_pipeline,
        alpha_pipeline,
        particle_additive_pipeline,
        asset_pipeline,
        asset_gbuffer_pipeline,
        asset_alpha_pipeline,
        shadow_pipeline,
        asset_shadow_pipeline,
        flare_vertex_shader,
        flare_fragment_shader,
        flare_pipeline,
    })
}
