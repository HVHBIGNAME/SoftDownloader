"""Prepare licensed interface audio and artwork. Requires FFmpeg for audio conversion."""

import argparse
import hashlib
from io import BytesIO
import math
from pathlib import Path
import subprocess
import struct
import urllib.request
from zipfile import ZipFile

SOUNDS_URL = "https://kenney.nl/media/pages/assets/interface-sounds/fa43c1dd4d-1677589452/kenney_interface-sounds.zip"
TWEMOJI_URL = "https://raw.githubusercontent.com/twitter/twemoji/v14.0.2"
SOUNDS = {
    "glass": ("switch_002", 900),
    "wood": ("switch_003", 1250),
    "digital": ("click_001", 1600),
}


def download(url: str) -> bytes:
    request = urllib.request.Request(url, headers={"User-Agent": "SoftDownloader asset preparation"})
    with urllib.request.urlopen(request, timeout=45) as response:
        data = response.read(32 * 1024 * 1024 + 1)
    if len(data) > 32 * 1024 * 1024:
        raise ValueError("Asset exceeds the 32 MiB preparation limit")
    return data


def soft_tap(archive: ZipFile, original: str, cutoff: int) -> list[int]:
    converted = subprocess.run([
        "ffmpeg", "-hide_banner", "-loglevel", "error", "-i", "pipe:0", "-vn",
        "-ac", "1", "-ar", "22050", "-t", "0.10",
        "-af", f"highpass=f=70,lowpass=f={cutoff}:p=2", "-f", "s16le", "pipe:1",
    ], input=archive.read(f"Audio/{original}.ogg"), capture_output=True, check=True)
    samples = struct.unpack(f"<{len(converted.stdout) // 2}h", converted.stdout)
    if not samples or not any(samples):
        raise ValueError(f"No audible samples in {original}")
    softened = [value * min(1, index / 132) * min(1, (len(samples) - 1 - index) / 441)
                for index, value in enumerate(samples)]
    gain = 2200 / max(1, max(abs(value) for value in softened))
    return [round(value * gain) for value in softened]


def write_license(path: Path, data: bytes) -> None:
    text = "\n".join(line.rstrip() for line in data.decode("utf-8").splitlines()).strip("\n")
    path.write_text(text + "\n", encoding="utf-8", newline="\n")


def prepare(archive: ZipFile, assets: Path) -> None:
    sounds = assets / "sounds"
    themes = assets / "themes"
    sounds.mkdir(exist_ok=True)
    themes.mkdir(exist_ok=True)
    for style, (original, cutoff) in SOUNDS.items():
        tap = soft_tap(archive, original, cutoff)
        patterns = {"click": tap,
                    "success": tap + [0] * 1212 + [round(value * 0.6) for value in tap],
                    "error": tap + [0] * 551 + [round(value * 0.45) for value in tap]}
        for cue, samples in patterns.items():
            path = sounds / f"{style}-{cue}.pcm"
            path.write_bytes(struct.pack(f"<{len(samples)}h", *samples))
            rms = math.sqrt(sum(value * value for value in samples) / len(samples))
            print(f"{path.name}: {len(samples) / 22050:.3f}s, peak {max(abs(value) for value in samples)}/32767, RMS {rms:.0f}")
    write_license(sounds / "LICENSE.txt", archive.read("License.txt"))
    (themes / "pumpkin.png").write_bytes(download(f"{TWEMOJI_URL}/assets/72x72/1f383.png"))
    write_license(themes / "LICENSE-TWEMOJI.txt", download(f"{TWEMOJI_URL}/LICENSE-GRAPHICS"))


def video_previews(folder: Path, clip_ids: list[int]) -> None:
    if not folder.is_dir():
        raise ValueError("Video preview folder must exist")
    for clip_id in clip_ids:
        data = download(f"https://assets.mixkit.co/videos/{clip_id}/{clip_id}-720.mp4")
        path = folder / f"mixkit-{clip_id}.mp4"
        path.write_bytes(data)
        subprocess.run([
            "ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-ss", "2",
            "-i", str(path), "-frames:v", "1", str(path.with_suffix(".png")),
        ], check=True)
        print(f"{path.name}: {len(data)} bytes; SHA-256 {hashlib.sha256(data).hexdigest()}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inspect", action="store_true")
    parser.add_argument("--video-dir", type=Path, help="Also download real stock clips here for visual review")
    parser.add_argument("--video-id", type=int, action="append", help="Review only these Mixkit clips (requires --video-dir)")
    args = parser.parse_args()
    if args.video_id:
        if not args.video_dir or any(clip_id <= 0 for clip_id in args.video_id):
            parser.error("Positive video IDs and --video-dir are required")
        video_previews(args.video_dir, args.video_id)
        return
    assets = Path(__file__).resolve().parent.parent / "assets"
    if not assets.is_dir():
        parser.error("Project assets folder was not found")
    with ZipFile(BytesIO(download(SOUNDS_URL))) as archive:
        if args.inspect:
            for name in archive.namelist():
                print(name)
            return
        prepare(archive, assets)
    if args.video_dir:
        video_previews(args.video_dir, [3352, 4281, 4033, 44373])


if __name__ == "__main__":
    main()
