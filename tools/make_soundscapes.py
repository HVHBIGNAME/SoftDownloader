"""Build original, deterministic stereo UI sounds (44.1 kHz PCM), licensed with the project."""

import math
import random
import struct
from pathlib import Path

RATE = 44100
STYLES = {
    "glass": (880, 0.75, 0.15),
    "wood": (260, 0.18, 0.55),
    "digital": (620, 0.40, 0.10),
    "aurora": (660, 0.60, 0.06),
    "cinema": (330, 0.35, 0.20),
}


def render(style, cue):
    base, shimmer, tactile = STYLES[style]
    duration = {"click": 0.16, "success": 0.78, "error": 0.48}[cue]
    count = int(RATE * duration)
    randomizer = random.Random(f"{style}/{cue}/v2")
    notes = {
        "click": [(0, 0)],
        "success": [(0, 0), (0.065, 4), (0.13, 7), (0.21, 12)],
        "error": [(0, 0), (0.14, -4)],
    }[cue]
    channels = [[0.0] * count for _ in range(2)]
    noise = 0.0
    for i in range(count):
        t = i / RATE
        noise = noise * 0.78 + randomizer.uniform(-1, 1) * 0.22
        click = noise * math.exp(-t * 85) * tactile
        for side in range(2):
            value = click
            for onset, semitone in notes:
                dt = t - onset
                if dt < 0:
                    continue
                hz = base * 2 ** (semitone / 12) * (1 + (side - 0.5) * 0.002)
                attack = 1 - math.exp(-dt * 900)
                decay = math.exp(-dt * (28 if cue == "click" else 9))
                tone = math.sin(
                    2 * math.pi * hz * dt
                    + 0.35 * math.sin(2 * math.pi * hz * 2 * dt) * math.exp(-dt * 40)
                )
                tone += (
                    shimmer
                    * 0.25
                    * math.sin(2 * math.pi * hz * 3.01 * dt)
                    * math.exp(-dt * 18)
                )
                value += tone * attack * decay * (0.72 if cue == "success" else 1)
            channels[side][i] = value
    for side in range(2):
        dry = channels[side][:]
        for delay, gain in [
            (0.029 + side * 0.007, 0.10),
            (0.057 + side * 0.009, 0.055),
            (0.091, 0.025),
        ]:
            shift = int(delay * RATE)
            for i in range(shift, count):
                channels[side][i] += dry[i - shift] * gain
    peak = max(abs(v) for channel in channels for v in channel)
    samples = []
    for i in range(count):
        fade = min(1, i / 180, (count - 1 - i) / 1100)
        for side in range(2):
            samples.append(round(channels[side][i] * fade * 11000 / max(peak, 1e-9)))
    return struct.pack(f"<{len(samples)}h", *samples)


def main():
    root = Path(__file__).resolve().parents[1]
    folder = root / "assets/sounds"
    assert folder.is_dir()
    for style in STYLES:
        for cue in ("click", "success", "error"):
            data = render(style, cue)
            (folder / f"{style}-{cue}.pcm").write_bytes(data)
            values = struct.unpack(f"<{len(data) // 2}h", data)
            rms = math.sqrt(sum(v * v for v in values) / len(values))
            print(
                f"{style}/{cue}: {len(data) / (RATE * 4):.2f}s, peak={max(abs(v) for v in values)}, RMS={rms:.0f}"
            )


if __name__ == "__main__":
    main()
