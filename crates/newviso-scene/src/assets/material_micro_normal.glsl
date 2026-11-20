// Signed source REDengine microblend normal detail, using the UV coordinates
// authored for the microblend, not the normal map's independent UV channel.
// Reconstruct the tangent frame from screen-space derivatives so UV0/UV1 can
// have unrelated orientations without reusing the wrong vertex tangent basis.
vec3 nv_ml_microblend_normal(vec3 world_normal, int flags) {
    if ((flags & MATERIAL_NEWVISO_MULTILAYER) == 0 ||
        abs(material.shading4.x) < 0.00001) {
        return world_normal;
    }
    vec2 uv = nv_material_uv(flags, NV_UV_MICROBLEND1, v_uv, v_uv1)
        * material.shading2.y + material.shading1.xy;
    vec2 packed_xy = texture(sampler2D(t_environment, s_material), uv).rg;
    vec2 micro_normal_xy = packed_xy * 2.0 - 1.0;

    vec3 pos_x = dFdx(v_world_position);
    vec3 pos_y = dFdy(v_world_position);
    vec2 uv_x = dFdx(uv);
    vec2 uv_y = dFdy(uv);
    float determinant = uv_x.x * uv_y.y - uv_x.y * uv_y.x;
    if (abs(determinant) < 1e-7) {
        return world_normal;
    }
    vec3 tangent = (pos_x * uv_y.y - pos_y * uv_x.y) / determinant;
    vec3 bitangent = (pos_y * uv_x.x - pos_x * uv_y.x) / determinant;
    tangent -= world_normal * dot(world_normal, tangent);
    bitangent -= world_normal * dot(world_normal, bitangent);
    float tx_len = length(tangent);
    float bt_len = length(bitangent);
    if (tx_len < 1e-6 || bt_len < 1e-6) {
        return world_normal;
    }
    vec3 relief = tangent / tx_len * micro_normal_xy.x +
                  bitangent / bt_len * micro_normal_xy.y;
    return normalize(world_normal + relief * material.shading4.x);
}
