#ifndef LIQUID_UNROLL
#define LIQUID_UNROLL [unroll]
#endif
// Background-conditioned convergence, not a replay of Apple's private shader.
// Filter steady scene coefficients separately from the finite entrance envelope.
// Otherwise a second low-pass delays both the density peak and its recovery.
// Shared refraction, transfer functions, voice strokes and the tagged control
// remain in liquid.hlsl / voice_content.hlsl. Scene pixels never leave the GPU.
cbuffer AdaptiveSurface : register(b3) {
    float4 canvas_space;
    float4 adaptive_time;
};
cbuffer MaterialTuning : register(b4) {
    float4 tuning_guards;  // scene guard, local guard, scene veil, local veil
    float4 tuning_neutral; // luminance low/high, chroma low/high
    float4 tuning_detail;  // detail low/high, scene complexity low/high
    float4 tuning_body;    // frost strength, milkiness, interior start/full
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
    LIQUID_UNROLL for(int j=0;j<3;j++){LIQUID_UNROLL for(int i=0;i<5;i++){
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
float3 adaptive_transmit(float3 scene,bool dark,float4 material,float neutral,float scene_white){
    float crest=material.w;float light_gain=max(.38,material.r);float dark_gain=max(.30,material.g);
    float3 transmitted=dark?scene*dark_gain*float3(.9783,1,1.0435)+float3(.006,.008,.012):scene*light_gain*float3(.990,1,1.010)+float3(.008,.009,.010);if(!dark){transmitted*=1+.11*neutral+.015*scene_white;}
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
    // Optical finish only: preserve the density/clarity trajectory and early rim.
    // Ease into the lighter white resting contour, then hold the factor constant.
    // Reduced motion starts at rest; black, chromatic and dark-theme relief is zero.
    float resting_relief=white_relief*(adaptive_time.z>.5?1:smoothstep(.40,.60,adaptive_time.x));
    float broad_d=liquid_field(p-float2(0,h*.098),dummy,ds).x;float contact_d=liquid_field(p-float2(0,h*.020),dummy,ds).x;
    float broad=(.018+.098*material.b+.017*complexity+.029*material.w)*exp(-.5*pow(max(0,broad_d)/(h*.225),2));
    float tight=(.010+.046*material.b+.016*material.w)*exp(-.5*pow(max(0,contact_d)/(h*.052),2));
    // Keep the exterior reach, but avoid a heavy two-ring button on white.
    // Black/chromatic scenes retain their previous shadow and rim treatment.
    broad*=lerp(1,.72,white_relief);tight*=lerp(1,.34,white_relief);
    broad*=lerp(1,.86,resting_relief);tight*=lerp(1,.48,resting_relief);
    float shadow=1-(1-broad)*(1-tight);
    float reveal=adaptive_time.z>.5?1:smoothstep(0,.10,adaptive_time.x);shadow*=reveal;
    if(coverage<=0){
#ifdef LIQUID_VOICE_CONTENT
        return float4(0,0,0,shadow)*voice_state.z;
#else
        return float4(0,0,0,shadow);
#endif
    }
    bool dark=style.x>.5;float radius=size.y*.5;float depth=saturate(inset/max(radius,1));float r=1-depth;float nz=sqrt(max(.025,1-r*r));float3 normal=normalize(float3(field.yz*r,nz));
    // C98: compress contrast around a broader symmetric macro local mean; no white body veil/scatter.
    // while the shell produces a stronger nonlinear bend near the optical rim.
    // Both terms converge smoothly so the silhouette stays stable.
    float3 ray=refract(float3(0,0,-1),normal,1.0/1.46);float strength=smoothstep(0,1,dynamics.z);
    // C9: move the strongest bend slightly inside the silhouette. Apple-like
    // glass reads as one coherent lens, not a highly distorted waterline.
    float shell=smoothstep(.025,.11,depth)*(1-smoothstep(.30,.72,depth));float body_focus=smoothstep(.055,.40,depth);
    // C32: keep cap-local radial focusing, but give the straight body a gentle
    // axial curvature that fades out before the caps. This restores broad
    // horizontal magnification without returning to whole-pill compression.
    float2 lens_q=local-geometry.zw*.5;
    float lens_half_line=max(0,(size.x-size.y)*.5);
    float2 lens_axis=float2(clamp(lens_q.x,-lens_half_line,lens_half_line),0);
    float2 lens_local=lens_q-lens_axis;
    float body_zone=1-smoothstep(lens_half_line*.68,lens_half_line*.96,abs(lens_q.x));
    float cap_zone=smoothstep(0,1,saturate((abs(lens_q.x)-lens_half_line)/max(radius,1)));
    float radial_gain=lerp(.46,1.40,cap_zone);
    float body_axis=clamp(lens_q.x/max(lens_half_line,1),-1,1);float body_y=clamp(lens_local.y/max(radius,1),-1,1);float vertical_curve=1-.18*body_axis*body_axis;float radial_y=lerp(.10,.50,cap_zone);float optical_thickness=sqrt(saturate(1-r*r));float pill_cap_gate=smoothstep(h*.08,h*.35,lens_half_line);float cap_shell_focus=cap_zone*r*optical_thickness*pill_cap_gate;float2 radial_focus=float2(field.y*h*.72*cap_shell_focus,-lens_local.y*radial_y*radial_gain*vertical_curve);
    float straight_gate=smoothstep(h*.08,h*.35,lens_half_line);float body_lens_profile=(1-.12*body_axis*body_axis)*(1-.28*body_y*body_y)*body_zone*straight_gate;float2 body_lens=float2(-lens_q.x*.075*(1-.24*body_y*body_y),-lens_local.y*.008*(1-.24*body_axis*body_axis))*body_lens_profile;float cross_profile=(1-.60*body_axis*body_axis)*(1-.52*body_y*body_y)*body_zone*straight_gate;float2 cross_lens=float2(-body_y*h*.006*(1-.65*body_axis*body_axis),0)*cross_profile;
    float2 magnify=(radial_focus+body_lens+cross_lens)*body_focus;
    float cap_refract=lerp(.88,1.74,cap_zone);
    float2 refract_bend=ray.xy/max(.30,-ray.z)*(h*(.22+.14*nz))*(1+.08*contact.z)*(.38+.62*shell)*cap_refract;
    float2 bend=(refract_bend+magnify)*strength;
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
    LIQUID_UNROLL for(int i=0;i<20;i++){
#else
    LIQUID_UNROLL for(int i=0;i<5;i++){
#endif
        support=max(support,1-smoothstep(-h*.012,h*.055,bar_distance(local,i)));
    }
        if(style.z<.5)support=0;
    float3 narrow=image1.SampleLevel(clamped,uv,0).rgb;
    float3 wide=image0.SampleLevel(clamped,uv,0).rgb;
    // Apple-style readability behavior, inferred from observable output rather
    // than private parameters: retain the low-frequency environment, but stop
    // dark glyph strokes on a bright neutral scene from remaining readable.
    // Empty white, dark and chromatic scenes produce zero or negligible risk.
    float narrow_y=dot(narrow,LUMA),wide_y=dot(wide,LUMA);
    float wide_chroma=max(wide.r,max(wide.g,wide.b))-min(wide.r,min(wide.g,wide.b));
    float bright_neutral=smoothstep(tuning_neutral.x,tuning_neutral.y,wide_y)*(1-smoothstep(tuning_neutral.z,tuning_neutral.w,wide_chroma));
    float dark_detail=smoothstep(tuning_detail.x,tuning_detail.y,max(0,wide_y-narrow_y));
    // Keep the optical rim and refraction alive: protection ramps up only after
    // entering the body and never changes coverage/window alpha.
    float protected_interior=smoothstep(h*tuning_body.z,h*tuning_body.w,inset);
    float scene_text_risk=dark?0:white_risk*smoothstep(tuning_detail.z,tuning_detail.w,complexity);
    float local_text_risk=dark?0:bright_neutral*dark_detail;
    float readability_guard=saturate((tuning_guards.x*scene_text_risk+tuning_guards.y*local_text_risk)*protected_interior);
    // C39: post-warp medium-scale low-pass. Keep the coherent lens mapping,
    // then suppress readable glyph strokes without washing out macro structure.
    float2 post_step=float2(h*.175/geometry.x,h*.045/geometry.y);
    float3 post_env=narrow*.14;
    post_env+=image1.SampleLevel(clamped,uv+float2(post_step.x,0),0).rgb*.18;
    post_env+=image1.SampleLevel(clamped,uv-float2(post_step.x,0),0).rgb*.18;
    post_env+=image1.SampleLevel(clamped,uv+float2(0,post_step.y),0).rgb*.05;
    post_env+=image1.SampleLevel(clamped,uv-float2(0,post_step.y),0).rgb*.05;
    post_env+=image1.SampleLevel(clamped,uv+post_step,0).rgb*.08;
    post_env+=image1.SampleLevel(clamped,uv-post_step,0).rgb*.08;
    post_env+=image1.SampleLevel(clamped,uv+float2(post_step.x,-post_step.y),0).rgb*.08;
    post_env+=image1.SampleLevel(clamped,uv+float2(-post_step.x,post_step.y),0).rgb*.08;
    float post_gain=lerp(.42,.62,cap_zone)*protected_interior*(1-edge*.76);
    float3 scene=lerp(narrow,post_env,saturate(post_gain));
    float wide_gain=lerp(.045,.095,cap_zone)*protected_interior*(1-edge);
    float body_soft=protected_interior*(1-cap_zone)*(1-edge*.44);float2 body_xstep=float2(h*.20/geometry.x,0);float2 body_xfar=float2(h*.40/geometry.x,0);float3 body_l=image0.SampleLevel(clamped,uv-body_xstep,0).rgb;float3 body_r=image0.SampleLevel(clamped,uv+body_xstep,0).rgb;float3 body_lf=image0.SampleLevel(clamped,uv-body_xfar,0).rgb;float3 body_rf=image0.SampleLevel(clamped,uv+body_xfar,0).rgb;float3 body_macro=wide*.32+body_l*.20+body_r*.20+body_lf*.14+body_rf*.14;scene=lerp(scene,body_macro,.16*body_soft);scene=lerp(scene,wide,wide_gain+.018*body_soft);float lowfreq_contrast=.006*protected_interior*lerp(1.0,.84,cap_zone)*(1-edge*.55);scene=lerp(scene,scene*scene,lowfreq_contrast);scene=lerp(scene,post_env,.028*body_soft);float2 well_step=float2(h*.055/geometry.x,h*.080/geometry.y);float foreground_halo=support;foreground_halo=max(foreground_halo,glyph_support.SampleLevel(clamped,local/geometry.zw+float2(well_step.x,0),0));foreground_halo=max(foreground_halo,glyph_support.SampleLevel(clamped,local/geometry.zw-float2(well_step.x,0),0));foreground_halo=max(foreground_halo,glyph_support.SampleLevel(clamped,local/geometry.zw+float2(0,well_step.y),0));foreground_halo=max(foreground_halo,glyph_support.SampleLevel(clamped,local/geometry.zw-float2(0,well_step.y),0));float foreground_well=saturate(foreground_halo)*protected_interior;scene=lerp(scene,post_env,.30*foreground_well);scene=lerp(scene,wide,.085*foreground_well);float center_x=1-smoothstep(.22,.62,abs(body_axis));float center_y=1-smoothstep(.48,.90,abs(body_y));float center_readability=center_x*center_y*straight_gate*protected_interior;scene=lerp(scene,post_env,.16*center_readability);scene=lerp(scene,wide,.070*center_readability);    // Issue #15: frost_strength above 1 drives extra protected-interior
    // diffusion instead of merely saturating at the existing wide blur.
    // Re-sample image0 at a compact cross so high-frequency glyph strokes
    // collapse into lower-frequency environmental structure without touching
    // rim/refraction/dispersion/shadow behavior.
    float diffusion_risk=saturate((.22*scene_text_risk+1.18*local_text_risk)*protected_interior);
    float frost_diffusion=saturate((tuning_body.x-.42)*2.20);
    float extra_diffusion=frost_diffusion*diffusion_risk;
    float2 diffusion_step=float2(h*.66/geometry.x,h*.46/geometry.y);
    float3 diffuse=wide*.20;
    diffuse+=image0.SampleLevel(clamped,uv+float2(diffusion_step.x,0),0).rgb*.12;
    diffuse+=image0.SampleLevel(clamped,uv-float2(diffusion_step.x,0),0).rgb*.12;
    diffuse+=image0.SampleLevel(clamped,uv+float2(0,diffusion_step.y),0).rgb*.12;
    diffuse+=image0.SampleLevel(clamped,uv-float2(0,diffusion_step.y),0).rgb*.12;
    diffuse+=image0.SampleLevel(clamped,uv+diffusion_step,0).rgb*.08;
    diffuse+=image0.SampleLevel(clamped,uv-diffusion_step,0).rgb*.08;
    diffuse+=image0.SampleLevel(clamped,uv+float2(diffusion_step.x,-diffusion_step.y),0).rgb*.08;
    diffuse+=image0.SampleLevel(clamped,uv+float2(-diffusion_step.x,diffusion_step.y),0).rgb*.08;
    scene=lerp(scene,diffuse,extra_diffusion);
    float straight_diffusion_guard=white_risk*protected_interior*(1-cap_zone)*straight_gate*(1-edge);
    scene=lerp(scene,diffuse,.94*straight_diffusion_guard);
    float2 dispersion=field.yz*(h*.004*edge)/geometry.xy;scene.r=lerp(scene.r,image1.SampleLevel(clamped,uv+dispersion,0).r,edge*.24);scene.b=lerp(scene.b,image1.SampleLevel(clamped,uv-dispersion,0).b,edge*.24);float3 body=adaptive_transmit(scene,dark,material,bright_neutral,white_risk);float broad_env=dot(wide,LUMA)-adapt.r;body+=(broad_env*.035*white_risk*protected_interior).xxx;
    // A restrained neutral veil lifts the protected interior without becoming an
    // opaque card. Local dark strokes get more lift than the surrounding field;
    // low-frequency environment and all edge optics continue through the glass.
    float readability_veil=saturate((tuning_guards.z*scene_text_risk+tuning_guards.w*local_text_risk)*protected_interior)*lerp(.16,.62,cap_zone);
    float milkiness=saturate(tuning_body.y*bright_neutral*protected_interior)*lerp(.12,.56,cap_zone);
    float2 mean_step=float2(h*.62/geometry.x,h*.48/geometry.y);float3 macro_mean_scene=wide*.20;macro_mean_scene+=image0.SampleLevel(clamped,uv+float2(mean_step.x,0),0).rgb*.12;macro_mean_scene+=image0.SampleLevel(clamped,uv-float2(mean_step.x,0),0).rgb*.12;macro_mean_scene+=image0.SampleLevel(clamped,uv+float2(0,mean_step.y),0).rgb*.12;macro_mean_scene+=image0.SampleLevel(clamped,uv-float2(0,mean_step.y),0).rgb*.12;macro_mean_scene+=image0.SampleLevel(clamped,uv+mean_step,0).rgb*.08;macro_mean_scene+=image0.SampleLevel(clamped,uv-mean_step,0).rgb*.08;macro_mean_scene+=image0.SampleLevel(clamped,uv+float2(mean_step.x,-mean_step.y),0).rgb*.08;macro_mean_scene+=image0.SampleLevel(clamped,uv+float2(-mean_step.x,mean_step.y),0).rgb*.08;float3 local_mean_scene=lerp(macro_mean_scene,post_env,.02);float3 local_mean_body=adaptive_transmit(local_mean_scene,dark,material,bright_neutral,white_risk);float text_contrast_guard=saturate((.12*scene_text_risk+.78*local_text_risk)*protected_interior);float straight_frost_guard=white_risk*protected_interior*(1-cap_zone)*straight_gate*(1-edge);float contrast_guard=saturate(readability_veil+milkiness+text_contrast_guard+.68*straight_frost_guard+.50*foreground_well+.35*center_readability);float contrast_scale=lerp(1,0.0,contrast_guard);body=local_mean_body+(body-local_mean_body)*contrast_scale;
    float2 direction=normalize(float2(-.32,-.94)+(contact.xy-float2(.3,.18))*.95);float facing=max(0,dot(field.yz,direction));float opposing=max(0,dot(field.yz,-direction));float top_light=max(0,dot(field.yz,normalize(float2(-.22,-.98))));float scale=h/26;
    // C10: a thick optical edge: bright outer rim, darker transition trough,
    // then a broader inner caustic instead of one narrow bright stroke.
    float rim_scale=scale*rim_fraction*1.18;
    float outer=exp(-pow((inset-.46*rim_scale)/(.66*rim_scale),2));
    float trough=exp(-pow((inset-1.34*rim_scale)/(.88*rim_scale),2));
    float inner=exp(-pow((inset-2.28*rim_scale)/(1.48*rim_scale),2));float fresnel=.040+.56*pow(1-normal.z,5);
    float3 ambient=(image0.SampleLevel(clamped,scene_uv(p-field.yz*h*.30),0).rgb+image0.SampleLevel(clamped,scene_uv(p+field.yz*h*.30),0).rgb)*.5;float arc=.40+.60*exp(-pow((p.x/geometry.z-contact.x)/.34,2));
    float reflection=outer*(.18+.46*pow(facing,3)*arc)+inner*(.048+.102*pow(facing,2))+fresnel*edge*.10;float directional_spec=outer*.060*pow(top_light,3)+inner*.022*pow(top_light,2);
    body=lerp(body,lerp(float3(1,1,1),ambient,.18),saturate(reflection));body=lerp(body,float3(1,1,1),saturate(directional_spec));
    body*=1-trough*(.006+pow(opposing,2)*(.040+.018*complexity+.006*material.w))*lerp(1,.72,white_relief)*lerp(1,.72,resting_relief);
    body+=inner*(.013+.031*pow(facing,4)+.017*contact.z);
    float2 spot=(p/geometry.zw-contact.xy)/float2(.30,.65);float contact_glow=exp(-dot(spot,spot))*contact.z*.09;body=lerp(body,1,saturate(contact_glow));
    float white_ink=dark?adapt.b:adapt.g;float y=dot(body,LUMA);if(!dark&&material.w>.18)white_ink=0;
    if(white_ink>.5){float needed=y>.16?1-.16/max(y,.001):0;body*=1-support*needed;}else{float support_gain=y<.30?min(4,.30/max(y,.05)):1;body*=lerp(1,support_gain,support);}
#ifndef LIQUID_TEST_INK_OFF
    if(style.z>.5){float foreground=adaptive_time.z>.5?1:lerp(.65,1,smoothstep(0,.09,adaptive_time.y));float fg=adaptive_text(local);if(voice_state.x!=2){LIQUID_UNROLL for(int k=0;k<20;k++){fg=max(fg,saturate(.5-bar_distance(local,k)));}}fg*=foreground;float3 ink=white_ink>.5?float3(.95,.97,1):float3(.010,.016,.023);
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
