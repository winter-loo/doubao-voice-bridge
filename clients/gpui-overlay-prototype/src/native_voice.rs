use std::io::{BufRead as _, BufReader, Write as _};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use std::ffi::c_void;
#[cfg(target_os = "linux")]
use std::os::unix::fs::PermissionsExt as _;
#[cfg(target_os = "linux")]
use std::path::PathBuf;
#[cfg(target_os = "linux")]
use std::process::{Command, Stdio};

use cpal::traits::{DeviceTrait as _, HostTrait as _, StreamTrait as _};
use cpal::{FromSample, I24, Sample as _, SampleFormat, SizedSample, Stream, StreamConfig, U24};
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{GlobalFree, HANDLE, HWND};
#[cfg(target_os = "windows")]
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
#[cfg(target_os = "windows")]
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_CONTROL,
};

use crate::client_core::{
    BridgeEvent, PcmNormalizer, decode_bridge_event, encode_s16le, pcm_level,
};
use crate::client_settings::ClientSettings;

const AUDIO_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const FINAL_TIMEOUT: Duration = Duration::from_secs(8);
const AUDIO_START_DELAY: Duration = Duration::from_millis(200);
const AUDIO_STOP_SILENCE: Duration = Duration::from_millis(500);
#[cfg(target_os = "windows")]
const CF_UNICODETEXT: u32 = 13;

#[derive(Clone, Debug)]
pub enum NativeVoiceEvent {
    Phase(String),
    Partial(String),
    AudioLevel { rms_dbfs: f32, peak_dbfs: f32 },
    Error(String),
    Finished,
}

#[derive(Clone, Debug)]
pub struct NativeVoiceConfig {
    pub server: String,
    pub audio_port: u16,
    pub token: Option<String>,
    pub input_device: Option<String>,
    pub clipboard_owner: isize,
}

impl NativeVoiceConfig {
    pub fn from_environment() -> Result<Self, String> {
        let settings = ClientSettings::load()?;
        let server = std::env::var("DOUBAO_BRIDGE_SERVER").unwrap_or(settings.server);
        parse_server(&server)?;
        let audio_port = std::env::var("DOUBAO_BRIDGE_AUDIO_PORT")
            .ok()
            .map(|value| {
                value
                    .parse::<u16>()
                    .map_err(|_| format!("invalid DOUBAO_BRIDGE_AUDIO_PORT: {value}"))
            })
            .transpose()?
            .unwrap_or(settings.audio_port);
        Ok(Self {
            server,
            audio_port,
            token: std::env::var("DOUBAO_BRIDGE_TOKEN").ok().or(settings.token),
            input_device: std::env::var("DOUBAO_VOICE_INPUT_DEVICE")
                .ok()
                .or(settings.input_device_id),
            clipboard_owner: 0,
        })
    }
}

#[cfg(target_os = "windows")]
#[derive(Clone, Debug)]
pub struct InputDeviceInfo {
    pub id: String,
    pub name: String,
}

#[cfg(target_os = "windows")]
pub fn input_devices() -> Result<Vec<InputDeviceInfo>, String> {
    cpal::default_host()
        .input_devices()
        .map_err(|error| format!("could not enumerate microphones: {error}"))?
        .map(|device| {
            let id = device
                .id()
                .map_err(|error| format!("could not read microphone ID: {error}"))?
                .to_string();
            let name = device
                .description()
                .map_err(|error| format!("could not read microphone name: {error}"))?
                .name()
                .to_string();
            Ok(InputDeviceInfo { id, name })
        })
        .collect()
}

pub struct NativeVoiceSession {
    stop: SyncSender<()>,
    worker: JoinHandle<()>,
}

impl NativeVoiceSession {
    pub fn start<F>(config: NativeVoiceConfig, notify: F) -> Result<Self, String>
    where
        F: Fn(NativeVoiceEvent) + Send + 'static,
    {
        let (stop, stop_rx) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("doubao-native-voice-session".to_string())
            .spawn(move || {
                let result = run_session(config, &stop_rx, &notify);
                if let Err(error) = result {
                    notify(NativeVoiceEvent::Error(error));
                }
                notify(NativeVoiceEvent::Finished);
            })
            .map_err(|error| format!("could not start native voice session: {error}"))?;
        Ok(Self { stop, worker })
    }

    pub fn request_stop(&self) {
        let _ = self.stop.try_send(());
    }

    pub fn is_finished(&self) -> bool {
        self.worker.is_finished()
    }
}

