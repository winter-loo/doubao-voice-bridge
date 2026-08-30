use std::ffi::c_void;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::net::{TcpStream, ToSocketAddrs as _};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::{ERROR_SUCCESS, HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, COLOR_WINDOW, CreateFontW, DEFAULT_CHARSET,
    DEFAULT_PITCH, FF_DONTCARE, FW_NORMAL, FW_SEMIBOLD, GetStockObject, HBRUSH, HDC,
    OUT_DEFAULT_PRECIS, SetBkMode, TRANSPARENT, WHITE_BRUSH,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
    RegCreateKeyExW, RegDeleteValueW, RegSetValueExW,
};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, BM_GETCHECK, BM_SETCHECK, BS_AUTOCHECKBOX, BS_DEFPUSHBUTTON, CB_ADDSTRING,
    CB_GETCURSEL, CB_SETCURSEL, CBS_DROPDOWNLIST, CREATESTRUCTW, CW_USEDEFAULT, CreatePopupMenu,
    CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow, DispatchMessageW, ES_AUTOHSCROLL,
    GWLP_USERDATA, GetCursorPos, GetMessageW, GetWindowTextW, HICON, HMENU, IDC_ARROW,
    IDI_APPLICATION, LoadCursorW, LoadIconW, MF_SEPARATOR, MF_STRING, MSG, PostMessageW,
    RegisterClassW, SW_HIDE, SW_SHOW, SetForegroundWindow, SetWindowLongPtrW, SetWindowTextW,
    ShowWindow, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_APP, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_CTLCOLORBTN, WM_CTLCOLORSTATIC, WM_DESTROY,
    WM_LBUTTONUP, WM_RBUTTONUP, WM_SETFONT, WNDCLASSW, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN,
    WS_EX_APPWINDOW, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};
use windows::core::{PCWSTR, w};

use crate::client_settings::ClientSettings;
use crate::native_voice::{InputDeviceInfo, input_devices};

const TRAY_MESSAGE: u32 = WM_APP + 24;
const TRAY_ID: u32 = 1;
const APP_ICON_RESOURCE_ID: usize = 1;
const TRAY_ICON_RESOURCE_ID: usize = 2;
const ID_SERVER: usize = 1001;
const ID_MICROPHONE: usize = 1002;
const ID_STARTUP: usize = 1003;
const ID_SAVE: usize = 1004;
const ID_SETTINGS: usize = 2001;
const ID_QUIT: usize = 2002;

static SHELL_STATE: OnceLock<Mutex<ShellState>> = OnceLock::new();

struct ShellState {
    overlay: isize,
    window: isize,
    server_edit: isize,
    microphone_combo: isize,
    startup_checkbox: isize,
    status_label: isize,
    devices: Vec<InputDeviceInfo>,
}

pub fn start(overlay: HWND) {
    let overlay = overlay.0 as isize;
    thread::spawn(move || {
        if let Err(error) = run_shell(overlay) {
            log_shell(&format!("shell failed: {error}"));
        }
    });
}

