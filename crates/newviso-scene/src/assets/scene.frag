#version 450

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
} frame;

layout(set = 0, binding = 1) uniform texture2D t_shadow;
layout(set = 0, binding = 2) uniform sampler s_shadow;

layout(set = 1, binding = 0) uniform texture2D t_base_color;
layout(set = 1, binding = 1) uniform texture2D t_normal;
layout(set = 1, binding = 2) uniform texture2D t_specular;
layout(set = 1, binding = 3) uniform texture2D t_emissive;
layout(set = 1, binding = 4) uniform texture2D t_environment;
layout(set = 1, binding = 5) uniform sampler s_material;
layout(set = 1, binding = 6, std140) uniform MaterialParams {
    vec4 shading0; // x bumpiness, y spec intensity, z spec falloff, w spec fresnel
    vec4 shading1; // x emissive multiplier, y alpha cutoff, z flags, w opacity
    vec4 shading2; // x environment reflection strength
} material;

layout(set = 2, binding = 0) uniform texture2D t_weather_puddle_layout;
layout(set = 2, binding = 1) uniform texture2D t_weather_puddle_normal;
layout(set = 2, binding = 2) uniform sampler s_weather;
layout(set = 2, binding = 3, std140) uniform WeatherMaterial {
    vec4 state0; // x rain, y accumulated wetness, z lightning flash, w time
    vec4 state1; // x ripple scale, y ripple bumpiness, z wind speed, w puddle frame
} weather;

layout(location = 0) in vec4 v_color;
layout(location = 1) in vec3 v_normal;
layout(location = 2) in vec3 v_world_position;
layout(location = 3) in vec4 v_shadow_coord;
layout(location = 4) flat in float v_overlay;
layout(location = 5) in vec2 v_uv;
layout(location = 6) in vec4 v_tangent;
layout(location = 0) out vec4 out_color;

const int MATERIAL_HAS_NORMAL = 1;
const int MATERIAL_HAS_SPECULAR = 2;
const int MATERIAL_HAS_EMISSIVE = 4;
const int MATERIAL_ALPHA_TEST = 8;
const int MATERIAL_ALPHA_BLEND = 16;
const int MATERIAL_ENVIRONMENT_REFLECTION = 32;
const int MATERIAL_USE_VERTEX_COLOR = 64;
const int MATERIAL_HAS_ENVIRONMENT_TEXTURE = 128;

vec2 dash_rotate(vec2 p, float a) {
    return mat2(cos(a),sin(a),-sin(a),cos(a))*p;
}

float dash_segment(vec2 p, vec2 a, vec2 b) {
    vec2 d=b-a;
    return length(p-a-d*clamp(dot(p-a,d)/dot(d,d),0.0,1.0));
}

// The imported atlas supplies the dial faces, lettering and original needle
// artwork. Remove only its stationary needle, then sample that same artwork at
// the live angle. The surrounding interior material keeps its authored UVs.
void dash_needle(inout vec4 color, vec2 pixel, vec2 size, vec2 center,
                 float radius, float value, float original_angle, float start_angle,
                 float sweep, bool red) {
    vec2 p=pixel-center;
    if(length(p)>radius) return;
    float zero=radians(original_angle);
    vec2 direction=vec2(cos(zero),sin(zero));
    vec2 side=vec2(-direction.y,direction.x);
    float along=dot(p,direction),across=abs(dot(p,side));
    float width=mix(radius*.055,radius*.009,clamp(along/(radius*.87),0.0,1.0));
    bool old_shape=along>-radius*.15 && along<radius*.89 && across<width+1.5;
    bool old_ink=red ? (color.r>color.g*1.5 && color.r>color.b*1.35) || max(max(color.r,color.g),color.b)<.02 : max(max(color.r,color.g),color.b)<.25;
    if(old_shape && old_ink && length(p)>radius*.10) {
        vec2 offset=side*max(radius*.10,5.0);
        vec4 a=texture(sampler2D(t_base_color,s_material),(pixel+offset)/size);
        vec4 b=texture(sampler2D(t_base_color,s_material),(pixel-offset)/size);
        color=mix(a,b,.5);
    }
    float angle=radians(start_angle+sweep*clamp(value,0.0,1.0));
    vec2 source=dash_rotate(p,zero-angle);
    float source_along=dot(source,direction),source_across=abs(dot(source,side));
    float source_width=mix(radius*.055,radius*.009,clamp(source_along/(radius*.87),0.0,1.0));
    if(source_along>-radius*.15 && source_along<radius*.89 && source_across<source_width+1.5) {
        vec4 needle=texture(sampler2D(t_base_color,s_material),(center+source)/size);
        bool ink=red ? (needle.r>needle.g*1.5 && needle.r>needle.b*1.35) || max(max(needle.r,needle.g),needle.b)<.02 : max(max(needle.r,needle.g),needle.b)<.14;
        if(ink) color=needle;
    }
}

