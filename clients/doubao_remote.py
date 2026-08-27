#!/usr/bin/env python3

import argparse
import array
import collections
import json
import math
import os
import platform
import re
import shlex
import shutil
import socket
import subprocess
import sys
import threading
import time


BRIDGE_PHASE_PREFIX = "[bridge_phase]"


def emit_bridge_phase(phase):
    print(f"{BRIDGE_PHASE_PREFIX} {phase}", file=sys.stderr, flush=True)


class BridgeClient:
    def __init__(self, server, token=None):
        host, port_text = server.rsplit(":", 1)
        self.host = host
        self.port = int(port_text)
        self.token = token
        self.sock = None
        self.final_text = ""
        self.latest_text = ""
        self.partial_text = ""
        self.phase = "idle"
        self.activation_failed = False
        self.recording_ready = threading.Event()
        self._stop = threading.Event()

    def connect(self):
        self.sock = socket.create_connection((self.host, self.port), timeout=10)
        self.sock.settimeout(None)
        if self.token:
            self.send(f"token {self.token}")

    def send(self, line):
        if not self.sock:
            raise RuntimeError("not connected")
        self.sock.sendall((line + "\n").encode("utf-8"))

    def listen(self):
        if not self.sock:
            raise RuntimeError("not connected")

        buf = b""
        while not self._stop.is_set():
            try:
                chunk = self.sock.recv(4096)
            except OSError:
                return
            if not chunk:
                return
            buf += chunk
            while b"\n" in buf:
                raw, buf = buf.split(b"\n", 1)
                if not raw.strip():
                    continue
                try:
                    event = json.loads(raw.decode("utf-8"))
                except json.JSONDecodeError:
                    print(raw.decode("utf-8", errors="replace"))
                    continue
                self.handle_event(event)

    def handle_event(self, event):
        event_type = event.get("type")
        if event_type == "text":
            self.clear_partial_preview()
            self.latest_text = event.get("text", "")
            delta = event.get("delta", "")
            if delta:
                print(delta, end="", flush=True)
        elif event_type == "partial":
            self.partial_text = event.get("text", "")
            if sys.stdout.isatty():
                print(f"\r\033[2K[partial] {self.partial_text}", end="", flush=True)
            else:
                print(f"[partial] {self.partial_text}", flush=True)
        elif event_type == "final":
            self.clear_partial_preview()
            self.final_text = event.get("text", "")
            print("\n[final]")
            print(self.final_text)
        elif event_type == "status":
            self.phase = event.get("phase", self.phase)
            emit_bridge_phase(self.phase)
            if self.phase == "recording":
                self.recording_ready.set()
            print(f"[event] {event}", file=sys.stderr)
        elif event_type == "voice_state":
            print(
                "[voice_state] "
                f"{event.get('label')} "
                f"input={event.get('selectedInputSourceID')} "
                f"match={event.get('selectedInputSourceMatchesDoubao')} "
                f"trusted={event.get('accessibilityTrusted')} "
                f"windows={event.get('doubaoWindowCount')} "
                f"keywords={event.get('keywordHits')} "
                f"likely={event.get('likelyVoiceUIActive')}",
                file=sys.stderr,
            )
        elif event_type == "audio_level":
            print(
                "[audio_level] "
                f"{event.get('label')} "
                f"bytes={event.get('bytes')} "
                f"packets={event.get('packets')} "
                f"rms={event.get('rmsDBFS'):.1f}dBFS "
                f"peak={event.get('peakDBFS'):.1f}dBFS",
                file=sys.stderr,
            )
        elif event_type == "focus_state":
            focused = event.get("focusedElement") or {}
            app = focused.get("app") or {}
            ax = focused.get("ax") or {}
            capture = event.get("capture") or {}
            value = (ax.get("value") or "").replace("\n", "\\n")
            selected = (ax.get("selectedText") or "").replace("\n", "\\n")
            print(
                "[focus_state] "
                f"{event.get('label')} "
                f"app={app.get('bundleIdentifier')} "
                f"role={ax.get('role')} "
                f"value={value[:80]!r} "
                f"selected={selected[:80]!r} "
                f"captureLen={capture.get('textLength')} "
                f"marked={capture.get('hasMarkedText')}",
                file=sys.stderr,
            )
        elif event_type == "error":
            if event.get("phase") == "voice_activation_failed":
                self.activation_failed = True
                self.recording_ready.set()
                emit_bridge_phase("voice_activation_failed")
            print(f"[{event_type}] {event}", file=sys.stderr)
        elif event_type == "auth":
            print(f"[{event_type}] {event}", file=sys.stderr)
        else:
            print(f"[event] {event}", file=sys.stderr)

    def clear_partial_preview(self):
        if not self.partial_text:
            return
        if sys.stdout.isatty():
            print("\r\033[2K", end="", flush=True)
        self.partial_text = ""

    def close(self):
        self._stop.set()
        if self.sock:
            try:
                self.sock.close()
            except OSError:
                pass


