// Engine-owned shared receiver for both forward and GBuffer materials.
// Uses the actual R32Float shadow map written by scene/shadow.frag.
// No second shadow pipeline or post-effect whose result may never be sampled.
#ifndef NEWVISO_SCENE_PCSS_GLSL
#define NEWVISO_SCENE_PCSS_GLSL

const vec2 NV_PCSS_DISK_32[32] = vec2[32](
    vec2(+0.125000, +0.000000),
    vec2(-0.159645, +0.146248),
    vec2(+0.024436, -0.278438),
    vec2(+0.201222, +0.262459),
    vec2(-0.369268, -0.065318),
    vec2(+0.349802, -0.222516),
    vec2(-0.117002, +0.435242),
    vec2(-0.223136, -0.429634),
    vec2(+0.484115, +0.176798),
    vec2(-0.503641, +0.207896),
    vec2(+0.242788, -0.518824),
    vec2(+0.179414, +0.572001),
    vec2(-0.540757, -0.313380),
    vec2(+0.634370, -0.139464),
    vec2(-0.387146, +0.550675),
    vec2(-0.089440, -0.690200),
    vec2(+0.549072, +0.462758),
    vec2(-0.738878, +0.030555),
    vec2(+0.538955, -0.536332),
    vec2(-0.036058, +0.779792),
    vec2(-0.512818, -0.614527),
    vec2(+0.812360, +0.109302),
    vec2(-0.688311, +0.478909),
    vec2(+0.188086, -0.836061),
    vec2(+0.435033, +0.759191),
    vec2(-0.850448, -0.271316),
    vec2(+0.826102, -0.381680),
    vec2(-0.357888, +0.855156),
    vec2(-0.319407, -0.888034),
    vec2(+0.849909, +0.446688),
    vec2(-0.944035, +0.248845),
    vec2(+0.536596, -0.834530)
);

float nv_pcss_depth(vec2 uv, vec2 texel) {
    return texture(
        sampler2D(t_shadow, s_shadow),
        clamp(uv, texel * 1.5, vec2(1.0) - texel * 1.5)
    ).r;
}

// Manual bilinear PCF: interpolate four *comparisons* to avoid pixel-edge jumps.
float nv_pcss_shadow_test(vec2 uv, vec2 texel, float receiver_depth, float bias) {
    ivec2 size_px = textureSize(sampler2D(t_shadow, s_shadow), 0);
    vec2 pixel = clamp(uv / texel - 0.5, vec2(0.0), vec2(size_px) - 1.0);
    ivec2 first = ivec2(floor(pixel));
    ivec2 next_pixel = min(first + ivec2(1), size_px - ivec2(1));
    vec2 weight = fract(pixel);
    float d00 = texelFetch(sampler2D(t_shadow,s_shadow), first, 0).r;
    float d10 = texelFetch(sampler2D(t_shadow,s_shadow), ivec2(next_pixel.x,first.y), 0).r;
    float d01 = texelFetch(sampler2D(t_shadow,s_shadow), ivec2(first.x,next_pixel.y), 0).r;
    float d11 = texelFetch(sampler2D(t_shadow,s_shadow), next_pixel, 0).r;
    float z = receiver_depth - bias;
    vec4 occ = vec4(
      d00 < 0.99999 && z > d00 ? 1.0 : 0.0,
      d10 < 0.99999 && z > d10 ? 1.0 : 0.0,
      d01 < 0.99999 && z > d01 ? 1.0 : 0.0,
      d11 < 0.99999 && z > d11 ? 1.0 : 0.0
    );
    return mix(mix(occ.x,occ.y,weight.x),mix(occ.z,occ.w,weight.x),weight.y);
}

float nv_pcss_filter(vec2 uv, vec2 texel, float receiver_depth,
                     float bias, float radius_texels, mat2 rotation) {
    float occluded = 0.0;
    float weights = 0.0;
    // Poisson disk edge integration: soft even at high-res shadow texels.
    for (int i = 0; i < 24; ++i) {
        vec2 disk = NV_PCSS_DISK_32[i];
        vec2 sample_uv = uv + (rotation * disk) * radius_texels * texel;
        float weight = 1.0 - 0.24 * dot(disk, disk);
        occluded += nv_pcss_shadow_test(sample_uv, texel, receiver_depth, bias) * weight;
        weights += weight;
    }
    return occluded / max(weights, 1.0e-5);
}

