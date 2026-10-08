"""Record only the tested application window, with explanatory captions."""

import os
from pathlib import Path
import subprocess
import threading
import time
from typing import TYPE_CHECKING

from PIL import Image, ImageDraw, ImageFont

if TYPE_CHECKING:
    from capture_ui import Session


class Movie:
    def __init__(self, session: "Session", output: Path) -> None:
        self.session = session
        self.finished = threading.Event()
        self.error: Exception | None = None
        width, height = session.snapshot().size
        self.size = width, height + 76
        self.font = ImageFont.truetype(str(Path(os.environ["WINDIR"]) / "Fonts" / "segoeui.ttf"), 19)
        self.process = subprocess.Popen([
            "ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-f", "rawvideo",
            "-pixel_format", "rgb24", "-video_size", f"{width}x{height + 76}",
            "-framerate", "24", "-i", "-", "-an", "-c:v", "libx264",
            "-preset", "veryfast", "-crf", "22", "-pix_fmt", "yuv420p", "-movflags", "+faststart", str(output),
        ], stdin=subprocess.PIPE)
        self.thread = threading.Thread(target=self.record, daemon=True)
        self.thread.start()

    def record(self) -> None:
        start = time.monotonic()
        written = 0
        try:
            while not self.finished.is_set():
                screen = self.session.snapshot()
                frame = Image.new("RGB", self.size, (21, 25, 30))
                frame.paste(screen)
                draw = ImageDraw.Draw(frame)
                draw.line([(0, screen.height), (frame.width, screen.height)], fill=(52, 61, 71))
                draw.text((32, screen.height + 24), self.session.caption, font=self.font, fill=(134, 185, 232))
                data = frame.tobytes()
                due = max(written + 1, int((time.monotonic() - start) * 24) + 1)
                for _ in range(due - written):
                    self.process.stdin.write(data)
                written = due
                self.finished.wait(max(0, start + written / 24 - time.monotonic()))
        except Exception as error:
            self.error = error

    def close(self) -> None:
        self.finished.set()
        self.thread.join(timeout=15)
        if self.thread.is_alive():
            self.process.kill()
            raise TimeoutError("Video recording did not stop")
        self.process.stdin.close()
        code = self.process.wait(timeout=30)
        if self.error:
            raise self.error
        if code:
            raise RuntimeError(f"FFmpeg exit code: {code}")