void dash_lamp(inout vec4 color, vec2 pixel, vec2 center, vec2 extent, int lamps, int bit, vec3 tint) {
    if(any(greaterThan(abs(pixel-center),extent))) return;
    float ink=1.0-smoothstep(.005,.035,max(max(color.r,color.g),color.b));
    bool on=(lamps & (1<<bit))!=0;
    color.rgb=mix(color.rgb,on?tint:vec3(.035),ink);
    if(on) color.a=max(color.a,ink*.9);
}

float dash_digit(vec2 p, int digit) {
    const int masks[10]=int[10](63,6,91,79,102,109,125,7,127,111);
    int mask=masks[clamp(digit,0,9)];
    float d=10.0;
    if((mask&1)!=0)d=min(d,dash_segment(p,vec2(.15,.05),vec2(.85,.05)));
    if((mask&2)!=0)d=min(d,dash_segment(p,vec2(.9,.1),vec2(.9,.48)));
    if((mask&4)!=0)d=min(d,dash_segment(p,vec2(.9,.52),vec2(.9,.9)));
    if((mask&8)!=0)d=min(d,dash_segment(p,vec2(.15,.95),vec2(.85,.95)));
    if((mask&16)!=0)d=min(d,dash_segment(p,vec2(.1,.52),vec2(.1,.9)));
    if((mask&32)!=0)d=min(d,dash_segment(p,vec2(.1,.1),vec2(.1,.48)));
    if((mask&64)!=0)d=min(d,dash_segment(p,vec2(.15,.5),vec2(.85,.5)));
    return 1.0-smoothstep(.045,.085,d);
}

void dash_gear(inout vec4 color,vec2 pixel,vec2 center,int gear) {
    vec2 p=(pixel-center)/vec2(10,15)+.5;
    if(any(lessThan(p,vec2(0)))||any(greaterThan(p,vec2(1)))) return;
    float ink;
    if(gear>0) ink=dash_digit(p,gear);
    else {
        float d=dash_segment(p,vec2(.1,.05),vec2(.1,.95));
        if(gear==0) {
            d=min(d,min(dash_segment(p,vec2(.9,.05),vec2(.9,.95)),dash_segment(p,vec2(.1,.05),vec2(.9,.95))));
        } else {
            d=min(d,min(dash_segment(p,vec2(.1,.05),vec2(.85,.05)),dash_segment(p,vec2(.1,.5),vec2(.85,.5))));
            d=min(d,min(dash_segment(p,vec2(.85,.05),vec2(.85,.5)),dash_segment(p,vec2(.1,.5),vec2(.9,.95))));
        }
        ink=1.0-smoothstep(.045,.085,d);
    }
    color=vec4(mix(vec3(.025),vec3(.45,.55,.3),ink),1.0);
}

