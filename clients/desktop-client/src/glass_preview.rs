//! A separate visual-test binary. It imports no voice, hotkey, transcript, tray or
//! clipboard modules, does not acquire the production single-instance mutex, and
//! exits automatically. It can coexist with the idle production client.

use std::time::{Duration, Instant};
use gpui::{App, Application, Bounds, Context, Corners, FontWeight, IntoElement, Render, Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, canvas, div, fill, point, prelude::*, px, rgb, size};
mod liquid_glass;
mod glass_bake;
mod glass_texture;
const GLASS_MIX_STEPS: u32 = 12;
const WIDTH: f32 = 108.0;
const HEIGHT: f32 = 26.0;

fn option(name: &str) -> Option<String> {
    let prefix = format!("--{name}=");
    std::env::args().find_map(|arg| arg.strip_prefix(&prefix).map(str::to_owned))
}

struct Preview { start: Instant, lifetime: Duration, dark: bool, content: String }
impl Render for Preview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.start.elapsed() >= self.lifetime { cx.quit(); }
        window.request_animation_frame();
        let elapsed = self.start.elapsed().as_secs_f32();
        let dark = self.dark;
        let content = if self.content == "cycle" {
            match (elapsed as u32 / 4) % 3 { 0 => "wave", 1 => "activating", _ => "optimizing" }
        } else { self.content.as_str() };
        let wave = content == "wave";
        let label = match content { "activating" => "激活中", "optimizing" => "优化识别中", _ => "" };
        let layer = canvas(|_, _, _| {}, move |bounds, _, window, _| {
            let scale = window.scale_factor();
            let w = (f32::from(bounds.size.width) * scale).round().max(1.0) as u32;
            let h = (f32::from(bounds.size.height) * scale).round().max(1.0) as u32;
            let radius = bounds.size.height / 2.0;
            let phase = ((elapsed / 1.12).fract() * liquid_glass::SHEEN_FRAMES as f32) as usize;
            let level = if dark { GLASS_MIX_STEPS } else { 0 };
            let painted = glass_texture::glass_texture(window, w, h, liquid_glass::Tint::NEUTRAL_KEY, level)
                .is_some_and(|image| window.paint_image(bounds, Corners::all(radius), image, phase, false).is_ok());
            if !painted { window.paint_quad(fill(bounds, rgb(if dark { 0x161c24 } else { 0xf7f9fc })).corner_radii(radius)); }
            if wave {
                let start = bounds.origin.x + (bounds.size.width - px(78.0)) / 2.0;
                for i in 0..20 {
                    let h = 3.0 + 9.0 * (elapsed * 5.0 + i as f32 * 0.61).sin().abs();
                    let t = i as f32 / 19.0;
                    let channel = |a: f32, b: f32| (a + (b-a)*t).round() as u32;
                    let color = (channel(67.0,100.0)<<16) | (channel(222.0,141.0)<<8) | channel(210.0,255.0);
                    let bar = Bounds::new(point(start+px(i as f32*4.0), bounds.origin.y+(bounds.size.height-px(h))/2.0), size(px(2.0),px(h)));
                    window.paint_quad(fill(bar, rgb(color)).corner_radii(px(1.0)));
                }
            }
        }).w(px(WIDTH)).h(px(HEIGHT)).absolute().top_0().left_0();
        div().size_full().flex().items_center().justify_center().child(
            div().id("glass-preview").relative().w(px(WIDTH)).h(px(HEIGHT)).rounded_full().overflow_hidden()
                .flex().items_center().justify_center().text_xs().font_weight(FontWeight::MEDIUM)
                .text_color(rgb(if dark { 0xffffff } else { 0x07131f }))
                .cursor_pointer().on_click(|_, _, cx| cx.quit()).child(layer)
                .child(div().relative().child(label))
        )
    }
}

#[cfg(target_os = "windows")]
fn configure(window: &Window) -> Result<(), String> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::{Foundation::{HWND, RECT}, Graphics::Gdi::{CreateRoundRectRgn, DeleteObject, SetWindowRgn}, UI::WindowsAndMessaging::{GetWindowRect, GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, HWND_TOPMOST, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW}};
    let handle = HasWindowHandle::window_handle(window).map_err(|e| e.to_string())?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else { return Err("expected a Win32 preview window".into()); };
    let hwnd = HWND(handle.hwnd.get() as *mut std::ffi::c_void);
    unsafe {
        let styles = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, styles | WS_EX_NOACTIVATE.0 as isize | WS_EX_TOOLWINDOW.0 as isize);
        SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED).map_err(|e| e.to_string())?;
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect).map_err(|e| e.to_string())?;
        let scale_x = (rect.right-rect.left) as f32 / 128.0;
        let scale_y = (rect.bottom-rect.top) as f32 / 42.0;
        let width = (WIDTH*scale_x).round() as i32;
        let height = (HEIGHT*scale_y).round() as i32;
        let left = (rect.right-rect.left-width)/2;
        let top = (rect.bottom-rect.top-height)/2;
        let region = CreateRoundRectRgn(left, top, left+width+1, top+height+1, height, height);
        if region.is_invalid() { return Err("could not create capsule region".into()); }
        if SetWindowRgn(hwnd, Some(region), true) == 0 {
            let _ = DeleteObject(region.into());
            return Err("could not clip preview to capsule".into());
        }
        eprintln!("[glass-preview] pid={} hwnd=0x{:X}; no microphone; click to close", std::process::id(), handle.hwnd.get());
    }
    Ok(())
}

fn main() {
    let theme = option("theme").unwrap_or_else(|| "light".into());
    let content = option("content").unwrap_or_else(|| "cycle".into());
    if !matches!(theme.as_str(), "light" | "dark") || !matches!(content.as_str(), "wave" | "activating" | "optimizing" | "cycle") {
        eprintln!("usage: GlassPreview --glass-backend=native|solid|transparent --theme=light|dark --content=wave|activating|optimizing|cycle --seconds=60");
        std::process::exit(2);
    }
    let lifetime = match option("seconds") {
        Some(value) => match value.parse::<u64>() { Ok(n) if (5..=600).contains(&n) => n, _ => { eprintln!("--seconds must be 5..600"); std::process::exit(2); } },
        None => 60,
    };
    Application::new().run(move |cx: &mut App| {
        let dimensions = size(px(128.0), px(42.0));
        let bounds = cx.primary_display().map(|display| {
            let screen = display.bounds();
            Bounds::new(point(screen.origin.x+(screen.size.width-dimensions.width)/2.0, screen.origin.y+screen.size.height-dimensions.height-px(60.0)), dimensions)
        }).unwrap_or_else(|| Bounds::centered(None, dimensions, cx));
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)), titlebar: None, focus: false,
            kind: WindowKind::PopUp, is_movable: false, is_resizable: false, is_minimizable: false,
            window_background: WindowBackgroundAppearance::Transparent, ..Default::default()
        };
        cx.open_window(options, |window, cx| {
            window.set_window_title("Doubao Glass Preview - no microphone");
            #[cfg(target_os = "windows")]
            configure(window).expect("configure preview window");
            cx.new(|_| Preview { start: Instant::now(), lifetime: Duration::from_secs(lifetime), dark: theme == "dark", content })
        }).expect("open glass preview");
    });
}
