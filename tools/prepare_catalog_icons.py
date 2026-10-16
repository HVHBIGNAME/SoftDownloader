"""Prepare a pinned, offline catalog icon atlas from reviewed public product artwork (Pillow)."""

import argparse
import difflib
import hashlib
import io
import json
import math
import re
import subprocess
import sys
import threading
import urllib.parse
import urllib.request
import xml.etree.ElementTree as ET
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from icon_artwork import IconLinks, repository_icons
from PIL import Image, ImageDraw, ImageOps, ImageStat

ROOT = Path(__file__).resolve().parents[1]
REPOSITORY = "homarr-labs/dashboard-icons"
LOCKS = {}
LOCKS_GUARD = threading.Lock()


def download(url, limit=3 * 1024 * 1024):
    request = urllib.request.Request(
        url, headers={"User-Agent": "Mozilla/5.0 SoftDownloader-Icon-Preparation"}
    )
    with urllib.request.urlopen(request, timeout=60) as response:
        data = response.read(limit + 1)
    if len(data) > limit:
        raise ValueError("Asset exceeds preparation size limit")
    return data


def packages():
    return [
        p
        for name in ("builtin", "extended")
        for p in json.loads(
            (ROOT / f"catalog/{name}.json").read_text(encoding="utf-8")
        )["packages"]
    ]


def discover(cache):
    path = cache / "dashboard-tree.json"
    if not path.exists():
        result = subprocess.run(
            ["gh", "api", f"repos/{REPOSITORY}/git/trees/main?recursive=1"],
            capture_output=True,
            check=True,
        )
        path.write_bytes(result.stdout)
    tree = json.loads(path.read_bytes())
    path.write_text(json.dumps(tree, indent=2), encoding="utf-8")
    if tree.get("truncated"):
        raise ValueError("Incomplete icon index")
    names = {
        Path(entry["path"]).stem
        for entry in tree["tree"]
        if entry["path"].startswith("png/") and entry["path"].endswith(".png")
    }
    aliases = {
        "vscode": "visual-studio-code",
        "obs": "obs",
        "7zip": "7zip",
        "java": "java",
        "python-313": "python",
        "claude-code": "claude",
        "nodejs": "nodejs",
    }
    sources = {}
    missing = []
    for package in packages():
        id = package["id"]
        install = package.get("install", {})
        if install.get("type") == "vscode_extension":
            publisher, extension = install["extension_id"].split(".", 1)
            sources[id] = {
                "url": f"https://{publisher}.gallery.vsassets.io/_apis/public/gallery/publisher/{publisher}/extension/{extension}/latest/assetbyname/Microsoft.VisualStudio.Services.Icons.Default",
                "credit": "VS Code Marketplace product icon",
            }
            continue
        normalize = lambda value: re.sub(r"[^a-z0-9]+", "-", value.lower()).strip("-")
        base = re.sub(r"-\d+(?:-\d+)*$", "", id)
        candidates = [
            aliases.get(id, id),
            aliases.get(base, base),
            normalize(package["name"]),
            normalize(package["name"].split(" · ")[0]),
        ]
        name = next((name for name in candidates if name in names), None)
        if name:
            sources[id] = {
                "url": f"https://raw.githubusercontent.com/{REPOSITORY}/{tree['sha']}/png/{name}.png",
                "credit": "Dashboard Icons · Apache-2.0",
            }
        else:
            missing.append(
                {
                    "id": id,
                    "name": package["name"],
                    "homepage": package.get("homepage"),
                    "source": package.get("source"),
                }
            )
    (cache / "discovered.json").write_text(
        json.dumps(sources, indent=2, ensure_ascii=False), encoding="utf-8"
    )
    (cache / "missing.json").write_text(
        json.dumps(missing, indent=2, ensure_ascii=False), encoding="utf-8"
    )
    print(f"Matched {len(sources)}/{len(packages())}; missing {len(missing)}")
    for package in missing:
        print(
            package["id"],
            "|",
            package["homepage"],
            "|",
            difflib.get_close_matches(package["id"], names, n=3, cutoff=0.55),
        )


def fetch_icon(item, cache):
    with LOCKS_GUARD:
        lock = LOCKS.setdefault(item[1].get("url", item[0]), threading.Lock())
    with lock:
        return fetch_locked(item, cache)


