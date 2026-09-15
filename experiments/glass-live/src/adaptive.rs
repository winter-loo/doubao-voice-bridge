//! Production adaptive optics. The tagged pipeline remains an executable control.
//! All scene statistics and temporal history stay on the GPU.
use super::*;
use crate::{adaptive_model::CanvasGeometry, voice_model::Phase};

#[repr(C)]
#[derive(Clone, Copy)]
struct SurfaceConstants {
    canvas: [f32;4], // visual margin xy, independent canvas size xy
    timeline: [f32;4], // visual age, foreground age, reduced motion, reserved
}
pub struct AdaptivePipeline {
    inner: Pipeline,
    pub(crate) layout: CanvasGeometry,
    pub(crate) canvas: Texture,
    history: [Texture;2],
    index: Cell<usize>,
    initialized: Cell<bool>,
    last_time: Cell<f32>,
    foreground_start: Cell<f32>,
    phase: Cell<Option<Phase>>,
    surface: ID3D11Buffer,
    shader: ID3D11PixelShader,
    reduce: ID3D11PixelShader,
}
impl Deref for AdaptivePipeline { type Target=Pipeline; fn deref(&self)->&Pipeline { &self.inner } }
impl AdaptivePipeline {
    pub unsafe fn new_voice(device:ID3D11Device,context:ID3D11DeviceContext,w:u32,h:u32,mask:&[u8])->AppResult<Self> {
        let inner=Pipeline::new_voice(device,context,w,h,mask)?;
        let layout=CanvasGeometry::new(w,h);
        let source=format!("#define LIQUID_VOICE_CONTENT 1\n{}\n{}\n{}\n{}",include_str!("glass.hlsl"),include_str!("voice_content.hlsl"),include_str!("liquid.hlsl"),include_str!("adaptive.hlsl"));
        let make=|entry|->AppResult<ID3D11PixelShader>{
            let b=compile_source(&source,entry,s!("ps_5_0"))?;let mut shader=None;
            inner.device.CreatePixelShader(blob_bytes(&b),None,Some(&mut shader))?;
            Ok(shader.ok_or("Missing adaptive shader")?)
        };
        let shader=make(s!("adaptive_material_ps"))?;let reduce=make(s!("adaptive_reduce_ps"))?;
        let history=[Texture::new(&inner.device,2,1,DXGI_FORMAT_R32G32B32A32_FLOAT)?,Texture::new(&inner.device,2,1,DXGI_FORMAT_R32G32B32A32_FLOAT)?];
        for t in &history { inner.context.ClearRenderTargetView(&t.target,&[0.;4]); }
        let canvas=Texture::new(&inner.device,layout.canvas_width(),layout.canvas_height(),DXGI_FORMAT_B8G8R8A8_UNORM)?;
        let mut surface=None;
        inner.device.CreateBuffer(&D3D11_BUFFER_DESC{ByteWidth:size_of::<SurfaceConstants>() as u32,Usage:D3D11_USAGE_DEFAULT,BindFlags:D3D11_BIND_CONSTANT_BUFFER.0 as u32,..Default::default()},None,Some(&mut surface))?;
        Ok(Self{inner,layout,canvas,history,index:Cell::new(0),initialized:Cell::new(false),last_time:Cell::new(-1.),foreground_start:Cell::new(0.),phase:Cell::new(None),surface:surface.ok_or("No adaptive uniforms")?,shader,reduce})
    }
    pub unsafe fn render_voice(&self,dark:bool,time:f32,phase:Phase,level:f32,opacity:f32) {
        if !time.is_finite() || time<0. { return; }
        if self.phase.get()!=Some(phase) { self.phase.set(Some(phase));self.foreground_start.set(time); }
        let old_time=self.last_time.get();
        let dt=if old_time<0. {0.} else {(time-old_time).clamp(0.,0.25)};
        self.last_time.set(time.max(old_time));
        let mut point=[0.30,0.18];let mut pressed=false;let mut drift=[0.;2];
        let hwnd=self.inner.hwnd.get();
        if !hwnd.0.is_null() {
            let mut cursor=POINT::default();let mut r=RECT::default();
            if GetCursorPos(&mut cursor).is_ok() && GetWindowRect(hwnd,&mut r).is_ok() {
                pressed=GetCapture()==hwnd;
                if WindowFromPoint(cursor)==hwnd || pressed { point=[(cursor.x-r.left) as f32/self.layout.width as f32,(cursor.y-r.top) as f32/self.layout.height as f32]; }
                if let Some((x,y))=self.inner.last_position.replace(Some((r.left,r.top))) {
                    let d=dt.max(0.001)*self.layout.height as f32*12.;drift=[(r.left-x) as f32/d,(r.top-y) as f32/d];
                }
            }
        }
        let reduced=self.inner.reduced_motion.get();
        let mut motion=self.inner.motion.borrow_mut();motion.step(time,point,pressed,drift,reduced);
        let c=OpticalConstants{contact:[motion.point[0],motion.point[1],motion.press,if reduced{0.}else{1.}],dynamics:[motion.drift[0],motion.drift[1],if reduced{1.}else{(time/0.24).clamp(0.,1.)},dt]};
        drop(motion);
        let opacity=if opacity.is_finite(){opacity.clamp(0.,1.)}else{0.};
        let level=if level.is_finite(){level.clamp(0.,1.)}else{0.};
        let voice=[phase as u32 as f32,if phase==Phase::Listening{level}else{0.},opacity,0.];
        let surface=SurfaceConstants{canvas:[self.layout.margin as f32,self.layout.margin as f32,self.canvas.width as f32,self.canvas.height as f32],timeline:[time,(time-self.foreground_start.get()).max(0.),if reduced{1.}else{0.},0.]};
        self.context.UpdateSubresource(&self.inner.interaction,0,None,(&c as *const OpticalConstants).cast(),0,0);
        self.context.UpdateSubresource(self.inner.voice.as_ref().unwrap(),0,None,voice.as_ptr().cast(),0,0);
        self.context.UpdateSubresource(&self.surface,0,None,(&surface as *const SurfaceConstants).cast(),0,0);
        self.context.PSSetConstantBuffers(1,Some(&[Some(self.inner.interaction.clone()),self.inner.voice.clone(),Some(self.surface.clone())]));
        let mut params=self.inner.base.params();params.style=[if dark{1.}else{0.},time,1.,1.];
        if !self.initialized.get() || dt>0. {
            let old=self.index.get();let next=1-old;
            self.inner.base.pass(&self.history[next],&self.reduce,&[Some(self.inner.base.wide.view.clone()),Some(self.history[old].view.clone()),None,Some(self.inner.base.linear.view.clone())],params);
            self.context.PSSetShaderResources(0,Some(&[None,None,None,None,None,None]));
            self.index.set(next);self.initialized.set(true);
        }
        self.inner.base.pass(&self.canvas,&self.shader,&[Some(self.inner.base.wide.view.clone()),Some(self.inner.base.narrow.view.clone()),Some(self.inner.base.mask.view.clone()),Some(self.inner.base.linear.view.clone()),Some(self.inner.halo.view.clone()),Some(self.history[self.index.get()].view.clone())],params);
        self.context.PSSetShaderResources(0,Some(&[None,None,None,None,None,None]));
        self.context.PSSetConstantBuffers(1,Some(&[None,None,None]));
        // Keep a logical-body view for palette/state checks. Only canvas is presented.
        let m=self.layout.margin;
        let area=D3D11_BOX{left:m,top:m,front:0,right:m+self.layout.width,bottom:m+self.layout.height,back:1};
        self.context.CopySubresourceRegion(&self.inner.output.texture,0,0,0,0,&self.canvas.texture,0,Some(&area));
    }
    /// Generated-fixture or explicitly requested review only; never called live.
    pub(crate) unsafe fn composite_fixture(&self)->AppResult<Vec<u8>> {
        ensure(self.layout.margin<=self.padding,"Fixture background does not include the entire visual canvas")?;
        let mut background=self.read_rgba(&self.raw)?;
        let layer=self.read_rgba(&self.canvas)?;
        let origin=(self.padding-self.layout.margin) as usize;
        for y in 0..self.canvas.height as usize {for x in 0..self.canvas.width as usize {
            let i=(y*self.canvas.width as usize+x)*4;let j=((y+origin)*self.raw.width as usize+x+origin)*4;
            for k in 0..3 {background[j+k]=(layer[i+k] as u32+(background[j+k] as u32*(255-layer[i+3]) as u32+127)/255).min(255) as u8;}
            background[j+3]=255;
        }}
        Ok(background)
    }
}

