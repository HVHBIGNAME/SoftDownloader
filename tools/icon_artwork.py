"""Find published icon candidates and read PE artwork without executing the program."""

import json
import struct
import subprocess
import urllib.parse
from html.parser import HTMLParser
from pathlib import Path


class IconLinks(HTMLParser):
    def __init__(self):
        super().__init__()
        self.links = []

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if (
            tag == "link"
            and "icon" in attrs.get("rel", "").lower()
            and attrs.get("href")
        ):
            self.links.append(attrs["href"])


def repository_icons(repository, cache, id):
    path = cache / (repository.replace("/", "-") + "-tree.json")
    if not path.exists():
        result = subprocess.run(
            ["gh", "api", f"repos/{repository}/git/trees/HEAD?recursive=1"],
            capture_output=True,
            check=True,
            timeout=45,
        )
        path.write_bytes(result.stdout)
    tree = json.loads(path.read_bytes())
    if tree.get("truncated"):
        raise ValueError("Incomplete repository icon index")
    candidates = []
    path.write_text(json.dumps(tree, indent=2), encoding="utf-8")
    for entry in tree["tree"]:
        file = entry["path"]
        p = Path(file)
        if p.suffix.lower() not in (".png", ".ico", ".svg", ".webp") or any(
            s in file.lower()
            for s in ("node_modules", "vendor/", "test/", "example/", "screenshot")
        ):
            continue
        stem = p.stem.lower()
        if stem not in (
            "icon",
            "logo",
            "app_icon",
            "appicon",
            "favicon",
            "applicationicon",
            "app",
            id,
        ) and not stem.startswith(("logo-", "icon256", "icon_256")):
            continue
        score = (
            (80 if p.suffix == ".ico" else 0)
            + (50 if stem == "app_icon" else 0)
            + (20 if "windows" in file.lower() else 0)
            - len(file) / 10
        )
        candidates.append((score, file))
    return [
        f"https://raw.githubusercontent.com/{repository}/{tree['sha']}/{urllib.parse.quote(file)}"
        for _, file in sorted(candidates, reverse=True)[:6]
    ]


def exe_icon(data):
    import pefile

    with pefile.PE(data=data, fast_load=True) as pe:
        pe.parse_data_directories(
            directories=[pefile.DIRECTORY_ENTRY["IMAGE_DIRECTORY_ENTRY_RESOURCE"]]
        )
        resources = {
            entry.id: entry.directory.entries
            for entry in pe.DIRECTORY_ENTRY_RESOURCE.entries
        }
        group = resources[14][0].directory.entries[0].data.struct
        directory = pe.get_data(group.OffsetToData, group.Size)
        count = struct.unpack_from("<H", directory, 4)[0]
        if not 1 <= count <= 256:
            raise ValueError("Invalid PE icon directory")
        images = {}
        for entry in resources[3]:
            info = entry.directory.entries[0].data.struct
            images[entry.id] = pe.get_data(info.OffsetToData, info.Size)
        header = bytearray(struct.pack("<HHH", 0, 1, count))
        payload = bytearray()
        for index in range(count):
            width, height, colors, reserved, planes, bits, length, id = (
                struct.unpack_from("<BBBBHHIH", directory, 6 + index * 14)
            )
            image = images[id]
            if len(image) != length:
                raise ValueError("Incomplete PE icon resource")
            header.extend(
                struct.pack(
                    "<BBBBHHII",
                    width,
                    height,
                    colors,
                    reserved,
                    planes,
                    bits,
                    length,
                    6 + count * 16 + len(payload),
                )
            )
            payload.extend(image)
        return bytes(header + payload)
