// Two-pass Gaussian on linear-light RGBA textures. The future host must use
// distinct input/output resources; never sample the active render target.
// Run with direction=(1,0), then (0,1). Produce wide and narrow blur inputs.
cbuffer BlurParameters : register(b0) {
    float2 input_size;
    float2 direction;
    float sigma;
    float3 padding;
};
Texture2D<float4> source_image : register(t0);
SamplerState linear_clamp : register(s0);

float4 blur_ps(float4 position : SV_POSITION) : SV_TARGET {
    float2 uv = position.xy / input_size;
    if (sigma < 0.01) return source_image.SampleLevel(linear_clamp, uv, 0);
    float safe_sigma = min(sigma, 20.0);
    int radius = min((int)ceil(3.0 * safe_sigma), 64);
    float3 color = 0;
    float weight_sum = 0;
    [loop] for (int i = -radius; i <= radius; ++i) {
        float weight = exp(-(float)(i*i) / (2.0*safe_sigma*safe_sigma));
        color += source_image.SampleLevel(linear_clamp, uv + direction*(float)i/input_size, 0).rgb * weight;
        weight_sum += weight;
    }
    return float4(color / weight_sum, 1.0);
}