/// One GPU canvas, one input-only logical capsule. No double-blended edges or
/// separate present clocks for the body and its shadow.
pub struct AdaptivePresenter { inner:super::super::Presenter,input:HWND,reduced:bool }
impl AdaptivePresenter {
    pub unsafe fn new(canvas:HWND,input:HWND,factory:&IDXGIFactory2,pipe:&AdaptivePipeline)->AppResult<Self> {
        for hwnd in [canvas,input] {let mut pid=0;GetWindowThreadProcessId(hwnd,Some(&mut pid));ensure(pid==std::process::id(),"Refusing foreign overlay window")?;}
        let mut animated=1i32;
        let reduced=SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION,0,Some((&mut animated as *mut i32).cast()),SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)).is_ok() && animated==0;
        let inner=super::super::Presenter::new(canvas,factory,&pipe.device,pipe.canvas.width,pipe.canvas.height)?;
        eprintln!("[liquid-optics] material=LENS_ADAPTIVE_2; logical={}x{}; visual={}x{}; GPU-only settling; click-through canvas; reduced-motion={reduced}",pipe.layout.width,pipe.layout.height,pipe.canvas.width,pipe.canvas.height);
        Ok(Self{inner,input,reduced})
    }
    pub unsafe fn bind_input(&self,pipe:&AdaptivePipeline) {pipe.inner.hwnd.set(self.input);pipe.inner.reduced_motion.set(self.reduced);}
    pub unsafe fn present(&self,pipe:&AdaptivePipeline)->AppResult<()> {
        self.bind_input(pipe);self.inner.present_texture(&pipe.context,&pipe.canvas)
    }
}

#[path="adaptive_checks.rs"]
mod checks;
pub unsafe fn self_test(directory:Option<&Path>)->AppResult<()> {checks::verify(directory)}
