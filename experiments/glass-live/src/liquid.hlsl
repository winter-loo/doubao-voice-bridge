// Independent optical model informed by Apple's WWDC25/219, not Apple's shader.
// The retained glass.hlsl provides resource b0/t0..2 and filter helpers only.
// This material transmits the captured scene; it does NOT use the V2.x affine paint.
cbuffer Interaction : register(b1) {
    float4 contact; // normalized pointer xy, spring press, enabled motion
    float4 dynamics; // spring drag xy, appearance progress, seconds since last update
};
Texture2D<float4> original_linear : register(t3);
Texture2D<float> glyph_support : register(t4);
Texture2D<float4> adaptation : register(t5);

float3 optical_encode(float3 c) {
    c=saturate(c);
    return float3(c.r<=.0031308?12.92*c.r:1.055*pow(c.r,1.0/2.4)-.055,
                  c.g<=.0031308?12.92*c.g:1.055*pow(c.g,1.0/2.4)-.055,
                  c.b<=.0031308?12.92*c.b:1.055*pow(c.b,1.0/2.4)-.055);
}
float2 scene_uv(float2 p) { return (p+filter.xx)/geometry.xy; }

// One GPU-only 1x1 reduction/history. No readback of the user's pixels.
// r=smoothed scene luminance; g/b=light/dark white-ink selection; a=1+detail.
float4 liquid_adapt_ps(float4 pos:SV_POSITION):SV_TARGET {
    float mean=0, detail=0;
    [unroll] for(int j=0;j<3;j++) { [unroll] for(int i=0;i<5;i++) {
        float2 uv=scene_uv(geometry.zw*float2(.12+.19*i,.26+.24*j));
        float3 wide=image0.SampleLevel(clamped,uv,0).rgb;
        float3 fine=original_linear.SampleLevel(clamped,uv,0).rgb;
        mean+=dot(wide,LUMA)/15;
        detail+=abs(dot(wide-fine,LUMA))/15;
    } }
    float4 old=image1.Load(int3(0,0,0));
    bool first=old.a<.5;
    float t=first?1:1-exp(-max(0,dynamics.w)/.085);
    float y=lerp(old.r,mean,t);
    float light_y=.88*y+.07, dark_y=.46*y+.008;
    float light_ink=light_y<.185?1:(light_y>.245?0:old.g);
    float dark_ink=dark_y<.185?1:(dark_y>.245?0:old.b);
    if(first) { light_ink=light_y<.215?1:0; dark_ink=dark_y<.215?1:0; }
    return float4(y,light_ink,dark_ink,1+lerp(max(0,old.a-1),saturate(detail*4),t));
}

