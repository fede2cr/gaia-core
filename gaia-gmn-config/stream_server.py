"""
gaia-gmn-config · MJPEG Camera Stream Server

A single-file HTTP server that captures from a V4L2 camera via ffmpeg and
serves the result as a multipart MJPEG stream.  Designed to run inside a
container for GMN camera pre-alignment.

Endpoints:
  GET /           → simple status page (JSON)
  GET /stream     → MJPEG multipart stream (for <img> tags)
  GET /snapshot   → single JPEG frame
"""

import io
import os
import signal
import subprocess
import sys
import threading
import time
from http.server import HTTPServer, BaseHTTPRequestHandler
from socketserver import ThreadingMixIn

# ── Configuration from environment ──────────────────────────────────────

VIDEO_DEVICE = os.environ.get("VIDEO_DEVICE", "/dev/video0")
STREAM_PORT = int(os.environ.get("STREAM_PORT", "8181"))
RESOLUTION = os.environ.get("RESOLUTION", "640x480")
FRAMERATE = os.environ.get("FRAMERATE", "10")

BOUNDARY = b"--gaiaframe"

# ── Shared frame buffer ─────────────────────────────────────────────────

class FrameBuffer:
    """Thread-safe holder for the latest JPEG frame."""

    def __init__(self):
        self.frame = None
        self.lock = threading.Lock()
        self.event = threading.Event()

    def update(self, jpeg_bytes: bytes):
        with self.lock:
            self.frame = jpeg_bytes
        self.event.set()
        self.event.clear()

    def get(self) -> bytes | None:
        with self.lock:
            return self.frame

    def wait(self, timeout=2.0) -> bool:
        return self.event.wait(timeout)


frame_buf = FrameBuffer()

# ── ffmpeg capture thread ────────────────────────────────────────────────

def probe_native_mjpeg(device: str) -> bool:
    """Check if the V4L2 device natively supports MJPEG output.

    Uses v4l2-ctl to list supported pixel formats.  If MJPEG (or MJPG)
    is among them, we can tell ffmpeg to request it directly instead of
    having it transcode from raw YUYV/NV12, which avoids white-screen
    issues on many USB cameras (e.g. Arducam on Raspberry Pi).
    """
    try:
        result = subprocess.run(
            ["v4l2-ctl", "--device", device, "--list-formats"],
            capture_output=True, text=True, timeout=5,
        )
        output = result.stdout.upper()
        return "MJPG" in output or "MJPEG" in output
    except Exception as e:
        print(f"[gaia-gmn-config] v4l2-ctl probe failed: {e}", flush=True)
        return False


def capture_thread():
    """Launch ffmpeg, read JPEG frames from pipe, update the shared buffer."""
    width, height = RESOLUTION.split("x")

    native_mjpeg = probe_native_mjpeg(VIDEO_DEVICE)
    if native_mjpeg:
        print(f"[gaia-gmn-config] Camera supports native MJPEG, using -input_format mjpeg",
              flush=True)

    cmd = [
        "ffmpeg",
        "-f", "v4l2",
    ]
    # If the camera natively supports MJPEG, request it directly.
    # This avoids transcoding and prevents white-screen issues.
    if native_mjpeg:
        cmd += ["-input_format", "mjpeg"]
    cmd += [
        "-video_size", RESOLUTION,
        "-framerate", FRAMERATE,
        "-i", VIDEO_DEVICE,
        # Output JPEG frames to pipe
        "-f", "image2pipe",
        "-vcodec", "mjpeg",
        "-q:v", "5",
        "pipe:1",
    ]

    print(f"[gaia-gmn-config] Starting ffmpeg: {' '.join(cmd)}", flush=True)

    proc = subprocess.Popen(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        bufsize=0,
    )

    # Log ffmpeg stderr in a separate thread so errors are visible.
    def _log_stderr():
        for line in proc.stderr:
            text = line.decode("utf-8", errors="replace").rstrip()
            if text:
                print(f"[ffmpeg] {text}", flush=True)

    stderr_thread = threading.Thread(target=_log_stderr, daemon=True)
    stderr_thread.start()

    try:
        buf = b""
        SOI = b"\xff\xd8"  # JPEG Start Of Image
        EOI = b"\xff\xd9"  # JPEG End Of Image

        while True:
            chunk = proc.stdout.read(4096)
            if not chunk:
                break
            buf += chunk

            # Extract complete JPEG frames from the buffer.
            while True:
                soi = buf.find(SOI)
                if soi == -1:
                    buf = b""
                    break

                eoi = buf.find(EOI, soi + 2)
                if eoi == -1:
                    # Incomplete frame, trim everything before SOI and wait.
                    buf = buf[soi:]
                    break

                # Complete JPEG frame: SOI .. EOI+2
                frame = buf[soi : eoi + 2]
                buf = buf[eoi + 2 :]
                frame_buf.update(frame)

    except Exception as e:
        print(f"[gaia-gmn-config] Capture error: {e}", flush=True)
    finally:
        proc.terminate()
        proc.wait()
        print("[gaia-gmn-config] ffmpeg process exited", flush=True)

