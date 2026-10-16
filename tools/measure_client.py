"""Measure a real Windows client's idle CPU and memory in isolated profiles (Pillow + psutil)."""

import argparse
import json
import os
import tempfile
import time
from pathlib import Path

import psutil
from capture_ui import USER, Session


def sample(process, seconds):
    before = process.cpu_times()
    started = time.monotonic()
    time.sleep(seconds)
    after = process.cpu_times()
    elapsed = time.monotonic() - started
    memory = process.memory_info()
    cpu = 100 * (after.user + after.system - before.user - before.system) / elapsed
    return {
        "seconds": round(elapsed, 3),
        "cpu_one_core_percent": round(cpu, 3),
        "cpu_machine_percent": round(cpu / (os.cpu_count() or 1), 3),
        "working_set_mib": round(memory.rss / 1024**2, 2),
        "private_mib": round(memory.private / 1024**2, 2),
        "threads": process.num_threads(),
        "handles": process.num_handles(),
    }


def measure(args, season):
    with tempfile.TemporaryDirectory(
        prefix="sd-resource-check-", dir=args.temp_root
    ) as temp:
        folder = Path(temp)
        profile = folder / "profile"
        profile.mkdir()
        (profile / "settings.json").write_text(
            json.dumps(
                {
                    "appearance": {"season": season},
                    "weather": {"enabled": False},
                    "background": {"video": None},
                }
            ),
            encoding="utf-8",
        )
        start = time.monotonic()
        session = Session(args.exe, profile, folder)
        try:
            result = {"window_seconds": round(time.monotonic() - start, 3)}
            process = psutil.Process(session.process.pid)
            time.sleep(args.settle)
            children = [p for p in process.children(recursive=True) if p.is_running()]
            if children:
                raise RuntimeError(
                    "Background processes still running; increase --settle"
                )
            result["visible_idle"] = sample(process, args.seconds)
            USER.ShowWindow(session.window, 6)
            time.sleep(3)
            result["minimized_idle"] = sample(process, args.seconds)
            return result
        finally:
            session.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--temp-root", type=Path, required=True)
    parser.add_argument("--settle", type=float, default=65)
    parser.add_argument("--seconds", type=float, default=10)
    args = parser.parse_args()
    if not args.output.parent.is_dir() or not args.temp_root.is_dir():
        raise ValueError("Output and temporary parent directories must exist")
    results = {"logical_cpus": os.cpu_count(), "settle_seconds": args.settle}
    for season in ("off", "automatic"):
        results[season] = measure(args, season)
    args.output.write_text(json.dumps(results, indent=2), encoding="utf-8")
    print(json.dumps(results, indent=2), flush=True)


if __name__ == "__main__":
    main()