fn run_shell(overlay: isize) -> Result<(), String> {
    unsafe {
        let module = GetModuleHandleW(None)
            .map_err(|error| format!("could not get application module: {error}"))?;
        let instance = HINSTANCE(module.0);
        let class = w!("DoubaoVoiceClientSettings");
        let window_class = WNDCLASSW {
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hIcon: load_application_icon(instance),
            hInstance: instance,
            lpszClassName: class,
            lpfnWndProc: Some(window_proc),
            hbrBackground: HBRUSH((COLOR_WINDOW.0 as isize + 1) as *mut c_void),
            ..Default::default()
        };
        if RegisterClassW(&window_class) == 0 {
            return Err("could not register settings window".to_string());
        }

        let window = CreateWindowExW(
            WS_EX_APPWINDOW,
            class,
            w!("Doubao Voice Client"),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            520,
            390,
            None,
            None,
            Some(instance),
            Some(&overlay as *const isize as *const c_void),
        )
        .map_err(|error| format!("could not create settings window: {error}"))?;

        let settings = ClientSettings::load().unwrap_or_default();
        if settings.setup_completed
            && let Err(error) = configure_autostart(settings.start_with_windows)
        {
            log_shell(&format!("could not refresh autostart: {error}"));
        }
        let devices = input_devices().unwrap_or_default();
        let controls = create_controls(window, instance, &settings, &devices)?;
        add_tray_icon(window, instance)?;
        let show_setup = !settings.setup_completed;
        SHELL_STATE
            .set(Mutex::new(ShellState {
                overlay,
                window: window.0 as isize,
                server_edit: controls.server_edit.0 as isize,
                microphone_combo: controls.microphone_combo.0 as isize,
                startup_checkbox: controls.startup_checkbox.0 as isize,
                status_label: controls.status_label.0 as isize,
                devices,
            }))
            .map_err(|_| "Windows shell was already initialized".to_string())?;

        if show_setup {
            show_settings();
        }

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}

struct Controls {
    server_edit: HWND,
    microphone_combo: HWND,
    startup_checkbox: HWND,
    status_label: HWND,
}

fn create_controls(
    parent: HWND,
    instance: HINSTANCE,
    settings: &ClientSettings,
    devices: &[InputDeviceInfo],
) -> Result<Controls, String> {
    unsafe {
        let title = create_control(
            w!("STATIC"),
            w!("Doubao Voice Client"),
            WS_CHILD | WS_VISIBLE,
            28,
            24,
            440,
            32,
            parent,
            0,
            instance,
        )?;
        apply_font(title, -22, FW_SEMIBOLD.0 as i32);
        create_control(
            w!("STATIC"),
            w!("Mac server"),
            WS_CHILD | WS_VISIBLE,
            28,
            78,
            440,
            20,
            parent,
            0,
            instance,
        )?;
        let server = wide(&settings.server);
        let server_edit = create_control(
            w!("EDIT"),
            PCWSTR(server.as_ptr()),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            28,
            102,
            448,
            30,
            parent,
            ID_SERVER,
            instance,
        )?;
        create_control(
            w!("STATIC"),
            w!("Microphone"),
            WS_CHILD | WS_VISIBLE,
            28,
            154,
            440,
            20,
            parent,
            0,
            instance,
        )?;
        let microphone_combo = create_control(
            w!("COMBOBOX"),
            w!(""),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
            28,
            178,
            448,
            220,
            parent,
            ID_MICROPHONE,
            instance,
        )?;
        let default_label = wide("System default");
        let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            microphone_combo,
            CB_ADDSTRING,
            None,
            Some(LPARAM(default_label.as_ptr() as isize)),
        );
        let mut selected = 0;
        for (index, device) in devices.iter().enumerate() {
            let label = wide(&device.name);
            let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                microphone_combo,
                CB_ADDSTRING,
                None,
                Some(LPARAM(label.as_ptr() as isize)),
            );
            if settings.input_device_id.as_deref() == Some(device.id.as_str()) {
                selected = index + 1;
            }
        }
        let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            microphone_combo,
            CB_SETCURSEL,
            Some(WPARAM(selected)),
            None,
        );
        let startup_checkbox = create_control(
            w!("BUTTON"),
            w!("Start with Windows"),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            28,
            226,
            260,
            26,
            parent,
            ID_STARTUP,
            instance,
        )?;
        let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            startup_checkbox,
            BM_SETCHECK,
            Some(WPARAM(usize::from(settings.start_with_windows))),
            None,
        );
        let status_label = create_control(
            w!("STATIC"),
            w!("Not connected"),
            WS_CHILD | WS_VISIBLE,
            28,
            272,
            290,
            24,
            parent,
            0,
            instance,
        )?;
        create_control(
            w!("BUTTON"),
            w!("Save and connect"),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_DEFPUSHBUTTON as u32),
            328,
            262,
            148,
            36,
            parent,
            ID_SAVE,
            instance,
        )?;
        Ok(Controls {
            server_edit,
            microphone_combo,
            startup_checkbox,
            status_label,
        })
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn create_control<P1, P2>(
    class: P1,
    text: P2,
    style: WINDOW_STYLE,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    parent: HWND,
    id: usize,
    instance: HINSTANCE,
) -> Result<HWND, String>
where
    P1: windows::core::Param<PCWSTR>,
    P2: windows::core::Param<PCWSTR>,
{
    unsafe {
        let control = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            text,
            style,
            x,
            y,
            width,
            height,
            Some(parent),
            Some(HMENU(id as *mut c_void)),
            Some(instance),
            None,
        )
        .map_err(|error| format!("could not create settings control: {error}"))?;
        apply_font(control, -16, FW_NORMAL.0 as i32);
        Ok(control)
    }
}

