"""Capture the actual weather renderer and verify animation in isolated offline GUI profiles."""

import argparse
import json
import tempfile
import time
from pathlib import Path

import tomllib
from capture_ui import USER, Session
from PIL import Image, ImageChops, ImageStat


def capture(args, kind, lighting, bare=True, paused=False, reduced=False, record=False):
    name = f"weather-design-{kind}-{lighting}"
    if not bare:
        name += "-ui"
    if paused or reduced:
        name += "-paused" if paused else "-reduced"
    with tempfile.TemporaryDirectory(
        prefix="sd-weather-motion-", dir=args.temp_root
    ) as temp:
        profile = Path(temp)
        settings = {
            "appearance": {"theme": "dark", "season": "off"},
            "weather": {"cinematic": False, "lighting": lighting},
            "background": {"paused": paused},
            "reduced_motion": reduced,
            "experience": {"guide_seen": True, "last_seen_version": args.version},
        }
        (profile / "settings.json").write_text(json.dumps(settings), encoding="utf-8")
        (profile / "preview.json").write_text(
            json.dumps({"kind": kind, "bare": bare}), encoding="utf-8"
        )
        session = Session(args.exe, profile, args.output)
        try:
            time.sleep(2)
            first = session.snapshot()
            time.sleep(1.2)
            second = session.snapshot()
            difference = (
                sum(ImageStat.Stat(ImageChops.difference(first, second)).mean) / 3
            )
            if paused or reduced:
                assert difference == 0, f"Paused scene moved: {difference}"
            else:
                assert difference > 0.1, f"Scene movement is not visible: {difference}"
            second.save(args.output / f"{name}.png", optimize=True)
            if not bare:
                USER.SetWindowPos(session.window, None, 50, 40, 1040, 739, 0x54)
                time.sleep(0.5)
                session.capture(f"{name}-narrow")
            if record:
                frames = []
                start = time.monotonic()
                for i in range(60):
                    frames.append(
                        session.snapshot().resize((768, 504), Image.Resampling.LANCZOS)
                    )
                    time.sleep(max(0, start + (i + 1) / 12 - time.monotonic()))
                palette = frames[0].quantize(colors=128)
                encoded = [
                    frame.quantize(palette=palette, dither=Image.Dither.NONE)
                    for frame in frames
                ]
                encoded[0].save(
                    args.output / f"{name}.gif",
                    save_all=True,
                    append_images=encoded[1:],
                    duration=83,
                    loop=0,
                )
            print(
                f"{name}: mean frame difference {difference:.3f}, {'stopped' if paused or reduced else 'moving'}",
                flush=True,
            )
        finally:
            session.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--exe", type=Path, required=True, help="Developer-only weather-preview.exe"
    )
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--temp-root", type=Path, required=True)
    args = parser.parse_args()
    assert args.output.is_dir() and args.temp_root.is_dir()
    root = Path(__file__).resolve().parents[1]
    args.version = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))[
        "package"
    ]["version"]
    for mode in ("day", "evening", "dawn", "dusk", "night"):
        capture(args, "clear", mode)
    for kind in (
        "clouds",
        "rain",
        "downpour",
        "hail",
        "snow",
        "blizzard",
        "fog",
        "storm",
    ):
        capture(args, kind, "day", record=kind == "rain")
    capture(args, "clear", "day", bare=False)
    capture(args, "clear", "night", bare=False)
    capture(args, "rain", "night", paused=True)
    capture(args, "snow", "day", reduced=True)


if __name__ == "__main__":
    main()
