"""Check whole-window video placement using a two-color clip and an isolated, silent profile."""

import argparse
import ctypes as ct
import json
import time
from ctypes import wintypes as wt
from pathlib import Path

from capture_ui import USER, Session
from ui_scenarios import preferences, wait_preferences


def scroll(session: Session, steps: int) -> None:
    screen = wt.POINT(1100, 600)
    USER.ClientToScreen(session.window, ct.byref(screen))
    USER.SendMessageW(session.window, 0x0200, 0, 1100 | (600 << 16))
    USER.SendMessageW(
        session.window,
        0x020A,
        ((steps * 120) & 0xFFFF) << 16,
        (screen.x & 0xFFFF) | ((screen.y & 0xFFFF) << 16),
    )
    time.sleep(0.5)


def assert_full_window(session: Session, name: str) -> None:
    image = session.snapshot()
    for x in (10, image.width - 10):
        for y, channel in [(image.height // 4, 0), (image.height * 3 // 4, 2)]:
            pixel = image.getpixel((x, y))
            other = max(value for index, value in enumerate(pixel) if index != channel)
            assert pixel[channel] > other + 12, (
                f"{name}: wallpaper missing/misaligned at {(x, y)}: {pixel}"
            )
    session.capture(name)


def fixture_scenario(session: Session) -> None:
    for name, y in [
        ("catalog", 151),
        ("favorites", 201),
        ("installed", 252),
        ("queue", 305),
        ("settings", 773),
    ]:
        session.click(100, y)
        assert_full_window(session, f"wallpaper-fixture-{name}")
    session.click(493, 142)
    before = session.snapshot().crop((224, 190, 248, 810))
    scroll(session, -12)
    assert_full_window(session, "wallpaper-fixture-scrolled")
    after = session.snapshot().crop((224, 190, 248, 810))
    assert before.tobytes() == after.tobytes(), "Scrolling moved the wallpaper"
    scroll(session, 24)
    session.click(100, 151)
    session.search("vscode")
    session.click(365, 345)
    assert_full_window(session, "wallpaper-fixture-details")
    USER.SetWindowPos(session.window, None, 50, 40, 1040, 739, 0x54)
    time.sleep(0.8)
    assert_full_window(session, "wallpaper-fixture-narrow")
    USER.SetWindowPos(session.window, None, 50, 40, 1296, 879, 0x54)
    time.sleep(0.5)
    session.key(0x1B)
    print(
        "Whole-window coverage verified on all pages, with details, scrolling and resizing"
    )


def landscape_scenario(session: Session, video: Path) -> None:
    session.click(100, 773)
    session.click(303, 142)
    session.click(276, 510)
    wait_preferences(
        session, lambda value: not value["reduced_motion"], "Motion did not enable"
    )
    session.click(493, 142)
    scroll(session, 24)
    session.click(350, 290)
    session.choose_file(video)
    wait_preferences(
        session,
        lambda value: value["background"]["video"] == str(video.resolve()),
        "Landscape did not load",
    )
    time.sleep(1)
    crop = (15, 340, 210, 680)
    first = session.snapshot().crop(crop)
    time.sleep(0.8)
    assert first.tobytes() != session.snapshot().crop(crop).tobytes(), (
        "Sidebar video did not advance"
    )
    session.capture("wallpaper-settings")
    session.click(614, 413)
    wait_preferences(
        session, lambda value: value["background"]["paused"], "Pause did not persist"
    )
    time.sleep(0.5)
    first = session.snapshot().crop(crop)
    time.sleep(0.6)
    assert first.tobytes() == session.snapshot().crop(crop).tobytes(), (
        "Paused sidebar changed"
    )
    scroll(session, -12)
    session.capture("wallpaper-scrolled")
    scroll(session, 24)
    session.click(100, 151)
    session.key(0x1B)
    session.capture("wallpaper-catalog")
    session.click(100, 773)
    session.click(303, 142)
    session.click(387, 258)
    wait_preferences(
        session,
        lambda value: value["appearance"]["theme"] == "light",
        "Light theme did not persist",
    )
    session.capture("wallpaper-light")
    session.click(303, 258)
    session.click(493, 142)
    session.click(490, 290)
    wait_preferences(
        session,
        lambda value: value["background"]["video"] is None,
        "Video did not clear",
    )
    time.sleep(0.5)
    session.capture("wallpaper-removed")
    print(
        "Real landscape verified behind the sidebar and cards; pause, light theme and removal checked"
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--profile", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--video", type=Path)
    args = parser.parse_args()
    if (
        args.profile.exists()
        or not args.profile.parent.is_dir()
        or not args.output.is_dir()
    ):
        parser.error(
            "Use a new isolated profile in an existing parent and an existing output folder"
        )
    fixture = Path(__file__).resolve().parents[1] / "tests/fixtures/video-colors.mp4"
    args.profile.mkdir()
    (args.profile / "settings.json").write_text(
        json.dumps(
            {
                "appearance": {"theme": "dark", "season": "off"},
                "reduced_motion": True,
                "background": {
                    "video": str(fixture),
                    "paused": True,
                    "dimming": 72,
                    "effects": False,
                },
                "sound": {"enabled": False, "volume": 0},
            }
        ),
        encoding="utf-8",
    )
    session = Session(args.exe, args.profile, args.output)
    try:
        time.sleep(30)
        fixture_scenario(session)
        if args.video:
            landscape_scenario(session, args.video)
        assert preferences(session)["sound"]["volume"] == 0
        session.close()
    finally:
        if session.process.poll() is None:
            session.capture("wallpaper-failure")
            session.process.terminate()
            session.process.wait(timeout=10)


if __name__ == "__main__":
    main()
