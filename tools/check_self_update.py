"""Exercise native EXE replacement and rollback on disposable Windows GUI copies."""

import argparse
import ctypes as ct
import hashlib
import json
import shutil
import struct
import subprocess
import tempfile
import time
from ctypes import wintypes as wt
from pathlib import Path

import tomllib
from capture_ui import USER, text, windows


def wait_until(check, message, timeout=90):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if check():
            return
        time.sleep(0.1)
    raise TimeoutError(message)


def native_path(path):
    return "\\\\?\\" + str(path.resolve())


def os_string(value):
    encoded = value.encode("utf-16-le")
    return {"Windows": list(struct.unpack(f"<{len(encoded) // 2}H", encoded))}


def hash_file(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def close_owned_process(pid, target):
    kernel = ct.WinDLL("kernel32", use_last_error=True)
    kernel.OpenProcess.argtypes = [wt.DWORD, wt.BOOL, wt.DWORD]
    kernel.OpenProcess.restype = wt.HANDLE
    kernel.QueryFullProcessImageNameW.argtypes = [
        wt.HANDLE,
        wt.DWORD,
        wt.LPWSTR,
        ct.POINTER(wt.DWORD),
    ]
    kernel.TerminateProcess.argtypes = [wt.HANDLE, wt.UINT]
    kernel.WaitForSingleObject.argtypes = [wt.HANDLE, wt.DWORD]
    kernel.CloseHandle.argtypes = [wt.HANDLE]
    handle = kernel.OpenProcess(0x1000 | 0x100000 | 1, False, pid)
    if not handle:
        return
    try:
        name, length = ct.create_unicode_buffer(32768), wt.DWORD(32768)
        if not kernel.QueryFullProcessImageNameW(handle, 0, name, ct.byref(length)):
            raise ct.WinError(ct.get_last_error())
        if Path(name.value.removeprefix("\\\\?\\")).resolve() != target.resolve():
            raise RuntimeError(
                "Refusing to close a process outside the disposable test folder"
            )
        for window in windows(pid):
            USER.PostMessageW(window, 0x10, 0, 0)
        if kernel.WaitForSingleObject(handle, 10000) != 0:
            kernel.TerminateProcess(handle, 1)
            kernel.WaitForSingleObject(handle, 10000)
    finally:
        kernel.CloseHandle(handle)


def scenario(exe, root, version, fail_start):
    folder = root / ("rollback" if fail_start else "success")
    folder.mkdir()
    profile = folder / "profile"
    job = profile / "updates" / "update-fixture"
    job.mkdir(parents=True)
    target = folder / "SoftDownloader.exe"
    shutil.copyfile(exe, target)
    with target.open("ab") as stream:
        stream.write(b"old-version-test-marker")
    original_hash = hash_file(target)
    candidate = folder / "incoming.exe"
    shutil.copyfile(exe, candidate)
    candidate_hash = hash_file(candidate)
    settings = {
        "updates": {"enabled": False},
        "sound": {"enabled": False, "volume": 0},
        "favorites": ["vscode"],
        "reduced_motion": True,
    }
    settings_file = profile / "settings.json"
    settings_file.write_text(json.dumps(settings), encoding="utf-8")
    parent = subprocess.Popen([str(target), "--data-dir", str(profile)], cwd=folder)
    helper = None
    try:
        wait_until(
            lambda: any(text(w) == "SoftDownloader" for w in windows(parent.pid)),
            "Original GUI did not start",
            30,
        )
        settings["restore_selection"] = ["vscode", "git"]
        settings_file.write_text(json.dumps(settings), encoding="utf-8")
        helper_exe = job / "runner.exe"
        shutil.copyfile(exe, helper_exe)
        plan = {
            "target": native_path(target),
            "incoming": native_path(candidate),
            "backup": native_path(candidate.with_suffix(".backup")),
            "working_directory": native_path(folder),
            "arguments": [os_string("--data-dir"), os_string(native_path(profile))],
            "parent_pid": parent.pid,
            "version": "999.0.0" if fail_start else version,
            "size": candidate.stat().st_size,
            "sha256": candidate_hash,
            "old_sha256": original_hash,
        }
        (job / "plan.json").write_text(json.dumps(plan), encoding="utf-8")
        helper = subprocess.Popen(
            [str(helper_exe), "--apply-update", str(job)], cwd=folder
        )
        wait_until(
            lambda: (job / "prepared").exists() or helper.poll() is not None,
            "Helper handshake timed out",
            30,
        )
        assert helper.poll() is None, (job / "error.txt").read_text(encoding="utf-8")
        assert hash_file(target) == original_hash, (
            "Replaced an executable before the original process exited"
        )
        for window in windows(parent.pid):
            USER.PostMessageW(window, 0x10, 0, 0)
        parent.wait(timeout=30)
        code = helper.wait(timeout=100)
        report = json.loads(
            (profile / "updates/result.json").read_text(encoding="utf-8")
        )
        assert (code != 0) == fail_start, report
        assert hash_file(target) == (original_hash if fail_start else candidate_hash), (
            report
        )
        assert (report["failed_sha256"] is not None) == fail_start, report
        resumed_pid = int((job / "restarted-pid").read_text())
        wait_until(
            lambda: any(text(w) == "SoftDownloader" for w in windows(resumed_pid)),
            "Restarted GUI did not appear",
            30,
        )
        restored = json.loads(settings_file.read_text(encoding="utf-8"))
        assert restored["favorites"] == ["vscode"] and restored["sound"]["volume"] == 0
        assert not candidate.with_suffix(".backup").exists()
        print(
            f"{'Rollback' if fail_start else 'Update'}: EXE hash, parent exit, native restart and profile preservation verified"
        )
    finally:
        if parent.poll() is None:
            close_owned_process(parent.pid, target)
            parent.wait(timeout=15)
        if helper is not None and helper.poll() is None:
            helper.terminate()
            helper.wait(timeout=15)
        pid_file = job / "restarted-pid"
        if pid_file.exists():
            close_owned_process(int(pid_file.read_text()), target)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--temp-root", type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    version = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))[
        "package"
    ]["version"]
    with tempfile.TemporaryDirectory(
        prefix="sd-update-check-", dir=args.temp_root
    ) as temporary:
        for failure in [False, True]:
            scenario(args.exe.resolve(), Path(temporary), version, failure)


if __name__ == "__main__":
    main()
