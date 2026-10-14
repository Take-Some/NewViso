#version 450
layout(set=0,binding=0,std140) uniform WaterFog {
    vec4 camera; vec4 forward; vec4 right; vec4 up;
    vec4 medium; vec4 ambient; vec4 sun_color; vec4 sun_direction; vec4 optics;
} fog;
layout(set=0,binding=1) uniform texture2D scene_color;
layout(set=0,binding=2) uniform texture2D scene_depth;
layout(set=0,binding=3) uniform texture2D water_color;
layout(set=0,binding=4) uniform texture2D caustic;
layout(set=0,binding=5) uniform texture2D bump;
layout(set=0,binding=6) uniform sampler s_linear;
layout(set=0,binding=7) uniform texture2D geometry_depth;
layout(location=0) in vec2 v_uv;
layout(location=0) out vec4 out_color;
void main() {
    vec3 original=texture(sampler2D(scene_color,s_linear),v_uv).rgb;
    ivec2 size=textureSize(sampler2D(scene_depth,s_linear),0);
    ivec2 pixel=clamp(ivec2(v_uv*vec2(size)),ivec2(0),size-1);
    float ndc=texelFetch(sampler2D(scene_depth,s_linear),pixel,0).r;
    vec2 clip=v_uv*2.-1.;
    // NewViso's Vulkan projection inverts clip Y.
    vec3 ray=fog.forward.xyz+fog.right.xyz*clip.x-fog.up.xyz*clip.y;
    float near=fog.forward.w,far=fog.right.w;
    float linear_depth=near*far/max(far-ndc*(far-near),.00001);
    vec3 world=fog.camera.xyz+ray*linear_depth;
    vec3 view=normalize(ray);
    // The source depth blend uses forward depth and the unnormalised view ray.
    // Stop the water path where that ray leaves through the surface.
    float exit_depth=ray.y>0.00001?max((fog.camera.w-fog.camera.y)/ray.y,0.):linear_depth;
    float water_depth=min(linear_depth,exit_depth);
    vec3 color=max(texture(sampler2D(water_color,s_linear),world.xz/4096.+.5).rgb,vec3(.025,.12,.18));
    // The half-resolution R32 target stores the original opaque scene's
    // Euclidean distance before water writes the main depth attachment.
    ivec2 geometry_size=textureSize(sampler2D(geometry_depth,s_linear),0);
    ivec2 geometry_pixel=clamp(ivec2(v_uv*vec2(geometry_size)),ivec2(0),geometry_size-1);
    float object_distance=texelFetch(sampler2D(geometry_depth,s_linear),geometry_pixel,0).r;
    vec3 object_world=fog.camera.xyz+view*object_distance;
    float visible_distance=linear_depth*length(ray);
    float visible_geometry=step(abs(object_distance-visible_distance),
        max(.05,object_distance*.01))*step(object_distance,far);
    float time=fog.up.w*fog.optics.x;
    vec2 normal=(texture(sampler2D(bump,s_linear),object_world.xz/50.+time).ga-.5)*vec2(1.,.3);
    vec3 light_pattern=texture(sampler2D(caustic,s_linear),object_world.xz-15.*time+normal).rgb*
        texture(sampler2D(caustic,s_linear),(object_world.zx+15.*time)*.64546+normal).rgb;
    float object_depth=max(fog.camera.w-object_world.y,0.);
    float below=step(object_world.y,fog.camera.w-.01)*visible_geometry;
    float sunlight=max(-fog.sun_direction.y,0.);
    // Source dual animated caustic samples; light loses energy with water depth.
    float caustic_fade=below*exp(-object_depth/max(fog.optics.y,.1))*sunlight;
    vec3 refracted=original*(1.+light_pattern*fog.medium.w*caustic_fade);
    vec2 extinction=clamp(exp(vec2(-20.,-60.*abs(ray.y))*fog.medium.y*water_depth*2.71828183),0.,1.);
    vec3 tinted=mix(2.*color*refracted,refracted,extinction.y);
    vec3 scatter=color*color*(fog.ambient.rgb+fog.sun_color.rgb*sunlight)*fog.optics.w;
    float pierce=pow(max(dot(view,-fog.sun_direction.xyz),0.),2.)*clamp(water_depth/10.,0.,1.);
    scatter+=color*fog.sun_color.rgb*pierce*fog.optics.z;
    vec3 resolved=mix(scatter,tinted,extinction.x);
    out_color=vec4(max(mix(original,resolved,fog.medium.x),vec3(0)),1.);
}
