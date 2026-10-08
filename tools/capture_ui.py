"""Capture the real Windows UI. Requires Pillow; the walkthrough also needs FFmpeg.

Only this launched process receives input. An isolated profile is mandatory.
The walkthrough stops at the installation confirmation and never runs installers.
"""

import argparse
import ctypes as ct
from ctypes import wintypes as wt
import logging
from pathlib import Path
import subprocess
import threading
import time

from PIL import Image

from ui_movie import Movie


USER = ct.WinDLL("user32", use_last_error=True)
CALLBACK = ct.WINFUNCTYPE(wt.BOOL, wt.HWND, wt.LPARAM)
USER.EnumWindows.argtypes = [CALLBACK, wt.LPARAM]
USER.EnumChildWindows.argtypes = [wt.HWND, CALLBACK, wt.LPARAM]
USER.GetWindowThreadProcessId.argtypes = [wt.HWND, ct.POINTER(wt.DWORD)]
USER.GetWindowTextW.argtypes = [wt.HWND, wt.LPWSTR, ct.c_int]
USER.GetClassNameW.argtypes = [wt.HWND, wt.LPWSTR, ct.c_int]
USER.GetClientRect.argtypes = [wt.HWND, ct.POINTER(wt.RECT)]
USER.SetWindowPos.argtypes = [wt.HWND, wt.HWND, ct.c_int, ct.c_int, ct.c_int, ct.c_int, wt.UINT]
USER.GetWindowLongPtrW.argtypes = [wt.HWND, ct.c_int]
USER.GetWindowLongPtrW.restype = ct.c_ssize_t
USER.SetForegroundWindow.argtypes = [wt.HWND]
USER.BringWindowToTop.argtypes = [wt.HWND]
USER.ClientToScreen.argtypes = [wt.HWND, ct.POINTER(wt.POINT)]
USER.SetCursorPos.argtypes = [ct.c_int, ct.c_int]
USER.WindowFromPoint.argtypes = [wt.POINT]
USER.WindowFromPoint.restype = wt.HWND
USER.GetForegroundWindow.restype = wt.HWND
USER.AttachThreadInput.argtypes = [wt.DWORD, wt.DWORD, wt.BOOL]
KERNEL = ct.WinDLL("kernel32", use_last_error=True)
KERNEL.GetCurrentThreadId.restype = wt.DWORD
USER.ShowWindow.argtypes = [wt.HWND, ct.c_int]
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


class MouseInput(ct.Structure):
    _fields_ = [("dx", wt.LONG), ("dy", wt.LONG), ("data", wt.DWORD),
                ("flags", wt.DWORD), ("time", wt.DWORD), ("extra", ct.c_size_t)]


class KeyboardInput(ct.Structure):
    _fields_ = [("virtual", wt.WORD), ("scan", wt.WORD), ("flags", wt.DWORD),
                ("time", wt.DWORD), ("extra", ct.c_size_t)]


class InputData(ct.Union):
    _fields_ = [("mouse", MouseInput), ("keyboard", KeyboardInput)]


class Input(ct.Structure):
    _fields_ = [("type", wt.DWORD), ("data", InputData)]


USER.SendInput.argtypes = [wt.UINT, ct.POINTER(Input), ct.c_int]
USER.SendInput.restype = wt.UINT


def key_button(code: int, flags: int = 0, unicode: bool = False) -> None:
    keyboard = KeyboardInput(virtual=0 if unicode else code, scan=code if unicode else 0, flags=flags | (4 if unicode else 0))
    event = Input(type=1, data=InputData(keyboard=keyboard))
    if USER.SendInput(1, ct.byref(event), ct.sizeof(event)) != 1:
        raise ct.WinError(ct.get_last_error())


