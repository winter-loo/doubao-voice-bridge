"""One-time source integration. CI commits ordinary Rust files, not generated runtime code.

Only exact reviewed sources are transformed. No user settings, local runtime data,
network credentials or production processes are read or modified by this script.
"""
from pathlib import Path
import hashlib
import json

ROOT = Path(__file__).resolve().parents[2]

def blob(data):
    return hashlib.sha1(f"blob {len(data)}\0".encode()+data).hexdigest()

def replace_once(s, old, new):
    if s.count(old) != 1:
        raise RuntimeError(f"Expected one integration site, found {s.count(old)}: {old[:90]!r}")
    return s.replace(old, new, 1)

def save(path, content):
    with path.open('w',encoding='utf-8',newline='\n') as f: f.write(content)

def shared_renderer():
    base=ROOT/'experiments/glass-live/src'
    old=(base/'main.rs').read_text(encoding='utf-8')
    if hashlib.sha256(old.encode()).hexdigest()!='c61e3dfe51c7ac8ae601aac2bac1d55b69aa1fe70e4bae03794388deae07ddb9':
        raise RuntimeError('Preview entry differs from reviewed baseline')
    old=replace_once(old,'//! Windows optical liquid glass. No microphone, network or global hotkeys.','//! Shared optical renderer and in-process voice overlay. No microphone, network or global hotkeys.')
    old=replace_once(old,'mod review;','mod review;\npub mod voice_model;\n#[cfg(windows)]\npub mod voice_overlay;\n#[cfg(windows)]\nmod voice_window;')
    old=replace_once(old,'fn run() -> AppResult<()> {','pub fn run_preview() -> AppResult<()> {')
    old=replace_once(old,'fn main() {\n    if let Err(e) = run() {\n        eprintln!("[glass-live] ERROR: {e}");\n        std::process::exit(1);\n    }\n}\n','')
    old+='\n#[cfg(windows)]\nmod voice_render_checks;\n#[cfg(windows)]\npub fn voice_self_test() -> AppResult<()> { unsafe { voice_render_checks::verify(None) } }\n'
    if (base/'lib.rs').exists() or (base/'liquid_reference.hlsl').exists():
        raise RuntimeError('Unexpected preexisting library/reference; no overwrite')
    reference=(base/'liquid.hlsl').read_text(encoding='utf-8')
    if blob(reference.encode())!='4e72b8fbb6b8db4ac6f8d426a26d2d368fb27692':
        raise RuntimeError('Approved optical reference differs')
    edits=json.loads((ROOT/'tools/migrations/voice-shared-edits.json').read_text(encoding='utf-8'))
    results={}
    for name,data in edits.items():
        path=ROOT/name
        text=path.read_text(encoding='utf-8')
        if blob(text.encode())!=data['sha']: raise RuntimeError(f'Reviewed source changed: {name}')
        lines=text.splitlines(keepends=True)
        for start,end,replacement in reversed(data['edits']): lines[start:end]=[replacement]
        results[path]=''.join(lines)
    save(base/'lib.rs',old)
    save(base/'liquid_reference.hlsl',reference)
    save(base/'main.rs','//! Standalone review; production uses this same library in-process.\nfn main() {\n    if let Err(error) = doubao_glass_live::run_preview() {\n        eprintln!("[glass-live] ERROR: {error}");\n        std::process::exit(1);\n    }\n}\n')
    for path,text in results.items(): save(path,text)

