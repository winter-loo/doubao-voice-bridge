use crate::{ensure, AppResult};
use std::{mem::size_of, path::Path, slice};
use windows::{core::{s, Interface, PCSTR}, Win32::{
    Foundation::{HMODULE, HWND},
    Graphics::{
        Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_DRIVER_TYPE_WARP, D3D_FEATURE_LEVEL_11_0,
            D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST, ID3DBlob, Fxc::D3DCompile},
        Direct3D11::*, DirectComposition::*, Dxgi::{Common::*, *},
    },
}};

// V2 keeps the same two separable passes and texture geometry. The stronger
// center filter fits inside the existing .65 * height source padding (3 sigma).
const CENTER_SIGMA_FRACTION: f32 = 0.20;
const EDGE_SIGMA_FRACTION: f32 = 0.045;

pub unsafe fn create_device(adapter: Option<&IDXGIAdapter1>) -> AppResult<(ID3D11Device, ID3D11DeviceContext)> {
    let mut device = None; let mut context = None;
    let base_adapter: Option<IDXGIAdapter> = adapter.map(|a| a.cast()).transpose()?;
    D3D11CreateDevice(base_adapter.as_ref(), if adapter.is_some() { D3D_DRIVER_TYPE_UNKNOWN } else { D3D_DRIVER_TYPE_WARP },
        HMODULE::default(), D3D11_CREATE_DEVICE_BGRA_SUPPORT, Some(&[D3D_FEATURE_LEVEL_11_0]), D3D11_SDK_VERSION,
        Some(&mut device), None, Some(&mut context))?;
    Ok((device.ok_or("D3D11 returned no device")?, context.ok_or("D3D11 returned no context")?))
}

