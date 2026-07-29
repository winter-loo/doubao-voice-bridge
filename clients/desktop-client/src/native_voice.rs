use std::collections::VecDeque;
use std::io::{BufRead as _, BufReader, Write as _};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::{
    Mutex,
    mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait as _, HostTrait as _, StreamTrait as _};
use cpal::{FromSample, I24, Sample as _, SampleFormat, SizedSample, Stream, StreamConfig, U24};

use crate::client_core::{
    BridgeEvent, PcmNormalizer, decode_bridge_event, encode_s16le, pcm_level,
};
use crate::client_settings::ClientSettings;
use crate::platform_paste::{PasteTarget, paste_text};

const AUDIO_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const FINAL_TIMEOUT: Duration = Duration::from_secs(8);
const AUDIO_START_DELAY: Duration = Duration::from_millis(200);
const AUDIO_STOP_SILENCE: Duration = Duration::from_millis(500);
const ARMING_PREROLL_BYTES: usize = 48_000 * 2 * 5;

const fn auto_paste_enabled() -> bool {
    cfg!(target_os = "windows")
}

#[derive(Clone, Debug, PartialEq)]
pub enum NativeVoiceEvent {
    Phase(String),
    Partial(String),
    Committed(String),
    Final(String),
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
    paste_target: PasteTarget,
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
            paste_target: PasteTarget::default(),
        })
    }

    #[cfg(target_os = "windows")]
    pub fn with_paste_target(mut self, paste_target: PasteTarget) -> Self {
        self.paste_target = paste_target;
        self
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

struct NativeVoiceSession {
    stop: SyncSender<()>,
    worker: JoinHandle<()>,
}

pub struct NativeVoiceController {
    active: Mutex<Option<NativeVoiceSession>>,
}

impl NativeVoiceController {
    pub const fn new() -> Self {
        Self {
            active: Mutex::new(None),
        }
    }

    pub fn start<F>(&self, config: NativeVoiceConfig, notify: F) -> Result<(), String>
    where
        F: Fn(NativeVoiceEvent) + Send + 'static,
    {
        let mut active = self
            .active
            .lock()
            .map_err(|_| "voice client lock is poisoned".to_string())?;
        if active
            .as_ref()
            .is_some_and(|session| !session.is_finished())
        {
            return Err("voice session is already active".to_string());
        }
        active.take();
        *active = Some(NativeVoiceSession::start(config, notify)?);
        Ok(())
    }

    pub fn request_stop(&self) -> bool {
        let Ok(active) = self.active.lock() else {
            return false;
        };
        let Some(session) = active.as_ref() else {
            return false;
        };
        if session.is_finished() {
            return false;
        }
        session.request_stop();
        true
    }

    pub fn stop_and_wait(&self) -> bool {
        let session = {
            let Ok(mut active) = self.active.lock() else {
                return false;
            };
            active.take()
        };
        session.is_some_and(NativeVoiceSession::stop_and_wait)
    }
}

impl NativeVoiceSession {
    fn start<F>(config: NativeVoiceConfig, notify: F) -> Result<Self, String>
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

    fn request_stop(&self) {
        let _ = self.stop.try_send(());
    }

    fn is_finished(&self) -> bool {
        self.worker.is_finished()
    }

    fn stop_and_wait(self) -> bool {
        let was_active = !self.is_finished();
        if was_active {
            self.request_stop();
        }
        let _ = self.worker.join();
        was_active
    }
}

struct AudioChunk {
    pcm: Vec<u8>,
    rms_dbfs: f32,
    peak_dbfs: f32,
}

struct PrerollAudio {
    chunks: VecDeque<Vec<u8>>,
    bytes: usize,
    limit: usize,
    recording: bool,
}

impl PrerollAudio {
    fn new(limit: usize) -> Self {
        Self {
            chunks: VecDeque::new(),
            bytes: 0,
            limit,
            recording: false,
        }
    }

    fn push(&mut self, pcm: Vec<u8>) -> Vec<Vec<u8>> {
        if self.recording {
            return vec![pcm];
        }

        self.bytes += pcm.len();
        self.chunks.push_back(pcm);
        while self.bytes > self.limit {
            let excess = self.bytes - self.limit;
            let Some(front) = self.chunks.front_mut() else {
                break;
            };
            if front.len() <= excess {
                self.bytes -= front.len();
                self.chunks.pop_front();
            } else {
                front.drain(..excess);
                self.bytes -= excess;
            }
        }
        Vec::new()
    }

    fn start_recording(&mut self) -> Vec<Vec<u8>> {
        self.recording = true;
        self.bytes = 0;
        self.chunks.drain(..).collect()
    }
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

        let (audio_tx, audio_rx) = mpsc::sync_channel(512);
        let (audio_error_tx, audio_error_rx) = mpsc::channel();
        let stream = start_microphone(config.input_device.as_deref(), audio_tx, audio_error_tx)?;
        let mut latest_text = String::new();
        let mut final_text = String::new();
        let mut audio_forwarding = PrerollAudio::new(ARMING_PREROLL_BYTES);

        loop {
            if drain_bridge_events(&bridge_rx, notify, &mut latest_text, &mut final_text)? {
                for pcm in audio_forwarding.start_recording() {
                    write_realtime_audio(&mut audio_socket, &pcm)?;
                }
            }
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
                    for pcm in audio_forwarding.push(chunk.pcm) {
                        write_realtime_audio(&mut audio_socket, &pcm)?;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("microphone stream stopped unexpectedly".to_string());
                }
            }
        }

        drop(stream);
        while let Ok(chunk) = audio_rx.try_recv() {
            write_realtime_audio(&mut audio_socket, &chunk.pcm)
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
        if auto_paste_enabled() && !text.is_empty() {
            paste_text(&text, config.paste_target)?;
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
) -> Result<bool, String>
where
    F: Fn(NativeVoiceEvent),
{
    let mut recording_started = false;
    loop {
        match events.try_recv() {
            Ok(event) => {
                recording_started |= matches!(
                    &event,
                    Ok(BridgeEvent::Phase(phase)) if phase == "recording"
                );
                handle_bridge_event(event, notify, latest_text, final_text)?;
            }
            Err(TryRecvError::Empty) => return Ok(recording_started),
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
        BridgeEvent::Text(text) => {
            *latest_text = text.clone();
            notify(NativeVoiceEvent::Committed(text));
        }
        BridgeEvent::Final(text) => {
            *final_text = text.clone();
            notify(NativeVoiceEvent::Final(text));
        }
        BridgeEvent::Error { phase, message } => {
            let context = phase.map(|phase| format!("{phase}: ")).unwrap_or_default();
            return Err(format!("{context}{message}"));
        }
        BridgeEvent::Other => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        NativeVoiceEvent, PrerollAudio, auto_paste_enabled, handle_bridge_event,
        pcm_playback_duration,
    };
    use crate::client_core::BridgeEvent;
    use std::{sync::Mutex, time::Duration};

    #[test]
    fn bridge_text_events_are_delivered_to_the_ui() {
        let received = Mutex::new(Vec::new());
        let notify = |event| received.lock().unwrap().push(event);
        let mut latest_text = String::new();
        let mut final_text = String::new();

        handle_bridge_event(
            Ok(BridgeEvent::Partial("你".to_string())),
            &notify,
            &mut latest_text,
            &mut final_text,
        )
        .unwrap();
        handle_bridge_event(
            Ok(BridgeEvent::Text("你好".to_string())),
            &notify,
            &mut latest_text,
            &mut final_text,
        )
        .unwrap();
        handle_bridge_event(
            Ok(BridgeEvent::Final("你好世界".to_string())),
            &notify,
            &mut latest_text,
            &mut final_text,
        )
        .unwrap();

        assert_eq!(
            received.into_inner().unwrap(),
            vec![
                NativeVoiceEvent::Partial("你".to_string()),
                NativeVoiceEvent::Committed("你好".to_string()),
                NativeVoiceEvent::Final("你好世界".to_string()),
            ]
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_voice_result_is_not_automatically_pasted() {
        assert!(!auto_paste_enabled());
    }

    #[test]
    fn audio_spoken_during_arming_is_forwarded_when_recording_starts() {
        let mut audio = PrerollAudio::new(8);

        assert!(audio.push(vec![1, 2, 3]).is_empty());
        assert!(audio.push(vec![4, 5]).is_empty());
        assert_eq!(audio.start_recording(), vec![vec![1, 2, 3], vec![4, 5]]);
        assert_eq!(audio.push(vec![6, 7]), vec![vec![6, 7]]);
    }

    #[test]
    fn arming_audio_keeps_only_the_configured_preroll_tail() {
        let mut audio = PrerollAudio::new(5);

        audio.push(vec![1, 2, 3]);
        audio.push(vec![4, 5, 6, 7]);

        assert_eq!(audio.start_recording(), vec![vec![3], vec![4, 5, 6, 7]]);
    }

    #[test]
    fn pcm_playback_duration_uses_48khz_mono_s16le_rate() {
        assert_eq!(pcm_playback_duration(96_000), Duration::from_secs(1));
    }
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

fn pcm_playback_duration(bytes: usize) -> Duration {
    Duration::from_secs_f64(bytes as f64 / (48_000.0 * 2.0))
}

fn write_realtime_audio(stream: &mut TcpStream, pcm: &[u8]) -> Result<(), String> {
    let started = Instant::now();
    stream
        .write_all(pcm)
        .map_err(|error| format!("audio connection failed: {error}"))?;
    thread::sleep(pcm_playback_duration(pcm.len()).saturating_sub(started.elapsed()));
    Ok(())
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
