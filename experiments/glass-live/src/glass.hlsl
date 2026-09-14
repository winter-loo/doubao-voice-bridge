// Original runtime shader. Evolves our glass-material reference; no third-party
// displacement textures or shader code. SDR BGRA capture -> linear FP16 passes
// -> sRGB-encoded, premultiplied BGRA8 composition surface. Alpha is only coverage.
cbuffer Parameters : register(b0) {
    float4 geometry; // ROI width,height; capsule width,height
    float4 filter;   // padding; sigma; horizontal/vertical direction
    float4 style;    // dark; time seconds; foreground enabled; blur multiplier
};
Texture2D<float4> image0 : register(t0);
Texture2D<float4> image1 : register(t1);
Texture2D<float> text_mask : register(t2);
SamplerState clamped : register(s0);

float4 fullscreen_vs(uint id : SV_VertexID) : SV_POSITION {
    float2 p = float2((id << 1) & 2, id & 2);
    return float4(p.x * 2 - 1, 1 - p.y * 2, 0, 1);
}
float3 linearize(float3 c) {
    return float3(c.r <= .04045 ? c.r / 12.92 : pow((c.r + .055) / 1.055, 2.4),
                  c.g <= .04045 ? c.g / 12.92 : pow((c.g + .055) / 1.055, 2.4),
                  c.b <= .04045 ? c.b / 12.92 : pow((c.b + .055) / 1.055, 2.4));
}
float3 encode_srgb(float3 c) {
    c = saturate(c);
    return float3(c.r <= .0031308 ? c.r * 12.92 : 1.055 * pow(c.r, 1 / 2.4) - .055,
                  c.g <= .0031308 ? c.g * 12.92 : 1.055 * pow(c.g, 1 / 2.4) - .055,
                  c.b <= .0031308 ? c.b * 12.92 : 1.055 * pow(c.b, 1 / 2.4) - .055);
}
float4 convert_ps(float4 p : SV_POSITION) : SV_TARGET {
    return float4(linearize(image0.SampleLevel(clamped, p.xy / geometry.xy, 0).rgb), 1);
}
float4 blur_ps(float4 p : SV_POSITION) : SV_TARGET {
    float sigma = clamp(filter.y, .01, 20);
    int radius = (int)ceil(3 * sigma);
    float3 sum = 0; float weight_sum = 0;
    [loop] for (int i = -radius; i <= radius; ++i) {
        float weight = exp(-float(i * i) / (2 * sigma * sigma));
        sum += image0.SampleLevel(clamped, (p.xy + filter.zw * i) / geometry.xy, 0).rgb * weight;
        weight_sum += weight;
    }
    return float4(sum / weight_sum, 1);
}
float3 capsule(float2 p) {
    float r = geometry.w * .5;
    float2 q = p - geometry.zw * .5;
    float half_line = (geometry.z - geometry.w) * .5;
    float2 v = float2(q.x - clamp(q.x, -half_line, half_line), q.y);
    float len = length(v);
    return len < .00001 ? float3(-r, 0, 0) : float3(len - r, v / len);
}
float4 material_ps(float4 pos : SV_POSITION) : SV_TARGET {
    float2 p = pos.xy;
    float3 f = capsule(p);
    float coverage = saturate(.5 - f.x);
    if (coverage <= 0) return 0;
    float h = geometry.w;
    float t = saturate(1 + f.x / max(1, h * .15));
    float edge = t * t * (3 - 2 * t);
    float2 uv = (p + filter.xx - f.yz * (h * .07 * edge)) / geometry.xy;
    // Broad low-pass in the center; restrained, narrower transmission at the rim.
    float3 c = lerp(image0.SampleLevel(clamped, uv, 0).rgb,
                    image1.SampleLevel(clamped, uv, 0).rgb, edge * .45);
    float luminance = dot(c, float3(.2126, .7152, .0722));
    c = lerp(luminance.xxx, c, .78);
    // Central contrast is deliberately bounded; a dark lens over a white page
    // must not become a pale gray button with unreadable white foreground.
    c = style.x > .5 ? c * .10 + float3(.010, .013, .020)
                        : lerp(c, float3(.94, .96, .98), .62);
    float facing = max(0, dot(f.yz, float2(-.305, -.952)));
    float s = h / 26;
    float rim = exp(-pow((f.x + 1.1 * s) / (.5 * s), 2));
    c = saturate(c * (1 - .07 * edge * (1 - facing)) + .16 * facing * facing * rim);
    float foreground = 0;
    if (style.z > .5) {
        foreground = text_mask.SampleLevel(clamped, p / geometry.zw, 0);
        // Illustrative voice bars, not microphone data. Foreground is AFTER blur.
        [unroll] for (int i = 0; i < 5; ++i) {
            float x = h * (.32 + i * .105);
            float length_y = h * (.07 + .12 * (.5 + .5 * sin(style.y * 3.5 + i * 1.3)));
            float2 q = abs(p - float2(x, h * .5)) - float2(h * .025, length_y);
            float d = length(max(q, 0)) + min(max(q.x, q.y), 0) - h * .012;
            foreground = max(foreground, saturate(.5 - d));
        }
        c = lerp(c, style.x > .5 ? float3(.95, .97, 1) : float3(.012, .020, .035), foreground);
    }
    // Encode BEFORE premultiplication; swapchain is UNORM (not an sRGB RTV).
    return float4(encode_srgb(c) * coverage, coverage);
}