vec4 vehicle_dashboard(vec2 uv) {
    vec2 size=vec2(textureSize(sampler2D(t_base_color,s_material),0));
    vec2 pixel=uv*size;
    vec4 color=texture(sampler2D(t_base_color,s_material),uv);
    int dashboard_bits=int(material.shading2.w+.5),profile=dashboard_bits>>16,lamps=dashboard_bits&65535;
    int gear=(int(material.shading2.z)%16)-1;
    float speed=material.shading0.x,revs=material.shading0.y,fuel=material.shading0.z,temp=material.shading0.w;
    bool running=(lamps&(1<<10))!=0;
    if(profile==1) {
        dash_needle(color,pixel,size,vec2(82,133),79,speed/80.0,144,144,252,false);
        dash_needle(color,pixel,size,vec2(429,133),79,revs,144,144,252,false);
        dash_needle(color,pixel,size,vec2(205,86),37,fuel,270,225,90,false);
        dash_needle(color,pixel,size,vec2(309,86),37,running?.65:0.0,270,225,90,false);
        dash_lamp(color,pixel,vec2(216,142),vec2(9,9),lamps,0,vec3(.05,.9,.2));
        dash_lamp(color,pixel,vec2(296,142),vec2(9,9),lamps,1,vec3(.05,.9,.2));
        dash_lamp(color,pixel,vec2(276,142),vec2(9,9),lamps,2,vec3(1,.03,.02));
        dash_lamp(color,pixel,vec2(237,142),vec2(9,9),lamps,7,vec3(.05,.9,.2));
        dash_lamp(color,pixel,vec2(257,142),vec2(9,9),lamps,8,vec3(.03,.2,1));
        dash_lamp(color,pixel,vec2(14,230),vec2(10,9),lamps,4,vec3(1,.35,.02));
        dash_lamp(color,pixel,vec2(42,230),vec2(10,9),lamps,5,vec3(1,.35,.02));
        dash_lamp(color,pixel,vec2(67,230),vec2(10,9),lamps,9,vec3(1,.03,.02));
        dash_lamp(color,pixel,vec2(95,230),vec2(10,9),lamps,3,vec3(1,.35,.02));
        dash_lamp(color,pixel,vec2(124,230),vec2(11,9),lamps,6,vec3(1,.03,.02));
        dash_gear(color,pixel,vec2(258,178),gear);
    } else {
        dash_needle(color,pixel,size,vec2(135,164),112,revs,144,144,252,true);
        dash_needle(color,pixel,size,vec2(363,164),112,speed/180.0,144,144,252,true);
        dash_needle(color,pixel,size,vec2(68,332),62,temp,144,144,252,true);
        dash_needle(color,pixel,size,vec2(233,315),72,material.shading1.x,144,144,252,true);
        dash_needle(color,pixel,size,vec2(164,445),60,1.0-material.shading2.y,144,144,252,true);
        dash_needle(color,pixel,size,vec2(309,445),60,fuel,144,144,252,true);
        dash_needle(color,pixel,size,vec2(450,445),60,material.shading2.x,144,144,252,true);
        dash_lamp(color,pixel,vec2(95,18),vec2(12,10),lamps,7,vec3(.05,.9,.2));
        dash_lamp(color,pixel,vec2(133,18),vec2(12,10),lamps,8,vec3(.03,.2,1));
        dash_lamp(color,pixel,vec2(169,18),vec2(12,10),lamps,2,vec3(1,.03,.02));
        dash_lamp(color,pixel,vec2(204,18),vec2(12,10),lamps,4,vec3(1,.35,.02));
        dash_lamp(color,pixel,vec2(235,18),vec2(12,10),lamps,9,vec3(1,.03,.02));
        dash_lamp(color,pixel,vec2(270,18),vec2(12,10),lamps,6,vec3(1,.03,.02));
        dash_lamp(color,pixel,vec2(307,18),vec2(12,10),lamps,3,vec3(1,.35,.02));
        dash_lamp(color,pixel,vec2(338,18),vec2(12,10),lamps,5,vec3(1,.35,.02));
        dash_lamp(color,pixel,vec2(381,18),vec2(12,10),lamps,0,vec3(.05,.9,.2));
        dash_lamp(color,pixel,vec2(414,18),vec2(12,10),lamps,1,vec3(.05,.9,.2));
        vec2 readout=(pixel-vec2(326,135))/vec2(12,17);
        if(readout.x>=0.0&&readout.x<6.0&&readout.y>=0.0&&readout.y<=1.0) {
            int digit_index=int(readout.x),divisor=int(pow(10.0,float(5-digit_index)));
            int miles=int(material.shading2.z)/16;
            float ink=dash_digit(vec2(fract(readout.x),readout.y),(miles/divisor)%10);
            color=vec4(mix(vec3(.028),vec3(.25,.35,.18),ink),1.0);
        }
        dash_gear(color,pixel,vec2(363,237),gear);
    }
    // Preserve the original lettering and face transparency; illumination is
    // controlled by the vehicle rather than by exterior paint or weather.
    if((lamps&(1<<11))!=0) color.rgb*=1.25;
    return color;
}