pub struct Texture {
    pub texture: ID3D11Texture2D,
    pub view: ID3D11ShaderResourceView,
    target: ID3D11RenderTargetView,
    pub width: u32, pub height: u32,
}
impl Texture {
    unsafe fn new(device: &ID3D11Device, width: u32, height: u32, format: DXGI_FORMAT) -> AppResult<Self> {
        let desc = D3D11_TEXTURE2D_DESC { Width: width, Height: height, MipLevels: 1, ArraySize: 1,
            Format: format, SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT, BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
            ..Default::default() };
        let mut tex = None; device.CreateTexture2D(&desc, None, Some(&mut tex))?;
        let texture = tex.ok_or("No texture")?;
        let mut srv = None; device.CreateShaderResourceView(&texture, None, Some(&mut srv))?;
        let mut rtv = None; device.CreateRenderTargetView(&texture, None, Some(&mut rtv))?;
        Ok(Self { texture, view: srv.ok_or("No SRV")?, target: rtv.ok_or("No RTV")?, width, height })
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Constants { geometry: [f32; 4], filter: [f32; 4], style: [f32; 4] }

unsafe fn compile_source(source: &str, entry: PCSTR, target: PCSTR) -> AppResult<ID3DBlob> {
    let mut code = None; let mut errors: Option<ID3DBlob> = None;
    let result = D3DCompile(source.as_ptr().cast(), source.len(), s!("glass-live.hlsl"), None, None,
        entry, target, 1 << 15, 0, &mut code, Some(&mut errors));
    if result.is_err() {
        let detail = errors.map(|b| String::from_utf8_lossy(slice::from_raw_parts(b.GetBufferPointer().cast(), b.GetBufferSize())).into_owned()).unwrap_or_default();
        return Err(format!("HLSL compilation failed: {detail}").into());
    }
    Ok(code.ok_or("Compiler returned no bytecode")?)
}
unsafe fn compile(entry: PCSTR, target: PCSTR) -> AppResult<ID3DBlob> {
    compile_source(include_str!("glass.hlsl"), entry, target)
}
unsafe fn blob_bytes(b: &ID3DBlob) -> &[u8] { slice::from_raw_parts(b.GetBufferPointer().cast(), b.GetBufferSize()) }

pub struct Pipeline {
    pub device: ID3D11Device, pub context: ID3D11DeviceContext,
    pub raw: Texture, linear: Texture, temporary: Texture, wide: Texture, narrow: Texture,
    pub output: Texture, mask: Texture,
    vertex: ID3D11VertexShader, convert: ID3D11PixelShader, blur: ID3D11PixelShader, material: ID3D11PixelShader,
    constants: ID3D11Buffer, sampler: ID3D11SamplerState, rasterizer: ID3D11RasterizerState,
    pub padding: u32,
}
impl Pipeline {
    pub unsafe fn new(device: ID3D11Device, context: ID3D11DeviceContext, width: u32, height: u32, mask: &[u8]) -> AppResult<Self> {
        ensure(width >= height && height >= 20 && height <= 120 && width <= 1024, "Invalid pipeline geometry")?;
        ensure(mask.len() == width as usize * height as usize, "Invalid foreground mask")?;
        let padding = (height as f32 * 0.65).ceil() as u32;
        ensure(padding >= (3.0 * (height as f32 * CENTER_SIGMA_FRACTION).min(20.0)).ceil() as u32,
            "Center blur support exceeds captured padding")?;
        let rw = width + padding * 2; let rh = height + padding * 2;
        let mut vertex = None; let b = compile(s!("fullscreen_vs"), s!("vs_5_0"))?;
        device.CreateVertexShader(blob_bytes(&b), None, Some(&mut vertex))?;
        let make_ps = |entry| -> AppResult<ID3D11PixelShader> {
            let b = compile(entry, s!("ps_5_0"))?; let mut ps = None;
            device.CreatePixelShader(blob_bytes(&b), None, Some(&mut ps))?; Ok(ps.ok_or("No pixel shader")?)
        };
        let convert = make_ps(s!("convert_ps"))?; let blur = make_ps(s!("blur_ps"))?; let material = make_ps(s!("material_ps"))?;
        let mut constants = None;
        device.CreateBuffer(&D3D11_BUFFER_DESC { ByteWidth: size_of::<Constants>() as u32,
            Usage: D3D11_USAGE_DEFAULT, BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32, ..Default::default() }, None, Some(&mut constants))?;
        let mut sampler = None;
        device.CreateSamplerState(&D3D11_SAMPLER_DESC { Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
            AddressU: D3D11_TEXTURE_ADDRESS_CLAMP, AddressV: D3D11_TEXTURE_ADDRESS_CLAMP, AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
            MaxLOD: f32::MAX, ..Default::default() }, Some(&mut sampler))?;
        let mut rasterizer = None;
        device.CreateRasterizerState(&D3D11_RASTERIZER_DESC { FillMode: D3D11_FILL_SOLID,
            CullMode: D3D11_CULL_NONE, DepthClipEnable: true.into(), ..Default::default() }, Some(&mut rasterizer))?;
        let mask_tex = Texture::new(&device, width, height, DXGI_FORMAT_R8_UNORM)?;
        context.UpdateSubresource(&mask_tex.texture, 0, None, mask.as_ptr().cast(), width, 0);
        Ok(Self {
            raw: Texture::new(&device, rw, rh, DXGI_FORMAT_B8G8R8A8_UNORM)?,
            linear: Texture::new(&device, rw, rh, DXGI_FORMAT_R16G16B16A16_FLOAT)?,
            temporary: Texture::new(&device, rw, rh, DXGI_FORMAT_R16G16B16A16_FLOAT)?,
            wide: Texture::new(&device, rw, rh, DXGI_FORMAT_R16G16B16A16_FLOAT)?,
            narrow: Texture::new(&device, rw, rh, DXGI_FORMAT_R16G16B16A16_FLOAT)?,
            output: Texture::new(&device, width, height, DXGI_FORMAT_B8G8R8A8_UNORM)?, mask: mask_tex,
            vertex: vertex.ok_or("No vertex shader")?, convert, blur, material,
            constants: constants.ok_or("No constants buffer")?, sampler: sampler.ok_or("No sampler")?,
            rasterizer: rasterizer.ok_or("No rasterizer")?, padding, device, context,
        })
    }
    unsafe fn pass(&self, target: &Texture, shader: &ID3D11PixelShader, views: &[Option<ID3D11ShaderResourceView>], c: Constants) {
        self.context.PSSetShaderResources(0, Some(&[None, None, None]));
        self.context.OMSetRenderTargets(Some(&[Some(target.target.clone())]), None);
        self.context.RSSetViewports(Some(&[D3D11_VIEWPORT { Width: target.width as f32, Height: target.height as f32,
            MinDepth: 0., MaxDepth: 1., ..Default::default() }]));
        self.context.RSSetState(&self.rasterizer);
        self.context.UpdateSubresource(&self.constants, 0, None, (&c as *const Constants).cast(), 0, 0);
        self.context.PSSetConstantBuffers(0, Some(&[Some(self.constants.clone())]));
        self.context.PSSetSamplers(0, Some(&[Some(self.sampler.clone())]));
        self.context.PSSetShaderResources(0, Some(views));
        self.context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        self.context.VSSetShader(&self.vertex, None); self.context.PSSetShader(shader, None);
        self.context.Draw(3, 0);
        self.context.PSSetShaderResources(0, Some(&[None, None, None]));
        self.context.OMSetRenderTargets(None, None);
    }
    fn params(&self) -> Constants {
        Constants { geometry: [self.raw.width as f32, self.raw.height as f32, self.output.width as f32, self.output.height as f32],
            filter: [self.padding as f32, 0., 0., 0.], style: [0.; 4] }
    }
    pub unsafe fn prepare(&self) {
        self.prepare_scales(CENTER_SIGMA_FRACTION, EDGE_SIGMA_FRACTION);
    }
    unsafe fn prepare_scales(&self, center_fraction: f32, edge_fraction: f32) {
        let mut c = self.params();
        self.pass(&self.linear, &self.convert, &[Some(self.raw.view.clone())], c);
        for (sigma, target) in [(self.output.height as f32 * center_fraction, &self.wide), (self.output.height as f32 * edge_fraction, &self.narrow)] {
            c.filter = [self.padding as f32, sigma.min(20.), 1., 0.];
            self.pass(&self.temporary, &self.blur, &[Some(self.linear.view.clone())], c);
            c.filter[2] = 0.; c.filter[3] = 1.;
            self.pass(target, &self.blur, &[Some(self.temporary.view.clone())], c);
        }
    }
    pub unsafe fn render(&self, dark: bool, time: f32, foreground: bool) {
        let mut c = self.params(); c.style = [if dark { 1. } else { 0. }, time, if foreground { 1. } else { 0. }, 1.];
        self.pass(&self.output, &self.material,
            &[Some(self.wide.view.clone()), Some(self.narrow.view.clone()), Some(self.mask.view.clone())], c);
    }
    /// Occasional user-triggered readback only. Never called by ordinary rendering.
    pub unsafe fn read_rgba(&self, texture: &Texture) -> AppResult<Vec<u8>> {
        let mut desc = D3D11_TEXTURE2D_DESC::default(); texture.texture.GetDesc(&mut desc);
        ensure(desc.Format == DXGI_FORMAT_B8G8R8A8_UNORM, "Readback expects BGRA8")?;
        desc.Usage = D3D11_USAGE_STAGING; desc.BindFlags = 0; desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32; desc.MiscFlags = 0;
        let mut staging = None; self.device.CreateTexture2D(&desc, None, Some(&mut staging))?;
        let staging = staging.ok_or("No readback texture")?;
        self.context.CopyResource(&staging, &texture.texture);
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        self.context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
        let mut rgba = vec![0; desc.Width as usize * desc.Height as usize * 4];
        for y in 0..desc.Height as usize {
            let row = slice::from_raw_parts((mapped.pData as *const u8).add(y * mapped.RowPitch as usize), desc.Width as usize * 4);
            for x in 0..desc.Width as usize {
                let out = &mut rgba[(y * desc.Width as usize + x) * 4..][..4]; let p = &row[x * 4..][..4];
                out.copy_from_slice(&[p[2], p[1], p[0], p[3]]);
            }
        }
        self.context.Unmap(&staging, 0); Ok(rgba)
    }
    pub unsafe fn snapshot(&self, directory: &Path, number: u32) -> AppResult<()> {
        let mut background = self.read_rgba(&self.raw)?;
        for p in background.chunks_exact_mut(4) { p[3] = 255; }
        let layer = self.read_rgba(&self.output)?;
        // Recompose exactly the same captured crop + premultiplied rendered layer.
        // This is NOT a screenshot of DWM's final desktop presentation.
        for y in 0..self.output.height as usize { for x in 0..self.output.width as usize {
            let i = (y * self.output.width as usize + x) * 4;
            let j = ((y + self.padding as usize) * self.raw.width as usize + x + self.padding as usize) * 4;
            for k in 0..3 { background[j+k] = (layer[i+k] as u32 + (background[j+k] as u32 * (255-layer[i+3]) as u32 + 127)/255).min(255) as u8; }
        } }
        let path = directory.join(format!("snapshot-{number:04}.png"));
        crate::png::write(&path, self.raw.width, self.raw.height, &background)?;
        eprintln!("[glass-live] saved local same-frame GPU composite: {}", path.display());
        Ok(())
    }
}

pub struct Presenter {
    chain: IDXGISwapChain1,
    composition: IDCompositionDevice,
    target: IDCompositionTarget,
    _visual: IDCompositionVisual,
}
impl Presenter {
    pub unsafe fn new(hwnd: HWND, factory: &IDXGIFactory2, device: &ID3D11Device, width: u32, height: u32) -> AppResult<Self> {
        let desc = DXGI_SWAP_CHAIN_DESC1 { Width: width, Height: height, Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 }, BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2, Scaling: DXGI_SCALING_STRETCH, SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED, ..Default::default() };
        let chain = factory.CreateSwapChainForComposition(device, &desc, None)?;
        let dxgi: IDXGIDevice = device.cast()?;
        let composition: IDCompositionDevice = DCompositionCreateDevice(&dxgi)?;
        let target = composition.CreateTargetForHwnd(hwnd, true)?;
        let visual = composition.CreateVisual()?; visual.SetContent(&chain)?; target.SetRoot(&visual)?; composition.Commit()?;
        Ok(Self { chain, composition, target, _visual: visual })
    }
    pub unsafe fn present(&self, pipe: &Pipeline) -> AppResult<()> {
        let back: ID3D11Texture2D = self.chain.GetBuffer(0)?;
        pipe.context.CopyResource(&back, &pipe.output.texture);
        self.chain.Present(1, DXGI_PRESENT(0)).ok()?; Ok(())
    }
}
impl Drop for Presenter {
    fn drop(&mut self) { unsafe { let _ = self.target.SetRoot(None); let _ = self.composition.Commit(); } }
}

pub unsafe fn self_test(directory: Option<&Path>) -> AppResult<()> {
    let (device, context) = create_device(None)?; // WARP; no HWND or desktop duplication.
    let pipe = Pipeline::new(device, context, 160, 40, &vec![0; 160*40])?;
    let (w,h) = (pipe.raw.width, pipe.raw.height);
    let mut fixture = vec![0u8; (w*h*4) as usize];
    for y in 0..h { for x in 0..w {
        let c = if x < w/2 { [64,48,220,255] } else { [224,112,32,255] };
        fixture[((y*w+x)*4) as usize..][..4].copy_from_slice(&c);
    } }
    pipe.context.UpdateSubresource(&pipe.raw.texture,0,None,fixture.as_ptr().cast(),w*4,0);
    pipe.prepare();
    if let Some(dir) = directory { std::fs::create_dir(dir)?; }
    for dark in [false,true] {
        pipe.render(dark,0.,false); let rgba = pipe.read_rgba(&pipe.output)?;
        ensure(rgba[3]==0 && rgba[((20*160+80)*4+3) as usize]==255, "GPU alpha silhouette regression")?;
        let a=&rgba[(20*160+24)*4..]; let b=&rgba[(20*160+136)*4..];
        ensure(a[0]>a[2]+5 && b[2]>b[0]+5, "GPU background color separation regression")?;
        if let Some(dir)=directory { pipe.snapshot(dir, if dark {2} else {1})?; }
    }
    for y in 0..h { for x in 0..w {
        let v=if x%2==0 {0} else {255}; fixture[((y*w+x)*4) as usize..][..4].copy_from_slice(&[v,v,v,255]);
    } }
    pipe.context.UpdateSubresource(&pipe.raw.texture,0,None,fixture.as_ptr().cast(),w*4,0); pipe.prepare(); pipe.render(false,0.,false);
    let rgba=pipe.read_rgba(&pipe.output)?; let mut variation=0u32;
    for x in 40..119 { variation+=rgba[(20*160+x)*4].abs_diff(rgba[(20*160+x+1)*4]) as u32; }
    ensure(variation < 79*3, "GPU failed to suppress one-pixel stripes")?;
    if let Some(dir)=directory { pipe.snapshot(dir,3)?; }
    fixture.fill(255); pipe.context.UpdateSubresource(&pipe.raw.texture,0,None,fixture.as_ptr().cast(),w*4,0);
    pipe.prepare(); pipe.render(true,0.,false); let rgba=pipe.read_rgba(&pipe.output)?;
    ensure(rgba[(20*160+80)*4..][..3].iter().all(|&v|v<110), "Dark text-protection tone is too bright on white")?;
    eprintln!("[glass-live-self-test] PASS: four HLSL entry points compiled; GPU color separation, silhouette alpha, stripe attenuation, dark tone and readback tested on WARP. No desktop capture/window; not hardware performance or presentation acceptance.");
    Ok(())
}

#[cfg(test)]
#[path = "material_tests.rs"]
mod material_tests;
