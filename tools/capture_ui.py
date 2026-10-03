"""Capture the real Windows UI. Requires Pillow; the walkthrough also needs FFmpeg.

Only this launched process receives input. An isolated profile is mandatory.
The walkthrough stops at the installation confirmation and never runs installers.
"""

import argparse
import ctypes as ct
from ctypes import wintypes as wt
import json
import os
from pathlib import Path
import subprocess
import threading
import time

from PIL import Image, ImageDraw, ImageFont


USER = ct.WinDLL("user32", use_last_error=True)
CALLBACK = ct.WINFUNCTYPE(wt.BOOL, wt.HWND, wt.LPARAM)
USER.EnumWindows.argtypes = [CALLBACK, wt.LPARAM]
USER.EnumChildWindows.argtypes = [wt.HWND, CALLBACK, wt.LPARAM]
USER.GetWindowThreadProcessId.argtypes = [wt.HWND, ct.POINTER(wt.DWORD)]
USER.GetWindowTextW.argtypes = [wt.HWND, wt.LPWSTR, ct.c_int]
USER.GetClassNameW.argtypes = [wt.HWND, wt.LPWSTR, ct.c_int]
USER.GetClientRect.argtypes = [wt.HWND, ct.POINTER(wt.RECT)]
USER.SetWindowPos.argtypes = [wt.HWND, wt.HWND, ct.c_int, ct.c_int, ct.c_int, ct.c_int, wt.UINT]
USER.SetForegroundWindow.argtypes = [wt.HWND]
USER.PostMessageW.argtypes = [wt.HWND, wt.UINT, wt.WPARAM, wt.LPARAM]
USER.SendMessageW.argtypes = [wt.HWND, wt.UINT, wt.WPARAM, wt.LPARAM]
USER.SendMessageW.restype = wt.LPARAM
USER.IsWindowVisible.argtypes = [wt.HWND]
USER.IsWindowEnabled.argtypes = [wt.HWND]
USER.GetDlgItem.argtypes = [wt.HWND, ct.c_int]
USER.GetDlgItem.restype = wt.HWND
USER.GetDlgCtrlID.argtypes = [wt.HWND]
USER.SetProcessDPIAware()
GDI = ct.WinDLL("gdi32", use_last_error=True)
USER.GetDC.argtypes = [wt.HWND]
USER.GetDC.restype = wt.HDC
USER.ReleaseDC.argtypes = [wt.HWND, wt.HDC]
USER.PrintWindow.argtypes = [wt.HWND, wt.HDC, wt.UINT]
GDI.CreateCompatibleDC.argtypes = [wt.HDC]
GDI.CreateCompatibleDC.restype = wt.HDC
GDI.SelectObject.argtypes = [wt.HDC, wt.HANDLE]
GDI.SelectObject.restype = wt.HANDLE
GDI.DeleteObject.argtypes = [wt.HANDLE]
GDI.DeleteDC.argtypes = [wt.HDC]


class BitmapInfo(ct.Structure):
    _fields_ = [
        ("size", wt.DWORD), ("width", wt.LONG), ("height", wt.LONG),
        ("planes", wt.WORD), ("bits", wt.WORD), ("compression", wt.DWORD),
        ("image_size", wt.DWORD), ("xppm", wt.LONG), ("yppm", wt.LONG),
        ("used", wt.DWORD), ("important", wt.DWORD), ("palette", wt.DWORD),
    ]


GDI.CreateDIBSection.argtypes = [wt.HDC, ct.POINTER(BitmapInfo), wt.UINT, ct.POINTER(ct.c_void_p), wt.HANDLE, wt.DWORD]
GDI.CreateDIBSection.restype = wt.HANDLE


def render_window(window: int) -> Image.Image:
    rect = wt.RECT()
    if not USER.GetClientRect(window, ct.byref(rect)):
        raise RuntimeError("Window is closed")
    screen = USER.GetDC(window)
    memory = GDI.CreateCompatibleDC(screen)
    info = BitmapInfo(size=40, width=rect.right, height=-rect.bottom, planes=1, bits=32)
    pixels = ct.c_void_p()
    bitmap = GDI.CreateDIBSection(screen, ct.byref(info), 0, ct.byref(pixels), None, 0)
    if not bitmap:
        GDI.DeleteDC(memory)
        USER.ReleaseDC(window, screen)
        raise RuntimeError("Could not allocate a window capture bitmap")
    previous = GDI.SelectObject(memory, bitmap)
    try:
        if not USER.PrintWindow(window, memory, 3):
            raise RuntimeError("PrintWindow failed")
        GDI.GdiFlush()
        raw = ct.string_at(pixels, rect.right * rect.bottom * 4)
        return Image.frombytes("RGB", (rect.right, rect.bottom), raw, "raw", "BGRX")
    finally:
        GDI.SelectObject(memory, previous)
        GDI.DeleteObject(bitmap)
        GDI.DeleteDC(memory)
        USER.ReleaseDC(window, screen)