fn apply_font(control: HWND, height: i32, weight: i32) {
    unsafe {
        let font = CreateFontW(
            height,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            DEFAULT_PITCH.0 as u32 | FF_DONTCARE.0 as u32,
            w!("Segoe UI"),
        );
        let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            control,
            WM_SETFONT,
            Some(WPARAM(font.0 as usize)),
            Some(LPARAM(1)),
        );
    }
}

fn load_application_icon(instance: HINSTANCE) -> HICON {
    unsafe {
        LoadIconW(Some(instance), PCWSTR(APP_ICON_RESOURCE_ID as *const u16))
            .or_else(|_| LoadIconW(None, IDI_APPLICATION))
            .unwrap_or_default()
    }
}

fn load_tray_icon(instance: HINSTANCE) -> HICON {
    unsafe {
        LoadIconW(Some(instance), PCWSTR(TRAY_ICON_RESOURCE_ID as *const u16))
            .or_else(|_| LoadIconW(Some(instance), PCWSTR(APP_ICON_RESOURCE_ID as *const u16)))
            .or_else(|_| LoadIconW(None, IDI_APPLICATION))
            .unwrap_or_default()
    }
}

fn add_tray_icon(window: HWND, instance: HINSTANCE) -> Result<(), String> {
    unsafe {
        let mut icon = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: window,
            uID: TRAY_ID,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: TRAY_MESSAGE,
            hIcon: load_tray_icon(instance),
            ..Default::default()
        };
        copy_wide(&mut icon.szTip, "Doubao Voice Client");
        if !Shell_NotifyIconW(NIM_ADD, &icon).as_bool() {
            return Err("could not add notification-area icon".to_string());
        }
        Ok(())
    }
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_CREATE => {
            unsafe {
                let create = &*(lparam.0 as *const CREATESTRUCTW);
                SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let command = wparam.0 & 0xffff;
            log_shell(&format!("command {command}"));
            match command {
                ID_SAVE => save_settings(),
                ID_SETTINGS => show_settings(),
                ID_QUIT => quit_application(),
                _ => {}
            }
            LRESULT(0)
        }
        TRAY_MESSAGE => {
            match lparam.0 as u32 {
                WM_LBUTTONUP => show_settings(),
                WM_RBUTTONUP => show_tray_menu(window),
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            unsafe {
                let _ = ShowWindow(window, SW_HIDE);
            }
            LRESULT(0)
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => unsafe {
            let device_context = HDC(wparam.0 as *mut c_void);
            let _ = SetBkMode(device_context, TRANSPARENT);
            LRESULT(GetStockObject(WHITE_BRUSH).0 as isize)
        },
        WM_DESTROY => {
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

fn show_settings() {
    let Some(state) = SHELL_STATE.get().and_then(|state| state.lock().ok()) else {
        return;
    };
    unsafe {
        let window = hwnd(state.window);
        let _ = ShowWindow(window, SW_SHOW);
        let _ = SetForegroundWindow(window);
    }
}

fn show_tray_menu(window: HWND) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else {
            return;
        };
        let _ = AppendMenuW(menu, MF_STRING, ID_SETTINGS, w!("Settings"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, ID_QUIT, w!("Quit"));
        let mut point = POINT::default();
        if GetCursorPos(&mut point).is_ok() {
            let _ = SetForegroundWindow(window);
            let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON, point.x, point.y, None, window, None);
        }
        let _ = DestroyMenu(menu);
    }
}

fn save_settings() {
    log_shell("saving settings");
    let Some(state) = SHELL_STATE.get().and_then(|state| state.lock().ok()) else {
        return;
    };
    let server = window_text(hwnd(state.server_edit));
    set_status(&state, "Connecting...");
    if let Err(error) = test_server(&server) {
        log_shell(&format!("connection test failed: {error}"));
        set_status(&state, &error);
        return;
    }

    let selected = unsafe {
        windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            hwnd(state.microphone_combo),
            CB_GETCURSEL,
            None,
            None,
        )
        .0
    };
    let input_device_id = if selected > 0 {
        state
            .devices
            .get(selected as usize - 1)
            .map(|device| device.id.clone())
    } else {
        None
    };
    let start_with_windows = unsafe {
        windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            hwnd(state.startup_checkbox),
            BM_GETCHECK,
            None,
            None,
        )
        .0 == 1
    };
    let mut settings = ClientSettings::load().unwrap_or_default();
    settings.server = server;
    settings.input_device_id = input_device_id;
    settings.start_with_windows = start_with_windows;
    settings.setup_completed = true;
    if let Err(error) = settings
        .save()
        .and_then(|_| configure_autostart(start_with_windows))
    {
        log_shell(&format!("settings save failed: {error}"));
        set_status(&state, &error);
        return;
    }
    log_shell("settings saved");
    set_status(&state, "Connected");
}