struct AudioChunk {
    pcm: Vec<u8>,
    rms_dbfs: f32,
    peak_dbfs: f32,
}

fn run_session<F>(
    config: NativeVoiceConfig,
    stop_rx: &Receiver<()>,
    notify: &F,
) -> Result<(), String>
where
    F: Fn(NativeVoiceEvent),
{
    let (control_host, control_port) = parse_server(&config.server)?;
    let mut control = connect_with_timeout(&control_host, control_port, Duration::from_secs(10))?;
    control
        .set_nodelay(true)
        .map_err(|error| format!("could not configure bridge socket: {error}"))?;
    let (bridge_tx, bridge_rx) = mpsc::channel();
    start_bridge_reader(
        control
            .try_clone()
            .map_err(|error| format!("could not clone bridge socket: {error}"))?,
        bridge_tx,
    )?;

    if let Some(token) = &config.token {
        write_command(&mut control, &format!("token {token}"))?;
    }
    write_command(&mut control, "start")?;
    let mut stop_sent = false;
    let result = (|| {
        thread::sleep(AUDIO_START_DELAY);

        let mut audio_socket =
            connect_with_timeout(&control_host, config.audio_port, AUDIO_CONNECT_TIMEOUT)?;
        audio_socket
            .set_nodelay(true)
            .map_err(|error| format!("could not configure audio socket: {error}"))?;

        let (audio_tx, audio_rx) = mpsc::sync_channel(32);
        let (audio_error_tx, audio_error_rx) = mpsc::channel();
        let stream = start_microphone(config.input_device.as_deref(), audio_tx, audio_error_tx)?;
        let mut latest_text = String::new();
        let mut final_text = String::new();

        loop {
            drain_bridge_events(&bridge_rx, notify, &mut latest_text, &mut final_text)?;
            if let Ok(error) = audio_error_rx.try_recv() {
                return Err(error);
            }
            if stop_rx.try_recv().is_ok() {
                break;
            }

            match audio_rx.recv_timeout(Duration::from_millis(20)) {
                Ok(chunk) => {
                    notify(NativeVoiceEvent::AudioLevel {
                        rms_dbfs: chunk.rms_dbfs,
                        peak_dbfs: chunk.peak_dbfs,
                    });
                    audio_socket
                        .write_all(&chunk.pcm)
                        .map_err(|error| format!("audio connection failed: {error}"))?;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("microphone stream stopped unexpectedly".to_string());
                }
            }
        }

        drop(stream);
        while let Ok(chunk) = audio_rx.try_recv() {
            audio_socket
                .write_all(&chunk.pcm)
                .map_err(|error| format!("audio connection failed while stopping: {error}"))?;
        }
        write_command(&mut control, "stop")?;
        stop_sent = true;
        send_silence(&mut audio_socket, AUDIO_STOP_SILENCE)?;
        let _ = audio_socket.shutdown(Shutdown::Write);

        let deadline = Instant::now() + FINAL_TIMEOUT;
        while Instant::now() < deadline && final_text.is_empty() {
            match bridge_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(event) => handle_bridge_event(event, notify, &mut latest_text, &mut final_text)?,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        let text = if final_text.is_empty() {
            latest_text
        } else {
            final_text
        };
        if !text.is_empty() {
            paste_text(&text, config.clipboard_owner)?;
        }
        Ok(())
    })();
    if !stop_sent {
        let _ = write_command(&mut control, "stop");
    }
    result
}

fn start_bridge_reader(
    stream: TcpStream,
    events: mpsc::Sender<Result<BridgeEvent, String>>,
) -> Result<(), String> {
    thread::Builder::new()
        .name("doubao-bridge-events".to_string())
        .spawn(move || {
            for line in BufReader::new(stream).lines() {
                let event = line
                    .map_err(|error| format!("bridge connection failed: {error}"))
                    .and_then(|line| {
                        decode_bridge_event(&line)
                            .map_err(|error| format!("invalid bridge event: {error}"))
                    });
                if events.send(event).is_err() {
                    break;
                }
            }
        })
        .map(|_| ())
        .map_err(|error| format!("could not start bridge event reader: {error}"))
}

fn drain_bridge_events<F>(
    events: &Receiver<Result<BridgeEvent, String>>,
    notify: &F,
    latest_text: &mut String,
    final_text: &mut String,
) -> Result<(), String>
where
    F: Fn(NativeVoiceEvent),
{
    loop {
        match events.try_recv() {
            Ok(event) => handle_bridge_event(event, notify, latest_text, final_text)?,
            Err(TryRecvError::Empty) => return Ok(()),
            Err(TryRecvError::Disconnected) => {
                return Err("bridge connection closed".to_string());
            }
        }
    }
}

fn handle_bridge_event<F>(
    event: Result<BridgeEvent, String>,
    notify: &F,
    latest_text: &mut String,
    final_text: &mut String,
) -> Result<(), String>
where
    F: Fn(NativeVoiceEvent),
{
    match event? {
        BridgeEvent::Phase(phase) => notify(NativeVoiceEvent::Phase(phase)),
        BridgeEvent::Partial(text) => notify(NativeVoiceEvent::Partial(text)),
        BridgeEvent::Text(text) => *latest_text = text,
        BridgeEvent::Final(text) => *final_text = text,
        BridgeEvent::Error { phase, message } => {
            let context = phase.map(|phase| format!("{phase}: ")).unwrap_or_default();
            return Err(format!("{context}{message}"));
        }
        BridgeEvent::Other => {}
    }
    Ok(())
}

fn start_microphone(
    requested_device: Option<&str>,
    audio: SyncSender<AudioChunk>,
    errors: mpsc::Sender<String>,
) -> Result<Stream, String> {
    let host = cpal::default_host();
    let device = if let Some(requested) = requested_device {
        let requested_folded = requested.to_lowercase();
        host.input_devices()
            .map_err(|error| format!("could not enumerate microphones: {error}"))?
            .find(|device| {
                device.id().is_ok_and(|id| id.to_string() == requested)
                    || device.description().is_ok_and(|description| {
                        description.name().to_lowercase() == requested_folded
                    })
            })
            .ok_or_else(|| format!("microphone not found: {requested}"))?
    } else {
        host.default_input_device()
            .ok_or_else(|| "no default microphone is available".to_string())?
    };
    let supported = device
        .default_input_config()
        .map_err(|error| format!("could not read microphone format: {error}"))?;
    let channels = supported.channels();
    let sample_rate = supported.sample_rate();
    let config: StreamConfig = supported.clone().into();

    let stream = match supported.sample_format() {
        SampleFormat::I8 => {
            build_stream::<i8>(&device, &config, sample_rate, channels, audio, errors)
        }
        SampleFormat::F32 => {
            build_stream::<f32>(&device, &config, sample_rate, channels, audio, errors)
        }
        SampleFormat::I16 => {
            build_stream::<i16>(&device, &config, sample_rate, channels, audio, errors)
        }
        SampleFormat::I24 => {
            build_stream::<I24>(&device, &config, sample_rate, channels, audio, errors)
        }
        SampleFormat::I32 => {
            build_stream::<i32>(&device, &config, sample_rate, channels, audio, errors)
        }
        SampleFormat::I64 => {
            build_stream::<i64>(&device, &config, sample_rate, channels, audio, errors)
        }
        SampleFormat::U8 => {
            build_stream::<u8>(&device, &config, sample_rate, channels, audio, errors)
        }
        SampleFormat::U16 => {
            build_stream::<u16>(&device, &config, sample_rate, channels, audio, errors)
        }
        SampleFormat::U24 => {
            build_stream::<U24>(&device, &config, sample_rate, channels, audio, errors)
        }
        SampleFormat::U32 => {
            build_stream::<u32>(&device, &config, sample_rate, channels, audio, errors)
        }
        SampleFormat::U64 => {
            build_stream::<u64>(&device, &config, sample_rate, channels, audio, errors)
        }
        SampleFormat::F64 => {
            build_stream::<f64>(&device, &config, sample_rate, channels, audio, errors)
        }
        format => Err(format!("unsupported microphone sample format: {format}")),
    }?;
    stream
        .play()
        .map_err(|error| format!("could not start microphone: {error}"))?;
    Ok(stream)
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    sample_rate: u32,
    channels: u16,
    audio: SyncSender<AudioChunk>,
    errors: mpsc::Sender<String>,
) -> Result<Stream, String>
where
    T: SizedSample + Copy,
    f32: FromSample<T>,
{
    let mut normalizer = PcmNormalizer::new(sample_rate, channels)?;
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                let input: Vec<f32> = data
                    .iter()
                    .map(|sample| f32::from_sample(*sample))
                    .collect();
                let samples = normalizer.push_f32(&input);
                if let Some((rms_dbfs, peak_dbfs)) = pcm_level(&samples) {
                    let chunk = AudioChunk {
                        pcm: encode_s16le(&samples),
                        rms_dbfs,
                        peak_dbfs,
                    };
                    match audio.try_send(chunk) {
                        Ok(()) | Err(TrySendError::Full(_)) => {}
                        Err(TrySendError::Disconnected(_)) => {}
                    }
                }
            },
            move |error| {
                let _ = errors.send(format!("microphone stream failed: {error}"));
            },
            None,
        )
        .map_err(|error| format!("could not open microphone: {error}"))
}

