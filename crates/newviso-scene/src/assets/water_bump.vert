#version 450
layout(location=0) in vec2 in_position;
layout(location=0) out vec2 v_uv;
void main(){gl_Position=vec4(in_position,0,1);v_uv=in_position*.5+.5;}
