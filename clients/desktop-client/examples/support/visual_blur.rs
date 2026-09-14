//! Native COMPOSITION-effect candidate for the minimal example only.
//! Unlike HostBackdrop, this samples immediately behind its visual and applies
//! the system Gaussian effect explicitly. No desktop capture, CPU pixel buffers,
//! shader implementation, global policy changes, or recovery timer are involved.
//! Cross-window sampling through this transparent HWND MUST be verified locally;
//! a successful effect factory is not evidence that its source contains the desktop.
//!
//! API references:
//! https://learn.microsoft.com/uwp/api/windows.ui.composition.compositor.createbackdropbrush
//! https://learn.microsoft.com/windows/win32/direct2d/gaussian-blur
//! https://learn.microsoft.com/windows/win32/api/windows.graphics.effects.interop/nn-windows-graphics-effects-interop-igraphicseffectd2d1interop

use std::ffi::c_void;
use windows::{
    core::{implement, Error, GUID, HSTRING, Interface, PCWSTR, Result},
    Foundation::{IPropertyValue, PropertyValue},
    Graphics::Effects::{IGraphicsEffect, IGraphicsEffect_Impl, IGraphicsEffectSource, IGraphicsEffectSource_Impl},
    UI::Composition::{Compositor, CompositionEffectSourceParameter, Desktop::DesktopWindowTarget},
    Win32::{
        Foundation::{E_BOUNDS, E_INVALIDARG, E_NOTIMPL, HWND, RECT},
        System::WinRT::{
            Composition::ICompositorDesktopInterop,
            Graphics::Direct2D::{IGraphicsEffectD2D1Interop, IGraphicsEffectD2D1Interop_Impl, GRAPHICS_EFFECT_PROPERTY_MAPPING},
        },
        UI::WindowsAndMessaging::GetClientRect,
    },
};

// Describe a built-in D2D effect to the Windows compositor, not a custom GPU shader.
// Property indices: standard deviation=0, optimization=1, border mode=2.
#[implement(IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectD2D1Interop)]
struct Gaussian {
    source: IGraphicsEffectSource,
    sigma: f32,
}
#[allow(non_snake_case)]
impl IGraphicsEffect_Impl for Gaussian_Impl {
    fn Name(&self) -> Result<HSTRING> { Ok(HSTRING::from("NativeBlur")) }
    fn SetName(&self, _value: &HSTRING) -> Result<()> { Err(Error::from_hresult(E_NOTIMPL)) }
}
impl IGraphicsEffectSource_Impl for Gaussian_Impl {}
#[allow(non_snake_case)]
impl IGraphicsEffectD2D1Interop_Impl for Gaussian_Impl {
    fn GetEffectId(&self) -> Result<GUID> {
        // CLSID_D2D1GaussianBlur from d2d1effects.h.
        Ok(GUID::from_u128(0x1feb6d69_2fe6_4ac9_8c58_1d7f93e7a6a5))
    }
    fn GetNamedPropertyMapping(&self, _name: &PCWSTR, _index: *mut u32,
        _mapping: *mut GRAPHICS_EFFECT_PROPERTY_MAPPING) -> Result<()> {
        // No dynamically named/animated properties are exposed by this fixed graph.
        Err(Error::from_hresult(E_INVALIDARG))
    }
    fn GetPropertyCount(&self) -> Result<u32> { Ok(3) }
    fn GetProperty(&self, index: u32) -> Result<IPropertyValue> {
        match index {
            0 => PropertyValue::CreateSingle(self.sigma)?.cast(),
            1 => PropertyValue::CreateUInt32(1)?.cast(), // balanced
            2 => PropertyValue::CreateUInt32(1)?.cast(), // hard border
            _ => Err(Error::from_hresult(E_BOUNDS)),
        }
    }
    fn GetSourceCount(&self) -> Result<u32> { Ok(1) }
    fn GetSource(&self, index: u32) -> Result<IGraphicsEffectSource> {
        if index == 0 { Ok(self.source.clone()) } else { Err(Error::from_hresult(E_BOUNDS)) }
    }
}

pub struct VisualBlur {
    target: DesktopWindowTarget,
    compositor: Compositor,
}
impl VisualBlur {
    /// The minimal host must already own a pumping dispatcher and COM apartment.
    /// Geometry and lower-target placement match the HostBackdrop control arm.
    pub fn new(identity: isize, width: u32, height: u32) -> std::result::Result<Self, String> {
        Self::create(identity, width, height).map_err(|e| format!("standard backdrop Gaussian graph: {e}"))
    }
    fn create(identity: isize, width: u32, height: u32) -> Result<Self> {
        let hwnd = HWND(identity as *mut c_void);
        let mut rect = RECT::default();
        unsafe { GetClientRect(hwnd, &mut rect)?; }
        let (cw, ch) = (rect.right - rect.left, rect.bottom - rect.top);
        if width == 0 || height == 0 || width > 4096 || height > 4096
            || width as i32 > cw || height as i32 > ch {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let compositor = Compositor::new()?;
        let interop: ICompositorDesktopInterop = compositor.cast()?;
        let target = unsafe { interop.CreateDesktopWindowTarget(hwnd, false)? };
        let owner = Self { target, compositor };
        let visual = owner.compositor.CreateSpriteVisual()?;
        let geometry = owner.compositor.CreateRoundedRectangleGeometry()?;
        let mut size = visual.Size()?;
        size.X = width as f32; size.Y = height as f32;
        visual.SetSize(size)?; geometry.SetSize(size)?;
        let mut radius = size;
        radius.X = width.min(height) as f32 / 2.0; radius.Y = radius.X;
        geometry.SetCornerRadius(radius)?;
        let clip = owner.compositor.CreateGeometricClipWithGeometry(&geometry)?;
        visual.SetClip(&clip)?;
        let mut offset = visual.Offset()?;
        offset.X = ((cw - width as i32) / 2) as f32;
        offset.Y = ((ch - height as i32) / 2) as f32; offset.Z = 0.0;
        visual.SetOffset(offset)?;
        let source_name = HSTRING::from("source");
        let source = CompositionEffectSourceParameter::Create(&source_name)?;
        let description: IGraphicsEffect = Gaussian { source: source.cast()?, sigma: 12.0 }.into();
        let factory = owner.compositor.CreateEffectFactory(&description)?;
        let effect = factory.CreateBrush()?;
        let backdrop = owner.compositor.CreateBackdropBrush()?;
        effect.SetSourceParameter(&source_name, &backdrop)?;
        visual.SetBrush(&effect)?;
        owner.target.SetRoot(&visual)?;
        eprintln!("[glass-visual-blur] attached; source=standard-backdrop; sigma=12-device-pixels; clip=capsule; target=lower; host-opt-in=not-enabled; no-capture");
        Ok(owner)
    }
}
impl Drop for VisualBlur {
    fn drop(&mut self) {
        let _ = self.target.Close();
        let _ = self.compositor.Close();
    }
}
