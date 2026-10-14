#version 450
layout(location=0) in vec3 in_position;
layout(location=0) out vec2 v_uv;
void main() {
    gl_Position=vec4(in_position,1.);
    v_uv=in_position.xy*.5+.5;
}
