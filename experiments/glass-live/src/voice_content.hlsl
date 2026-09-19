#ifndef LIQUID_UNROLL
#define LIQUID_UNROLL [unroll]
#endif
// Foreground/state adapter only. The optical material below remains shared.
cbuffer VoiceContent : register(b2) {
    float4 voice_state; // phase; actual gated audio level; lifecycle opacity; reserved
};
static const float VOICE_AMPLITUDES[20] = {
    .24,.32,.44,.58,.42,.64,.88,1.,.78,.56,.92,.74,.58,.68,.49,.42,.35,.30,.25,.20
};
float bar_distance(float2 p,int i) {
    float h=geometry.w;
    if(voice_state.x==3) {
        if(i>=5) return 1000000;
        // Optimizing retains the approved five-stroke composition exactly.
        float x=h*(.32+i*.105);
        float hy=h*(.07+.12*(.5+.5*sin(style.y*3.5+i*1.3)));
        float2 q=abs(p-float2(x,h*.5))-float2(h*.025,hy);
        return length(max(q,0))+min(max(q.x,q.y),0)-h*.012;
    }
    if(voice_state.x!=2 || i>=20) return 1000000;
    float scale=h/26;
    float voice=saturate(voice_state.y);
    float phase=style.y/1.12*6.283185307179586;
    float offset=i*.53;
    float seed=((i*73+19)%101)/101.*6.283185307179586;
    float primary=(sin(phase*(.82+i*.013)+offset+seed)+1)*.5;
    float secondary=(sin(phase*2.17-offset*.71+seed*.37)+1)*.5;
    float height=3+13*VOICE_AMPLITUDES[i]*sqrt(voice)*(.24+.76*(.62*primary+.38*secondary));
    float x=(geometry.z-78*scale)*.5+(i*4+1)*scale;
    float2 q=abs(p-float2(x,h*.5))-float2(.65*scale,max(0,height*.5-.35)*scale);
    return length(max(q,0))+min(max(q.x,q.y),0)-.35*scale;
}
float wave_mask(float2 p) {
    float fg=text_mask.SampleLevel(clamped,p/geometry.zw,0);
    LIQUID_UNROLL for(int i=0;i<20;i++) { fg=max(fg,saturate(.5-bar_distance(p,i))); }
    return fg;
}
