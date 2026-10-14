// Heightfield layers are actual full-resolution GPU DDS textures:
 // base_color=grass, normal=sand, specular=rock,
 // emissive=soil, environment=wood. The terrain fragment shader interprets
 // these five sampler slots only for the heightfield surface marker.
vec3 nv_terrain_bitmap(int layer,vec2 uv) {
    float tiling = layer==1 ? 0.09 : (layer==2 ? 0.18 : (layer==4 ? 0.48 : 0.30));
    vec2 p=uv*tiling;
    if(layer==0) return texture(sampler2D(t_base_color,s_material),p).rgb;
    if(layer==1) return texture(sampler2D(t_normal,s_material),p).rgb;
    if(layer==2) return texture(sampler2D(t_specular,s_material),p).rgb;
    if(layer==3) return texture(sampler2D(t_emissive,s_material),p).rgb;
    return texture(sampler2D(t_environment,s_material),p).rgb;
}
vec3 nv_terrain_triplanar(int layer,vec3 p,vec3 n) {
    if(abs(n.y)>.88) return nv_terrain_bitmap(layer,p.xz);
    vec3 w=pow(abs(n),vec3(4.0));
    w/=max(w.x+w.y+w.z,0.0001);
    return nv_terrain_bitmap(layer,p.yz)*w.x+
           nv_terrain_bitmap(layer,p.xz)*w.y+
           nv_terrain_bitmap(layer,p.xy)*w.z;
}
bool nv_heightfield_material() {
    return (int(material.shading1.z + 0.5) & 256) != 0;
}
void nv_island_surface(inout vec4 color,inout vec3 normal,
                       inout float specular,inout float roughness){
    if (!nv_heightfield_material()) return;
    float marker=v_color.a;
    if(marker<0.895||marker>0.97)return;
    vec3 p=v_world_position;
    vec3 n=normalize(v_normal);
    if(marker>.953) {
        float beach=1.0-smoothstep(0.3,2.8,p.y);
        float rockness=1.0-smoothstep(0.40,0.85,n.y);
        float grassiness=smoothstep(1.4,5.3,p.y);
        vec3 ground;
        if(beach>.99) {
            ground=nv_terrain_triplanar(1,p,n)*vec3(1.03,1.03,.96);
        } else if(rockness>.99) {
            ground=nv_terrain_triplanar(2,p,n)*vec3(.97,.96,.93);
        } else {
            vec3 soil=nv_terrain_triplanar(3,p,n);
            if(grassiness>.98) {
                ground=nv_terrain_triplanar(0,p,n);
            } else if(grassiness<.02) {
                ground=soil;
            } else {
                ground=mix(soil,nv_terrain_triplanar(0,p,n),grassiness);
            }
            if(beach>.02) ground=mix(ground,nv_terrain_triplanar(1,p,n),
                                    beach);
            if(rockness>.02)ground=mix(ground,nv_terrain_triplanar(2,p,n),
                                      rockness*.92);
        }
        color.rgb=ground;
        roughness=mix(.88,.97,beach);
    } else if(abs(marker-.91)<.011) {
        color.rgb=nv_terrain_triplanar(1,p,n);roughness=.92;
    } else if(abs(marker-.92)<.011) {
        color.rgb=nv_terrain_triplanar(0,p,n);roughness=.88;
    } else if(abs(marker-.93)<.011) {
        color.rgb=nv_terrain_triplanar(2,p,n);roughness=.83;
    } else if(abs(marker-.94)<.011) {
        color.rgb=nv_terrain_bitmap(4,vec2(p.x,p.z)*.6);
        roughness=.75;
    } else {
        color.rgb=nv_terrain_triplanar(3,p,n);roughness=.9;
    }
    // The actual heightfield slope determines the geometric normal.
    float luminance=dot(color.rgb,vec3(.2126,.7152,.0722));
    normal=normalize(n+vec3(dFdx(luminance)*.045,0.0,dFdy(luminance)*.045));
    color.a=1.0;
    specular*=.7;
}
