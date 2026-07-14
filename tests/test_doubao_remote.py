import json
from pathlib import Path
import signal
import socket
import subprocess
import sys
import threading
import time
import unittest

from clients import doubao_remote


class RecordArgumentsTests(unittest.TestCase):
    def test_record_without_seconds_has_no_duration_limit(self):
        args = doubao_remote.parse_args(["record"])

        self.assertIsNone(args.seconds)

    def test_record_accepts_a_fixed_duration(self):
        args = doubao_remote.parse_args(["record", "--seconds", "12.5"])

        self.assertEqual(args.seconds, 12.5)


class RecordCommandTests(unittest.TestCase):
    def test_record_without_seconds_stops_cleanly_on_sigint(self):
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

        server_thread = threading.Thread(target=serve_bridge, daemon=True)
        server_thread.start()
        script = Path(__file__).parents[1] / "clients" / "doubao_remote.py"
        process = subprocess.Popen(
            [
                sys.executable,
                str(script),
                "--server", f"{host}:{port}",
                "--ffmpeg", "/usr/bin/true",
                "--audio-transport", "tcp",
                "record",
                "--audio-start-delay", "0",
                "--recording-timeout", "1",
                "--final-timeout", "1",
            ],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )

        try:
            self.assertTrue(start_seen.wait(2), "client did not start recording")
            time.sleep(0.1)
            self.assertIsNone(process.poll(), "client exited before Ctrl+C")
            process.send_signal(signal.SIGINT)
            stdout, stderr = process.communicate(timeout=3)
        finally:
            listener.close()
            if process.poll() is None:
                process.kill()
                process.wait()

        self.assertEqual(process.returncode, 0, stderr)
        self.assertTrue(stop_seen.is_set(), "client did not send stop after Ctrl+C")
        self.assertIn("[final]\ninterrupted recording", stdout)


if __name__ == "__main__":
    unittest.main()