def main():
    p=ROOT/'clients/desktop-client/src/main.rs'
    s=p.read_text(encoding='utf-8')
    if 'mod voice_glass;' in s:
        if not (ROOT/'experiments/glass-live/src/lib.rs').exists() or not (ROOT/'experiments/glass-live/src/liquid_reference.hlsl').exists():
            raise RuntimeError('Partial integration; refusing to skip')
        print('Voice integration already applied; no source rewritten.')
        return
    if blob(s.encode()) != 'e4e4a82bb3bf21306a8699adb71f71b899f0ca3b':
        raise RuntimeError('main.rs differs from reviewed base; refusing a fuzzy rewrite')
    shared_renderer()
    s=replace_once(s,'mod windows_shell;','mod windows_shell;\n#[cfg(target_os = "windows")]\nmod voice_glass;')
    s=replace_once(s,'    OVERLAY_PHASE.store(phase as u8, Ordering::Release);','    OVERLAY_PHASE.store(phase as u8, Ordering::Release);\n    #[cfg(target_os = "windows")]\n    voice_glass::phase(phase, overlay_generation());')
    s=replace_once(s,'fn main() {','fn main() {\n    #[cfg(target_os = "windows")]\n    if voice_glass::self_test_requested() { return; }')
    s=replace_once(s,'            let radius = bounds.size.height / 2.0;','''            let radius = bounds.size.height / 2.0;
            #[cfg(target_os = "windows")]
            if voice_glass::enabled() {
                // The optical surface is presented on its dedicated GPU thread.
                // This GPUI HWND is only the explicitly named SOLID recovery path.
                if !voice_glass::presenting() {
                    window.paint_quad(fill(bounds, rgba(lerp_rgba(0xf6f7f9ff, 0x202730ff, glass_mix()))).corner_radii(radius));
                }
                if !show_waveform { return; }
            }''')
    s=replace_once(s,'            let scale = window.scale_factor();','''            #[cfg(target_os = "windows")]
            let use_old_material = !voice_glass::enabled();
            #[cfg(not(target_os = "windows"))]
            let use_old_material = true;
            if use_old_material {
            let scale = window.scale_factor();''')
    s=replace_once(s,'            if !show_waveform {\n                return;\n            }','''            }
            if !show_waveform {
                return;
            }''')
    s=replace_once(s,'        hide_overlay(hwnd);\n        start_hotkey_thread','        hide_overlay(hwnd);\n        super::voice_glass::initialize(hwnd.0 as isize);\n        start_hotkey_thread')
    s=replace_once(s,'        set_overlay_phase(phase);\n        unsafe {','        set_overlay_phase(phase);\n        if super::voice_glass::owns_visibility() { return; }\n        unsafe {')
    start=s.index('        let reading = sample_background(hwnd, None);')
    end=s.index('        show_phase(hwnd, OverlayPhase::Activating);',start)
    s=s[:start]+'        if !super::voice_glass::enabled() {\n'+s[start:end]+'        }\n'+s[end:]
    s=replace_once(s,'            append_voice_client_log(&format!("[gpui] failed to start voice client: {error}\\n"));','            super::voice_glass::failed(generation);\n            append_voice_client_log(&format!("[gpui] failed to start voice client: {error}\\n"));')
    s=replace_once(s,'                    append_voice_client_log(&format!("[native-client] {error}\\n"));','                    super::voice_glass::failed(generation);\n                    append_voice_client_log(&format!("[native-client] {error}\\n"));')
    s=replace_once(s,'                    hide_overlay(HWND(hwnd_value as *mut c_void));\n                }\n            }\n        })','                    super::voice_glass::finished(generation);\n                    hide_overlay(HWND(hwnd_value as *mut c_void));\n                }\n            }\n        })')
    s=replace_once(s,'    fn finish_input_hwnd(hwnd: HWND) {','    pub(crate) fn finish_input_hwnd(hwnd: HWND) {')
    s=replace_once(s,'                if unsafe { IsWindowVisible(hwnd) }.as_bool() {','                if !super::voice_glass::enabled() && unsafe { IsWindowVisible(hwnd) }.as_bool() {')
    s=replace_once(s,'                if !unsafe { IsWindowVisible(hwnd) }.as_bool() {','                if super::voice_glass::enabled() || !unsafe { IsWindowVisible(hwnd) }.as_bool() {')
    s=replace_once(s,'                    if IsWindowVisible(hwnd).as_bool() {\n                        finish_input_hwnd(hwnd);','                    if overlay_phase() != OverlayPhase::Hidden {\n                        finish_input_hwnd(hwnd);')
    save(p,s)
    p=ROOT/'clients/desktop-client/Cargo.toml';s=p.read_text(encoding='utf-8')
    anchor='[target.\'cfg(target_os = "windows")\'.dependencies]\n'
    s=replace_once(s,anchor,anchor+'doubao-glass-live = { path = "../../experiments/glass-live" }\n')
    save(p,s)
    p=ROOT/'clients/desktop-client/src/client_settings.rs';s=p.read_text(encoding='utf-8')
    s=replace_once(s,'    pub setup_completed: bool,','    pub setup_completed: bool,\n    /// Explicit opt-in: local GPU desktop capture while the voice glass is visible.\n    pub liquid_glass: bool,')
    s=replace_once(s,'            setup_completed: false,','            setup_completed: false,\n            liquid_glass: false,')
    s=replace_once(s,'        assert!(!settings.setup_completed);','        assert!(!settings.setup_completed);\n        assert!(!settings.liquid_glass);')
    save(p,s)
    p=ROOT/'clients/desktop-client/src/windows_shell.rs';s=p.read_text(encoding='utf-8')
    s=replace_once(s,'const ID_SAVE: usize = 1004;','const ID_SAVE: usize = 1004;\nconst ID_LIQUID_GLASS: usize = 1005;')
    s=replace_once(s,'    startup_checkbox: isize,','    startup_checkbox: isize,\n    liquid_checkbox: isize,')
    s=replace_once(s,'    startup_checkbox: HWND,','    startup_checkbox: HWND,\n    liquid_checkbox: HWND,')
    s=replace_once(s,'                startup_checkbox: controls.startup_checkbox.0 as isize,','                startup_checkbox: controls.startup_checkbox.0 as isize,\n                liquid_checkbox: controls.liquid_checkbox.0 as isize,')
    s=replace_once(s,'            520,\n            390,','            520,\n            465,')
    s=replace_once(s,'        let status_label = create_control(','        let liquid_checkbox = create_control(\n            w!("BUTTON"), w!("Liquid Glass (local desktop sampling)"),\n            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),\n            28, 262, 448, 28, parent, ID_LIQUID_GLASS, instance,\n        )?;\n        let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(\n            liquid_checkbox, BM_SETCHECK, Some(WPARAM(usize::from(crate::voice_glass::enabled()))), None,\n        );\n        create_control(w!("STATIC"),w!("Glass is excluded from system screenshots/recording."),\n            WS_CHILD | WS_VISIBLE,28,292,448,24,parent,0,instance)?;\n        let status_label = create_control(')
    s=replace_once(s,'            272,','            347,')
    s=replace_once(s,'            328,\n            262,','            328,\n            337,')
    s=replace_once(s,'            startup_checkbox,\n            status_label,','            startup_checkbox,\n            liquid_checkbox,\n            status_label,')
    s=replace_once(s,'                ID_SAVE => save_settings(),','                ID_SAVE => save_settings(),\n                ID_LIQUID_GLASS => change_liquid_glass(),')
    s=replace_once(s,'        WM_CLOSE => {','        windows::Win32::UI::WindowsAndMessaging::WM_SETTINGCHANGE | windows::Win32::UI::WindowsAndMessaging::WM_THEMECHANGED => {\n            crate::voice_glass::refresh_theme();\n            LRESULT(0)\n        }\n        WM_CLOSE => {')
    s=replace_once(s,'fn quit_application() {','fn quit_application() {\n    crate::voice_glass::shutdown();')
    s+='''
// Independent of server reachability. No mutex is held across the modal consent UI.
fn change_liquid_glass() {
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_YESNO, MB_ICONINFORMATION, IDYES};
    let (window, checkbox) = {
        let Some(state) = SHELL_STATE.get().and_then(|s| s.lock().ok()) else { return; };
        (state.window, state.liquid_checkbox)
    };
    let requested = unsafe { windows::Win32::UI::WindowsAndMessaging::SendMessageW(
        hwnd(checkbox), BM_GETCHECK, None, None).0 == 1 };
    let mut enabled = requested;
    if requested && !crate::voice_glass::enabled() {
        enabled = unsafe { MessageBoxW(Some(hwnd(window)),
            w!("Liquid Glass reads your primary SDR desktop locally on the GPU only while the voice overlay is visible. No screenshots are saved or uploaded. The glass window is excluded from ordinary system screenshots and recording to prevent recursive capture. Unsupported displays use the solid overlay. Enable?"),
            w!("Enable Liquid Glass"), MB_YESNO | MB_ICONINFORMATION) } == IDYES;
    }
    let Some(state) = SHELL_STATE.get().and_then(|s| s.lock().ok()) else { return; };
    let result = ClientSettings::load().and_then(|mut settings| {
        settings.liquid_glass = enabled;
        settings.save()
    });
    if let Err(error) = result {
        set_status(&state,&error); enabled = crate::voice_glass::enabled();
    } else {
        crate::voice_glass::configure(enabled);
        set_status(&state,if enabled { "Liquid Glass enabled" } else { "Liquid Glass disabled" });
    }
    unsafe { let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
        hwnd(checkbox),BM_SETCHECK,Some(WPARAM(usize::from(enabled))),None); }
}
'''
    save(p,s)
    print('Applied exact voice state, hotkey, fallback, consent, theme and shared-library hooks.')

if __name__=='__main__': main()
