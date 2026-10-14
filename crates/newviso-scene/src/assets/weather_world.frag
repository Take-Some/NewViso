#version 450

layout(set = 0, binding = 1) uniform texture2D t_diffuse;
layout(set = 0, binding = 2) uniform texture2D t_distortion;
layout(set = 0, binding = 3) uniform texture2D t_splash;
layout(set = 0, binding = 4) uniform sampler s_weather;

layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec4 v_color;
layout(location = 2) in float v_life;
layout(location = 3) in float v_distance_fade;
layout(location = 4) in float v_alpha_fade;
layout(location = 5) in vec4 v_uv_bounds;
layout(location = 6) in float v_snow;
layout(location = 0) out vec4 out_color;

void main() {
    // Keep filtering within this frame of the source sprite sheet.
    vec2 texel_size = 1.0 / vec2(textureSize(sampler2D(t_diffuse,s_weather),0));
    vec2 uv = clamp(v_uv, v_uv_bounds.xy+texel_size*0.5, v_uv_bounds.zw-texel_size*0.5);
    vec4 texel = texture(sampler2D(t_diffuse,s_weather),uv);
    float alpha = texel.a*v_color.a*v_distance_fade*v_alpha_fade;
    vec3 colour=texel.rgb*v_color.rgb;
    if (v_snow > 0.5) {
        // Imported mist RGB describes density. Treat it as coverage for
        // scattering white ice crystals, rather than rendering dark smoke.
        float density=max(texel.r,max(texel.g,texel.b));
        alpha*=density;
        colour=mix(vec3(0.90,0.94,1.0),vec3(1.0),density)*v_color.rgb;
    }
    if (alpha <= 0.001) discard;
    out_color = vec4(colour,alpha);
}