// Analytic capsule geometry and normal, including bounded input-driven flex.
float3 liquid_field(float2 p, out float2 local, out float2 size) {
    float h=geometry.w;
    float margin=max(1.0,h*.045);
    size=geometry.zw-float2(margin*1.2,margin*2.3);
    float press=contact.z*contact.w;
    float2 stretch=float2(1-.018*press,1-.075*press-.022*abs(dynamics.x)*contact.w);
    float2 q=p-geometry.zw*.5;
    q.y+=h*.008;
    q.y-=q.x/geometry.z*h*.045*dynamics.x*contact.w;
    q/=stretch;
    // Materialize by changing lens shape and optical strength, not an opaque fade.
    float appearing=lerp(.40,1,smoothstep(0,1,dynamics.z));
    size.y*=appearing;
    float r=size.y*.5;
    float half_line=max(0,(size.x-size.y)*.5);
    float2 v=float2(q.x-clamp(q.x,-half_line,half_line),q.y);
    float len=length(v);
    local=q+geometry.zw*.5;
    return float3((len-r)*min(stretch.x,stretch.y),len>.00001?v/len:float2(0,0));
}
float wave_mask(float2 p) {
    float fg=text_mask.SampleLevel(clamped,p/geometry.zw,0);
    float h=geometry.w;
    [unroll] for(int i=0;i<5;i++) {
        float x=h*(.32+i*.105);
        float hy=h*(.07+.12*(.5+.5*sin(style.y*3.5+i*1.3)));
        float2 q=abs(p-float2(x,h*.5))-float2(h*.025,hy);
        float d=length(max(q,0))+min(max(q.x,q.y),0)-h*.012;
        fg=max(fg,saturate(.5-d));
    }
    return fg;
}
float3 transmit(float3 scene,bool dark) {
    // Real scene dynamic range, not the former .84+.105*y / .013+.045*y paint.
    return dark?scene*float3(.45,.46,.48)+float3(.006,.008,.012)
               :scene*float3(.88,.89,.90)+float3(.072,.074,.077);
}
float4 liquid_material_ps(float4 pos:SV_POSITION):SV_TARGET {
    float2 p=pos.xy, local, size;
    float3 field=liquid_field(p,local,size);
    float h=geometry.w, inset=max(0,-field.x);
    float coverage=saturate(.5-field.x);
    float4 adapt=adaptation.Load(int3(0,0,0));
    float complexity=saturate(adapt.a-1);
    // Shadow is confined to this small transparent canvas, not a rectangular fill.
    float2 dummy,ds; float shadow_d=liquid_field(p-float2(0,h*.026),dummy,ds).x;
    float shadow=(.10+.16*complexity)*exp(-pow(max(0,shadow_d)/(h*.052),2));
    shadow*=smoothstep(0,1,dynamics.z);
    if(coverage<=0) return float4(0,0,0,shadow);
    bool dark=style.x>.5;
    float radius=size.y*.5;
    float r=saturate(1-inset/max(radius,1));
    float nz=sqrt(max(.025,1-r*r));
    float3 normal=normalize(float3(field.yz*r,nz));
    float3 ray=refract(float3(0,0,-1),normal,1.0/1.46);
    float strength=smoothstep(0,1,dynamics.z);
    float2 bend=ray.xy/max(.30,-ray.z)*(h*(.26+.18*nz))*(1+.12*contact.z)*strength;
    bend+=(geometry.zw*.5-local)*.025*strength;
#ifdef LIQUID_TEST_NO_LENS
    bend=0;
#endif
    float2 uv=scene_uv(local+bend);
    float edge=1-smoothstep(h*.075,h*.28,inset);
    float support=glyph_support.SampleLevel(clamped,local/geometry.zw,0);
    // Keep live bar silhouettes sharp, frost only the immediate glyph neighborhood.
    float2 bq=abs(local-float2(h*.53,h*.50))-float2(h*.32,h*.24);
    float bar_support=1-smoothstep(0,h*.065,length(max(bq,0)));
    support=max(support,bar_support);
    if(style.z<.5) support=0;
    float frost=lerp(.62,1,support)*(1-edge*.91);
    float3 narrow=image1.SampleLevel(clamped,uv,0).rgb;
    float3 wide=image0.SampleLevel(clamped,uv,0).rgb;
    float3 scene=lerp(narrow,wide,saturate(frost));
    // Minute dispersion stays at the refracting perimeter, never rainbow outlines.
    float2 dispersion=field.yz*(h*.004*edge)/geometry.xy;
    scene.r=lerp(scene.r,image1.SampleLevel(clamped,uv+dispersion,0).r,edge*.24);
    scene.b=lerp(scene.b,image1.SampleLevel(clamped,uv-dispersion,0).b,edge*.24);
    float3 body=transmit(scene,dark);

    // Fresnel/geometry-controlled reflection with a moving contact light.
    float2 direction=normalize(float2(-.32,-.94)+(contact.xy-float2(.3,.18))*.95);
    float facing=max(0,dot(field.yz,direction));
    float opposing=max(0,dot(field.yz,-direction));
    float scale=h/26;
    float outer=exp(-pow((inset-.48*scale)/(.33*scale),2));
    float shoulder=exp(-pow((inset-1.32*scale)/(.52*scale),2));
    float fresnel=.035+.50*pow(1-normal.z,5);
    float3 ambient=(image0.SampleLevel(clamped,scene_uv(p-field.yz*h*.25),0).rgb
                   +image0.SampleLevel(clamped,scene_uv(p+field.yz*h*.25),0).rgb)*.5;
    float arc=.40+.60*exp(-pow((p.x/geometry.z-contact.x)/.34,2));
    float reflection=outer*(.18+.50*pow(facing,3)*arc)+fresnel*edge*.10;
    body=lerp(body,lerp(float3(1,1,1),ambient,.18),saturate(reflection));
    body*=1-shoulder*pow(opposing,2)*(.16+.07*complexity);
    // A separated inner caustic follows the bent light, not a broad white bevel.
    body+=shoulder*pow(facing,4)*(.018+.032*contact.z);
    float2 spot=(p/geometry.zw-contact.xy)/float2(.30,.65);
    float contact_glow=exp(-dot(spot,spot))*contact.z*.09;
    body=lerp(body,1,saturate(contact_glow));

    // Adaptive ink polarity is uniform per control and hysteretic on the GPU.
    // Legibility protection is local to strokes, NOT a solid capsule-shaped paint.
    float white_ink=dark?adapt.b:adapt.g;
    float y=dot(body,LUMA);
    if(white_ink>.5) {
        float needed=y>.16?1-.16/max(y,.001):0;
        body*=1-support*needed;
    } else {
        float needed=y<.30?(.30-y)/max(.001,1-y):0;
        body=lerp(body,1,support*needed);
    }
#ifndef LIQUID_TEST_INK_OFF
    if(style.z>.5) {
        float fg=wave_mask(local)*smoothstep(.28,.88,dynamics.z);
        float3 ink=white_ink>.5?float3(.95,.97,1):float3(.010,.016,.023);
        body=lerp(body,ink,fg);
    }
#endif
    float alpha=coverage+shadow*(1-coverage);
    return float4(optical_encode(body)*coverage,alpha);
}