fn parse_server(server: &str) -> Result<(String, u16), String> {
    let (host, port) = server
        .rsplit_once(':')
        .filter(|(host, port)| !host.is_empty() && !port.is_empty())
        .ok_or_else(|| format!("invalid DOUBAO_BRIDGE_SERVER: {server}"))?;
    let port = port
        .parse::<u16>()
        .map_err(|_| format!("invalid DOUBAO_BRIDGE_SERVER: {server}"))?;
    Ok((host.trim_matches(['[', ']']).to_string(), port))
}

fn connect_with_timeout(host: &str, port: u16, timeout: Duration) -> Result<TcpStream, String> {
    let deadline = Instant::now() + timeout;
    let mut last_error = None;
    loop {
        let addresses = (host, port)
            .to_socket_addrs()
            .map_err(|error| format!("could not resolve {host}:{port}: {error}"))?;
        for address in addresses {
            match TcpStream::connect_timeout(&address, Duration::from_secs(2)) {
                Ok(stream) => return Ok(stream),
                Err(error) => last_error = Some(error),
            }
        }
        if Instant::now() >= deadline {
            let detail = last_error
                .map(|error| error.to_string())
                .unwrap_or_else(|| "no addresses resolved".to_string());
            return Err(format!("could not connect to {host}:{port}: {detail}"));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn write_command(stream: &mut TcpStream, command: &str) -> Result<(), String> {
    stream
        .write_all(format!("{command}\n").as_bytes())
        .map_err(|error| format!("could not send bridge command: {error}"))
}

fn send_silence(stream: &mut TcpStream, duration: Duration) -> Result<(), String> {
    let chunk = [0; 1_920];
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        stream
            .write_all(&chunk)
            .map_err(|error| format!("could not send stop silence: {error}"))?;
        thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn paste_text(text: &str, clipboard_owner: isize) -> Result<(), String> {
    set_clipboard_text(text, clipboard_owner)?;
    let inputs = [
        keyboard_input(VK_CONTROL, false),
        keyboard_input(VIRTUAL_KEY(b'V' as u16), false),
        keyboard_input(VIRTUAL_KEY(b'V' as u16), true),
        keyboard_input(VK_CONTROL, true),
    ];
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent != inputs.len() as u32 {
        return Err("Windows blocked the paste shortcut; the text remains on the clipboard".into());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn keyboard_input(key: VIRTUAL_KEY, key_up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                dwFlags: if key_up {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                ..Default::default()
            },
        },
    }
}

#[cfg(target_os = "windows")]
fn set_clipboard_text(text: &str, clipboard_owner: isize) -> Result<(), String> {
    let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let owner = Some(HWND(clipboard_owner as *mut c_void));
        let mut clipboard_open = false;
        for _ in 0..10 {
            if OpenClipboard(owner).is_ok() {
                clipboard_open = true;
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        if !clipboard_open {
            return Err("could not open clipboard after 10 attempts".to_string());
        }
        let result = (|| {
            EmptyClipboard().map_err(|error| format!("could not clear clipboard: {error}"))?;
            let memory = GlobalAlloc(GMEM_MOVEABLE, utf16.len() * std::mem::size_of::<u16>())
                .map_err(|error| format!("could not allocate clipboard memory: {error}"))?;
            let pointer = GlobalLock(memory);
            if pointer.is_null() {
                let _ = GlobalFree(Some(memory));
                return Err("could not lock clipboard memory".to_string());
            }
            std::ptr::copy_nonoverlapping(utf16.as_ptr(), pointer.cast::<u16>(), utf16.len());
            let _ = GlobalUnlock(memory);
            if SetClipboardData(CF_UNICODETEXT, Some(HANDLE(memory.0))).is_err() {
                let _ = GlobalFree(Some(memory));
                return Err("could not set clipboard text".to_string());
            }
            Ok(())
        })();
        let _ = CloseClipboard();
        result
    }
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinuxClipboardTool {
    WlCopy,
    Xclip,
    Xsel,
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinuxPasteTool {
    Ydotool,
    Xdotool,
}

#[cfg(target_os = "linux")]
fn choose_linux_clipboard_tool<F>(wayland: bool, mut available: F) -> Option<LinuxClipboardTool>
where
    F: FnMut(&str) -> bool,
{
    if wayland && available("wl-copy") {
        return Some(LinuxClipboardTool::WlCopy);
    }
    if available("xclip") {
        return Some(LinuxClipboardTool::Xclip);
    }
    if available("xsel") {
        return Some(LinuxClipboardTool::Xsel);
    }
    None
}

#[cfg(target_os = "linux")]
fn choose_linux_paste_tool<F>(mut available: F) -> Option<LinuxPasteTool>
where
    F: FnMut(&str) -> bool,
{
    if available("ydotool") {
        return Some(LinuxPasteTool::Ydotool);
    }
    if available("xdotool") {
        return Some(LinuxPasteTool::Xdotool);
    }
    None
}

#[cfg(target_os = "linux")]
fn command_exists(command: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|directory| {
        let candidate: PathBuf = directory.join(command);
        candidate
            .metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    })
}

#[cfg(target_os = "linux")]
fn write_clipboard(command: &str, args: &[&str], text: &str) -> Result<(), String> {
    let mut child = Command::new(command)
        .args(args)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not start {command}: {error}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| format!("could not open {command} stdin"))?
        .write_all(text.as_bytes())
        .map_err(|error| format!("could not send text to {command}: {error}"))?;
    let status = child
        .wait()
        .map_err(|error| format!("could not wait for {command}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{command} exited with {status}"))
    }
}

#[cfg(target_os = "linux")]
fn paste_text(text: &str, _clipboard_owner: isize) -> Result<(), String> {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    match choose_linux_clipboard_tool(wayland, command_exists) {
        Some(LinuxClipboardTool::WlCopy) => write_clipboard("wl-copy", &[], text)?,
        Some(LinuxClipboardTool::Xclip) => {
            write_clipboard("xclip", &["-selection", "clipboard"], text)?
        }
        Some(LinuxClipboardTool::Xsel) => {
            write_clipboard("xsel", &["--clipboard", "--input"], text)?
        }
        None => {
            return Err("install wl-copy, xclip, or xsel to receive recognized text".to_string());
        }
    }

    let (command, args): (&str, &[&str]) = match choose_linux_paste_tool(command_exists) {
        Some(LinuxPasteTool::Ydotool) => ("ydotool", &["key", "29:1", "47:1", "47:0", "29:0"]),
        Some(LinuxPasteTool::Xdotool) => ("xdotool", &["key", "--clearmodifiers", "ctrl+v"]),
        None => {
            return Err(
                "recognized text is on the clipboard; install ydotool or xdotool to paste it"
                    .to_string(),
            );
        }
    };
    let status = Command::new(command)
        .args(args)
        .status()
        .map_err(|error| format!("could not start {command}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "recognized text is on the clipboard, but {command} exited with {status}"
        ))
    }
}

#[cfg(all(test, target_os = "linux"))]
mod linux_tests {
    use super::{
        LinuxClipboardTool, LinuxPasteTool, choose_linux_clipboard_tool, choose_linux_paste_tool,
    };

    #[test]
    fn prefers_session_native_clipboard_then_x11_fallbacks() {
        assert_eq!(
            choose_linux_clipboard_tool(true, |tool| matches!(tool, "wl-copy" | "xclip")),
            Some(LinuxClipboardTool::WlCopy)
        );
        assert_eq!(
            choose_linux_clipboard_tool(false, |tool| matches!(tool, "wl-copy" | "xclip")),
            Some(LinuxClipboardTool::Xclip)
        );
        assert_eq!(
            choose_linux_clipboard_tool(false, |tool| tool == "xsel"),
            Some(LinuxClipboardTool::Xsel)
        );
    }

    #[test]
    fn prefers_wayland_capable_global_input_injector() {
        assert_eq!(
            choose_linux_paste_tool(|tool| matches!(tool, "ydotool" | "xdotool")),
            Some(LinuxPasteTool::Ydotool)
        );
        assert_eq!(
            choose_linux_paste_tool(|tool| tool == "xdotool"),
            Some(LinuxPasteTool::Xdotool)
        );
    }
}
