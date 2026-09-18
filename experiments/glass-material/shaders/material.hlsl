// GPU counterpart to src/lib.rs. Not yet connected to a desktop capture source
// or GPUI. Inputs are LINEAR RGB; output is premultiplied linear RGBA for a
// compositor surface. The CPU fixture exports straight-alpha sRGB PAM instead.
cbuffer GlassParameters : register(b0) {
    float2 input_size;
    float2 capsule_origin;
    float2 capsule_size;
    float bevel_ratio;
    float refraction_ratio;
    float saturation;
    float tint_opacity;
    float2 padding;
    float4 material_tint;
};
Texture2D<float4> wide_blur : register(t0);
Texture2D<float4> narrow_blur : register(t1);
SamplerState linear_clamp : register(s0);

float3 capsule_field(float2 p) {
    float radius = capsule_size.y * 0.5;
    float half_segment = (capsule_size.x-capsule_size.y)*0.5;
    float2 q = p-capsule_size*0.5;
    float2 d = float2(q.x-clamp(q.x,-half_segment,half_segment),q.y);
    float length_d = length(d);
    return length_d < 1e-6 ? float3(-radius,0,0) : float3(length_d-radius,d/length_d);
}
float4 material_ps(float4 position : SV_POSITION) : SV_TARGET {
    float2 p = position.xy; // render target is exactly capsule_size pixels
    float3 field = capsule_field(p);
    float coverage = saturate(0.5-field.x);
    if (coverage <= 0) return 0;
    float t = saturate(1.0+field.x/max(1.0,capsule_size.y*0.5*bevel_ratio));
    float edge = t*t*(3.0-2.0*t);
    float2 uv = (capsule_origin+p-field.yz*(capsule_size.y*refraction_ratio*edge))/input_size;
    float3 color = lerp(wide_blur.SampleLevel(linear_clamp,uv,0).rgb,narrow_blur.SampleLevel(linear_clamp,uv,0).rgb,edge*0.60);
    float luminance = dot(color,float3(0.2126,0.7152,0.0722));
    color = lerp(luminance.xxx,color,saturation);
    color = lerp(color,material_tint.rgb,tint_opacity);
    float facing = max(0.0,dot(field.yz,float2(-0.3047757,-0.9524242)));
    float scale = capsule_size.y/26.0;
    float band = (field.x+1.55*scale)/(0.60*scale);
    float key = 0.28*pow(facing,2.2)*exp(-band*band);
    float shadow = 0.08*edge*(1.0-facing);
    color = saturate(color*(1.0-shadow)+key);
    // Alpha 1 in the interior, coverage only at the edge. Foreground is later.
    return float4(color*coverage,coverage);
}