def format_input_devices(device_names):
    return "\n".join(f"  - {name}" for name in device_names)


def parse_dshow_audio_devices(output):
    devices = []
    for line in output.splitlines():
        match = re.search(r'\]\s+"(.*)"\s+\(audio\)\s*$', line)
        if match and match.group(1) not in devices:
            devices.append(match.group(1))
    return devices


def list_windows_audio_devices(ffmpeg):
    try:
        result = subprocess.run(
            [ffmpeg, "-hide_banner", "-list_devices", "true", "-f", "dshow", "-i", "dummy"],
            capture_output=True,
            encoding="utf-8",
            errors="replace",
            check=False,
        )
    except OSError as exc:
        raise SystemExit(f"Could not enumerate Windows audio input devices with {ffmpeg}: {exc}") from exc

    return parse_dshow_audio_devices(f"{result.stdout}\n{result.stderr}")


def windows_input_args(device_names, requested_device):
    if not device_names:
        raise SystemExit("No Windows DirectShow audio input devices were found.")

    if requested_device:
        matches = [name for name in device_names if name.casefold() == requested_device.casefold()]
        if not matches:
            raise SystemExit(
                f"Windows audio input device not found: {requested_device}\n"
                f"Available audio input devices:\n{format_input_devices(device_names)}"
            )
        selected = matches[0]
    elif len(device_names) == 1:
        selected = device_names[0]
    else:
        raise SystemExit(
            "Multiple Windows audio input devices were found. "
            "Select one with --input-device NAME:\n"
            f"{format_input_devices(device_names)}"
        )

    return ["-f", "dshow", "-i", f"audio={selected}"]


def default_input_args(ffmpeg="ffmpeg", input_device=None):
    system = platform.system().lower()
    if system == "linux":
        if input_device:
            raise SystemExit("--input-device currently supports Windows only; use --input-args on Linux")
        return ["-f", "pulse", "-i", "default"]
    if system == "darwin":
        if input_device:
            raise SystemExit("--input-device currently supports Windows only; use --input-args on macOS")
        return ["-f", "avfoundation", "-i", ":0"]
    if system == "windows":
        return windows_input_args(list_windows_audio_devices(ffmpeg), input_device)
    raise SystemExit(f"Unsupported platform for default microphone args: {platform.system()}")


def resolved_input_args(args):
    configured_sources = [
        option
        for option, value in (
            ("--input-args", args.input_args),
            ("--input-device", args.input_device),
            ("--input-file", args.input_file),
        )
        if value
    ]
    if len(configured_sources) > 1:
        raise SystemExit(f"{', '.join(configured_sources)} cannot be used together")
    if args.input_file:
        loop_args = ["-stream_loop", "-1"] if args.loop_input else []
        return ["-re", *loop_args, "-i", args.input_file]
    if args.input_args:
        return shlex.split(args.input_args)
    return default_input_args(ffmpeg=args.ffmpeg, input_device=args.input_device)


class DirectFFmpegAudioStream:
    def __init__(self, process, live_release_tail=False):
        self.process = process
        self.live_release_tail = live_release_tail

    @classmethod
    def start(cls, args, output_url, input_args, live_release_tail=False):
        cmd = ffmpeg_capture_cmd(args, input_args, output_url)
        print("[ffmpeg]", " ".join(shlex.quote(part) for part in cmd), file=sys.stderr)
        return cls(subprocess.Popen(cmd), live_release_tail=live_release_tail)

    def stop_microphone(self):
        self.process.terminate()
        try:
            self.process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            self.process.kill()

    def send_silence(self, seconds):
        if seconds > 0:
            time.sleep(seconds)

    def send_tail(self, seconds):
        pass

    def close(self):
        pass


