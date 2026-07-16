import json
import io
from pathlib import Path
import socket
import subprocess
import sys
import threading
import time
import unittest
from unittest import mock

from clients import doubao_remote


class TTYBuffer(io.StringIO):
    def isatty(self):
        return True


class RecordArgumentsTests(unittest.TestCase):
    def test_record_without_seconds_has_no_duration_limit(self):
        args = doubao_remote.parse_args(["record"])

        self.assertIsNone(args.seconds)

    def test_record_accepts_a_fixed_duration(self):
        args = doubao_remote.parse_args(["record", "--seconds", "12.5"])

        self.assertEqual(args.seconds, 12.5)

    def test_record_accepts_stdin_stop_control(self):
        args = doubao_remote.parse_args(["record", "--stdin-stop"])

        self.assertTrue(args.stdin_stop)


class BridgeClientEventTests(unittest.TestCase):
    def test_partial_text_replaces_the_terminal_preview(self):
        client = doubao_remote.BridgeClient("127.0.0.1:4387")
        output = TTYBuffer()

        with mock.patch("sys.stdout", output):
            client.handle_event({"type": "partial", "text": "hello"})
            client.handle_event({"type": "partial", "text": "hello world"})

        self.assertEqual(client.partial_text, "hello world")
        self.assertEqual(
            output.getvalue(),
            "\r\x1b[2K[partial] hello\r\x1b[2K[partial] hello world",
        )

    def test_committed_text_clears_the_partial_preview(self):
        client = doubao_remote.BridgeClient("127.0.0.1:4387")
        output = TTYBuffer()

        with mock.patch("sys.stdout", output):
            client.handle_event({"type": "partial", "text": "hel"})
            client.handle_event({"type": "text", "text": "hello", "delta": "hello"})

        self.assertEqual(client.partial_text, "")
        self.assertEqual(
            output.getvalue(),
            "\r\x1b[2K[partial] hel\r\x1b[2Khello",
        )


class WindowsInputDeviceTests(unittest.TestCase):
    def test_cli_accepts_an_input_device_name(self):
        args = doubao_remote.parse_args(["--input-device", "USB Headset", "record"])

        self.assertEqual(args.input_device, "USB Headset")

    def test_parses_only_directshow_audio_device_names(self):
        output = """
[in#0 @ 000001] "HP HD Camera" (video)
[in#0 @ 000001]   Alternative name "@device_pnp_camera"
[in#0 @ 000001] "Microphone Array" (audio)
[in#0 @ 000001]   Alternative name "@device_cm_microphone"
[in#0 @ 000001] "USB Headset" (audio)
"""

        self.assertEqual(
            doubao_remote.parse_dshow_audio_devices(output),
            ["Microphone Array", "USB Headset"],
        )

    def test_multiple_devices_without_selection_lists_every_name(self):
        with self.assertRaises(SystemExit) as raised:
            doubao_remote.windows_input_args(["Desk Microphone", "USB Headset"], None)

        message = str(raised.exception)
        self.assertIn("Desk Microphone", message)
        self.assertIn("USB Headset", message)
        self.assertIn("--input-device", message)

    def test_single_device_is_selected_automatically(self):
        self.assertEqual(
            doubao_remote.windows_input_args(["Microphone Array"], None),
            ["-f", "dshow", "-i", "audio=Microphone Array"],
        )

    def test_requested_device_is_selected_by_name(self):
        self.assertEqual(
            doubao_remote.windows_input_args(["Desk Microphone", "USB Headset"], "usb headset"),
            ["-f", "dshow", "-i", "audio=USB Headset"],
        )


class PasteTextTests(unittest.TestCase):
    def test_windows_clipboard_reads_utf8_input(self):
        with (
            mock.patch.object(doubao_remote.platform, "system", return_value="Windows"),
            mock.patch.object(doubao_remote.subprocess, "run") as run,
        ):
            doubao_remote.paste_text("你好")

        clipboard_call = run.call_args_list[0]
        self.assertIn("UTF8Encoding", clipboard_call.args[0][-1])
        self.assertEqual(clipboard_call.kwargs["input"], "你好".encode("utf-8"))
        self.assertTrue(clipboard_call.kwargs["check"])


class RecordCommandTests(unittest.TestCase):
    def test_input_device_errors_happen_before_connecting_to_the_bridge(self):
        args = doubao_remote.parse_args(["record"])

        with (
            mock.patch.object(doubao_remote, "resolved_input_args", side_effect=SystemExit("choose a device")),
            mock.patch.object(doubao_remote, "BridgeClient") as bridge_client,
            self.assertRaisesRegex(SystemExit, "choose a device"),
        ):
            doubao_remote.command_record(args)

        bridge_client.assert_not_called()

    def test_record_stops_cleanly_from_stdin_control(self):
        listener = socket.socket()
        listener.bind(("127.0.0.1", 0))
        listener.listen(1)
        listener.settimeout(3)
        host, port = listener.getsockname()
        start_seen = threading.Event()
        stop_seen = threading.Event()

        def serve_bridge():
            connection, _ = listener.accept()
            with connection, connection.makefile("r", encoding="utf-8") as lines:
                connection.sendall(b'{"type":"hello","authRequired":false}\n')
                try:
                    for raw_line in lines:
                        command = raw_line.strip()
                        if command == "start":
                            start_seen.set()
                            event = {"type": "status", "phase": "recording", "recording": True}
                            connection.sendall((json.dumps(event) + "\n").encode())
                        elif command == "stop":
                            stop_seen.set()
                            event = {"type": "final", "text": "interrupted recording"}
                            connection.sendall((json.dumps(event) + "\n").encode())
                except ConnectionResetError:
                    pass

        server_thread = threading.Thread(target=serve_bridge, daemon=True)
        server_thread.start()
        script = Path(__file__).parents[1] / "clients" / "doubao_remote.py"
        process = subprocess.Popen(
            [
                sys.executable,
                str(script),
                "--server", f"{host}:{port}",
                "--ffmpeg", sys.executable,
                "--audio-transport", "tcp",
                "--input-file", str(script),
                "record",
                "--audio-start-delay", "0",
                "--recording-timeout", "1",
                "--final-timeout", "1",
                "--stdin-stop",
            ],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )

        try:
            self.assertTrue(start_seen.wait(2), "client did not start recording")
            time.sleep(0.1)
            self.assertIsNone(process.poll(), "client exited before stdin stop")
            process.stdin.write("stop\n")
            process.stdin.flush()
            stdout, stderr = process.communicate(timeout=3)
        finally:
            listener.close()
            if process.poll() is None:
                process.kill()
                process.wait()

        self.assertEqual(process.returncode, 0, stderr)
        self.assertTrue(stop_seen.is_set(), "client did not send stop after stdin control")
        self.assertIn("[final]\ninterrupted recording", stdout)


if __name__ == "__main__":
    unittest.main()
