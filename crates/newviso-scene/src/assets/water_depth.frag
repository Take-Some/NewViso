#version 450
#extension GL_GOOGLE_include_directive : require
layout(set = 0, binding = 0, std140) uniform SceneFrame {
    mat4 view_proj;
    mat4 shadow_view_proj;
    vec4 globals;
    vec4 shadow_params;
    vec4 light_meta[16];
    vec4 light_pos[16];
    vec4 light_dir[16];
    vec4 light_color[16];
    vec4 light_cone[16];
    vec4 camera_position;
    vec4 environment_ambient;
    vec4 environment_fog_color_density;
    vec4 environment_fog_params;
    vec4 environment_haze_color_density;
    vec4 environment_haze_params;
    vec4 environment_clear_color;
    vec4 environment_reference[64];
    vec4 water_impacts[8];
    vec4 water_surface;
    vec4 water_optics;
    vec4 water_pass;
} frame;
layout(location=0) in vec4 v_color;
layout(location=1) in vec3 v_normal;
layout(location=2) in vec3 v_world_position;
layout(location=3) in vec4 v_shadow_coord;
layout(location=4) flat in float v_overlay;
layout(location=5) in vec2 v_uv;
layout(location=6) in vec4 v_tangent;

layout(set=1,binding=0) uniform texture2D t_base_color;
layout(set=1,binding=5) uniform sampler s_material;
layout(set=1,binding=6,std140) uniform MaterialParams {vec4 shading0;vec4 shading1;vec4 shading2;} material;
layout(location=0) out float out_distance;
void main() {
    int flags=int(material.shading1.z+.5);
    if((flags&8)!=0 && texture(sampler2D(t_base_color,s_material),v_uv).a<material.shading1.y)discard;
    if((flags&16)!=0)discard;
    out_distance=length(v_world_position-frame.camera_position.xyz);
}
