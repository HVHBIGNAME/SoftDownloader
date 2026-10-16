"""Capture real queue/settings widgets with explicitly synthetic, non-installable preview data."""

import argparse
import ctypes
import json
import tempfile
import time
from ctypes import wintypes
from pathlib import Path

from capture_ui import USER, Session


def scroll_preview(window):
    rect = wintypes.RECT()
    if not USER.GetClientRect(window, ctypes.byref(rect)):
        raise ctypes.WinError()
    point = wintypes.POINT(int(rect.right * 0.65), int(rect.bottom * 0.65))
    USER.PostMessageW.argtypes = [
        wintypes.HWND,
        wintypes.UINT,
        wintypes.WPARAM,
        wintypes.LPARAM,
    ]
    if not USER.PostMessageW(window, 0x0200, 0, (point.y << 16) | point.x):
        raise ctypes.WinError()
    if not USER.ClientToScreen(window, ctypes.byref(point)):
        raise ctypes.WinError()
    wheel = ((-720) & 0xFFFF) << 16
    position = ((point.y & 0xFFFF) << 16) | (point.x & 0xFFFF)
    if not USER.PostMessageW(window, 0x020A, wheel, position):
        raise ctypes.WinError()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--temp-root", type=Path, required=True)
    parser.add_argument(
        "--scenarios",
        nargs="+",
        default=[
            "mixed",
            "success",
            "cancelled",
            "settings",
            "catalog",
            "catalog_list",
        ],
    )
    parser.add_argument("--zoom", type=float, default=1.0)
    parser.add_argument("--light", action="store_true")
    parser.add_argument("--scroll", action="store_true")
    args = parser.parse_args()
    if not args.output.is_dir() or not args.temp_root.is_dir():
        raise ValueError("Output and temporary directories must exist")
    suffix = ("-light" if args.light else "") + (
        f"-zoom{args.zoom:g}" if args.zoom != 1 else ""
    )
    for scenario in args.scenarios:
        with tempfile.TemporaryDirectory(
            prefix="sd-client-preview-", dir=args.temp_root
        ) as temp:
            profile = Path(temp)
            (profile / "scenario.json").write_text(
                json.dumps(
                    {"scenario": scenario, "zoom": args.zoom, "light": args.light}
                ),
                encoding="utf-8",
            )
            session = Session(args.exe, profile, args.output)
            try:
                time.sleep(1)
                session.capture(f"client-{scenario}{suffix}")
                USER.SetWindowPos(session.window, None, 50, 40, 1040, 739, 0x54)
                time.sleep(0.5)
                session.capture(f"client-{scenario}{suffix}-narrow")
                if args.scroll:
                    scroll_preview(session.window)
                    time.sleep(0.7)
                    session.capture(f"client-{scenario}{suffix}-scrolled")
            finally:
                session.close()
    print(
        f"Captured {len(args.scenarios)} scenarios at 1280x840 and 1024x700, "
        f"zoom={args.zoom:g}, light={args.light}"
    )


if __name__ == "__main__":
    main()
