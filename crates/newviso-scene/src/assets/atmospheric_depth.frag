#version 450

layout(location = 0) in float v_linear_depth;
layout(location = 0) out float out_linear_depth;

void main() {
    out_linear_depth = v_linear_depth;
}