def focus_window(window: int) -> None:
    foreground = USER.GetForegroundWindow()
    if foreground == window or USER.SetForegroundWindow(window):
        return
    current = KERNEL.GetCurrentThreadId()
    owner = USER.GetWindowThreadProcessId(foreground, None)
    target = USER.GetWindowThreadProcessId(window, None)
    attached = USER.AttachThreadInput(current, owner, True)
    target_attached = USER.AttachThreadInput(current, target, True)
    try:
        USER.BringWindowToTop(window)
        USER.SetForegroundWindow(window)
        time.sleep(0.1)
    finally:
        if target_attached:
            USER.AttachThreadInput(current, target, False)
        if attached:
            USER.AttachThreadInput(current, owner, False)


def render_window(window: int) -> Image.Image:
    rect = wt.RECT()
    if not USER.GetClientRect(window, ct.byref(rect)):
        raise RuntimeError("Window is closed")
    if rect.right <= 0 or rect.bottom <= 0:
        USER.ShowWindow(window, 9)
        USER.SetWindowPos(window, None, 50, 40, 1296, 879, 0x54)
        time.sleep(0.3)
        if not USER.GetClientRect(window, ct.byref(rect)) or rect.right <= 0 or rect.bottom <= 0:
            raise RuntimeError("Window has no drawable client area")
    screen = USER.GetDC(window)
    memory = GDI.CreateCompatibleDC(screen)
    info = BitmapInfo(size=40, width=rect.right, height=-rect.bottom, planes=1, bits=32)
    pixels = ct.c_void_p()
    bitmap = GDI.CreateDIBSection(screen, ct.byref(info), 0, ct.byref(pixels), None, 0)
    if not bitmap:
        error = ct.get_last_error()
        GDI.DeleteDC(memory)
        USER.ReleaseDC(window, screen)
        raise RuntimeError(f"Could not allocate {rect.right}x{rect.bottom} capture bitmap: Windows error {error}")
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
        from ui_scenarios import silent_profile

        silent_profile(profile)
        self.output = output
        self.profile = profile
        self.render_lock = threading.RLock()
        self.process = subprocess.Popen([str(exe.resolve()), "--data-dir", str(profile.resolve())], cwd=exe.resolve().parent)
        try:
            self.window = self.wait_window("SoftDownloader")
            self.assert_normal_window()
        except Exception:
            self.process.terminate()
            self.process.wait(timeout=10)
            raise
        USER.ShowWindow(self.window, 9)
        USER.SetWindowPos(self.window, None, 50, 40, 1296, 879, 0x54)
        focus_window(self.window)
        USER.SendMessageW(self.window, 0x0006, 1, 0)
        USER.SendMessageW(self.window, 0x0007, 0, 0)
        self.caption = "Каталог программ для Windows"

    def assert_normal_window(self) -> None:
        if USER.GetWindowLongPtrW(self.window, -20) & 0x00000008:
            raise RuntimeError("Application window unexpectedly has WS_EX_TOPMOST")

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
            self.assert_normal_window()
            return render_window(self.window)

    def capture(self, name: str) -> None:
        image = self.snapshot()
        if max(high - low for low, high in image.getextrema()) < 32:
            raise RuntimeError("Capture contains a blank application surface")
        image.save(self.output / f"{name}.png", optimize=True)

    def click(self, x: int, y: int) -> None:
        with self.render_lock:
            for _ in range(4):
                focus_window(self.window)
                screen = wt.POINT(x, y)
                if not USER.ClientToScreen(self.window, ct.byref(screen)):
                    raise ct.WinError(ct.get_last_error())
                if not USER.SetCursorPos(screen.x, screen.y):
                    raise ct.WinError(ct.get_last_error())
                time.sleep(0.2)
                if USER.WindowFromPoint(screen) == self.window:
                    break
                time.sleep(0.5)
            if USER.WindowFromPoint(screen) != self.window:
                hit = USER.WindowFromPoint(screen)
                raise RuntimeError(f"Target application is covered by {text(hit, True)}; refusing to click another window")
            position = (x & 0xFFFF) | ((y & 0xFFFF) << 16)
            for message, buttons in [(0x0200, 0), (0x0201, 1), (0x0200, 1), (0x0202, 0)]:
                USER.SendMessageW(self.window, message, buttons, position)
        time.sleep(0.45)

    def key(self, code: int, control: bool = False) -> None:
        with self.render_lock:
            if USER.GetForegroundWindow() != self.window:
                raise RuntimeError("Target application is not focused; refusing keyboard input")
            try:
                if control:
                    key_button(0x11)
                key_button(code)
                time.sleep(0.04)
                key_button(code, 2)
            finally:
                if control:
                    key_button(0x11, 2)
        time.sleep(0.25)

    def search(self, query: str) -> None:
        self.key(ord("F"), control=True)
        self.key(ord("A"), control=True)
        self.key(0x08)
        encoded = query.encode("utf-16-le")
        with self.render_lock:
            for index in range(0, len(encoded), 2):
                if USER.GetForegroundWindow() != self.window:
                    raise RuntimeError("Target application lost keyboard focus")
                unit = int.from_bytes(encoded[index:index + 2], "little")
                key_button(unit, unicode=True)
                key_button(unit, 2, unicode=True)
                time.sleep(0.02)
        time.sleep(0.7)

    def choose_file(self, path: Path) -> None:
        dialog = self.wait_window(class_name="#32770")
        USER.SetWindowPos(dialog, None, 185, 155, 980, 640, 0x14)
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
        focus_window(self.window)
        time.sleep(1)

    def click_primary(self) -> None:
        from ui_scenarios import preferences

        appearance = preferences(self).get("appearance")
        color = appearance.get("accent", [134, 185, 232]) if appearance is not None else [134, 185, 232]
        image = self.snapshot()
        run = None
        for y in range(420, image.height - 8, 2):
            start = None
            for x in range(250, image.width - 10):
                pixel = image.getpixel((x, y))
                accent = all(abs(actual - wanted) <= 3 for actual, wanted in zip(pixel, color))
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

    def transfer_scenario(self, example: Path, review: bool = False) -> None:
        from ui_scenarios import transfer
        transfer(self, example, review)


