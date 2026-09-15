// Configured runtime material; glass.hlsl remains an immutable V2.2 reference.
// Generated constants come from material_config.rs. All colors below are linear.
float3 configured_tone(float3 scene, float protection, bool dark) {
    float4 tone = dark ? cfg_dark_luminance : cfg_light_luminance;
    float2 gain = dark ? cfg_dark_chroma : cfg_light_chroma;
    float3 bias = dark ? cfg_dark_tint : cfg_light_tint;
    float y = dot(scene, LUMA);
    float3 chroma = scene - y.xxx;
    float luminance = lerp(tone.x + tone.y * y, tone.z + tone.w * y, protection);
    float chroma_gain = lerp(gain.x, gain.y, protection);
    return saturate(luminance.xxx + chroma * chroma_gain + bias);
}
// The soft band contains the fixed waveform and text layout. It is NOT fitted
// to background ink and cannot shimmer as the desktop scrolls underneath it.
float configured_content_weight(float2 p, float inset) {
    float2 half_size = cfg_content_extent * geometry.zw * .5;
    float radius = min(cfg_content_corner * geometry.w, min(half_size.x, half_size.y));
    float2 q = abs(p - cfg_content_center * geometry.zw) - (half_size - radius);
    float distance = length(max(q, 0)) + min(max(q.x, q.y), 0) - radius;
    float weight = 1 - smoothstep(-cfg_content_feather * geometry.w, 0, distance);
    // Keep the outer 8% inset completely untouched, including antialias coverage.
    return weight * smoothstep(.08 * geometry.w, .20 * geometry.w, inset);
}
float3 configured_veil(float3 body, float weight, bool dark) {
    float4 r = dark ? cfg_dark_readability : cfg_light_readability;
    float3 target = dark ? cfg_dark_veil_rgb : cfg_light_veil_rgb;
    return saturate(lerp(body, target, weight * r.x) + weight * r.y);
}
float3 configured_rim(float3 body, float2 p, float3 field, bool dark) {
    float scale = geometry.w / cfg_reference_height;
    float inset = max(0, -field.x);
    float facing = max(0, dot(field.yz, cfg_light_direction));
    float opposing = max(0, dot(field.yz, -cfg_light_direction));
    float4 strengths = dark ? cfg_dark_rim : cfg_light_rim;
    float outer_rim = exp(-pow((inset - cfg_outer_rim.x * scale) / (cfg_outer_rim.y * scale), 2));
    float inner_rim = exp(-pow((inset - cfg_inner_rim.x * scale) / (cfg_inner_rim.y * scale), 2));
    float light_arc = cfg_arc.z + cfg_arc.w * exp(-pow((p.x - geometry.z * cfg_arc.x) / (geometry.z * cfg_arc.y), 2));
    body *= 1 - inner_rim * opposing * opposing * strengths.x;
    float highlight = outer_rim * (pow(facing, 3) * light_arc * strengths.y + pow(opposing, 3) * strengths.z);
    body = lerp(body, 1, saturate(highlight));
    body += inner_rim * pow(facing, 3) * strengths.w;
    return saturate(body);
}
float3 configured_surface(float3 body, float2 p, float inset, bool dark) {
    float2 uv = p / geometry.zw;
    float interior = smoothstep(cfg_surface_inset.x * geometry.w, cfg_surface_inset.y * geometry.w, inset);
    float2 light_shape = (uv - cfg_surface_position) / cfg_surface_spread;
    float reflection = exp(-dot(light_shape, light_shape)) * interior;
    float underside = exp(-pow((uv.y - cfg_underside.x) / cfg_underside.y, 2)) * interior;
    float2 strengths = dark ? cfg_dark_surface : cfg_light_surface;
    body *= 1 - underside * strengths.x;
    body += reflection * strengths.y;
    return saturate(body);
}
float4 configured_material_ps(float4 pos : SV_POSITION) : SV_TARGET {
    float2 p = pos.xy;
    float3 f = capsule(p);
    float coverage = saturate(.5 - f.x);
    if (coverage <= 0) return 0;
    float h = geometry.w;
    float inset = max(0, -f.x);
    float bevel = 1 - smoothstep(0, h * cfg_bevel, inset);
    float protection = smoothstep(h * cfg_protection.x, h * cfg_protection.y, inset);
    float2 uv = (p + filter.xx - f.yz * (h * cfg_refraction * bevel)) / geometry.xy;
    float3 scene = lerp(image0.SampleLevel(clamped, uv, 0).rgb,
                        image1.SampleLevel(clamped, uv, 0).rgb, bevel * cfg_narrow_mix);
    bool dark = style.x > .5;
    float3 c = configured_tone(scene, protection, dark);
    c = configured_veil(c, configured_content_weight(p, inset), dark);
    c = configured_rim(c, p, f, dark);
#ifndef GLASS_SURFACE_REFLECTION_TEST_OFF
    c = configured_surface(c, p, inset, dark);
#endif
    if (style.z > .5) {
        float foreground = text_mask.SampleLevel(clamped, p / geometry.zw, 0);
        // Foreground layout, colors and animation remain unchanged.
        [unroll] for (int i = 0; i < 5; ++i) {
            float x = h * (.32 + i * .105);
            float length_y = h * (.07 + .12 * (.5 + .5 * sin(style.y * 3.5 + i * 1.3)));
            float2 q = abs(p - float2(x, h * .5)) - float2(h * .025, length_y);
            float d = length(max(q, 0)) + min(max(q.x, q.y), 0) - h * .012;
            foreground = max(foreground, saturate(.5 - d));
        }
        c = lerp(c, dark ? cfg_dark_foreground : cfg_light_foreground, foreground);
    }
    return float4(encode_srgb(c) * coverage, coverage);
}
// Explicit offscreen test entry: validates the mask actually executed by HLSL.
float4 configured_content_mask_ps(float4 p : SV_POSITION) : SV_TARGET {
    float w = configured_content_weight(p.xy, max(0, -capsule(p.xy).x));
    return float4(w, w, w, 1);
}
