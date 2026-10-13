"""Prepare original UI audio and licensed video previews. Requires FFmpeg for video frames."""

import argparse
import hashlib
import json
import subprocess
import urllib.request
from pathlib import Path


def download(url: str) -> bytes:
    request = urllib.request.Request(
        url, headers={"User-Agent": "SoftDownloader asset preparation"}
    )
    with urllib.request.urlopen(request, timeout=45) as response:
        data = response.read(32 * 1024 * 1024 + 1)
    if len(data) > 32 * 1024 * 1024:
        raise ValueError("Asset exceeds the 32 MiB preparation limit")
    return data


def halloween_poster(source: Path) -> None:
    root = Path(__file__).resolve().parents[1]
    presets = json.loads(
        (root / "catalog/backgrounds.json").read_text(encoding="utf-8")
    )
    preset = next(p for p in presets if p["id"] == "halloween-forest")
    if hashlib.sha256(source.read_bytes()).hexdigest() != preset["sha256"]:
        raise ValueError("Expected the verified Mixkit Halloween source clip")
    subprocess.run(
        [
            "ffmpeg",
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-ss",
            "2",
            "-i",
            str(source),
            "-vf",
            "scale=960:540",
            "-frames:v",
            "1",
            str(root / "assets/themes/halloween-scene.png"),
        ],
        check=True,
    )


def video_previews(folder: Path, clip_ids: list[int]) -> None:
    if not folder.is_dir():
        raise ValueError("Video preview folder must exist")
    for clip_id in clip_ids:
        data = download(f"https://assets.mixkit.co/videos/{clip_id}/{clip_id}-720.mp4")
        path = folder / f"mixkit-{clip_id}.mp4"
        path.write_bytes(data)
        subprocess.run(
            [
                "ffmpeg",
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-ss",
                "2",
                "-i",
                str(path),
                "-frames:v",
                "1",
                str(path.with_suffix(".png")),
            ],
            check=True,
        )
        print(
            f"{path.name}: {len(data)} bytes; SHA-256 {hashlib.sha256(data).hexdigest()}"
        )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--halloween-poster",
        type=Path,
        help="Create the bundled scene from the verified Mixkit 33884 clip",
    )
    parser.add_argument(
        "--video-dir",
        type=Path,
        help="Also download real stock clips here for visual review",
    )
    parser.add_argument(
        "--video-id",
        type=int,
        action="append",
        help="Review only these Mixkit clips (requires --video-dir)",
    )
    args = parser.parse_args()
    if args.halloween_poster:
        halloween_poster(args.halloween_poster)
        return
    if args.video_id:
        if not args.video_dir or any(clip_id <= 0 for clip_id in args.video_id):
            parser.error("Positive video IDs and --video-dir are required")
        video_previews(args.video_dir, args.video_id)
        return
    assets = Path(__file__).resolve().parent.parent / "assets"
    if not assets.is_dir():
        parser.error("Project assets folder was not found")
    from make_soundscapes import main as prepare_sounds

    prepare_sounds()
    if args.video_dir:
        video_previews(args.video_dir, [3352, 4281, 4033, 44373])


if __name__ == "__main__":
    main()
