// Shared UV-role selection for every NewViso native multilayer material.
// Material parameters pick UV0 or UV1 independently; both streams originate
// in the imported mesh and remain valid after vertex/skin deformation.
const int NV_UV_COLOR1      = 65536;    // 1 << 16
const int NV_UV_NORMAL1     = 131072;   // 1 << 17
const int NV_UV_ROUGHNESS1  = 262144;   // 1 << 18
const int NV_UV_MASK1       = 524288;   // 1 << 19
const int NV_UV_MICROBLEND1 = 1048576;  // 1 << 20

vec2 nv_material_uv(int flags, int selector, vec2 uv0, vec2 uv1) {
    return (flags & selector) != 0 ? uv1 : uv0;
}
