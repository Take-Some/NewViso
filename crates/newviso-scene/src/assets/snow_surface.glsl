// Shared snow material. This is intentionally applied ONLY to heightfields
// explicitly tagged mesh.heightfield.surface_kind="snow". Stable world-space
// detail avoids sliding when camera or deformable snow vertices move.
#ifndef NEWVISO_SNOW_SURFACE_GLSL
#define NEWVISO_SNOW_SURFACE_GLSL

bool nv_is_snow_surface(float marker) {
    // Private GPU vertex semantic. Authored material alpha stays opaque.
    return marker > 1.115 && marker < 1.135;
}
float nv_snow_hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.17, 311.71))) * 43758.5453);
}
float nv_snow_noise(vec2 p) {
    vec2 cell = floor(p);
    vec2 f = fract(p);
    vec2 smooth_f = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(nv_snow_hash(cell), nv_snow_hash(cell + vec2(1.0, 0.0)), smooth_f.x),
        mix(nv_snow_hash(cell + vec2(0.0, 1.0)), nv_snow_hash(cell + vec2(1.0, 1.0)), smooth_f.x),
        smooth_f.y
    );
}
// Return a tiny crystalline sparkle mask used only for illuminated snow.
float nv_snow_surface(in vec3 pos, in vec3 camera_pos, in vec3 strata,
                      inout vec4 albedo, inout vec3 normal,
                      inout float roughness, inout float specular_intensity) {
    float camera_distance = length(pos - camera_pos);
    float detail = 1.0 - smoothstep(14.0, 62.0, camera_distance);
    vec2 wp = pos.xz;
    float powder=clamp(strata.r,0.0,1.0);
    float crust=clamp(strata.g,0.0,1.0);
    float moisture=clamp(strata.b,0.0,1.0);
    // Three spatial bands in METRES, not stretched mesh UVs.
    float sastrugi = nv_snow_noise(wp * 0.38);
    float clumps = nv_snow_noise(wp * 3.3 + vec2(4.6, 1.1));
    float crystals = nv_snow_noise(wp * 26.0 - vec2(3.1, 4.2));
    float wind_lines = sin(wp.x * 4.7 + wp.y * 1.8 + sastrugi * 6.0);
    // Geometric slope still dominates the visual normal. The fine grain only
    // increases micro-surface scattering close to the eye.
    vec2 grain = vec2(
        sin(wp.x * 12.3 + clumps * 3.9) + 0.35 * sin(wp.y * 48.0),
        cos(wp.y * 13.1 - clumps * 3.4) + 0.35 * cos(wp.x * 44.0)
    );
    // Powder has a highly textured, softly scattered top. Crust and slush
    // damp those microfacets, leaving the real coarse geometry untouched.
    float fluffy=powder*(1.0-0.75*crust)*(1.0-0.8*moisture);
    normal = normalize(normal + vec3(grain.x, 0.0, grain.y) * (0.075 * detail * fluffy));
    float variation = (sastrugi - 0.5) * 0.065
                    + (clumps - 0.5) * 0.055 * detail
                    + wind_lines * 0.012 * detail;
    // Warmth under direct light is provided by existing coloured scene lights.
    // In shadow the diffuse reflectance is marginally colder and bluer.
    vec3 cold_tint = mix(vec3(0.86, 0.91, 1.02), vec3(1.01, 1.00, 0.985),
                         smoothstep(0.40, 0.95, normal.y));
    vec3 powder_tint=vec3(0.91,0.955,1.0);
    vec3 wet_tint=vec3(0.68,0.78,0.88);
    vec3 hard_tint=vec3(0.82,0.89,0.97);
    vec3 snow_tint=mix(powder_tint,hard_tint,crust*0.68);
    snow_tint=mix(snow_tint,wet_tint,moisture*0.8);
    // Low-amplitude forward scattering approximation at silhouette angles:
    // powder returns diffuse light rather than hard plastic highlights.
    float view_facing=clamp(dot(normal,normalize(camera_pos-pos)),0.0,1.0);
    float rim=pow(1.0-view_facing,3.0)*powder*(1.0-moisture)*detail*0.055;
    albedo.rgb=clamp(albedo.rgb*cold_tint*snow_tint*
          (1.0+variation+rim),vec3(0.0),vec3(1.0));
    roughness=clamp(0.84 + 0.1*(clumps-0.5)*fluffy
        - 0.24*crust - 0.39*moisture, 0.22, 0.96);
    specular_intensity=0.1 + 0.12*crust + 0.28*moisture + 0.055*detail;
    // Subpixel anti-aliasing: tiny crystals glint only in direct illumination.
    float hash = nv_snow_hash(floor(wp * 92.0));
    float sparkle = smoothstep(0.992, 1.0, hash)*detail*
        (0.2+0.8*powder)*(1.0-0.9*moisture);
    return sparkle;
}
#endif