float sample_shadow(vec3 normal, float n_dot_l) {
    if (frame.globals.w < 0.5) return 0.0;

    vec3 n = normalize(normal);
    // Receiver-plane normal offset reduces acne without erasing contact shadows.
    float normal_scale = frame.shadow_params.y * (1.0 - clamp(n_dot_l, 0.0, 1.0));
    vec3 receiver_position = v_world_position + n * normal_scale;
    vec4 shadow_clip = frame.shadow_view_proj * vec4(receiver_position, 1.0);
    if (abs(shadow_clip.w) < 1.0e-6) return 0.0;

    vec3 projected = shadow_clip.xyz / shadow_clip.w;
    vec2 uv = projected.xy * 0.5 + 0.5;
    float receiver_depth = projected.z;
    if (any(lessThanEqual(uv, vec2(0.0))) ||
        any(greaterThanEqual(uv, vec2(1.0))) ||
        receiver_depth <= 0.0 || receiver_depth >= 1.0) {
        return 0.0;
    }

    ivec2 extent = textureSize(sampler2D(t_shadow, s_shadow), 0);
    vec2 texture_size = max(vec2(extent), vec2(1.0));
    vec2 texel = 1.0 / texture_size;
    float resolution = min(texture_size.x, texture_size.y);
    float softness = clamp(frame.shadow_params.w, 0.25, 4.0);
    // Receiver-plane depth bias: compute the amount the illuminated surface
    // depth changes across ONE shadow texel (not a screen pixel).
    // This is the actual fix for the dense contour-line self-shadow acne
    // produced by a near-zero constant bias on large flat receivers.
    vec2 duvdx = dFdx(uv);
    vec2 duvdy = dFdy(uv);
    float dzdx = dFdx(receiver_depth);
    float dzdy = dFdy(receiver_depth);
    float determinant = duvdx.x * duvdy.y - duvdx.y * duvdy.x;
    vec2 depth_gradient = vec2(0.0);
    if (abs(determinant) > 1.0e-10) {
        depth_gradient = vec2(
            (dzdx * duvdy.y - dzdy * duvdx.y) / determinant,
            (duvdx.x * dzdy - duvdy.x * dzdx) / determinant
        );
    }
    float receiver_plane_bias = dot(
        abs(depth_gradient), texel
    );
    float compare_bias = max(frame.shadow_params.x, 0.00004)
        + min(receiver_plane_bias * 1.65, 0.0030);

    // Constant kernel orientation: no discontinuity at shadow texel cell borders.
    const mat2 rotation = mat2(0.9063078, -0.4226183, 0.4226183, 0.9063078);

    // Blocker search: closer casters produce a tighter penumbra.
    float search_radius = clamp(resolution * 0.00125 * softness, 3.0, 15.0);
    float blocker_sum = 0.0;
    float blocker_count = 0.0;
    for (int i = 0; i < 16; ++i) {
        vec2 sample_uv = uv + rotation * NV_PCSS_DISK_32[i * 2]
            * search_radius * texel;
        float candidate = nv_pcss_depth(sample_uv, texel);
        if (candidate > 0.00001 && candidate < 0.99999) {
            float gap = receiver_depth - compare_bias - candidate;
            float confidence = smoothstep(0.0, max(fwidth(receiver_depth), 0.00008), gap);
            blocker_sum += candidate * confidence;
            blocker_count += confidence;
        }
    }
    // Fully lit neighborhoods do not need the expensive 24-tap filter.
    // A genuine contact edge will have local blockers and take the PCSS path.
    if (blocker_count < 0.001 &&
        nv_pcss_shadow_test(uv, texel, receiver_depth, compare_bias) < 0.001) {
        return 0.0;
    }
    // Blend contact softness into distance-sensitive PCSS continuously.
    float average_blocker = blocker_sum / max(blocker_count, 1.0e-5);
    float separation = max(receiver_depth - average_blocker, 0.0);
    float radius = clamp(1.15 + separation * resolution * 0.18 * softness,
                         1.15, 20.0 * softness);
    float penumbra = mix(1.15, radius, smoothstep(0.0, 5.0, blocker_count));

    // Manual PCF works with the engine's R32Float depth-color attachment;
    // texture2DShadow cannot sample it directly.
    return clamp(
        nv_pcss_filter(uv, texel, receiver_depth, compare_bias, penumbra, rotation),
        0.0, 1.0
    );
}

#endif
