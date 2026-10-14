// Shared linear-radiance environment math. Define ENV_LANE(i) before inclusion.
#ifndef NEWVISO_ENVIRONMENT_GLSL
#define NEWVISO_ENVIRONMENT_GLSL
bool environment_enabled() { return ENV_LANE(0).x > 0.5; }
vec3 environment_ambient(vec3 n, float occlusion, float interior, float natural_visibility) {
    float wrap = ENV_LANE(1).w;
    float down = max(0.0, (n.y + wrap) / max(1.0 + wrap, 1e-5));
    vec3 natural = ENV_LANE(1).rgb * down + ENV_LANE(2).rgb;
    vec3 directional = ENV_LANE(7).rgb * clamp(dot(ENV_LANE(8).xyz,n),0.0,1.0);
    vec3 artificial_down = mix(ENV_LANE(3).rgb,ENV_LANE(5).rgb,interior);
    vec3 artificial_up = mix(ENV_LANE(4).rgb,ENV_LANE(6).rgb,interior);
    float ao = clamp(1.0 - (1.0-clamp(occlusion,0.0,1.0))*ENV_LANE(4).w,0.0,1.0);
    return ((natural+directional)*ENV_LANE(2).w*natural_visibility
        +(artificial_down*down+artificial_up)*ENV_LANE(3).w)*ao;
}
vec4 environment_fog(vec3 eye, vec3 point, float haze_scale) {
    vec3 delta = point-eye;
    float full_distance = length(delta);
    float distance_fog = max(0.0,full_distance-ENV_LANE(9).y);
    float delta_height = delta.y*distance_fog/max(full_distance,1e-6);
    float falloff = ENV_LANE(10).x;
    float height_density = exp(min(-falloff*(eye.y-ENV_LANE(9).z),20.0));
    float t = falloff*delta_height;
    float integral = abs(delta_height)>0.01 && abs(t)>1e-5 ? (1.0-exp(clamp(-t,-80.0,80.0)))/t : 1.0;
    float ground = (1.0-clamp(exp(min(1.0,-min(1.0,height_density*ENV_LANE(9).x)
        *distance_fog*integral)),0.0,1.0))*ENV_LANE(9).w;
    float haze_blend = haze_scale*ENV_LANE(10).w*(1.0-ground);
    float haze = haze_blend*(1.0-exp(-ENV_LANE(10).y*max(0.0,distance_fog-ENV_LANE(10).z)));
    vec3 atmosphere = mix(ENV_LANE(13).rgb,ENV_LANE(14).rgb,pow(clamp(height_density,0.0,1.0),0.3));
    vec3 direction = delta/max(full_distance,1e-6);
    float sun = pow(clamp(dot(direction,ENV_LANE(17).xyz),0.0,1.0),ENV_LANE(11).y);
    float moon = pow(clamp(dot(direction,ENV_LANE(18).xyz),0.0,1.0),ENV_LANE(11).z);
    vec3 sun_color = mix(atmosphere,ENV_LANE(12).rgb,ENV_LANE(12).w);
    vec3 sky_color = mix(mix(atmosphere,ENV_LANE(16).rgb,moon),sun_color,sun);
    float atmosphere_blend=1.0-exp(-ENV_LANE(11).x/max(ENV_LANE(11).w,1.0)*distance_fog);
    vec3 ground_color=mix(ENV_LANE(14).rgb,sky_color,atmosphere_blend);
    return vec4(mix(ground_color,ENV_LANE(15).rgb,haze_blend),clamp(ground+haze,0.0,1.0));
}
vec3 environment_apply_fog(vec3 color, vec3 eye, vec3 point) {
    vec4 fog=environment_fog(eye,point,1.0);
    return mix(color,fog.rgb,fog.a);
}
vec3 environment_filmic_raw(vec3 x,vec4 abcd,vec3 efw) {
    return (x*(abcd.x*x+abcd.z*abcd.y)+abcd.w*efw.x)
        /max(x*(abcd.x*x+abcd.y)+abcd.w*efw.y,vec3(1e-8))-efw.x/max(efw.y,1e-8);
}
vec3 environment_filmic(vec3 color,float ev) {
    float range=ENV_LANE(24).w-ENV_LANE(22).w;
    if(abs(range)<1e-5)range=1.0;
    float t=clamp((ev-ENV_LANE(22).w)/range,0.0,1.0);
    vec4 abcd=mix(ENV_LANE(21),ENV_LANE(23),t);
    vec3 efw=mix(ENV_LANE(22).xyz,ENV_LANE(24).xyz,t);
    vec3 white=environment_filmic_raw(vec3(max(efw.z,1e-5)),abcd,efw);
    return clamp(environment_filmic_raw(max(color,vec3(0.0))*exp2(ev),abcd,efw)
        /max(white,vec3(1e-8)),0.0,1.0);
}
#endif

