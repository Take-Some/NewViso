// The analytic ocean matches newviso_scene::water::WaterSurface and Island TS.
float nv_ocean_wave_height(vec2 p,float t) {
    vec2 d=normalize(vec2(0.72,0.69));
    vec2 d1=vec2(-d.y,d.x);
    vec2 d2=vec2(d.x*.7-d.y*.7,d.x*.7+d.y*.7);
    const float k=6.28318530718/18.0;
    return 0.30*.64*sin(dot(p,d)*k-t*2.0*k)
       + 0.30*.26*sin(dot(p,d1)*k/.48-t*2.7*k/.48)
       + 0.30*.10*sin(dot(p,d2)*k/.22-t*3.6*k/.22);
}
vec2 nv_ocean_wave_slope(vec2 p,float t) {
    vec2 d=normalize(vec2(0.72,0.69));
    vec2 d1=vec2(-d.y,d.x);
    vec2 d2=vec2(d.x*.7-d.y*.7,d.x*.7+d.y*.7);
    const float k=6.28318530718/18.0;
    return d*(.30*.64*k*cos(dot(p,d)*k-t*2.0*k))
      + d1*(.30*.26*k/.48*cos(dot(p,d1)*k/.48-t*2.7*k/.48))
      + d2*(.30*.10*k/.22*cos(dot(p,d2)*k/.22-t*3.6*k/.22));
}
float nv_ocean_geometry_fade(vec2 p) {
    return 1.0-smoothstep(112.0,130.0,max(abs(p.x),abs(p.y)));
}
