"""Prepare README media from reviewed capture_ui output. Requires Pillow and FFmpeg."""

import argparse
from pathlib import Path
import subprocess

from PIL import Image


SCREENSHOTS = (
    "catalog", "categories", "confirmation", "export", "installed", "restore",
    "selection", "settings", "favorites", "sounds", "theme-light",
    "season-winter", "season-halloween", "background-playing",
)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--video", type=Path, required=True)
    args = parser.parse_args()
    output = Path(__file__).resolve().parents[1] / "docs" / "assets"
    if not output.is_dir() or not args.video.is_file():
        parser.error("Documentation folder and recorded video must exist")
    missing = [name for name in SCREENSHOTS if not (args.source / f"{name}.png").is_file()]
    if missing:
        parser.error(f"Missing reviewed captures: {', '.join(missing)}")
    for name in SCREENSHOTS:
        with Image.open(args.source / f"{name}.png") as image:
            image.convert("RGB").save(output / f"{name}.png", optimize=True)
    subprocess.run([
        "ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-i", str(args.video),
        "-filter_complex", "fps=4,scale=800:-2:flags=lanczos,split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4",
        "-loop", "0", str(output / "workflow.gif"),
    ], check=True)
    print(f"Prepared {len(SCREENSHOTS)} screenshots and workflow.gif from the recorded application")


if __name__ == "__main__":
    main()