class PCMLevelMeter:
    def __init__(self, interval=0.08):
        self.interval = interval
        self.last_emit = None
        self.carry = b""
        self.sum_squares = 0
        self.sample_count = 0
        self.peak = 0

    def push(self, chunk, now=None):
        data = self.carry + chunk
        if len(data) % 2:
            self.carry = data[-1:]
            data = data[:-1]
        else:
            self.carry = b""

        if data:
            samples = array.array("h")
            samples.frombytes(data)
            if sys.byteorder != "little":
                samples.byteswap()
            self.sum_squares += sum(sample * sample for sample in samples)
            self.sample_count += len(samples)
            self.peak = max(self.peak, max((abs(sample) for sample in samples), default=0))

        if self.sample_count == 0:
            return None

        now = time.monotonic() if now is None else now
        if self.last_emit is not None and now - self.last_emit < self.interval:
            return None

        rms = math.sqrt(self.sum_squares / self.sample_count)
        rms_dbfs = 20 * math.log10(rms / 32768.0) if rms > 0 else -120.0
        peak_dbfs = 20 * math.log10(self.peak / 32768.0) if self.peak > 0 else -120.0
        self.last_emit = now
        self.sum_squares = 0
        self.sample_count = 0
        self.peak = 0
        return max(rms_dbfs, -120.0), max(peak_dbfs, -120.0)


class TCPAudioStream:
    def __init__(self, process, audio_socket, pump_thread, tail_limit):
        self.process = process
        self.audio_socket = audio_socket
        self.pump_thread = pump_thread
        self.send_lock = threading.Lock()
        self.tail_lock = threading.Lock()
        self.tail_limit = tail_limit
        self.tail = collections.deque()
        self.tail_size = 0

    @classmethod
    def start(cls, args, host, port, input_args):
        deadline = time.time() + args.audio_connect_timeout
        last_error = None
        while True:
            try:
                audio_socket = socket.create_connection((host, port), timeout=2)
                break
            except OSError as exc:
                last_error = exc
                if time.time() >= deadline:
                    raise last_error
                time.sleep(0.05)
        audio_socket.settimeout(None)
        cmd = ffmpeg_capture_cmd(args, input_args, "-")
        print("[ffmpeg]", " ".join(shlex.quote(part) for part in cmd), file=sys.stderr)
        process = subprocess.Popen(cmd, stdout=subprocess.PIPE)
        stream = cls(process, audio_socket, None, int(48000 * 2 * args.audio_tail_buffer))
        stream.pump_thread = threading.Thread(target=stream._pump_stdout, daemon=True)
        stream.pump_thread.start()
        return stream

    def _pump_stdout(self):
        meter = PCMLevelMeter()
        while True:
            chunk = self.process.stdout.read(4096)
            if not chunk:
                return
            level = meter.push(chunk)
            if level:
                rms_dbfs, peak_dbfs = level
                print(
                    f"[local_audio_level] rms={rms_dbfs:.1f}dBFS peak={peak_dbfs:.1f}dBFS",
                    file=sys.stderr,
                    flush=True,
                )
            self._remember_tail(chunk)
            try:
                with self.send_lock:
                    self.audio_socket.sendall(chunk)
            except OSError:
                return

    def _remember_tail(self, chunk):
        if self.tail_limit <= 0:
            return
        with self.tail_lock:
            self.tail.append(chunk)
            self.tail_size += len(chunk)
            while self.tail_size > self.tail_limit and self.tail:
                removed = self.tail.popleft()
                self.tail_size -= len(removed)

    def _recent_tail(self, seconds):
        wanted = int(48000 * 2 * seconds)
        if wanted <= 0:
            return b""
        with self.tail_lock:
            data = b"".join(self.tail)
        if len(data) <= wanted:
            return data
        return data[-wanted:]

    def stop_microphone(self):
        self.process.terminate()
        try:
            self.process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            self.process.kill()
        if self.pump_thread:
            self.pump_thread.join(timeout=2)

    def send_silence(self, seconds):
        if seconds <= 0:
            return
        bytes_per_second = 48000 * 2
        chunk = b"\x00" * 1920
        deadline = time.time() + seconds
        while time.time() < deadline:
            try:
                with self.send_lock:
                    self.audio_socket.sendall(chunk)
            except OSError:
                return
            time.sleep(len(chunk) / bytes_per_second)

    def send_tail(self, seconds):
        data = self._recent_tail(seconds)
        if not data:
            return
        try:
            with self.send_lock:
                self.audio_socket.sendall(data)
        except OSError:
            return

    def close(self):
        try:
            self.audio_socket.shutdown(socket.SHUT_WR)
        except OSError:
            pass
        try:
            self.audio_socket.close()
        except OSError:
            pass


