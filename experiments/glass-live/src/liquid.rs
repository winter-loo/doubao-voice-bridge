use super::*;
use std::{cell::{Cell, RefCell}, ops::Deref};
use windows::Win32::{Foundation::{BOOL, POINT, RECT}, UI::{Input::KeyboardAndMouse::GetCapture, WindowsAndMessaging::{GetCursorPos, GetWindowRect, GetWindowThreadProcessId, WindowFromPoint, SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS}}};
use super::super::motion::Motion;

#[repr(C)]
#[derive(Clone, Copy)]
struct OpticalConstants { contact: [f32;4], dynamics: [f32;4] }

/// Exact same GPU capture/presentation transport; a different optical renderer.
/// Deref shares the raw texture and device with the existing host, NOT its material.
pub struct Pipeline {
    base: super::Pipeline,
    interaction: ID3D11Buffer,
    halo: Texture,
    adaptation: [Texture;2],
    adapt_index: Cell<usize>,
    adapt_shader: ID3D11PixelShader,
    motion: RefCell<Motion>,
    hwnd: Cell<HWND>,
    last_position: Cell<Option<(i32,i32)>>,
    reduced_motion: Cell<bool>,
}
impl Deref for Pipeline { type Target=super::Pipeline; fn deref(&self)->&Self::Target { &self.base } }

