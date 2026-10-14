#include "ocean_waves.glsl"
// World-space ocean shading with real weather ripple-normal texture.
// The large-scale wave phases match newviso_scene::water::WaterSurface exactly.
float nv_hash21(vec2 p) {
    p = fract(p * vec2(123.34, 456.21));
    p += dot(p,p + 45.32);
    return fract(p.x*p.y);
}
float nv_noise(vec2 x) {
    vec2 c=floor(x), f=fract(x);
    vec2 u=f*f*(3.0-2.0*f);
    return mix(mix(nv_hash21(c),nv_hash21(c+vec2(1,0)),u.x),
               mix(nv_hash21(c+vec2(0,1)),nv_hash21(c+vec2(1,1)),u.x),u.y);
}
float nv_fbm(vec2 p) {
    float v=0.0, w=0.53;
    for(int i=0;i<4;++i) {
        v+=w*nv_noise(p);
        p=mat2(1.6,-1.2,1.2,1.6)*p;
        w*=0.48;
    }
    return v;
}
float nv_water_marker() {
    return 1.0-step(0.011,abs(v_color.a-0.82));
}
void nv_water_appearance(inout vec4 color, inout vec3 normal,
                         inout float specular, inout float roughness) {
    if(nv_water_marker()<0.5 || v_normal.y<0.8) return;
    vec2 p=v_world_position.xz;
    float t=frame.camera_position.w;
    // Physical slope from the SAME analytic wave function as the GPU mesh.
    vec2 slope=nv_ocean_wave_slope(p,t)*nv_ocean_geometry_fade(p);
    vec2 driftA=p*.22+vec2(t*.032,t*.013);
    vec2 driftB=mat2(.64,-.77,.77,.64)*p*.38-vec2(t*.021,-t*.029);
    // Two real original DDS normal maps, independently advected.
    // Neither material is affected by weather enable/disable.
    vec3 rippleA=texture(sampler2D(t_normal,s_material),driftA).rgb;
    vec3 rippleB=texture(sampler2D(t_environment,s_material),driftB).rgb;
    vec2 waves=(rippleA.rg-.5)*.11+(rippleB.rg-.5)*.055;
    normal=normalize(vec3(-slope.x+waves.x,1.0,-slope.y+waves.y));
    vec2 motion=vec2(t*.042,t*-.035);
    float details=nv_fbm(p*2.2+motion);
    float f0=dot(p,normalize(vec2(.72,.69)))*(6.2831853/18.0)-t*2.0*6.2831853/18.0;
    float f1=dot(p,vec2(-.69,.72))*(6.2831853/(18.0*.48))-t*2.7*6.2831853/(18.0*.48);
    float f2=dot(p,vec2(.94,-.35))*(6.2831853/(18.0*.22))-t*3.6*6.2831853/(18.0*.22);
    vec3 eye=normalize(frame.camera_position.xyz-v_world_position);
    float fresnel=pow(1.0-clamp(dot(normal,eye),0.0,1.0),5.0);
    float crest=clamp((sin(f0)*0.64+sin(f1)*0.26+sin(f2)*0.10)*0.5+0.5,0.0,1.0);
    float foam_noise=nv_fbm(p*0.42+motion*6.0);
    // Coastal surf belongs to the Island reference world, not a solid
    // collision polygon. Low-angle crests accumulate foam near the shore.
    // Irregular elliptical shoreline follows the *contour*, not a tiled
    // circular foam decal. Offshore fades smoothly to open-water optics.
    float coastAngle=atan(p.y-3.0,p.x+3.5);
    float islandOutline=1.0 + .095*sin(3.0*coastAngle+.36)
        +.062*sin(5.0*coastAngle-1.05)+.041*sin(8.0*coastAngle+2.3);
    float ellipse=length((p-vec2(-3.5,3.0))/vec2(55.0,51.0))
        /max(islandOutline,.65);
    float shoreline=1.0-smoothstep(.84,1.11,ellipse);
    shoreline*=smoothstep(.66,.81,ellipse);
    float surf=shoreline*smoothstep(0.43,0.75,foam_noise+0.2*sin(f0+t));
    float whitecap=smoothstep(0.86,0.98,crest+0.12*(details-0.5));
    float foam=clamp(surf*0.85+whitecap*0.28,0.0,0.95);
    // Event-driven impacts from physical objects / swimmers propagate
    // concentric waves from world coordinates. Age and radius both decay.
    float wake=0.0;
    vec2 wakeSlope=vec2(0.0);
    for(int i=0;i<8;++i) {
        vec4 hit=frame.water_impacts[i];
        if(hit.w<=0.0)continue;
        float age=t-hit.z;
        if(age<0.0||age>2.8)continue;
        vec2 delta=p-hit.xy;
        float distance=length(delta);
        float radius=age*(2.1+hit.w*.18)+.15;
        float envelope=exp(-age*1.1)*min(hit.w*.25,1.0);
        float band=exp(-pow((distance-radius)/(.20+age*.12),2.0));
        float ring=sin((distance-radius)*19.0);
        wake+=envelope*band;
        wakeSlope+=delta/max(distance,.03)*ring*band*envelope*.15;
    }
    normal=normalize(normal+vec3(wakeSlope.x,0.0,wakeSlope.y));
    foam=clamp(foam+wake*.68,0.0,.97);
    vec3 ocean=vec3(0.018,0.165,0.235);
    ocean=mix(ocean,vec3(0.18,0.42,0.53),fresnel*0.75+details*0.18);
    color.rgb=mix(ocean,vec3(0.71,0.85,0.83),foam);
    color.a=1.0;
    specular=1.25;
    roughness=mix(0.08,0.55,foam);
}
