#version 450

layout(set = 0, binding = 0, std140) uniform SkyFrame {
    vec4 right;
    vec4 up;
    vec4 forward;
    vec4 projection;
    vec4 visual_globals;
    vec4 visual_dir_size[4];
    vec4 visual_color_intensity[4];
    vec4 visual_halo_kind[4];
    vec4 cloud_params;
    vec4 cloud_motion;
} sky;

layout(location = 0) in vec3 in_position;
layout(location = 1) in vec2 in_uv;
layout(location = 0) out vec2 v_uv;
layout(location = 1) out vec3 v_direction;
layout(location = 2) noperspective out vec2 v_view_plane;

void main() {
    // The imported dome is used only for topology.  Directional sky shading is
    // generated from the normalized dome direction, so broken/odd model UVs
    // cannot stretch clouds across the screen.
    vec3 dome_direction = normalize(in_position);
    float view_x = dot(sky.right.xyz, dome_direction);
    float view_y = dot(sky.up.xyz, dome_direction);
    float view_z = dot(sky.forward.xyz, dome_direction);

    gl_Position = vec4(
        view_x * sky.projection.x,
        -view_y * sky.projection.y,
        view_z,
        view_z
    );

    v_uv = in_uv;
    v_direction = dome_direction;

    // x/z and y/z are linear in screen space after perspective division.
    // noperspective interpolation reconstructs the exact per-pixel view ray
    // and removes cloud scale changes caused by sparse/uneven dome topology.
    float safe_view_z = max(view_z, 0.0001);
    v_view_plane = vec2(view_x, view_y) / safe_view_z;
}
