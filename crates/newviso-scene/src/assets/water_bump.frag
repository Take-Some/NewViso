#version 450
layout(location=0) in vec2 v_uv;
layout(location=0) out vec4 out_field;
layout(set=0,binding=0,std140) uniform RippleStep {vec4 constants;vec4 simulation;vec4 noise_offset;} step;
layout(set=0,binding=1) uniform texture2D height_velocity;
layout(set=0,binding=2) uniform texture2D forcing_noise;
layout(set=0,binding=3) uniform sampler s_repeat;
float height_at(vec2 uv){return texture(sampler2D(height_velocity,s_repeat),uv).r;}
void main(){
    const float t=1./256.;
    vec2 p=v_uv;
    vec4 color=texture(sampler2D(height_velocity,s_repeat),p);
    float h=color.r;
    vec4 a=vec4(height_at(p+vec2(t,0)),height_at(p+vec2(0,t)),height_at(p-vec2(t,0)),height_at(p-vec2(0,t)));
    vec4 b=vec4(height_at(p+vec2(t,t)),height_at(p+vec2(t,-t)),height_at(p-vec2(t,t)),height_at(p+vec2(-t,t)));
    vec4 minima=min(a,vec4(height_at(p+vec2(2*t,0)),b.r,h,b.g));
    minima=min(minima,vec4(b.r,height_at(p+vec2(0,2*t)),b.a,h));
    minima=min(minima,vec4(h,b.a,height_at(p-vec2(2*t,0)),b.b));
    minima=min(minima,vec4(b.g,h,b.b,height_at(p-vec2(0,2*t))));
    vec2 bump=(minima.zw-minima.xy)*3.;
    float average=dot(a,vec4(.18))+dot(b,vec4(.07));
    float disturbance=2.*texture(sampler2D(forcing_noise,s_repeat),p+step.noise_offset.xy).g-1.;
    float velocity=(.5-h)*.18+(average-h)+disturbance*step.simulation.y;
    float dt=step.simulation.z;
    float next_height=h+(color.g-.5)*step.simulation.x*dt+velocity*.5*dt*dt;
    float next_velocity=color.g+velocity*dt;
    // Source targets are normalized 16-bit buffers; retain their saturation.
    out_field=clamp(vec4(next_height,next_velocity,bump+.5),0.,1.);
}
