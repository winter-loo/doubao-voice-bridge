// Background-conditioned convergence, not a replay of Apple's private shader.
// Filter steady scene coefficients separately from the finite entrance envelope.
// Otherwise a second low-pass delays both the density peak and its recovery.
// Shared refraction, transfer functions, voice strokes and the tagged control
// remain in liquid.hlsl / voice_content.hlsl. Scene pixels never leave the GPU.
cbuffer AdaptiveSurface : register(b3) {
    float4 canvas_space;
    float4 adaptive_time;
};
float white_density_crest(float age,float risk,float complexity) {
    // Engineering presentation-time envelope, not measured Apple parameters.
    // Continuous attack and a single release: do not low-pass this result again.
    // The finite tail prevents residual density from breathing on a held scene.
    float attack=smoothstep(.090,.130,age);
    float release=exp(-max(0,age-.130)/.140);
    float tail=1-smoothstep(.50,.70,age);
    return saturate(attack*release*tail*risk*(1-.42*complexity));
}
float4 adaptive_reduce_ps(float4 position:SV_POSITION):SV_TARGET {
    float3 mean=0;float detail=0,second=0;
    [unroll] for(int j=0;j<3;j++){[unroll] for(int i=0;i<5;i++){
        float2 uv=scene_uv(geometry.zw*float2(.12+.19*i,.26+.24*j));
        float3 wide=image0.SampleLevel(clamped,uv,0).rgb;
        float3 fine=original_linear.SampleLevel(clamped,uv,0).rgb;
        mean+=wide/15;float sy=dot(wide,LUMA);second+=sy*sy/15;detail+=abs(dot(wide-fine,LUMA))/15;
    }}
    float current_y=dot(mean,LUMA);
    float chroma=max(mean.r,max(mean.g,mean.b))-min(mean.r,min(mean.g,mean.b));
    float complexity=saturate(detail*4+sqrt(max(0,second-current_y*current_y))*.5);
    float white_risk=smoothstep(.35,.90,current_y)*(1-smoothstep(.04,.22,chroma));
    float light_target=clamp(.93+.025*white_risk-.065*complexity,.78,.96);
    float dark_target=clamp(.46-.025*complexity,.38,.48);
    float edge_risk=smoothstep(.03,.78,current_y);
    float4 old=image1.Load(int3(0,0,0));float4 old_material=image1.Load(int3(1,0,0));
    bool first=old.a<.5;bool reduced=adaptive_time.z>.5;float dt=max(0,dynamics.w);
    float tracking=reduced?1:1-exp(-dt/.060);float settling=reduced?1:1-exp(-dt/.115);
    float y=first?current_y:lerp(old.r,current_y,tracking);
    float light_y=light_target*current_y+.008+.012*white_risk;float dark_y=dark_target*current_y+.008;
    float light_ink=light_y<.185?1:(light_y>.245?0:old.g);float dark_ink=dark_y<.185?1:(dark_y>.245?0:old.b);
    if(first){light_ink=light_y<.215?1:0;dark_ink=dark_y<.215?1:0;}
    if(position.x<1)return float4(y,light_ink,dark_ink,1+(first?complexity:lerp(max(0,old.a-1),complexity,tracking)));
    // History contains only steady transmission, edge risk and neutral-white risk.
    // Never store the time-shaped crest here: that would filter its timing twice.
    float4 target=float4(light_target,dark_target,edge_risk,white_risk);
    if(first)return reduced?target:float4(light_target,dark_target,edge_risk*.72,white_risk);
    return lerp(old_material,target,settling);
}
float adaptive_text(float2 p){
    float2 uv=p/geometry.zw;float sharp=text_mask.SampleLevel(clamped,uv,0);
    float clarity=adaptive_time.z>.5?1:smoothstep(0,.11,adaptive_time.y);float radius=geometry.w*.034*(1-clarity);
    float2 step=float2(radius/geometry.z,radius/geometry.w);float soft=sharp*.40;
    soft+=(text_mask.SampleLevel(clamped,uv+float2(step.x,0),0)+text_mask.SampleLevel(clamped,uv-float2(step.x,0),0)+text_mask.SampleLevel(clamped,uv+float2(0,step.y),0)+text_mask.SampleLevel(clamped,uv-float2(0,step.y),0))*.15;
    return lerp(soft,sharp,clarity);
}
float3 adaptive_transmit(float3 scene,bool dark,float4 material){
    float crest=material.w;float light_gain=max(.38,material.r);float dark_gain=max(.30,material.g);
    float3 transmitted=dark?scene*dark_gain*float3(.9783,1,1.0435)+float3(.006,.008,.012):scene*light_gain*float3(.990,1,1.010)+float3(.008,.009,.010);
    if(!dark)transmitted*=1-.055*crest;return transmitted;
}
float4 adaptive_material_ps(float4 pos:SV_POSITION):SV_TARGET {
    float2 p=pos.xy-canvas_space.xy,local,size;float3 field=liquid_field(p,local,size);float h=geometry.w,inset=max(0,-field.x);float coverage=saturate(.5-field.x);
    float4 adapt=adaptation.Load(int3(0,0,0));float complexity=saturate(adapt.a-1);float4 material=adaptation.Load(int3(1,0,0));float2 dummy,ds;
    float white_risk=saturate(material.w);
    float crest=adaptive_time.z>.5?0:white_density_crest(adaptive_time.x,white_risk,complexity);
    // Apply the transient once, without feeding it back into steady history.
    material.r-=.42*crest;material.g-=.08*crest;material.w=crest;
    float white_relief=style.x>.5?0:white_risk;
    float broad_d=liquid_field(p-float2(0,h*.065),dummy,ds).x;float contact_d=liquid_field(p-float2(0,h*.025),dummy,ds).x;
    float broad=(.018+.102*material.b+.020*complexity+.035*material.w)*exp(-.5*pow(max(0,broad_d)/(h*.16),2));
    float tight=(.020+.080*material.b+.025*material.w)*exp(-.5*pow(max(0,contact_d)/(h*.040),2));
    // Keep the exterior reach, but avoid a heavy two-ring button on white.
    // Black/chromatic scenes retain their previous shadow and rim treatment.
    broad*=lerp(1,.65,white_relief);tight*=lerp(1,.52,white_relief);
    float shadow=1-(1-broad)*(1-tight);
    float reveal=adaptive_time.z>.5?1:smoothstep(0,.10,adaptive_time.x);shadow*=reveal;
    if(coverage<=0){
#ifdef LIQUID_VOICE_CONTENT
        return float4(0,0,0,shadow)*voice_state.z;
#else
        return float4(0,0,0,shadow);
#endif
    }
    bool dark=style.x>.5;float radius=size.y*.5;float r=saturate(1-inset/max(radius,1));float nz=sqrt(max(.025,1-r*r));float3 normal=normalize(float3(field.yz*r,nz));
    float3 ray=refract(float3(0,0,-1),normal,1.0/1.46);float strength=smoothstep(0,1,dynamics.z);float2 bend=ray.xy/max(.30,-ray.z)*(h*(.26+.18*nz))*(1+.12*contact.z)*strength;bend+=(geometry.zw*.5-local)*.025*strength;
#ifdef LIQUID_TEST_NO_LENS
    bend=0;
#endif
    // liquid_field contracts the entrance height, not the logical text/wave grid.
    // Full-height rim bands otherwise flood the thin opening capsule's center,
    // causing a false brightness pulse even on black with zero density crest.
    // Derive the ratio from the field's actual size; it is exactly one at rest.
    float full_height=h-2.3*max(1.0,h*.045);
    float rim_fraction=saturate(size.y/max(1.0,full_height));
    float rim_height=h*rim_fraction;
    float2 uv=scene_uv(local+bend);float edge=1-smoothstep(rim_height*.075,rim_height*.28,inset);float support=glyph_support.SampleLevel(clamped,local/geometry.zw,0);
#ifdef LIQUID_VOICE_CONTENT
    [unroll] for(int i=0;i<20;i++){
#else
    [unroll] for(int i=0;i<5;i++){
#endif
        support=max(support,1-smoothstep(-h*.012,h*.055,bar_distance(local,i)));
    }
    if(style.z<.5)support=0;float frost=lerp(.62,1,support)*(1-edge*.91);float3 narrow=image1.SampleLevel(clamped,uv,0).rgb;float3 wide=image0.SampleLevel(clamped,uv,0).rgb;float3 scene=lerp(narrow,wide,saturate(frost));
    float2 dispersion=field.yz*(h*.004*edge)/geometry.xy;scene.r=lerp(scene.r,image1.SampleLevel(clamped,uv+dispersion,0).r,edge*.24);scene.b=lerp(scene.b,image1.SampleLevel(clamped,uv-dispersion,0).b,edge*.24);float3 body=adaptive_transmit(scene,dark,material);
    float2 direction=normalize(float2(-.32,-.94)+(contact.xy-float2(.3,.18))*.95);float facing=max(0,dot(field.yz,direction));float opposing=max(0,dot(field.yz,-direction));float scale=h/26;
    // Keep scale unchanged for the original 20-bar content below.
    float rim_scale=scale*rim_fraction;
    float outer=exp(-pow((inset-.48*rim_scale)/(.33*rim_scale),2));float shoulder=exp(-pow((inset-1.32*rim_scale)/(.52*rim_scale),2));float fresnel=.035+.50*pow(1-normal.z,5);
    float3 ambient=(image0.SampleLevel(clamped,scene_uv(p-field.yz*h*.25),0).rgb+image0.SampleLevel(clamped,scene_uv(p+field.yz*h*.25),0).rgb)*.5;float arc=.40+.60*exp(-pow((p.x/geometry.z-contact.x)/.34,2));float reflection=outer*(.18+.50*pow(facing,3)*arc)+fresnel*edge*.10;
    body=lerp(body,lerp(float3(1,1,1),ambient,.18),saturate(reflection));body*=1-shoulder*pow(opposing,2)*(.16+.07*complexity+.025*material.w)*lerp(1,.65,white_relief);body+=shoulder*pow(facing,4)*(.018+.032*contact.z);
    float2 spot=(p/geometry.zw-contact.xy)/float2(.30,.65);float contact_glow=exp(-dot(spot,spot))*contact.z*.09;body=lerp(body,1,saturate(contact_glow));
    float white_ink=dark?adapt.b:adapt.g;float y=dot(body,LUMA);if(!dark&&material.w>.18)white_ink=0;
    if(white_ink>.5){float needed=y>.16?1-.16/max(y,.001):0;body*=1-support*needed;}else{float needed=y<.30?(.30-y)/max(.001,1-y):0;body=lerp(body,1,support*needed);}
#ifndef LIQUID_TEST_INK_OFF
    if(style.z>.5){float foreground=adaptive_time.z>.5?1:lerp(.65,1,smoothstep(0,.09,adaptive_time.y));float fg=adaptive_text(local);if(voice_state.x!=2){[unroll] for(int k=0;k<20;k++){fg=max(fg,saturate(.5-bar_distance(local,k)));}}fg*=foreground;float3 ink=white_ink>.5?float3(.95,.97,1):float3(.010,.016,.023);
#ifdef LIQUID_VOICE_CONTENT
        if(voice_state.x==2){float first=(geometry.z-78*scale)*.5+scale;int i=(int)clamp(floor((local.x-first)/(4*scale)+.5),0,19);float3 srgb=floor(lerp(float3(67,222,210),float3(100,141,255),i/19.0)+.5)/255.0;float wave=saturate(.5-bar_distance(local,i));body=lerp(body,linearize(srgb),wave);fg=adaptive_text(local)*foreground;}
#endif
        body=lerp(body,ink,fg);
    }
#endif
    float alpha=coverage+shadow*(1-coverage);
#ifdef LIQUID_VOICE_CONTENT
    return float4(optical_encode(body)*coverage,alpha)*voice_state.z;
#else
    return float4(optical_encode(body)*coverage,alpha);
#endif
}