def main() -> None:
    logging.basicConfig(level=logging.INFO, format="%(message)s")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--profile", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--wait", type=int, default=55)
    parser.add_argument("--transfer", type=Path, help="Import this example after testing export")
    parser.add_argument("--video", type=Path, help="Record the client window to MP4 (requires FFmpeg)")
    parser.add_argument("--features", action="store_true", help="Exercise favorites, layouts, custom sets, undo and removal preview")
    parser.add_argument("--appearance", action="store_true", help="Verify themes, sound settings and real video download/playback/pause")
    parser.add_argument("--check-layout", action="store_true", help="Also capture the minimum-size window after the walkthrough")
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
        from ui_scenarios import features, layout, narrow_window, removal_preview

        layout(session, "list")
        session.capture("catalog")
        time.sleep(2)
        session.caption = "Группы и подгруппы помогают быстро найти нужный софт."
        session.click(600, 170)
        session.capture("categories")
        session.key(0x1B)
        time.sleep(2)
        if args.features:
            features(session)
            removal_preview(session)
        if args.appearance:
            from ui_appearance_scenarios import appearance
            appearance(session)
        session.click(100, 252)
        time.sleep(1)
        session.capture("installed")
        session.click(100, 773)
        session.caption = "Google Диск встроен для будущих дополнений. Основной каталог работает самостоятельно."
        time.sleep(1)
        session.capture("settings")
        if args.transfer:
            session.transfer_scenario(args.transfer, review=args.features)
        if movie:
            movie.close()
            movie = None
        if args.check_layout:
            narrow_window(session)
        session.close()
        print(f"Screens captured in {args.output}; app closed normally")
    except Exception:
        if session.process.poll() is None:
            session.capture("failure")
        raise
    finally:
        if movie:
            movie.close()
        if session.process.poll() is None:
            session.process.terminate()
            session.process.wait(timeout=10)


if __name__ == "__main__":
    main()