def fetch_locked(item, cache):
    id, source = item
    try:
        if "symbol" in source:
            tile = tool_symbol(source["symbol"])
            return (
                id,
                tile,
                {
                    **source,
                    "credit": "Original SoftDownloader interface symbol, MIT; not a product logo",
                },
                None,
            )
        key = hashlib.sha256(source["url"].encode()).hexdigest()
        path = cache / (key + (".exe" if source.get("format") == "exe" else ".image"))
        if not path.exists():
            path.write_bytes(
                download(
                    source["url"],
                    32 * 1024 * 1024
                    if source.get("format") == "exe"
                    else 3 * 1024 * 1024,
                )
            )
        data = path.read_bytes()
        if (
            source.get("sha256")
            and hashlib.sha256(data).hexdigest() != source["sha256"]
        ):
            raise ValueError("Source artwork no longer matches pinned SHA-256")
        content = data
        if source.get("format") == "exe":
            from icon_artwork import exe_icon

            content = exe_icon(data)
        with open_icon(content) as image:
            if image.width * image.height > 4096 * 4096:
                raise ValueError("Icon dimensions too large")
            if image.format == "ICO":
                image = image.ico.getimage(
                    max(image.ico.sizes(), key=lambda size: size[0] * size[1])
                )
            image = image.convert("RGBA")
            image = image.crop(image.getbbox())
            image = ImageOps.contain(image, (54, 54), Image.Resampling.LANCZOS)
        if not image.getbbox():
            raise ValueError("Empty icon")
        mean = ImageStat.Stat(image.convert("RGB"), mask=image.getchannel("A")).mean
        background = (43, 49, 61, 255) if sum(mean) / 3 > 210 else (241, 244, 248, 255)
        tile = Image.new("RGBA", (64, 64))
        ImageDraw.Draw(tile).rounded_rectangle(
            (0, 0, 63, 63), radius=12, fill=background
        )
        tile.alpha_composite(image, ((64 - image.width) // 2, (64 - image.height) // 2))
        return id, tile, {**source, "sha256": hashlib.sha256(data).hexdigest()}, None
    except (OSError, ValueError, ET.ParseError) as error:
        return id, None, None, str(error)


def tool_symbol(kind):
    image = Image.new("RGBA", (256, 256))
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((0, 0, 255, 255), radius=48, fill="#273247")
    color = "#9cceff"
    if kind == "power":
        draw.arc((58, 62, 198, 206), 305, 235, fill=color, width=15)
        draw.line((128, 42, 128, 126), fill=color, width=16)
    elif kind == "search":
        draw.ellipse((52, 48, 160, 156), outline=color, width=14)
        draw.line((150, 150, 207, 207), fill=color, width=17)
    elif kind in ("git", "network"):
        draw.line((70, 60, 70, 191, 187, 116, 187, 60), fill=color, width=12)
        for x, y in ((70, 60), (70, 191), (187, 60)):
            draw.ellipse(
                (x - 22, y - 22, x + 22, y + 22),
                fill="#273247",
                outline=color,
                width=10,
            )
    else:
        draw.rounded_rectangle((40, 55, 216, 202), radius=17, outline=color, width=12)
        draw.line((65, 107, 96, 134, 65, 161), fill=color, width=11)
        draw.line((121, 160, 183, 160), fill=color, width=11)
        if kind == "diff":
            draw.line((138, 98, 185, 98), fill="#ffcf89", width=10)
    return image.resize((64, 64), Image.Resampling.LANCZOS)


def open_icon(data):
    if b"<svg" in data[:2000]:
        import resvg_py

        root = ET.fromstring(data)
        for node in root.iter():
            for key, value in node.attrib.items():
                if key.endswith("href") and not value.startswith(("#", "data:")):
                    raise ValueError("External SVG resource")
        if re.search(rb"url\(\s*[\"']?(?!#|data:)[a-zA-Z/\\]", data):
            raise ValueError("External SVG URL")
        data = resvg_py.svg_to_bytes(
            svg_string=data.decode("utf-8"),
            width=128,
            height=128,
            skip_system_fonts=True,
        )
    return Image.open(io.BytesIO(data))


def web_source(package, override, cache):
    id = package["id"]
    package_source = package.get("source")
    repository = override.get("repository") or (
        package_source.get("repository")
        if package_source and package_source.get("type") == "github"
        else None
    )
    homepage = override.get("homepage", package.get("homepage", ""))
    parsed = urllib.parse.urlsplit(homepage)
    if not repository and parsed.hostname == "github.com":
        repository = "/".join(parsed.path.strip("/").split("/")[:2])
    urls = []
    diagnostics = []
    if repository:
        try:
            urls.extend(repository_icons(repository, cache, id))
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            diagnostics.append(f"Repository lookup {id}: {error}")
    elif parsed.hostname not in ("apps.microsoft.com", "learn.microsoft.com"):
        try:
            parser = IconLinks()
            parser.feed(download(homepage).decode("utf-8", errors="replace"))
            urls.extend(
                urllib.parse.urljoin(homepage, link) for link in reversed(parser.links)
            )
        except (OSError, ValueError) as error:
            diagnostics.append(f"Website lookup {id}: {error}")
        urls.append(urllib.parse.urljoin(homepage, "/favicon.ico"))
    for url in dict.fromkeys(urls):
        if not url.startswith("https://"):
            continue
        source = {
            "url": url,
            "credit": f"Product artwork from {repository or parsed.hostname}",
        }
        _, _, _, error = fetch_icon((id, source), cache)
        if not error:
            return id, source, diagnostics
        diagnostics.append(f"Icon candidate {id}: {url}: {error}")
    return id, None, diagnostics


def discover_web(cache):
    sources = json.loads((cache / "discovered.json").read_text(encoding="utf-8"))
    saved = cache / "sources.json"
    if saved.exists():
        sources.update(json.loads(saved.read_text(encoding="utf-8")))
    overrides = json.loads(
        (ROOT / "catalog/icon-overrides.json").read_text(encoding="utf-8")
    )
    tree = json.loads((cache / "dashboard-tree.json").read_bytes())
    for id, override in overrides.items():
        if "symbol" in override:
            sources[id] = override
            continue
        if "dashboard" in override:
            sources[id] = {
                "url": f"https://raw.githubusercontent.com/{REPOSITORY}/{tree['sha']}/png/{override['dashboard']}.png",
                "credit": "Dashboard Icons · Apache-2.0",
            }
        elif "url" in override:
            sources[id] = override
        elif (
            "repository" in override
            and id in sources
            and not sources[id]
            .get("url", "")
            .startswith(f"https://raw.githubusercontent.com/{override['repository']}/")
        ):
            del sources[id]
    pending = [p for p in packages() if p["id"] not in sources]
    diagnostics = {}
    with ThreadPoolExecutor(max_workers=6) as pool:
        for id, source, errors in pool.map(
            lambda p: web_source(p, overrides.get(p["id"], {}), cache), pending
        ):
            if source:
                sources[id] = source
            if errors:
                diagnostics[id] = errors
    (cache / "discovery-errors.json").write_text(
        json.dumps(diagnostics, indent=2), encoding="utf-8"
    )
    saved.write_text(
        json.dumps(sources, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    print(f"Sources: {len(sources)}/{len(packages())}")
    print("Missing:", ", ".join(p["id"] for p in packages() if p["id"] not in sources))


def build(cache, source_path):
    sources = json.loads(source_path.read_text(encoding="utf-8"))
    sources = sources.get("packages", sources)
    ids = {p["id"] for p in packages()}
    missing = ids - sources.keys()
    if missing:
        print("Missing sources:", ", ".join(sorted(missing)))
    with ThreadPoolExecutor(max_workers=6) as pool:
        results = list(
            pool.map(lambda item: fetch_icon(item, cache), sorted(sources.items()))
        )
    failed = {id: error for id, _, _, error in results if error}
    (cache / "failed.json").write_text(json.dumps(failed, indent=2), encoding="utf-8")
    for id, error in failed.items():
        print(f"FAILED {id}: {error}")
    ready = [(id, image, source) for id, image, source, error in results if not error]
    images = []
    tiles = {}
    index = {}
    for id, image, source in ready:
        key = hashlib.sha256(image.tobytes()).hexdigest()
        if key not in tiles:
            tiles[key] = len(images)
            images.append(image)
        i = tiles[key]
        index[id] = {**source, "tile": i}
    atlas = Image.new("RGBA", (16 * 68, math.ceil(len(images) / 16) * 68))
    for i, image in enumerate(images):
        atlas.alpha_composite(image, (i % 16 * 68 + 2, i // 16 * 68 + 2))
    atlas.save(cache / "atlas-preview.png", optimize=True)
    print(
        f"Prepared {len(ready)}/{len(ids)} icons; preview: {cache / 'atlas-preview.png'}"
    )
    if failed or missing:
        raise SystemExit(1)
    atlas.save(ROOT / "assets/catalog-icons.png", optimize=True)
    (ROOT / "catalog/icons.json").write_text(
        json.dumps(
            {"columns": 16, "cell": 68, "size": 64, "packages": index},
            ensure_ascii=False,
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    license_path = ROOT / "assets/dashboard-icons-LICENSE.txt"
    if not license_path.exists():
        license_path.write_bytes(
            download(f"https://raw.githubusercontent.com/{REPOSITORY}/main/LICENSE")
        )
    print(
        f"Published {len(images)} unique tiles for {len(index)} packages; {atlas.width}x{atlas.height}; {(ROOT / 'assets/catalog-icons.png').stat().st_size} PNG bytes"
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--discover", action="store_true")
    parser.add_argument("--web", action="store_true")
    parser.add_argument("--sources", type=Path)
    args = parser.parse_args()
    if not args.cache.parent.is_dir():
        parser.error("Cache parent must exist")
    args.cache.mkdir(exist_ok=True)
    sys.path.insert(0, str(args.cache / "python"))
    if args.discover:
        discover(args.cache)
    elif args.web:
        discover_web(args.cache)
    elif args.sources:
        build(args.cache, args.sources)
    else:
        parser.error("Choose --discover or --sources")


if __name__ == "__main__":
    main()
