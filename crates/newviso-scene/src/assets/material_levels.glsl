// REDengine multilayer roughness level remapping. The two values are
// authored white/black endpoints; output is linearly interpolated after
// input normalization, without treating roughness as metalness.
float nv_remap_roughness(float value, vec4 levels) {
    float span = levels.x - levels.y;
    float level_t = abs(span) < 1e-6
        ? clamp(value, 0.0, 1.0)
        : clamp((value - levels.y) / span, 0.0, 1.0);
    return clamp(mix(levels.w, levels.z, level_t), 0.035, 1.0);
}
