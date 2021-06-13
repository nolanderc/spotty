#include <metal_stdlib>

using namespace metal;

struct VertexInput {
    float2 position;
    float2 tex_coord;
    float4 color;
};

struct VertexOutput {
    float4 position [[position]];
    float4 color;
    float2 tex_coord;
};

struct WindowUniforms {
    float2 size;
};

vertex VertexOutput vertex_shader(
    unsigned int vertex_id [[vertex_id]],
    const device VertexInput* vertex_array [[buffer(0)]],
    const device WindowUniforms* window [[buffer(1)]]
) {
    const device VertexInput& in = vertex_array[vertex_id];

    float2 clip = 2 * in.position / window->size - 1;

    VertexOutput out;
    out.position = float4(clip.x, -clip.y, 0.0, 1.0);
    out.color = in.color;
    out.tex_coord = in.tex_coord;

    return out;
}

fragment float4 fragment_shader(
    VertexOutput in [[stage_in]],
    texture2d<float> texture [[ texture(0) ]]
) {
    constexpr sampler texture_sampler(mag_filter::nearest, min_filter::nearest);
    float4 color = texture.sample(texture_sampler, in.tex_coord);
    return in.color * color;
}

