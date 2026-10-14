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

#define ENV_LANE(i) frame.environment_reference[i]
#include "environment.glsl"
layout(set=0,binding=1) uniform texture2D t_shadow;
layout(set=0,binding=2) uniform sampler s_shadow;
#include "shadow_pcss.glsl"
layout(set=1,binding=0) uniform texture2D water_bump;
layout(set=1,binding=1) uniform texture2D water_bump2;
layout(set=1,binding=2) uniform texture2D water_foam;
layout(set=1,binding=3) uniform texture2D water_fog;
layout(set=1,binding=4) uniform texture2D water_noise;
layout(set=1,binding=5) uniform sampler s_material;
layout(set=3,binding=0,std140) uniform WaterOptics {
    mat4 reflection_view_proj;
    vec4 projection;
} optics;
layout(set=3,binding=1) uniform texture2D reflection_color;
layout(set=3,binding=2) uniform texture2D refraction_color;
layout(set=3,binding=3) uniform texture2D refraction_depth;
layout(set=3,binding=4) uniform texture2D dynamic_ripple;
layout(set=3,binding=5) uniform sampler s_screen;
layout(set=2,binding=1) uniform texture2D rain_ripple;
layout(set=2,binding=2) uniform sampler s_weather;
layout(set=2,binding=3,std140) uniform WeatherMaterial {vec4 state0;vec4 state1;} weather;
layout(location=0) out vec4 out_color;
vec2 screen_uv(vec4 p) {return p.xy/max(p.w,0.00001)*.5+.5;}
bool valid_uv(vec2 p) {return all(greaterThanEqual(p,vec2(0)))&&all(lessThanEqual(p,vec2(1)));}
float depth_at(vec2 uv) {
    // Source refraction depth uses point filtering to preserve silhouettes.
    ivec2 size=textureSize(sampler2D(refraction_depth,s_screen),0);
    ivec2 pixel=clamp(ivec2(floor(uv*vec2(size))),ivec2(0),size-1);
    return texelFetch(sampler2D(refraction_depth,s_screen),pixel,0).r;
}
void main() {
    if(v_normal.y<.1 || frame.water_optics.z<.5)discard;
    vec2 p=v_world_position.xz;
    // Far water must not cover the moving dynamic grid. The surfaces otherwise
    // overlap and produce flat triangles that erase the animated wave crests.
    vec2 grid_position=(p-frame.water_pass.zw)/2.;
    if(v_overlay<.5 && frame.water_optics.w>.5 &&
       all(greaterThanEqual(grid_position,vec2(0))) &&
       all(lessThanEqual(grid_position,vec2(255))))discard;
    float seconds=frame.camera_position.w;
    vec2 tex=p*frame.water_surface.y;
    // PC channel packing and the high-detail 3.7-frequency octave match the source.
    vec2 bump=texture(sampler2D(dynamic_ripple,s_material),tex).ba*2.-1.;
    bump+=texture(sampler2D(dynamic_ripple,s_material),tex*3.7).ba*2.-1.;
    // Source PSCopyRain adds four times the rain normal to the wet octave.
    bump+=4.*(textureLod(sampler2D(rain_ripple,s_weather),tex*3.7*2.,1.).rg*2.-1.)*clamp(weather.state0.x,0.,1.);
    float screen_depth=length(v_world_position-frame.camera_position.xyz);
    if(screen_depth>200.) {
        vec2 noise_offset=texture(sampler2D(water_noise,s_material),floor(tex)/256.).ag*100.;
        vec2 cell_value=2.*abs(.5-fract(tex));
        float cell_mask=pow(max(cell_value.x,cell_value.y),32.);
        vec2 a=texture(sampler2D(dynamic_ripple,s_material),tex+noise_offset).ba*2.-1.;
        vec2 b=texture(sampler2D(dynamic_ripple,s_material),tex).ba*2.-1.;
        bump=mix(a,b,cell_mask);
    }
    vec2 low=-2.*texture(sampler2D(water_bump,s_material),p.yx/448.+seconds/400.).ga+1.;
    low+=-2.*texture(sampler2D(water_bump,s_material),p/512.-seconds/400.).ag+1.;
    bump+=low*frame.water_surface.w;
    vec3 n=normalize(v_normal+vec3(bump.x,0,bump.y)*frame.water_surface.z);
    vec3 view=normalize(v_world_position-frame.camera_position.xyz);
    vec2 uv=screen_uv(frame.view_proj*vec4(v_world_position,1));
    if(optics.projection.x < -.5) {
        vec3 sample_color;
        if(optics.projection.x > -1.5)
            sample_color=texture(sampler2D(reflection_color,s_screen),uv).rgb;
        else if(optics.projection.x > -2.5)
            sample_color=texture(sampler2D(refraction_color,s_screen),uv).rgb;
        else
            sample_color=vec3(clamp(depth_at(uv)/optics.projection.y,0.,1.));
        out_color=vec4(sample_color,1.);
        return;
    }
    float surface_depth=length(v_world_position-frame.camera_position.xyz);
    vec2 pixel=1./vec2(textureSize(sampler2D(refraction_depth,s_screen),0));
    float scene_depth=max(max(depth_at(uv+pixel),depth_at(uv-pixel)),
                          max(depth_at(uv+pixel*vec2(1,-1)),depth_at(uv+pixel*vec2(-1,1))));
    float depth=max(scene_depth-surface_depth,0.);
    if(optics.projection.z>0.) {
        // Source UnderwaterCommon: eta 1.22, Snell window/TIR and dual RGB samples.
        vec3 underside=-n;
        vec3 transmission=refract(view,underside,optics.projection.w);
        float blend=clamp(transmission.y,0.,1.);
        vec3 point=v_world_position+(vec3(underside.x,0,underside.z)+view)*min(depth*blend,5.);
        vec2 a=mix(uv,screen_uv(frame.view_proj*vec4(point,1)),blend);
        vec2 b=a*.9+uv*.1;
        if(!valid_uv(a)||depth_at(a)<=surface_depth)a=uv;
        if(!valid_uv(b)||depth_at(b)<=surface_depth)b=uv;
        vec3 transmitted=texture(sampler2D(refraction_color,s_screen),a).rgb*vec3(1,.5,0)+
                         texture(sampler2D(refraction_color,s_screen),b).rgb*vec3(0,.5,1);
        vec3 softened=normalize(mix(underside,vec3(0,-1,0),5./6.));
        vec2 reflected_uv=screen_uv(optics.reflection_view_proj*
            vec4(v_world_position+vec3(softened.x,0,softened.z)*.18,1));
        vec3 reflected=texture(sampler2D(reflection_color,s_screen),clamp(reflected_uv,0.,1.)).rgb;
        out_color=vec4(max(mix(reflected,transmitted,blend),vec3(0)),1.);
        return;
    }
    vec3 refracted=refract(view,n,1./1.5);
    vec3 refracted_point=v_world_position+(view-vec3(n.x,0,n.z))*min(depth*abs(refracted.y),5.);
    vec2 ruv=screen_uv(frame.view_proj*vec4(refracted_point,1));
    // Silhouette guard prevents above-water objects from being pulled into the water.
    if(!valid_uv(ruv)||depth_at(ruv)<=surface_depth+.02)ruv=uv;
    vec3 refracted_color=texture(sampler2D(refraction_color,s_screen),ruv).rgb;
    vec3 ambient=max(frame.environment_ambient.rgb*frame.environment_ambient.a,vec3(0));
    vec3 light_color=vec3(0),specular=vec3(0);
    float shadow=1.;
    int light_count=clamp(int(frame.globals.x+.5),0,16);
    for(int i=0;i<light_count;i++) {
        if(frame.light_meta[i].x>.5)continue;
        vec3 light=normalize(-frame.light_dir[i].xyz);
        float visibility=1.;
        if(i==int(frame.globals.z+.5))visibility=1.-sample_shadow(n,max(dot(n,light),0.));
        vec3 radiance=frame.light_color[i].rgb*max(frame.light_meta[i].y,0.);
        light_color+=radiance*max(dot(n,light)*.7+.3,0.)*visibility;
        vec3 half_angle=normalize(light-view);
        specular+=radiance*pow(max(dot(n,half_angle),0.),112.)*visibility;
        shadow=min(shadow,visibility);
    }
    // Authored water-colour/fog lookup and the source's two extinction channels.
    vec3 water_color=texture(sampler2D(water_fog,s_material),p/4096.+.5).rgb;
    water_color=max(water_color,vec3(.025,.12,.18));
    vec2 extinction=clamp(exp(vec2(-20.,-60.*abs(view.y))*.015*depth*2.71828183),0.,1.);
    vec3 scatter=water_color*water_color*(ambient+light_color);
    vec3 tint=mix(2.*water_color*refracted_color,refracted_color,extinction.y);
    vec3 color=mix(scatter,tint,extinction.x);
    // The reference softens the reflection normal by 5/6 toward the water plane.
    vec3 reflection_normal=normalize(mix(n,vec3(0,1,0),5./6.));
    vec3 reflected_point=v_world_position+vec3(reflection_normal.x,0,reflection_normal.z)*.18;
    vec2 reflected_uv=screen_uv(optics.reflection_view_proj*vec4(reflected_point,1));
    vec3 reflected=texture(sampler2D(reflection_color,s_screen),clamp(reflected_uv,0.,1.)).rgb;
    float facing=clamp(mix(dot(-view,n),1.,.3),0.,1.);
    float fresnel=.02+.98*pow(1.-facing,5.);
    // The source planar-reflection sampler clamps at the screen edge. Switching
    // reflection off for out-of-range UVs creates a moving triangular seam.
    color=mix(color,reflected,fresnel);
    color+=specular;
    float foam_texture=texture(sampler2D(water_foam,s_material),
        p*frame.water_optics.y-vec2(0,v_color.r)+.25).g;
    // Shoreline foam comes from actual scene depth, not a hardcoded island outline.
    float bed_depth=depth*max(abs(view.y),.03);
    float shallow=(1.-smoothstep(.15,1.1,bed_depth))*smoothstep(.015,.12,bed_depth);
    float wash=smoothstep(.015,.20,max(v_color.r,0.));
    float foam=clamp(clamp(v_color.b*4.,0.,1.)*foam_texture+
        shallow*wash*(.35+foam_texture*.65),0.,1.);
    float whitecap=smoothstep(.22,.60,1.-v_normal.y)*max(frame.water_optics.x,0.)*foam_texture;
    foam=clamp(foam+whitecap,0.,1.);
    color=mix(color,(ambient+light_color)*vec3(.8,.9,.92),foam);
    if(environment_enabled())color=environment_apply_fog(color,frame.camera_position.xyz,v_world_position);
    out_color=vec4(max(color,vec3(0)),1.);
}
