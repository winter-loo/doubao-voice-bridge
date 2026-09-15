// Original runtime shader. No third-party shader code or displacement assets.
// SDR capture -> linear FP16 blur -> sRGB premultiplied BGRA8.
// V2.1: narrow optical rim; retain the V2 protected center and foreground.
// Alpha remains silhouette coverage, NEVER material transparency.
cbuffer Parameters : register(b0) {
    float4 geometry; // ROI width,height; capsule width,height
    float4 filter;   // padding; sigma; horizontal/vertical direction
    float4 style;    // dark; time seconds; foreground enabled; reserved
};
Texture2D<float4> image0 : register(t0);
Texture2D<float4> image1 : register(t1);
Texture2D<float> text_mask : register(t2);
SamplerState clamped : register(s0);

// Dimensionless art parameters; lengths below scale with physical capsule height.
// Keep filter support/padding consistent with gpu.rs::prepare().
static const float BEVEL_FRACTION = .12;
static const float REFRACTION_FRACTION = .065;
static const float RIM_NARROW_MIX = .52;
static const float3 LUMA = float3(.2126, .7152, .0722);

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

// Only the transmitting edge endpoint changes; protection=1 is exactly V2.
// Keep chroma distinct from luminance and never let raw text leak through alpha.
float3 light_tone(float3 scene, float protection) {
    float y = dot(scene, LUMA);
    float3 chroma = scene - y.xxx;
    // The old .48 + .44*y endpoint produced a dark, broad lip over black ink.
    float luminance = lerp(.70 + .24 * y, .84 + .105 * y, protection);
    float chroma_gain = lerp(.40, .24, protection);
    return saturate(luminance.xxx + chroma * chroma_gain + float3(-.002, 0, .004));
}
float3 dark_tone(float3 scene, float protection) {
    float y = dot(scene, LUMA);
    float3 chroma = scene - y.xxx;
    // Reduce the pale perimeter on white pages, not the charcoal center.
    float luminance = lerp(.014 + .070 * y, .013 + .045 * y, protection);
    float chroma_gain = lerp(.11, .09, protection);
    return saturate(luminance.xxx + chroma * chroma_gain + float3(-.001, 0, .003));
}
float3 rim_lighting(float3 body, float2 p, float3 field, bool dark) {
    float h = geometry.w;
    float scale = h / 26;
    float inset = max(0, -field.x);
    float facing = max(0, dot(field.yz, float2(-.35, -.93675)));
    float opposing = max(0, dot(field.yz, float2(.35, .93675)));
    // A fine outer reflection plus a weaker, narrower interior shoulder.
    float outer_rim = exp(-pow((inset - .50 * scale) / (.35 * scale), 2));
    float inner_rim = exp(-pow((inset - 1.15 * scale) / (.38 * scale), 2));
    // Modulate along the capsule as well as by its normal: no uniform white stroke.
    float light_arc = .30 + .70 * exp(-pow((p.x - geometry.z * .28) / (geometry.z * .42), 2));
    float inner_shadow = inner_rim * opposing * opposing * (dark ? .08 : .05);
    body *= 1 - inner_shadow;
    float highlight = outer_rim * (pow(facing, 3) * light_arc * (dark ? .17 : .70)
                                    + pow(opposing, 3) * (dark ? .008 : .025));
    body = lerp(body, 1, saturate(highlight));
    body += inner_rim * pow(facing, 3) * (dark ? .004 : .006);
    return saturate(body);
}
float4 material_ps(float4 pos : SV_POSITION) : SV_TARGET {
    float2 p = pos.xy;
    float3 f = capsule(p);
    float coverage = saturate(.5 - f.x);
    if (coverage <= 0) return 0;
    float h = geometry.w;
    float inset = max(0, -f.x);
    float bevel = 1 - smoothstep(0, h * BEVEL_FRACTION, inset);
    // Reach the unchanged protected body after .13h, rather than .23h.
    // For the 39px lens this removes the soft ~9px lip; no extra blur pass.
    float protection = smoothstep(h * .025, h * .13, inset);
    float2 uv = (p + filter.xx - f.yz * (h * REFRACTION_FRACTION * bevel)) / geometry.xy;
    // Both sources are already blurred. The thinner edge mixes LESS of the
    // narrow source, so displaced ink cannot become a row of sharp dark flecks.
    float3 scene = lerp(image0.SampleLevel(clamped, uv, 0).rgb,
                        image1.SampleLevel(clamped, uv, 0).rgb, bevel * RIM_NARROW_MIX);
    bool dark = style.x > .5;
    float3 c = dark ? dark_tone(scene, protection) : light_tone(scene, protection);
    c = rim_lighting(c, p, f, dark);
    if (style.z > .5) {
        float foreground = text_mask.SampleLevel(clamped, p / geometry.zw, 0);
        // Foreground layout, colors and animation are unchanged from V1.
        [unroll] for (int i = 0; i < 5; ++i) {
            float x = h * (.32 + i * .105);
            float length_y = h * (.07 + .12 * (.5 + .5 * sin(style.y * 3.5 + i * 1.3)));
            float2 q = abs(p - float2(x, h * .5)) - float2(h * .025, length_y);
            float d = length(max(q, 0)) + min(max(q.x, q.y), 0) - h * .012;
            foreground = max(foreground, saturate(.5 - d));
        }
        c = lerp(c, dark ? float3(.95, .97, 1) : float3(.012, .020, .035), foreground);
    }
    // Encode BEFORE premultiplication; swapchain is UNORM (not an sRGB RTV).
    return float4(encode_srgb(c) * coverage, coverage);
}