def ffmpeg_capture_cmd(args, input_args, output_url):
    return [
        args.ffmpeg,
        "-hide_banner",
        "-loglevel", "warning",
        *input_args,
        "-vn",
        "-ac", "1",
        "-ar", "48000",
        "-f", "s16le",
        output_url,
    ]


def start_audio_stream(args, udp_host, udp_port, input_args):
    if args.audio_transport == "udp":
        output_url = f"udp://{udp_host}:{udp_port}?pkt_size=960"
        return DirectFFmpegAudioStream.start(args, output_url, input_args)
    elif args.audio_transport == "tcp":
        if not args.python_tcp_audio:
            output_url = f"tcp://{udp_host}:{udp_port}"
            return DirectFFmpegAudioStream.start(args, output_url, input_args, live_release_tail=True)
        return TCPAudioStream.start(args, udp_host, udp_port, input_args)
    else:
        raise SystemExit(f"Unsupported audio transport: {args.audio_transport}")


def paste_text(text):
    if not text:
        return

    system = platform.system().lower()
    if system == "darwin":
        subprocess.run(["pbcopy"], input=text.encode("utf-8"), check=True)
        subprocess.run(["osascript", "-e", 'tell application "System Events" to keystroke "v" using command down'], check=False)
        return

    if system == "linux":
        if shutil.which("wl-copy"):
            subprocess.run(["wl-copy"], input=text.encode("utf-8"), check=True)
        elif shutil.which("xclip"):
            subprocess.run(["xclip", "-selection", "clipboard"], input=text.encode("utf-8"), check=True)
        elif shutil.which("xsel"):
            subprocess.run(["xsel", "--clipboard", "--input"], input=text.encode("utf-8"), check=True)
        else:
            raise SystemExit("No clipboard tool found. Install wl-copy, xclip, or xsel.")

        if shutil.which("ydotool"):
            subprocess.run(["ydotool", "key", "29:1", "47:1", "47:0", "29:0"], check=False)
        elif shutil.which("xdotool"):
            subprocess.run(["xdotool", "key", "ctrl+v"], check=False)
        else:
            print("Clipboard updated, but no ydotool/xdotool was found for paste.", file=sys.stderr)
        return

    if system == "windows":
        ps_set = (
            "[Console]::InputEncoding = [System.Text.UTF8Encoding]::new($false); "
            "Set-Clipboard -Value ([Console]::In.ReadToEnd())"
        )
        subprocess.run(["powershell", "-NoProfile", "-Command", ps_set], input=text.encode("utf-8"), check=True)
        ps_paste = (
            "Add-Type -AssemblyName System.Windows.Forms; "
            "[System.Windows.Forms.SendKeys]::SendWait('^v')"
        )
        subprocess.run(["powershell", "-NoProfile", "-Command", ps_paste], check=False)
        return

    raise SystemExit(f"Paste is not implemented for {platform.system()}")


