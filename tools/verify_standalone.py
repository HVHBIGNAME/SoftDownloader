"""Verify the single-file Windows distribution; --gui also opens an isolated window."""

import argparse
import ctypes
import hashlib
import shutil
import struct
import subprocess
import tempfile
import time
from pathlib import Path


SYSTEM_LIBRARIES = {
    "ADVAPI32.DLL", "AUTHZ.DLL", "BCRYPT.DLL", "BCRYPTPRIMITIVES.DLL", "CFGMGR32.DLL",
    "COMBASE.DLL", "COMCTL32.DLL", "COMDLG32.DLL", "CRYPT32.DLL", "D3D11.DLL", "D3D12.DLL", "DWMAPI.DLL",
    "DXGI.DLL", "GDI32.DLL", "IMM32.DLL", "KERNEL32.DLL", "MSIMG32.DLL", "MSVCRT.DLL",
    "NCRYPT.DLL", "NTDLL.DLL", "OLE32.DLL", "OLEAUT32.DLL", "OPENGL32.DLL", "POWRPROF.DLL",
    "PROPSYS.DLL", "PSAPI.DLL", "RPCRT4.DLL", "SECUR32.DLL", "SETUPAPI.DLL", "SHELL32.DLL",
    "SHLWAPI.DLL", "USER32.DLL", "USERENV.DLL", "USP10.DLL", "UXTHEME.DLL", "VERSION.DLL",
    "WINHTTP.DLL", "WINMM.DLL", "WINSPOOL.DRV", "WINTRUST.DLL", "WS2_32.DLL", "WSOCK32.DLL",
}


def imported_libraries(data: bytes) -> list[str]:
    if data[:2] != b"MZ":
        raise ValueError("Not a Windows executable")
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        raise ValueError("Missing PE header")
    machine, section_count = struct.unpack_from("<HH", data, pe + 4)
    optional_size = struct.unpack_from("<H", data, pe + 20)[0]
    optional = pe + 24
    if machine != 0x8664 or struct.unpack_from("<H", data, optional)[0] != 0x20B:
        raise ValueError("Expected a native Windows x64 executable")
    sections = optional + optional_size

    def offset(rva: int) -> int:
        for index in range(section_count):
            virtual_size, address, raw_size, raw_pointer = struct.unpack_from("<IIII", data, sections + 40 * index + 8)
            if address <= rva < address + max(virtual_size, raw_size):
                if rva - address >= raw_size:
                    raise ValueError("Import points outside file-backed data")
                return raw_pointer + rva - address
        raise ValueError(f"Unmapped RVA: {rva:#x}")

    imports_rva, imports_size = struct.unpack_from("<II", data, optional + 112 + 8)
    if not imports_rva:
        raise ValueError("No Windows imports found")
    table = offset(imports_rva)
    names = []
    for index in range(min(imports_size // 20, 1024)):
        descriptor = struct.unpack_from("<IIIII", data, table + index * 20)
        if not any(descriptor):
            return sorted(set(names))
        start = offset(descriptor[3])
        end = data.index(b"\0", start, start + 260)
        names.append(data[start:end].decode("ascii").upper())
    raise ValueError("Unterminated import table")


def verify_window(executable: Path, folder: Path) -> None:
    from capture_ui import Session

    screenshots = folder / "screenshots"
    screenshots.mkdir()
    session = Session(executable, folder / "profile", screenshots)
    try:
        time.sleep(55)
        session.capture("standalone")
        session.close()
        print("Isolated GUI started and closed normally with embedded catalog and icons")
    finally:
        if session.process.poll() is None:
            session.process.terminate()
            session.process.wait(timeout=10)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--file", type=Path, required=True)
    parser.add_argument("--temp-root", type=Path)
    parser.add_argument("--gui", action="store_true", help="Requires Windows desktop and Pillow")
    args = parser.parse_args()
    data = args.file.read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    checksum = Path(str(args.file) + ".sha256").read_text(encoding="ascii").split()[0]
    if digest != checksum:
        raise ValueError("SHA-256 sidecar does not match the executable")
    libraries = imported_libraries(data)
    unexpected = [name for name in libraries if name not in SYSTEM_LIBRARIES and not name.startswith(("API-MS-WIN-", "EXT-MS-WIN-"))]
    if unexpected:
        raise ValueError(f"Unexpected external runtime libraries: {unexpected}")
    extract = ctypes.WinDLL("shell32").ExtractIconExW
    extract.argtypes = [ctypes.c_wchar_p, ctypes.c_int, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_uint]
    extract.restype = ctypes.c_uint
    if extract(str(args.file.resolve()), -1, None, None, 0) < 1:
        raise ValueError("Application icon resource is missing")
    with tempfile.TemporaryDirectory(prefix="softdownloader-standalone-", dir=args.temp_root) as temporary:
        folder = Path(temporary)
        application = folder / "app"
        application.mkdir()
        executable = application / "SoftDownloader.exe"
        shutil.copyfile(args.file, executable)
        subprocess.run([str(executable), "--help"], cwd=application, check=True, capture_output=True, timeout=15)
        if args.gui:
            verify_window(executable, folder)
        if [path.name for path in application.iterdir()] != ["SoftDownloader.exe"]:
            raise ValueError("Standalone launch wrote unexpected companion files")
    print(f"Standalone x64 EXE: {len(data)} bytes ({len(data) / 1024**2:.2f} MiB)")
    print(f"SHA-256 verified: {digest}")
    print(f"Embedded icon verified; only Windows imports: {', '.join(libraries)}")


if __name__ == "__main__":
    main()