/// Local glyph neighborhood only. No full-label/whole-capsule opaque backplate.
fn make_support(mask:&[u8],w:usize,h:usize)->Vec<u8> {
    let radius=(h as f32*0.040).ceil().max(1.0) as usize;
    let mut horizontal=vec![0u8;mask.len()]; let mut dilated=horizontal.clone();
    for y in 0..h { for x in 0..w {
        horizontal[y*w+x]=(x.saturating_sub(radius)..=(x+radius).min(w-1)).map(|xx|mask[y*w+xx]).max().unwrap_or(0);
    } }
    for y in 0..h { for x in 0..w {
        dilated[y*w+x]=(y.saturating_sub(radius)..=(y+radius).min(h-1)).map(|yy|horizontal[yy*w+x]).max().unwrap_or(0);
    } }
    let sigma=(h as f32*0.028).max(0.65); let support=(sigma*3.0).ceil() as i32;
    let weights:Vec<f32>=(-support..=support).map(|i|(-(i*i) as f32/(2.0*sigma*sigma)).exp()).collect();
    let sum:f32=weights.iter().sum(); let mut temp=vec![0f32;mask.len()]; let mut out=vec![0u8;mask.len()];
    for y in 0..h { for x in 0..w {
        temp[y*w+x]=(-support..=support).zip(&weights).map(|(i,k)|dilated[y*w+(x as i32+i).clamp(0,w as i32-1) as usize] as f32*k).sum::<f32>()/sum;
    } }
    for y in 0..h { for x in 0..w {
        let v=(-support..=support).zip(&weights).map(|(i,k)|temp[(y as i32+i).clamp(0,h as i32-1) as usize*w+x]*k).sum::<f32>()/sum;
        out[y*w+x]=((v.round().clamp(0.0,255.0)) as u8).max(mask[y*w+x]);
    } }
    out
}
impl Pipeline {
    pub unsafe fn new(device:ID3D11Device,context:ID3D11DeviceContext,w:u32,h:u32,mask:&[u8])->AppResult<Self> {
        let mut base=super::Pipeline::new(device,context,w,h,mask)?;
        let source=format!("{}\n{}",include_str!("glass.hlsl"),include_str!("liquid.hlsl"));
        let mut material=None; let b=compile_source(&source,s!("liquid_material_ps"),s!("ps_5_0"))?;
        base.device.CreatePixelShader(blob_bytes(&b),None,Some(&mut material))?;
        base.material=material.ok_or("No liquid optical shader")?;
        let mut adapt_shader=None; let b=compile_source(&source,s!("liquid_adapt_ps"),s!("ps_5_0"))?;
        base.device.CreatePixelShader(blob_bytes(&b),None,Some(&mut adapt_shader))?;
        let mut interaction=None;
        base.device.CreateBuffer(&D3D11_BUFFER_DESC{ByteWidth:size_of::<OpticalConstants>() as u32,Usage:D3D11_USAGE_DEFAULT,BindFlags:D3D11_BIND_CONSTANT_BUFFER.0 as u32,..Default::default()},None,Some(&mut interaction))?;
        let halo=Texture::new(&base.device,w,h,DXGI_FORMAT_R8_UNORM)?;
        let support=make_support(mask,w as usize,h as usize);
        base.context.UpdateSubresource(&halo.texture,0,None,support.as_ptr().cast(),w,0);
        let adaptation=[Texture::new(&base.device,1,1,DXGI_FORMAT_R16G16B16A16_FLOAT)?,Texture::new(&base.device,1,1,DXGI_FORMAT_R16G16B16A16_FLOAT)?];
        for texture in &adaptation { base.context.ClearRenderTargetView(&texture.target,&[0.0;4]); }
        Ok(Self{base,interaction:interaction.ok_or("No optical uniform buffer")?,halo,adaptation,adapt_index:Cell::new(0),
            adapt_shader:adapt_shader.ok_or("No GPU adaptation shader")?,motion:RefCell::new(Motion::default()),
            hwnd:Cell::new(HWND::default()),last_position:Cell::new(None),reduced_motion:Cell::new(false)})
    }
    pub unsafe fn prepare(&self) {
        // 4.485px bulk scattering / 0.702px transmitting rim for a 39px canvas.
        // Refraction plus support remains bounded by the existing 26px ROI margin.
        self.base.prepare_scales(0.115,0.018);
    }
    pub unsafe fn render(&self,dark:bool,time:f32,foreground:bool) {
        let mut point=[0.30,0.18]; let mut pressed=false; let mut drift=[0.0;2];
        let hwnd=self.hwnd.get();
        if !hwnd.0.is_null() {
            let mut cursor=POINT::default(); let mut r=RECT::default();
            if GetCursorPos(&mut cursor).is_ok() && GetWindowRect(hwnd,&mut r).is_ok() {
                let owned=WindowFromPoint(cursor)==hwnd;
                pressed=GetCapture()==hwnd;
                if owned || pressed { point=[(cursor.x-r.left) as f32/self.output.width as f32,(cursor.y-r.top) as f32/self.output.height as f32]; }
                if let Some((x,y))=self.last_position.replace(Some((r.left,r.top))) {
                    let dt=(time-self.motion.borrow().last_time).max(0.001);
                    drift=[(r.left-x) as f32/(self.output.height as f32*dt*12.0),(r.top-y) as f32/(self.output.height as f32*dt*12.0)];
                }
            }
        }
        self.render_input(dark,time,foreground,point,pressed,drift,self.reduced_motion.get());
    }
    /// Deterministic input entry point also used by the live renderer. A repeated
    /// timestamp does not tick springs/history, preserving exact same-frame pairs.
    unsafe fn render_input(&self,dark:bool,time:f32,foreground:bool,point:[f32;2],pressed:bool,drift:[f32;2],reduced:bool) {
        let mut state=self.motion.borrow_mut(); let dt=state.step(time,point,pressed,drift,reduced);
        let c=OpticalConstants{contact:[state.point[0],state.point[1],state.press,if reduced{0.0}else{1.0}],
            dynamics:[state.drift[0],state.drift[1],if reduced{1.0}else{(time/0.24).clamp(0.0,1.0)},dt]};
        drop(state);
        self.context.UpdateSubresource(&self.interaction,0,None,(&c as *const OpticalConstants).cast(),0,0);
        self.context.PSSetConstantBuffers(1,Some(&[Some(self.interaction.clone())]));
        let mut params=self.base.params(); params.style=[if dark{1.0}else{0.0},time,if foreground{1.0}else{0.0},1.0];
        if dt>0.0 {
            let old=self.adapt_index.get();let next=1-old;
            self.base.pass(&self.adaptation[next],&self.adapt_shader,
                &[Some(self.base.wide.view.clone()),Some(self.adaptation[old].view.clone()),None,Some(self.base.narrow.view.clone())],params);
            self.context.PSSetShaderResources(0,Some(&[None,None,None,None,None,None]));
            self.adapt_index.set(next);
        }
        self.base.pass(&self.output,&self.base.material,
            &[Some(self.base.wide.view.clone()),Some(self.base.narrow.view.clone()),Some(self.base.mask.view.clone()),Some(self.base.linear.view.clone()),Some(self.halo.view.clone()),Some(self.adaptation[self.adapt_index.get()].view.clone())],params);
        self.context.PSSetShaderResources(0,Some(&[None,None,None,None,None,None]));
        self.context.PSSetConstantBuffers(1,Some(&[None]));
    }
    pub unsafe fn snapshot(&self,dir:&Path,number:u32)->AppResult<()> { self.base.snapshot(dir,number) }
}

