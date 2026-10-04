use super::*;

pub(super) fn main_deferred_hdr_render_graph(
    frame_index: u64,
    width: u32,
    height: u32,
    camera: FrameCameraContext,
) -> RenderGraphDesc {
    const SURFACE: RenderGraphResourceId = RenderGraphResourceId(1);
    const GBUFFER_ALBEDO: RenderGraphResourceId = RenderGraphResourceId(2);
    const GBUFFER_NORMAL: RenderGraphResourceId = RenderGraphResourceId(3);
    const GBUFFER_MATERIAL: RenderGraphResourceId = RenderGraphResourceId(4);
    const GBUFFER_DEPTH: RenderGraphResourceId = RenderGraphResourceId(5);
    const SCENE_HDR: RenderGraphResourceId = RenderGraphResourceId(6);
    const SSR_SIGNAL: RenderGraphResourceId = RenderGraphResourceId(7);
    const BLOOM_SIGNAL: RenderGraphResourceId = RenderGraphResourceId(8);
    const VISIBILITY_CANDIDATES: RenderGraphResourceId = RenderGraphResourceId(9);
    const VISIBILITY_INDIRECT: RenderGraphResourceId = RenderGraphResourceId(10);

    const BACKGROUND: RenderGraphPassId = RenderGraphPassId(1);
    const GBUFFER: RenderGraphPassId = RenderGraphPassId(2);
    const DEFERRED_LIGHTING: RenderGraphPassId = RenderGraphPassId(3);
    const TRANSPARENT: RenderGraphPassId = RenderGraphPassId(4);
    const SSR: RenderGraphPassId = RenderGraphPassId(5);
    const BLOOM: RenderGraphPassId = RenderGraphPassId(6);
    const POSTFX: RenderGraphPassId = RenderGraphPassId(7);
    const VISIBILITY: RenderGraphPassId = RenderGraphPassId(8);

    let extent = Extent2D::new(width.max(1), height.max(1));
    let surface = RenderGraphResourceDesc::external_swapchain(
        SURFACE,
        "newviso.main.surface",
        RenderGraphResourceUsage::ColorAttachment,
        extent,
        RenderTextureFormat::Bgra8Srgb,
    )
    .with_semantic(RenderGraphResourceSemantic::SurfaceColor);

    let gbuffer_albedo = RenderGraphResourceDesc::transient_texture(
        GBUFFER_ALBEDO,
        "newviso.main.gbuffer.albedo",
        RenderGraphResourceUsage::ColorAttachment,
        extent,
        RenderTextureFormat::Rgba8Unorm,
    )
    .with_semantic(RenderGraphResourceSemantic::GBufferAlbedo);
    let gbuffer_normal = RenderGraphResourceDesc::transient_texture(
        GBUFFER_NORMAL,
        "newviso.main.gbuffer.normal",
        RenderGraphResourceUsage::ColorAttachment,
        extent,
        RenderTextureFormat::Rgba16Float,
    )
    .with_semantic(RenderGraphResourceSemantic::GBufferNormal);
    let gbuffer_material = RenderGraphResourceDesc::transient_texture(
        GBUFFER_MATERIAL,
        "newviso.main.gbuffer.material",
        RenderGraphResourceUsage::ColorAttachment,
        extent,
        RenderTextureFormat::Rgba8Unorm,
    )
    .with_semantic(RenderGraphResourceSemantic::GBufferMaterial);
    let gbuffer_depth = RenderGraphResourceDesc::transient_texture(
        GBUFFER_DEPTH,
        "newviso.main.gbuffer.depth",
        RenderGraphResourceUsage::DepthAttachmentSampled,
        extent,
        RenderTextureFormat::Depth32Float,
    )
    .with_semantic(RenderGraphResourceSemantic::GBufferDepth);
    let scene_hdr = RenderGraphResourceDesc::transient_texture(
        SCENE_HDR,
        "newviso.main.scene_hdr",
        RenderGraphResourceUsage::ColorAttachment,
        extent,
        RenderTextureFormat::Rgba16Float,
    )
    .with_semantic(RenderGraphResourceSemantic::LitColor);
    let ssr_signal = RenderGraphResourceDesc::transient_texture(
        SSR_SIGNAL,
        "newviso.main.ssr_reflection",
        RenderGraphResourceUsage::ColorAttachment,
        extent,
        RenderTextureFormat::Rgba16Float,
    )
    .with_semantic(RenderGraphResourceSemantic::ScreenSpaceReflection);
    let bloom_signal = RenderGraphResourceDesc::transient_texture(
        BLOOM_SIGNAL,
        "newviso.main.bloom_composite",
        RenderGraphResourceUsage::ColorAttachment,
        extent,
        RenderTextureFormat::Rgba16Float,
    )
    .with_semantic(RenderGraphResourceSemantic::BloomComposite);
    // These are externally-owned frame-ring buffers. The graph declarations do not
    // allocate them; they make the compute -> indirect-draw dependency explicit so
    // the recorded VisibilityCull commands receive a real execution slot.
    let visibility_candidates = RenderGraphResourceDesc::external(
        VISIBILITY_CANDIDATES,
        "newviso.main.visibility.candidates",
        RenderGraphResourceUsage::StorageBuffer,
    );
    let visibility_indirect = RenderGraphResourceDesc::external(
        VISIBILITY_INDIRECT,
        "newviso.main.visibility.indirect",
        RenderGraphResourceUsage::StorageBuffer,
    );

    let mut visibility = RenderGraphPassDesc::new(
        VISIBILITY,
        "newviso.main.visibility_cull",
        RenderGraphPassKind::VisibilityCull,
    )
    .with_domain(RenderGraphPassDomain::Render3d)
    .reads(
        VISIBILITY_CANDIDATES,
        RenderGraphResourceUsage::StorageBuffer,
    )
    .writes(VISIBILITY_INDIRECT, RenderGraphResourceUsage::StorageBuffer);
    visibility.queue = RenderGraphQueueKind::Compute;

    // Background is produced first. Deferred resolve LOAD-preserves this HDR target
    // and discards pixels without GBuffer coverage, so the sky survives untouched.
    let background = RenderGraphPassDesc::new(
        BACKGROUND,
        "newviso.main.background",
        RenderGraphPassKind::ForwardOpaque,
    )
    .with_domain(RenderGraphPassDomain::Render3d)
    .writes(SCENE_HDR, RenderGraphResourceUsage::ColorAttachment)
    .draw_list(RenderDrawListKind::OpaqueForward);

    let gbuffer = RenderGraphPassDesc::new(
        GBUFFER,
        "newviso.main.gbuffer",
        RenderGraphPassKind::GBuffer,
    )
    .with_domain(RenderGraphPassDomain::Render3d)
    .writes(GBUFFER_ALBEDO, RenderGraphResourceUsage::ColorAttachment)
    .writes(GBUFFER_NORMAL, RenderGraphResourceUsage::ColorAttachment)
    .writes(GBUFFER_MATERIAL, RenderGraphResourceUsage::ColorAttachment)
    .writes(
        GBUFFER_DEPTH,
        RenderGraphResourceUsage::DepthAttachmentSampled,
    )
    .reads(VISIBILITY_INDIRECT, RenderGraphResourceUsage::StorageBuffer)
    .draw_list(RenderDrawListKind::OpaqueForward);

    let deferred = RenderGraphPassDesc::new(
        DEFERRED_LIGHTING,
        "newviso.main.deferred_lighting",
        RenderGraphPassKind::DeferredLighting,
    )
    .with_domain(RenderGraphPassDomain::Render3d)
    .reads(GBUFFER_ALBEDO, RenderGraphResourceUsage::SampledTexture)
    .reads(GBUFFER_NORMAL, RenderGraphResourceUsage::SampledTexture)
    .reads(GBUFFER_MATERIAL, RenderGraphResourceUsage::SampledTexture)
    .reads(GBUFFER_DEPTH, RenderGraphResourceUsage::SampledTexture)
    .writes(SCENE_HDR, RenderGraphResourceUsage::ColorAttachment);

    let transparent = RenderGraphPassDesc::new(
        TRANSPARENT,
        "newviso.main.transparent",
        RenderGraphPassKind::Transparent,
    )
    .with_domain(RenderGraphPassDomain::Render3d)
    .reads(GBUFFER_DEPTH, RenderGraphResourceUsage::DepthAttachment)
    .writes(SCENE_HDR, RenderGraphResourceUsage::ColorAttachment)
    .draw_list(RenderDrawListKind::Transparent);

    let ssr = RenderGraphPassDesc::new(
        SSR,
        "newviso.main.ssr",
        RenderGraphPassKind::ScreenSpaceReflections,
    )
    .with_domain(RenderGraphPassDomain::PostProcess)
    .reads(SCENE_HDR, RenderGraphResourceUsage::SampledTexture)
    .reads(GBUFFER_DEPTH, RenderGraphResourceUsage::SampledTexture)
    .reads(GBUFFER_NORMAL, RenderGraphResourceUsage::SampledTexture)
    .reads(GBUFFER_MATERIAL, RenderGraphResourceUsage::SampledTexture)
    .writes(SSR_SIGNAL, RenderGraphResourceUsage::ColorAttachment);

    // Bloom is an explicit graph side-signal. The provider owns prefilter,
    // downsample and upsample targets; the root graph owns only the final HDR
    // bloom composite consumed by the tonemap pass.
    let bloom = RenderGraphPassDesc::new(
        BLOOM,
        "newviso.main.bloom",
        RenderGraphPassKind::BloomExtract,
    )
    .with_domain(RenderGraphPassDomain::PostProcess)
    .reads(SCENE_HDR, RenderGraphResourceUsage::SampledTexture)
    .writes(BLOOM_SIGNAL, RenderGraphResourceUsage::ColorAttachment);

    let postfx =
        RenderGraphPassDesc::new(POSTFX, "newviso.main.postfx", RenderGraphPassKind::PostFx)
            .with_domain(RenderGraphPassDomain::PostProcess)
            .reads(SCENE_HDR, RenderGraphResourceUsage::SampledTexture)
            .reads(GBUFFER_DEPTH, RenderGraphResourceUsage::SampledTexture)
            .reads(SSR_SIGNAL, RenderGraphResourceUsage::SampledTexture)
            .reads(BLOOM_SIGNAL, RenderGraphResourceUsage::SampledTexture)
            .writes(SURFACE, RenderGraphResourceUsage::ColorAttachment);

    let mut graph = RenderGraphDesc::new("newviso.main.deferred_hdr.v2")
        .add_resource(surface)
        .add_resource(gbuffer_albedo)
        .add_resource(gbuffer_normal)
        .add_resource(gbuffer_material)
        .add_resource(gbuffer_depth)
        .add_resource(scene_hdr)
        .add_resource(ssr_signal)
        .add_resource(bloom_signal)
        .add_resource(visibility_candidates)
        .add_resource(visibility_indirect)
        .add_pass(visibility)
        .add_pass(background)
        .add_pass(gbuffer)
        .add_pass(deferred)
        .add_pass(transparent)
        .add_pass(ssr)
        .add_pass(bloom)
        .add_pass(postfx);
    graph.frame_index = frame_index;
    graph.camera = camera.sanitized();
    graph.visibility.gpu_visibility_enabled = true;
    graph.visibility.hiz_enabled = true;
    // These legacy planning flags are intentionally disabled until their actual
    // shader/dispatch implementations exist again in the provider.
    graph.visibility.pvs_sort_enabled = false;
    graph.visibility.zone_cull_enabled = false;
    graph
}