fn test_server(server: &str) -> Result<(), String> {
    let (host, port) = server
        .rsplit_once(':')
        .filter(|(host, port)| !host.is_empty() && !port.is_empty())
        .ok_or_else(|| "Enter an address like 100.64.0.1:4387".to_string())?;
    let port = port
        .parse::<u16>()
        .map_err(|_| "The server port is invalid".to_string())?;
    let addresses = (host.trim_matches(['[', ']']), port)
        .to_socket_addrs()
        .map_err(|_| "The Mac address could not be resolved".to_string())?;
    for address in addresses {
        if TcpStream::connect_timeout(&address, Duration::from_secs(2)).is_ok() {
            return Ok(());
        }
    }
    Err("Could not connect to the Mac".to_string())
}

fn configure_autostart(enabled: bool) -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("could not locate application: {error}"))?;
    let command = format!("\"{}\"", executable.display());
    let subkey = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let value_name = wide("DoubaoVoiceClient");
    unsafe {
        let mut key = HKEY::default();
        let result = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        );
        if result != ERROR_SUCCESS {
            return Err(format!(
                "could not open Windows startup settings: {result:?}"
            ));
        }
        let result = if enabled {
            let utf16 = wide(&command);
            let bytes = std::slice::from_raw_parts(
                utf16.as_ptr().cast::<u8>(),
                utf16.len() * std::mem::size_of::<u16>(),
            );
            RegSetValueExW(key, PCWSTR(value_name.as_ptr()), None, REG_SZ, Some(bytes))
        } else {
            RegDeleteValueW(key, PCWSTR(value_name.as_ptr()))
        };
        let _ = RegCloseKey(key);
        if enabled && result != ERROR_SUCCESS {
            return Err(format!("could not update Start with Windows: {result:?}"));
        }
    }
    Ok(())
}

fn quit_application() {
    let Some(state) = SHELL_STATE.get().and_then(|state| state.lock().ok()) else {
        return;
    };
    unsafe {
        let icon = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd(state.window),
            uID: TRAY_ID,
            ..Default::default()
        };
        let _ = Shell_NotifyIconW(NIM_DELETE, &icon);
        let _ = PostMessageW(Some(hwnd(state.overlay)), WM_CLOSE, WPARAM(0), LPARAM(0));
        let _ = DestroyWindow(hwnd(state.window));
    }
}

fn set_status(state: &ShellState, text: &str) {
    let text = wide(text);
    unsafe {
        let _ = SetWindowTextW(hwnd(state.status_label), PCWSTR(text.as_ptr()));
    }
}

fn window_text(window: HWND) -> String {
    let mut buffer = [0u16; 512];
    let length = unsafe { GetWindowTextW(window, &mut buffer) };
    String::from_utf16_lossy(&buffer[..length.max(0) as usize])
        .trim()
        .to_string()
}

fn hwnd(value: isize) -> HWND {
    HWND(value as *mut c_void)
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn copy_wide(target: &mut [u16], value: &str) {
    for (target, source) in target.iter_mut().zip(wide(value)) {
        *target = source;
    }
}

fn log_shell(message: &str) {
    let path = std::env::temp_dir().join("doubao-voice-client-shell.log");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{message}");
    }
}