float sample_shadow(vec3 normal, float n_dot_l) {
    if (frame.globals.w < 0.5) {
        return 0.0;
    }

    float normal_scale = frame.shadow_params.y * (1.0 - n_dot_l);
    vec3 receiver_position = v_world_position + normalize(normal) * normal_scale;
    vec4 shadow_coord = frame.shadow_view_proj * vec4(receiver_position, 1.0);
    if (abs(shadow_coord.w) < 1e-6) {
        return 0.0;
    }

    vec3 ndc = shadow_coord.xyz / shadow_coord.w;
    vec2 uv = ndc.xy * 0.5 + 0.5;
    float depth = ndc.z;
    if (uv.x <= 0.0 || uv.x >= 1.0 || uv.y <= 0.0 || uv.y >= 1.0
        || depth <= 0.0 || depth >= 1.0) {
        return 0.0;
    }

    float bias = max(frame.shadow_params.x, 0.000001);
    float inv_resolution = 1.0 / max(frame.shadow_params.z, 1.0);
    float occluded = 0.0;
    for (int y = -1; y <= 1; ++y) {
        for (int x = -1; x <= 1; ++x) {
            float stored = texture(
                sampler2D(t_shadow, s_shadow),
                uv + vec2(x, y) * inv_resolution
            ).r;
            occluded += (depth - bias > stored) ? 1.0 : 0.0;
        }
    }
    return occluded / 9.0;
}

vec3 rsc7_surface_normal(int flags) {
    vec3 n = normalize(v_normal);
    if ((flags & MATERIAL_HAS_NORMAL) == 0) {
        return n;
    }

    vec3 tangent = v_tangent.xyz - n * dot(v_tangent.xyz, n);
    if (dot(tangent, tangent) < 1e-8) {
        vec3 axis = abs(n.y) < 0.999 ? vec3(0.0, 1.0, 0.0) : vec3(1.0, 0.0, 0.0);
        tangent = cross(axis, n);
    }
    tangent = normalize(tangent);
    vec3 bitangent = normalize(cross(n, tangent)) * (v_tangent.w < 0.0 ? -1.0 : 1.0);

    vec3 encoded = texture(sampler2D(t_normal, s_material), v_uv).rgb;
    vec3 tangent_normal;
    if (encoded.b < 0.01) {
        vec2 xy = encoded.rg * 2.0 - 1.0;
        tangent_normal = vec3(xy, sqrt(max(1.0 - dot(xy, xy), 0.0)));
    } else {
        tangent_normal = encoded * 2.0 - 1.0;
    }
    tangent_normal.xy *= max(material.shading0.x, 0.0);
    tangent_normal = normalize(tangent_normal);

    return normalize(mat3(tangent, bitangent, n) * tangent_normal);
}