pub struct Presenter { inner:super::Presenter,hwnd:HWND,reduced:bool }
impl Presenter {
    pub unsafe fn new(hwnd:HWND,factory:&IDXGIFactory2,device:&ID3D11Device,w:u32,h:u32)->AppResult<Self> {
        let mut pid=0;GetWindowThreadProcessId(hwnd,Some(&mut pid));
        ensure(pid==std::process::id(),"Refusing to bind optical input to a foreign window")?;
        let mut animated=BOOL(1);
        let reduced=SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION,0,Some((&mut animated as *mut BOOL).cast()),SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)).is_ok() && !animated.as_bool();
        eprintln!("[liquid-optics] material=LENS_TRANSMISSION_1; refracted live scene; local glyph protection; adaptive ink; input springs; reduced-motion={reduced}; NOT V2.3 paint");
        Ok(Self{inner:super::Presenter::new(hwnd,factory,device,w,h)?,hwnd,reduced})
    }
    pub unsafe fn present(&self,pipe:&Pipeline)->AppResult<()> {
        pipe.hwnd.set(self.hwnd);pipe.reduced_motion.set(self.reduced);self.inner.present(&pipe.base)
    }
}

pub unsafe fn self_test(directory:Option<&Path>)->AppResult<()> {
    let (device,context)=create_device(None)?;
    let p=Pipeline::new(device,context,160,40,&vec![0;160*40])?;
    let (w,h)=(p.raw.width,p.raw.height);
    let mut pixels=vec![0u8;(w*h*4) as usize];
    for y in 0..h {for x in 0..w { let c=if x<w/2{[64,48,220,255]}else{[224,112,32,255]};pixels[((y*w+x)*4) as usize..][..4].copy_from_slice(&c);}}
    p.context.UpdateSubresource(&p.raw.texture,0,None,pixels.as_ptr().cast(),w*4,0);p.prepare();
    if let Some(d)=directory{std::fs::create_dir(d)?;}
    for dark in [false,true] {
        p.render(dark,2.0,false);let a=p.read_rgba(&p.output)?;
        let left=&a[(20*160+25)*4..][..4];let right=&a[(20*160+135)*4..][..4];
        ensure(left[0]>left[2]+40 && right[2]>right[0]+40,"Optical material erased the transmitted colors")?;
        ensure(left[3]==255 && a[3]==0,"Optical silhouette is invalid")?;
        ensure(a.chunks_exact(4).all(|v|v[..3].iter().all(|c|*c<=v[3])),"Invalid premultiplied optical output")?;
        if let Some(d)=directory{p.snapshot(d,if dark{2}else{1})?;}
    }
    for y in 0..h {for x in 0..w{let v=if x%2==0{0}else{255};pixels[((y*w+x)*4) as usize..][..4].copy_from_slice(&[v,v,v,255]);}}
    p.context.UpdateSubresource(&p.raw.texture,0,None,pixels.as_ptr().cast(),w*4,0);p.prepare();p.render(false,3.0,false);
    let a=p.read_rgba(&p.output)?;let change:u32=(40..119).map(|x|a[(20*160+x)*4].abs_diff(a[(20*160+x+1)*4]) as u32).sum();
    ensure(change<79*8,"One-pixel source aliases through optical material")?;
    if let Some(d)=directory{p.snapshot(d,3)?;}
    eprintln!("[liquid-optics-self-test] PASS: actual optical shader, GPU adaptation, strong color transmission, premultiplication, silhouette, fine-detail suppression; WARP; no capture/window or FPS claim");
    Ok(())
}

#[cfg(test)] mod tests { include!("liquid_tests.rs"); }