# ── HTTP handler ─────────────────────────────────────────────────────────

class StreamHandler(BaseHTTPRequestHandler):
    """Serves the MJPEG stream and status info."""

    def log_message(self, format, *args):
        # Suppress default noisy logging.
        pass

    def do_GET(self):
        if self.path == "/stream":
            self.send_mjpeg_stream()
        elif self.path == "/snapshot":
            self.send_snapshot()
        else:
            self.send_status()

    def send_status(self):
        has_frame = frame_buf.get() is not None
        body = (
            '{"service":"gaia-gmn-config",'
            f'"device":"{VIDEO_DEVICE}",'
            f'"resolution":"{RESOLUTION}",'
            f'"framerate":{FRAMERATE},'
            f'"streaming":{str(has_frame).lower()}}}'
        ).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Access-Control-Allow-Origin", "*")
        self.end_headers()
        self.wfile.write(body)

    def send_snapshot(self):
        frame = frame_buf.get()
        if frame is None:
            self.send_error(503, "No frame available yet")
            return
        self.send_response(200)
        self.send_header("Content-Type", "image/jpeg")
        self.send_header("Content-Length", str(len(frame)))
        self.send_header("Access-Control-Allow-Origin", "*")
        self.end_headers()
        self.wfile.write(frame)

    def send_mjpeg_stream(self):
        self.send_response(200)
        self.send_header(
            "Content-Type",
            "multipart/x-mixed-replace; boundary=gaiaframe",
        )
        self.send_header("Cache-Control", "no-cache, no-store, must-revalidate")
        self.send_header("Pragma", "no-cache")
        self.send_header("Access-Control-Allow-Origin", "*")
        self.end_headers()

        last_frame = None
        try:
            while True:
                frame = frame_buf.get()
                # Only send when a new frame is available.
                if frame is not None and frame is not last_frame:
                    self.wfile.write(BOUNDARY + b"\r\n")
                    self.wfile.write(b"Content-Type: image/jpeg\r\n")
                    self.wfile.write(
                        f"Content-Length: {len(frame)}\r\n\r\n".encode()
                    )
                    self.wfile.write(frame)
                    self.wfile.write(b"\r\n")
                    self.wfile.flush()
                    last_frame = frame
                else:
                    # Wait briefly for a new frame to avoid busy-looping.
                    frame_buf.wait(timeout=0.5)
        except (BrokenPipeError, ConnectionResetError):
            pass  # Client disconnected, normal.

# ── Main ─────────────────────────────────────────────────────────────────

class ThreadingHTTPServer(ThreadingMixIn, HTTPServer):
    """Handle each request in a new thread so the MJPEG stream doesn't block
    other endpoints (status, snapshot, additional stream viewers)."""
    daemon_threads = True


def main():
    print(f"[gaia-gmn-config] device={VIDEO_DEVICE} port={STREAM_PORT} "
          f"res={RESOLUTION} fps={FRAMERATE}", flush=True)

    # Start capture in a background daemon thread.
    t = threading.Thread(target=capture_thread, daemon=True)
    t.start()

    # Give ffmpeg a moment to produce the first frame.
    time.sleep(1)

    server = ThreadingHTTPServer(("0.0.0.0", STREAM_PORT), StreamHandler)
    print(f"[gaia-gmn-config] HTTP server listening on :{STREAM_PORT}", flush=True)

    # Graceful shutdown on SIGTERM.
    def shutdown(sig, frame):
        print("[gaia-gmn-config] Shutting down...", flush=True)
        server.shutdown()
        sys.exit(0)

    signal.signal(signal.SIGTERM, shutdown)
    signal.signal(signal.SIGINT, shutdown)

    server.serve_forever()


if __name__ == "__main__":
    main()
