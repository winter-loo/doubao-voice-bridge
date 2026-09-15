// Test-only baseline: our material_ps from 7d836941, renamed, otherwise unchanged.
// Compiled with the unchanged common conversion/capsule helpers from glass.hlsl.
// Baseline preparation MUST use sigma fractions .16/.035, not the V2 fractions.
// This source is included only by cfg(test); it is not a runtime material switch.
float4 baseline_ps(float4 pos : SV_POSITION) : SV_TARGET {
    float2 p = pos.xy;
    float3 f = capsule(p);
    float coverage = saturate(.5 - f.x);
    if (coverage <= 0) return 0;
    float h = geometry.w;
    float t = saturate(1 + f.x / max(1, h * .15));
    float edge = t * t * (3 - 2 * t);
    float2 uv = (p + filter.xx - f.yz * (h * .07 * edge)) / geometry.xy;
    float3 c = lerp(image0.SampleLevel(clamped, uv, 0).rgb,
                    image1.SampleLevel(clamped, uv, 0).rgb, edge * .45);
    float luminance = dot(c, float3(.2126, .7152, .0722));
    c = lerp(luminance.xxx, c, .78);
    c = style.x > .5 ? c * .10 + float3(.010, .013, .020)
                        : lerp(c, float3(.94, .96, .98), .62);
    float facing = max(0, dot(f.yz, float2(-.305, -.952)));
    float s = h / 26;
    float rim = exp(-pow((f.x + 1.1 * s) / (.5 * s), 2));
    c = saturate(c * (1 - .07 * edge * (1 - facing)) + .16 * facing * facing * rim);
    float foreground = 0;
    if (style.z > .5) {
        foreground = text_mask.SampleLevel(clamped, p / geometry.zw, 0);
        [unroll] for (int i = 0; i < 5; ++i) {
            float x = h * (.32 + i * .105);
            float length_y = h * (.07 + .12 * (.5 + .5 * sin(style.y * 3.5 + i * 1.3)));
            float2 q = abs(p - float2(x, h * .5)) - float2(h * .025, length_y);
            float d = length(max(q, 0)) + min(max(q.x, q.y), 0) - h * .012;
            foreground = max(foreground, saturate(.5 - d));
        }
        c = lerp(c, style.x > .5 ? float3(.95, .97, 1) : float3(.012, .020, .035), foreground);
    }
    return float4(encode_srgb(c) * coverage, coverage);
}
