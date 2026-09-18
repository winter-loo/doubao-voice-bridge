// TEST ONLY: V2 material from b20608c, independently compiled beside V2.1.
// Shared conversion, coverage, capsule geometry and foreground behavior did not
// change. Pin the old art constants here; do not inherit runtime rim parameters.
float3 v2_light_tone(float3 scene, float protection) {
    float y = dot(scene, LUMA);
    float3 chroma = scene - y.xxx;
    float luminance = lerp(.48 + .44 * y, .84 + .105 * y, protection);
    float chroma_gain = lerp(.62, .24, protection);
    return saturate(luminance.xxx + chroma * chroma_gain + float3(-.002, 0, .004));
}
float3 v2_dark_tone(float3 scene, float protection) {
    float y = dot(scene, LUMA);
    float3 chroma = scene - y.xxx;
    float luminance = lerp(.016 + .105 * y, .013 + .045 * y, protection);
    float chroma_gain = lerp(.13, .09, protection);
    return saturate(luminance.xxx + chroma * chroma_gain + float3(-.001, 0, .003));
}
float3 v2_rim_lighting(float3 body, float2 p, float3 field, bool dark) {
    float h = geometry.w;
    float scale = h / 26;
    float inset = max(0, -field.x);
    float facing = max(0, dot(field.yz, float2(-.35, -.93675)));
    float opposing = max(0, dot(field.yz, float2(.35, .93675)));
    float outer_rim = exp(-pow((inset - .60 * scale) / (.44 * scale), 2));
    float inner_rim = exp(-pow((inset - 1.60 * scale) / (.65 * scale), 2));
    float light_arc = .30 + .70 * exp(-pow((p.x - geometry.z * .28) / (geometry.z * .42), 2));
    float inner_shadow = inner_rim * opposing * opposing * (dark ? .18 : .10);
    body *= 1 - inner_shadow;
    float highlight = outer_rim * (pow(facing, 3) * light_arc * (dark ? .12 : .40)
                                    + pow(opposing, 3) * (dark ? .025 : .065));
    body = lerp(body, 1, saturate(highlight));
    body += inner_rim * pow(facing, 3) * (dark ? .009 : .013);
    return saturate(body);
}
float4 v2_material_ps(float4 pos : SV_POSITION) : SV_TARGET {
    float2 p = pos.xy;
    float3 f = capsule(p);
    float coverage = saturate(.5 - f.x);
    if (coverage <= 0) return 0;
    float h = geometry.w;
    float inset = max(0, -f.x);
    float bevel = 1 - smoothstep(0, h * .18, inset);
    float protection = smoothstep(h * .055, h * .23, inset);
    float2 uv = (p + filter.xx - f.yz * (h * .095 * bevel)) / geometry.xy;
    float3 scene = lerp(image0.SampleLevel(clamped, uv, 0).rgb,
                        image1.SampleLevel(clamped, uv, 0).rgb, bevel * .72);
    bool dark = style.x > .5;
    float3 c = dark ? v2_dark_tone(scene, protection) : v2_light_tone(scene, protection);
    c = v2_rim_lighting(c, p, f, dark);
    if (style.z > .5) {
        float foreground = text_mask.SampleLevel(clamped, p / geometry.zw, 0);
        [unroll] for (int i = 0; i < 5; ++i) {
            float x = h * (.32 + i * .105);
            float length_y = h * (.07 + .12 * (.5 + .5 * sin(style.y * 3.5 + i * 1.3)));
            float2 q = abs(p - float2(x, h * .5)) - float2(h * .025, length_y);
            float d = length(max(q, 0)) + min(max(q.x, q.y), 0) - h * .012;
            foreground = max(foreground, saturate(.5 - d));
        }
        c = lerp(c, dark ? float3(.95, .97, 1) : float3(.012, .020, .035), foreground);
    }
    return float4(encode_srgb(c) * coverage, coverage);
}