def command_record(args):
    udp_host = args.udp_host or args.server.rsplit(":", 1)[0]
    input_args = resolved_input_args(args)
    client = BridgeClient(args.server, token=args.token)
    client.connect()

    listener = threading.Thread(target=client.listen, daemon=True)
    listener.start()

    client.send("start")
    if args.audio_start_delay > 0:
        time.sleep(args.audio_start_delay)
    audio = start_audio_stream(args, udp_host, args.udp_port, input_args)

    try:
        if args.wait_recording:
            if not client.recording_ready.wait(timeout=args.recording_timeout):
                raise RuntimeError(f"Timed out waiting for recording phase after {args.recording_timeout}s")
            if client.activation_failed:
                raise RuntimeError("Doubao voice activation failed")
        if args.seconds is None:
            if args.stdin_stop:
                print("[recording] Waiting for stdin stop signal.", file=sys.stderr)
                sys.stdin.readline()
            else:
                print("[recording] Press Ctrl+C to stop.", file=sys.stderr)
                while True:
                    time.sleep(1)
        else:
            time.sleep(args.seconds)
    except KeyboardInterrupt:
        pass
    except RuntimeError as exc:
        print(f"[error] {exc}", file=sys.stderr)
    finally:
        if args.release_before_stop_microphone or getattr(audio, "live_release_tail", False):
            client.send("stop")
            if args.audio_stop_delay > 0:
                time.sleep(args.audio_stop_delay)
            audio.stop_microphone()
        else:
            audio.stop_microphone()
            audio.send_tail(args.audio_pre_stop_tail)
            audio.send_silence(args.audio_pre_stop_silence)
            client.send("stop")
            audio.send_silence(args.audio_stop_delay)
        audio.close()

    deadline = time.time() + args.final_timeout
    while time.time() < deadline and not client.final_text:
        time.sleep(0.05)

    final_text = client.final_text or client.latest_text
    if args.paste and final_text:
        paste_text(final_text)

    client.close()


def command_send(args):
    client = BridgeClient(args.server, token=args.token)
    client.connect()
    client.send(args.command)
    time.sleep(0.2)
    client.close()


def build_parser():
    parser = argparse.ArgumentParser(description="Remote client for doubao-bridge-mac")
    parser.add_argument("--server", default="127.0.0.1:4387", help="Mac bridge TCP host:port")
    parser.add_argument("--token", help="Optional bridge auth token")
    parser.add_argument("--ffmpeg", default="ffmpeg", help="ffmpeg executable")
    parser.add_argument("--udp-host", help="Mac UDP audio host. Defaults to --server host")
    parser.add_argument("--udp-port", type=int, default=5004, help="Mac UDP audio port")
    parser.add_argument("--audio-transport", choices=["udp", "tcp"], default="udp", help="Raw PCM audio transport")
    parser.add_argument("--input-args", help="ffmpeg input args for the local microphone")
    parser.add_argument("--input-device", help="Windows DirectShow audio input device name")
    parser.add_argument("--input-file", help="Use a local audio file instead of the microphone")
    parser.add_argument("--loop-input", action=argparse.BooleanOptionalAction, default=True, help="Loop --input-file while recording")

    subparsers = parser.add_subparsers(dest="command_name", required=True)

    record = subparsers.add_parser("record", help="Record until Ctrl+C or for a fixed duration")
    record.add_argument(
        "--seconds",
        type=float,
        help="Stop after this many seconds. Omit to record until Ctrl+C.",
    )
    record.add_argument("--final-timeout", type=float, default=4.0)
    record.add_argument("--audio-start-delay", type=float, default=0.0)
    record.add_argument("--audio-connect-timeout", type=float, default=5.0)
    record.add_argument("--audio-tail-buffer", type=float, default=1.0)
    record.add_argument("--audio-pre-stop-tail", type=float, default=0.5)
    record.add_argument("--audio-pre-stop-silence", type=float, default=0.0)
    record.add_argument("--audio-stop-delay", type=float, default=0.2)
    record.add_argument("--wait-recording", action=argparse.BooleanOptionalAction, default=True)
    record.add_argument("--recording-timeout", type=float, default=8.0)
    record.add_argument("--release-before-stop-microphone", action="store_true")
    record.add_argument("--python-tcp-audio", action="store_true", help="Experimental: proxy TCP audio through Python instead of ffmpeg direct TCP")
    record.add_argument("--stdin-stop", action="store_true", help="Stop when stdin receives a line or closes")
    record.add_argument("--paste", action="store_true", help="Paste final recognized text into active app")
    record.set_defaults(func=command_record)

    send = subparsers.add_parser("send", help="Send a raw bridge control command")
    send.add_argument("command", choices=["start", "stop", "toggle", "clear", "status", "diagnose", "voice-state", "focus-state", "test-hotkey", "settings"])
    send.set_defaults(func=command_send)

    return parser


def parse_args(argv=None):
    return build_parser().parse_args(argv)


def main():
    args = parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