def text(window: int, class_name: bool = False) -> str:
    value = ct.create_unicode_buffer(1024)
    (USER.GetClassNameW if class_name else USER.GetWindowTextW)(window, value, len(value))
    return value.value


def windows(pid: int, parent: int | None = None) -> list[int]:
    result = []

    @CALLBACK
    def collect(window: int, _: int) -> bool:
        owner = wt.DWORD()
        USER.GetWindowThreadProcessId(window, ct.byref(owner))
        if owner.value == pid and USER.IsWindowVisible(window):
            result.append(window)
        return True

    if parent:
        USER.EnumChildWindows(parent, collect, 0)
    else:
        USER.EnumWindows(collect, 0)
    return result


class Session:
    def __init__(self, exe: Path, profile: Path, output: Path) -> None:
        self.output = output
        self.profile = profile
        self.render_lock = threading.RLock()
        self.process = subprocess.Popen([str(exe), "--data-dir", str(profile)])
        self.window = self.wait_window("SoftDownloader")
        USER.SetWindowPos(self.window, wt.HWND(-1), 50, 40, 1296, 879, 0)
        USER.SetForegroundWindow(self.window)
        self.caption = "Каталог программ для Windows"

    def wait_window(self, title: str = "", class_name: str = "", timeout: float = 20) -> int:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise RuntimeError(f"Application exited: {self.process.returncode}")
            for window in windows(self.process.pid):
                if (not title or text(window) == title) and (not class_name or text(window, True) == class_name):
                    return window
            time.sleep(0.1)
        raise TimeoutError(f"Window not found: {title or class_name}")

    def snapshot(self) -> Image.Image:
        with self.render_lock:
            return render_window(self.window)

    def capture(self, name: str) -> None:
        image = self.snapshot()
        red, green, blue = image.getpixel((image.width - 10, image.height // 2))
        if not 3 <= red <= green <= blue <= 70:
            raise RuntimeError("Capture did not contain the application surface")
        image.save(self.output / f"{name}.png", optimize=True)

    def click(self, x: int, y: int) -> None:
        point = (y << 16) | x
        with self.render_lock:
            USER.PostMessageW(self.window, 0x200, 0, point)
            USER.PostMessageW(self.window, 0x201, 1, point)
            USER.PostMessageW(self.window, 0x202, 0, point)
            time.sleep(0.15)
        time.sleep(0.45)

    def choose_file(self, path: Path) -> None:
        dialog = self.wait_window(class_name="#32770")
        USER.SetWindowPos(dialog, wt.HWND(-1), 185, 155, 980, 640, 0)
        edits = [child for child in windows(self.process.pid, dialog)
                 if text(child, True) == "Edit" and USER.IsWindowEnabled(child)]
        if not edits:
            raise RuntimeError("Native dialog filename field was not found")
        value = ct.c_wchar_p(str(path.resolve()))
        USER.SendMessageW(edits[-1], 0x000C, 0, ct.cast(value, ct.c_void_p).value)
        time.sleep(0.5)
        button = USER.GetDlgItem(dialog, 1)
        if not button:
            raise RuntimeError("Native dialog confirmation button was not found")
        USER.PostMessageW(button, 0x00F5, 0, 0)
        deadline = time.monotonic() + 15
        while dialog in windows(self.process.pid) and time.monotonic() < deadline:
            time.sleep(0.1)
        if dialog in windows(self.process.pid):
            raise RuntimeError("Native file dialog did not close")
        time.sleep(1)

    def click_primary(self) -> None:
        image = self.snapshot()
        run = None
        for y in range(420, image.height - 8, 2):
            start = None
            for x in range(250, image.width - 10):
                r, g, b = image.getpixel((x, y))
                accent = 160 < r < 215 and g > 210 and 70 < b < 155
                if accent and start is None:
                    start = x
                if not accent and start is not None:
                    if x - start >= 90:
                        run = ((start + x) // 2, y)
                    start = None
        if run is None:
            raise RuntimeError("No enabled primary action found")
        print(f"Primary action at {run[0]}, {run[1] - 12}")
        self.click(run[0], run[1] - 12)

    def close(self) -> None:
        USER.PostMessageW(self.window, 0x10, 0, 0)
        try:
            code = self.process.wait(timeout=20)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            raise
        if code:
            raise RuntimeError(f"Application exit code: {code}")

    def transfer_scenario(self, example: Path) -> None:
        exported = self.profile / f"programs-{time.time_ns()}.json"
        self.caption = "1. Откройте установленные программы. Экспорт сохранит полный список."
        self.click(100, 183)
        self.capture("installed")
        time.sleep(2)
        self.caption = "2. Экспорт списка: имена, версии, издатели и ID программ в одном JSON-файле."
        self.click(1186, 43)
        self.choose_file(exported)
        self.capture("export")
        document = json.loads(exported.read_text(encoding="utf-8"))
        if document.get("format") != "softdownloader.program-list" or not document["programs"]:
            raise RuntimeError("The exported program list is invalid")
        for entry in document["programs"]:
            if set(entry) != {"name", "version", "publisher", "package_ids"}:
                raise RuntimeError("The exported list contains unexpected fields")
        print(f"Exported {len(document['programs'])} programs, without machine-specific data")
        time.sleep(2)
        self.caption = "3. На другом ПК нажмите «Импорт списка» и выберите сохранённый JSON."
        self.click(1050, 43)
        self.choose_file(example)
        self.caption = "4. В примере уже установленное пропускается. Доступные программы можно отметить."
        self.capture("restore")
        time.sleep(3)
        self.click_primary()
        self.caption = "5. Список добавлен к выбору. При необходимости измените состав программ."
        time.sleep(1)
        self.capture("selection")
        time.sleep(2)
        self.click_primary()
        self.caption = "6. Проверьте план. Установка начнётся только после вашего подтверждения."
        time.sleep(1)
        self.capture("confirmation")
        image = self.snapshot()
        if image.getpixel((image.width - 10, image.height // 2))[0] >= 14:
            raise RuntimeError("Installation confirmation did not open")
        time.sleep(3)


class Movie:
    def __init__(self, session: Session, output: Path) -> None:
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
                frame = Image.new("RGB", self.size, (18, 21, 24))
                frame.paste(screen)
                draw = ImageDraw.Draw(frame)
                draw.line([(0, screen.height), (frame.width, screen.height)], fill=(46, 52, 57))
                draw.text((32, screen.height + 24), self.session.caption, font=self.font, fill=(188, 239, 119))
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


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--profile", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--wait", type=int, default=55)
    parser.add_argument("--transfer", type=Path, help="Import this example after testing export")
    parser.add_argument("--video", type=Path, help="Record the client window to MP4 (requires FFmpeg)")
    args = parser.parse_args()
    if not args.exe.is_file() or not args.profile.parent.is_dir() or not args.output.is_dir():
        parser.error("Executable and output/profile parent folders must exist")
    session = Session(args.exe, args.profile, args.output)
    movie = None
    try:
        time.sleep(args.wait)
        if args.video:
            if not args.video.parent.is_dir():
                parser.error("Video parent directory must exist")
            movie = Movie(session, args.video)
        session.capture("catalog")
        time.sleep(2)
        session.caption = "Группы и подгруппы помогают быстро найти нужный софт."
        session.click(105, 529)
        session.capture("categories")
        time.sleep(2)
        session.click(100, 183)
        time.sleep(1)
        session.capture("installed")
        session.click(100, 779)
        session.caption = "Google Диск встроен для будущих дополнений. Основной каталог работает самостоятельно."
        time.sleep(1)
        session.capture("settings")
        if args.transfer:
            session.transfer_scenario(args.transfer)
        if movie:
            movie.close()
            movie = None
        session.close()
        print(f"Screens captured in {args.output}; app closed normally")
    finally:
        if movie:
            movie.close()
        if session.process.poll() is None:
            session.process.terminate()
            session.process.wait(timeout=10)


if __name__ == "__main__":
    main()
