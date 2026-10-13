"""Capture onboarding, release notes, seasons and live weather in isolated Windows profiles."""

import argparse
import hashlib
import json
import tempfile
import time
from pathlib import Path

import tomllib
from capture_ui import USER, Session
from prepare_ui_assets import download


def scenario(exe, output, temp_root, name, settings, weather_page=False):
    with tempfile.TemporaryDirectory(
        prefix="sd-experience-", dir=temp_root
    ) as temporary:
        profile = Path(temporary)
        (profile / "settings.json").write_text(json.dumps(settings), encoding="utf-8")
        session = Session(exe, profile, output)
        try:
            time.sleep(35 if name == "weather-live" else 10)
            session.capture(f"{name}-0.7")
            if weather_page:
                session.click(95, 422)
                time.sleep(2)
                session.capture("weather-panel-0.7")
            USER.SetWindowPos(session.window, None, 50, 40, 1040, 739, 0x54)
            time.sleep(1)
            session.capture(f"{name}-narrow-0.7")
        finally:
            session.close()
    print(f"Captured {name}", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--temp-root", type=Path, required=True)
    parser.add_argument(
        "--interact",
        action="store_true",
        help="Also navigate to the weather page; requires an unlocked desktop",
    )
    args = parser.parse_args()
    assert args.output.is_dir() and args.temp_root.is_dir()
    root = Path(__file__).resolve().parents[1]
    version = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))[
        "package"
    ]["version"]
    base = {
        "appearance": {"theme": "dark", "season": "off"},
        "experience": {"guide_seen": True, "last_seen_version": version},
    }
    cases = [
        (
            "guide",
            {**base, "experience": {"guide_seen": False, "last_seen_version": version}},
        ),
        (
            "whats-new",
            {**base, "experience": {"guide_seen": True, "last_seen_version": "0.6.1"}},
        ),
        (
            "halloween",
            {**base, "appearance": {"theme": "midnight", "season": "halloween"}},
        ),
        (
            "weather-live",
            {
                **base,
                "weather": {
                    "enabled": True,
                    "city": {
                        "name": "Москва",
                        "country": "Россия",
                        "latitude": 55.75204,
                        "longitude": 37.61781,
                    },
                },
            },
        ),
    ]
    for name, settings in cases:
        scenario(
            args.exe,
            args.output,
            args.temp_root,
            name,
            settings,
            args.interact and name == "weather-live",
        )
    preset = next(
        p
        for p in json.loads(
            (root / "catalog/backgrounds.json").read_text(encoding="utf-8")
        )
        if p["id"] == "winter-forest"
    )
    data = download(preset["url"])
    assert (
        len(data) == preset["size"]
        and hashlib.sha256(data).hexdigest() == preset["sha256"]
    )
    with tempfile.TemporaryDirectory(
        prefix="sd-snow-scene-", dir=args.temp_root
    ) as temporary:
        clip = Path(temporary) / "snow.mp4"
        clip.write_bytes(data)
        scenario(
            args.exe,
            args.output,
            args.temp_root,
            "snow",
            {
                **base,
                "appearance": {"theme": "dark", "season": "winter"},
                "background": {"video": str(clip), "dimming": 58},
            },
        )


if __name__ == "__main__":
    main()
