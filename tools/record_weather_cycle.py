"""Record smooth weather/time-of-day transitions using the real offline preview renderer."""

import argparse
import json
import tempfile
import time
from pathlib import Path

import tomllib
from capture_ui import Session
from ui_movie import Movie


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--temp-root", type=Path, required=True)
    args = parser.parse_args()
    assert args.output.is_dir() and args.temp_root.is_dir()
    version = tomllib.loads(
        (Path(__file__).resolve().parents[1] / "Cargo.toml").read_text(encoding="utf-8")
    )["package"]["version"]
    with tempfile.TemporaryDirectory(
        prefix="sd-weather-cycle-", dir=args.temp_root
    ) as temp:
        profile = Path(temp)
        (profile / "settings.json").write_text(
            json.dumps(
                {
                    "weather": {"lighting": "day", "cinematic": False},
                    "appearance": {"theme": "dark", "season": "off"},
                    "experience": {"guide_seen": True, "last_seen_version": version},
                }
            ),
            encoding="utf-8",
        )
        (profile / "preview.json").write_text(
            json.dumps({"kind": "rain", "cycle": True}), encoding="utf-8"
        )
        start = time.monotonic()
        session = Session(args.exe, profile, args.output)
        print(
            f"Native preview window ready in {time.monotonic() - start:.2f}s",
            flush=True,
        )
        movie = None
        try:
            session.caption = "Ускоренная демонстрация переходов. Авто в клиенте следует реальному времени города."
            movie = Movie(session, args.output / "weather-transitions.mp4")
            started = time.monotonic()
            for second in (2, 7, 12, 17, 22, 27, 32):
                time.sleep(max(0, started + second - time.monotonic()))
                session.capture(f"weather-transition-{second:02}")
            time.sleep(max(0, started + 36 - time.monotonic()))
        finally:
            try:
                if movie is not None:
                    movie.close()
            finally:
                session.close()
        print("Recorded 36 seconds of live, continuous scene transitions", flush=True)


if __name__ == "__main__":
    main()