void main() {
    if (v_overlay > 0.5) {
        out_color = v_color;
        return;
    }

    int flags = int(material.shading1.z + 0.5);
    if((flags&512)!=0) {
        out_color=vehicle_dashboard(v_uv);
        if(out_color.a<=.002) discard;
        return;
    }
    vec4 base_texel = texture(sampler2D(t_base_color, s_material), v_uv);
    // COLOR0 is not implicitly an albedo tint. Imported RSC7 shaders often
    // use vertex-colour channels as masks/AO/auxiliary data; direct colour
    // modulation is therefore an explicit material capability.
    vec4 base_color = base_texel;
    bool damaged_glass = v_color.a < -0.5;
    if ((flags & MATERIAL_USE_VERTEX_COLOR) != 0 && !damaged_glass) {
        base_color *= v_color;
    }
    base_color.a *= clamp(material.shading1.w, 0.0, 1.0);

    if ((flags & 16384) != 0) {
        // Particle textures are authored colour/opacity, independent of car paint,
        // wetness, normals and the scene's surface-lighting model.
        int diffuse_mode = int(material.shading2.y + 0.5);
        if (diffuse_mode >= 1 && diffuse_mode <= 3) {
            float channel = diffuse_mode == 1 ? base_texel.r : (diffuse_mode == 2 ? base_texel.g : base_texel.b);
            base_color = vec4(channel) * v_color;
        }
        if (diffuse_mode == 4) {
            base_color.a = max(max(base_texel.r, base_texel.g), base_texel.b) * v_color.a;
        }
        if (diffuse_mode == 5) {
            base_color = vec4(mix(base_texel.r, base_texel.g, 0.5)) * v_color;
        }
        if (base_color.a <= 0.002) { discard; }
        out_color = base_color;
        return;
    }
    if ((flags & MATERIAL_ALPHA_TEST) != 0 && base_color.a < material.shading1.y) {
        discard;
    }
    if ((flags & MATERIAL_ALPHA_BLEND) != 0 && base_color.a <= 0.002) {
        discard;
    }
    if (damaged_glass) {
        vec2 delta = v_uv - v_color.xy;
        float radius = length(delta);
        float angle = atan(delta.y, delta.x);
        float damage = clamp(v_color.z, 0.0, 1.0);
        float reach = 0.08 + damage * 0.65;
        float width = max(length(fwidth(v_uv)), 0.001);
        float ray_distance = abs(sin(angle * 7.0 + sin(radius * 53.0) * 0.12)) * radius;
        float rays = 1.0 - smoothstep(width * 0.35, width * 1.5, ray_distance);
        float rings = 1.0 - smoothstep(width * 0.25, width, abs(sin(radius * 67.0 + sin(angle * 5.0) * 0.3)) * 0.015);
        float cracks = max(rays, rings * 0.5) * (1.0 - smoothstep(reach * 0.7, reach, radius));
        base_color.rgb = mix(base_color.rgb, vec3(0.65,0.78,0.82), cracks * 0.8);
        base_color.a = max(base_color.a, cracks * (0.35 + damage * 0.45));
    }
    vec3 n = rsc7_surface_normal(flags);

    // Weather wetness is global renderer state rather than a material flag.
    // Horizontal authored surfaces progressively darken, gain a moving GTA
    // puddle normal and become more specular while vertical walls remain
    // largely unchanged.
    float rain_amount = clamp(weather.state0.x, 0.0, 1.0);
    float accumulated_wetness = clamp(weather.state0.y, 0.0, 1.0);
    float wet_surface = 0.0;
    // Uniform across the draw, so implicit texture derivatives remain valid.
    // Dry weather preserves the original normal without two texture fetches.
    if (accumulated_wetness > 0.0) {
        float horizontal = smoothstep(0.28, 0.86, max(n.y, 0.0));
        vec2 weather_uv = v_world_position.xz * max(weather.state1.x, 0.001);
        float puddle_layout = texture(
            sampler2D(t_weather_puddle_layout, s_weather),
            weather_uv
        ).r;
        vec3 puddle_encoded = texture(
            sampler2D(t_weather_puddle_normal, s_weather),
            weather_uv * 1.75
        ).rgb;
        vec2 puddle_xy = puddle_encoded.rg * 2.0 - 1.0;
        float puddle_z = sqrt(max(1.0 - dot(puddle_xy, puddle_xy), 0.0));
        vec3 puddle_normal = normalize(vec3(puddle_xy.x, puddle_z, puddle_xy.y));
        float ripple_strength = clamp(weather.state1.y, 0.0, 2.0);
        puddle_normal = normalize(mix(
            vec3(0.0, 1.0, 0.0),
            puddle_normal,
            clamp(ripple_strength, 0.0, 1.0)
        ));
        wet_surface = accumulated_wetness
            * horizontal
            * mix(0.32, 1.0, clamp(puddle_layout, 0.0, 1.0));
        n = normalize(mix(n, puddle_normal, wet_surface * 0.48));
    }
    base_color.rgb *= mix(1.0, 0.72, wet_surface);

    vec3 v = normalize(frame.camera_position.xyz - v_world_position);
    vec3 diffuse_lighting =
        max(frame.environment_ambient.rgb, vec3(0.0))
        * max(frame.environment_ambient.a, 0.0);
    vec3 specular_lighting = vec3(0.0);

    vec3 specular_map = (flags & MATERIAL_HAS_SPECULAR) != 0
        ? texture(sampler2D(t_specular, s_material), v_uv).rgb
        : vec3(1.0);
    float specular_intensity = max(material.shading0.y, 0.0)
        + wet_surface * (0.9 + rain_amount * 1.4);
    float specular_falloff = mix(
        clamp(material.shading0.z, 1.0, 512.0),
        112.0,
        wet_surface
    );
    float authored_fresnel = clamp(material.shading0.w, 0.0, 1.0);
    float view_fresnel = pow(1.0 - max(dot(n, v), 0.0), 5.0);
    float fresnel_gain = mix(1.0, 0.25 + 0.75 * view_fresnel, authored_fresnel);

    int light_count = clamp(int(frame.globals.x + 0.5), 0, 16);
    int shadow_light_index = int(frame.globals.z + 0.5);

    for (int i = 0; i < light_count; ++i) {
        int light_type = int(frame.light_meta[i].x + 0.5);
        float intensity = max(frame.light_meta[i].y, 0.0);
        float range = max(frame.light_meta[i].z, 0.001);
        vec3 color = max(frame.light_color[i].rgb, vec3(0.0));
        vec3 l = vec3(0.0);
        float attenuation = 1.0;

        if (light_type == 0) {
            l = normalize(-frame.light_dir[i].xyz);
        } else {
            vec3 to_light = frame.light_pos[i].xyz - v_world_position;
            float distance_to_light = length(to_light);
            if (distance_to_light <= 1e-5 || distance_to_light >= range) {
                continue;
            }
            l = to_light / distance_to_light;
            float normalized_distance = distance_to_light / range;
            attenuation = 1.0 - normalized_distance;
            attenuation *= attenuation;

            if (light_type == 2) {
                vec3 from_light = -l;
                float cone_cos = dot(normalize(frame.light_dir[i].xyz), from_light);
                float inner_cos = frame.light_cone[i].x;
                float outer_cos = frame.light_cone[i].y;
                attenuation *= smoothstep(outer_cos, inner_cos, cone_cos);
            } else if (light_type == 3) {
                attenuation *= 0.75;
            }
        }

        float n_dot_l = max(dot(n, l), 0.0);
        if (n_dot_l <= 0.0) {
            continue;
        }

        float shadow = i == shadow_light_index ? sample_shadow(n, n_dot_l) : 0.0;
        float visibility = 1.0 - shadow;
        vec3 radiance = color * intensity * attenuation * visibility;
        diffuse_lighting += radiance * n_dot_l;

        if (specular_intensity > 0.0) {
            vec3 h = normalize(l + v);
            float n_dot_h = max(dot(n, h), 0.0);
            float specular_power = pow(n_dot_h, specular_falloff);
            specular_lighting += radiance
                * specular_map
                * specular_intensity
                * specular_power
                * fresnel_gain;
        }
    }

    vec3 emissive = vec3(0.0);
    if ((flags & MATERIAL_HAS_EMISSIVE) != 0) {
        emissive = texture(sampler2D(t_emissive, s_material), v_uv).rgb
            * max(material.shading1.x, 0.0);
    }

    vec3 environment_reflection = vec3(0.0);
    if ((flags & MATERIAL_ENVIRONMENT_REFLECTION) != 0) {
        vec3 reflected = normalize(reflect(-v, n));
        const float PI = 3.14159265359;
        vec2 env_uv = vec2(
            atan(reflected.z, reflected.x) / (2.0 * PI) + 0.5,
            asin(clamp(reflected.y, -1.0, 1.0)) / PI + 0.5
        );
        vec3 environment_color;
        if ((flags & MATERIAL_HAS_ENVIRONMENT_TEXTURE) != 0) {
            environment_color = texture(sampler2D(t_environment, s_material), env_uv).rgb;
        } else {
            // RSC7 glass_env/glass_pv_env can reference the global scene
            // environment without a material-local sampler. Approximate that
            // source-neutrally from the active NewViso environment instead of
            // sampling the intentionally black missing-texture fallback.
            float sky_factor = clamp(reflected.y * 0.5 + 0.5, 0.0, 1.0);
            vec3 ambient_env = max(
                frame.environment_ambient.rgb * max(frame.environment_ambient.a, 0.0),
                vec3(0.0)
            );
            vec3 background_env = max(frame.environment_clear_color.rgb, vec3(0.0));
            vec3 atmospheric_env = max(
                mix(
                    frame.environment_fog_color_density.rgb,
                    frame.environment_haze_color_density.rgb,
                    sky_factor
                ),
                background_env
            );
            environment_color = max(
                mix(background_env, atmospheric_env, 0.65),
                ambient_env
            );
        }
        float facing = max(dot(n, v), 0.0);
        float glass_fresnel = 0.08 + 0.92 * pow(1.0 - facing, 5.0);
        float reflection_gain =
            max(material.shading0.y, 0.20)
            * max(material.shading2.x, 0.0)
            * mix(0.45, 1.0, 1.0 - base_color.a);
        environment_reflection =
            environment_color * glass_fresnel * reflection_gain;
    }

    float wet_view_fresnel = pow(1.0 - max(dot(n, v), 0.0), 5.0);
    vec3 wet_environment = max(
        mix(
            frame.environment_clear_color.rgb,
            frame.environment_haze_color_density.rgb,
            0.45
        ),
        vec3(0.0)
    ) * wet_surface * (0.10 + 0.42 * wet_view_fresnel);
    vec3 lightning_ambient = vec3(0.92, 0.96, 1.0)
        * clamp(weather.state0.z, 0.0, 1.0)
        * 1.35;

    vec3 surface_color =
        base_color.rgb * (diffuse_lighting + lightning_ambient)
        + specular_lighting
        + environment_reflection
        + wet_environment
        + emissive;
    float camera_distance = length(v_world_position - frame.camera_position.xyz);

    float haze_distance = max(
        camera_distance - max(frame.environment_haze_params.x, 0.0),
        0.0
    );
    float haze = 1.0 - exp(
        -max(frame.environment_haze_color_density.a, 0.0) * haze_distance
    );
    surface_color = mix(
        surface_color,
        max(frame.environment_haze_color_density.rgb, vec3(0.0)),
        clamp(haze, 0.0, 1.0)
    );

    float fog_distance = max(
        camera_distance - max(frame.environment_fog_params.x, 0.0),
        0.0
    );
    float height_above_base = max(
        v_world_position.y - frame.environment_fog_params.z,
        0.0
    );
    float height_factor = exp(
        -height_above_base * max(frame.environment_fog_params.y, 0.0)
    );
    float fog = 1.0 - exp(
        -max(frame.environment_fog_color_density.a, 0.0)
        * fog_distance
        * height_factor
    );
    fog = min(
        clamp(fog, 0.0, 1.0),
        clamp(frame.environment_fog_params.w, 0.0, 1.0)
    );
    surface_color = mix(
        surface_color,
        max(frame.environment_fog_color_density.rgb, vec3(0.0)),
        fog
    );

    out_color = vec4(surface_color, base_color.a);
}
