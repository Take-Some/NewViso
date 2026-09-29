#version 450

layout(set = 0, binding = 0, std140) uniform AtmosphericCloudFrame {
    mat4 view_proj;
    mat4 model;
    vec4 camera_alpha;
    vec4 sun_direction_intensity;
    vec4 layer_color_density;
    vec4 layer_params;
    vec4 uv_layer0;
    vec4 uv_layer1;
    vec4 uv_layer2;
    vec4 cloud_day_shadow;
    vec4 cloud_day_light;
    vec4 cloud_night;
    vec4 soft_depth_params;
    vec4 density_shift_scale;
    vec4 scatter;
    vec4 piercing;
    vec4 scale_diffuse_fill_ambient;
    vec4 wrap_lighting;
    vec4 anim_combine;
    vec4 anim_sculpt;
    vec4 anim_blend_weights;
    vec4 rescale_uv12;
    vec4 rescale_uv3_layer1;
    vec4 layer_anim_scale23;
} cloud;

layout(location = 0) in vec3 in_position;
layout(location = 1) in vec3 in_normal;
layout(location = 2) in vec4 in_tangent;
layout(location = 3) in vec4 in_color;
layout(location = 4) in vec2 in_uv;

layout(location = 0) out vec2 v_uv0;
layout(location = 1) out vec2 v_uv1;
layout(location = 2) out vec2 v_uv2;
layout(location = 3) out vec3 v_normal;
layout(location = 4) out vec3 v_tangent;
layout(location = 5) out vec3 v_binormal;
layout(location = 6) out vec4 v_color;
layout(location = 7) out vec3 v_world_position;

void main() {
    vec4 world = cloud.model * vec4(in_position, 1.0);
    gl_Position = cloud.view_proj * world;
    v_world_position = world.xyz;

    mat3 basis = mat3(cloud.model);
    vec3 normal_ws = normalize(basis * in_normal);
    vec3 tangent_ws = normalize(basis * in_tangent.xyz);
    vec3 binormal_ws = normalize(basis * (cross(in_normal, in_tangent.xyz) * in_tangent.w));
    v_normal = normal_ws;
    v_tangent = tangent_ws;
    v_binormal = binormal_ws;
    v_color = in_color;

    vec2 rescale0 = cloud.rescale_uv12.xy;
    vec2 rescale1 = cloud.rescale_uv12.zw;
    vec2 rescale2 = cloud.rescale_uv3_layer1.xy;
    vec2 anim_scale0 = cloud.rescale_uv3_layer1.zw;
    vec2 anim_scale1 = cloud.layer_anim_scale23.xy;
    vec2 anim_scale2 = cloud.layer_anim_scale23.zw;

    // GTA CloudsVS:
    // uvN = sourceUV * gRescaleUVN + gUVOffsetN
    //     + animatedOffsetN * cloudLayerAnimScaleN.
    // Native CloudHat material gUVOffsetN is zero in the shipped v_clouds
    // closure; clouds.xml mUVVelocity drives the animated offsets.
    v_uv0 = in_uv * rescale0 + cloud.uv_layer0.xy * anim_scale0;
    v_uv1 = in_uv * rescale1 + cloud.uv_layer1.xy * anim_scale1;
    v_uv2 = in_uv * rescale2 + cloud.uv_layer2.xy * anim_scale2;
}
