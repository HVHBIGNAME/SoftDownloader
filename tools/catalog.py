#!/usr/bin/env python3
"""Prepare a SoftDownloader catalog in a Google Drive desktop folder (Python 3.11+)."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import sys
import tempfile
from datetime import datetime, timezone
from urllib.parse import parse_qs, urlparse

MAX_CATALOG_BYTES = 8 * 1024 * 1024
MAX_ARTIFACT_BYTES = 1024**4
ID_PATTERN = re.compile(r"[a-z0-9_-]{1,64}\Z")
DRIVE_ID_PATTERN = re.compile(r"[A-Za-z0-9_-]{10,200}\Z")
ROOTS = ("roaming_app_data", "local_app_data", "documents")
BUILTIN_PATH = Path(__file__).resolve().parents[1] / "catalog" / "builtin.json"


def builtin_catalog() -> dict:
    return json.loads(BUILTIN_PATH.read_text(encoding="utf-8"))


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def safe_relative(value: str) -> Path:
    require(isinstance(value, str) and bool(value), "Expected a non-empty relative path")
    parts = re.split(r"[/\\]", value)
    for part in parts:
        require(part not in ("", ".", ".."), f"Unsafe path: {value}")
        require(not re.search(r'[\x00-\x1f\x7f<>:"|?*]', part), f"Invalid path: {value}")
        require(not part.endswith((".", " ")), f"Invalid Windows path: {value}")
        stem = part.split(".")[0].upper()
        require(not re.fullmatch(r"CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9]", stem), f"Reserved Windows name: {part}")
    return Path(*parts)


def within(root: Path, relative: str) -> Path:
    result = (root / safe_relative(relative)).resolve()
    require(result.is_relative_to(root.resolve()), "Path leaves the catalog folder")
    return result


def drive_id(value: str) -> str:
    value = value.strip()
    if "://" not in value:
        require(bool(DRIVE_ID_PATTERN.fullmatch(value)), "Invalid Google Drive file ID")
        return value
    url = https_url(value)
    require(url.hostname in ("drive.google.com", "drive.usercontent.google.com"), "Expected a Google Drive file link")
    parts = url.path.strip("/").split("/")
    require("folders" not in parts, "Share a file, not a folder")
    match = re.search(r"/file/d/([^/]+)", url.path)
    identifier = match.group(1) if match else parse_qs(url.query).get("id", [""])[0]
    require(bool(DRIVE_ID_PATTERN.fullmatch(identifier)), "No valid file ID in the Google Drive link")
    return identifier


def https_url(value: str):
    url = urlparse(value)
    require(url.scheme == "https" and bool(url.hostname) and not url.username and not url.password, "Use an HTTPS URL without credentials")
    return url


def read_catalog(path: Path) -> dict:
    with path.open("rb") as stream:
        content = stream.read(MAX_CATALOG_BYTES + 1)
    require(len(content) <= MAX_CATALOG_BYTES, "Catalog exceeds 8 MiB")
    catalog = json.loads(content.decode("utf-8-sig"))
    validate_catalog(catalog)
    return catalog


def write_catalog(path: Path, catalog: dict) -> None:
    validate_catalog(catalog)
    catalog["updated_at"] = datetime.now(timezone.utc).isoformat(timespec="seconds")
    content = (json.dumps(catalog, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
    require(len(content) <= MAX_CATALOG_BYTES, "Catalog exceeds 8 MiB")
    # Keep the existing file identity: Drive Desktop tracks updates to this file.
    with path.open("wb") as stream:
        stream.write(content)
        stream.flush()
        os.fsync(stream.fileno())


def validate_catalog(catalog: dict, public: bool = False) -> None:
    require(isinstance(catalog, dict), "Catalog must be an object")
    require(catalog.get("schema_version") == 1, "Unsupported schema_version")
    require(bool(catalog.get("title", "").strip()), "Catalog title is required")
    categories = catalog.get("categories", [])
    packages = catalog.get("packages", [])
    require(isinstance(categories, list) and isinstance(packages, list), "categories and packages must be arrays")
    require(len(categories) <= 200 and len(packages) <= 5000, "Catalog is too large")
    groups = unique_index(categories, "category")
    entries = unique_index(packages, "package")
    builtin = builtin_catalog()
    combined_groups = unique_index(builtin["categories"], "category")
    combined_groups.update(groups)
    validate_graph({key: [item["parent"]] if item.get("parent") else [] for key, item in combined_groups.items()})
    dependencies = {item["id"]: item.get("depends_on", []) for item in builtin["packages"]}
    dependencies.update({key: item.get("depends_on", []) for key, item in entries.items()})
    validate_graph(dependencies)
    for package in packages:
        require(package.get("category") in combined_groups, f"Unknown category for {package['id']}")
        require(bool(package.get("version", "").strip()), f"Missing version for {package['id']}")
        require(package.get("kind", "app") in ("app", "addon"), "kind must be app or addon")
        if package.get("homepage"):
            https_url(package["homepage"])
        if package.get("enabled", True):
            require(bool(package.get("artifact") or package.get("source")) and bool(package.get("install")), f"Missing artifact/source or install for {package['id']}")
        if package.get("source"):
            validate_source(package["source"])
        artifact = package.get("artifact")
        if artifact:
            validate_artifact(artifact, public and package.get("enabled", True))
        install = package.get("install")
        if install:
            validate_install(install)
            if artifact:
                require(artifact["file_name"].lower().endswith("." + install["type"]), "Installer extension does not match install.type")


def unique_index(items: list, kind: str) -> dict:
    result = {}
    for item in items:
        require(isinstance(item, dict), f"Each {kind} must be an object")
        identifier = item.get("id", "")
        require(isinstance(identifier, str) and bool(ID_PATTERN.fullmatch(identifier)), f"Invalid {kind} ID: {identifier}")
        require(identifier not in result, f"Duplicate {kind}: {identifier}")
        require(bool(item.get("name", "").strip()), f"Missing name: {identifier}")
        result[identifier] = item
    return result


def validate_graph(graph: dict) -> None:
    visiting, visited = set(), set()

    def visit(identifier: str) -> None:
        require(identifier in graph, f"Missing dependency/parent: {identifier}")
        if identifier in visited:
            return
        require(identifier not in visiting, f"Dependency/category cycle: {identifier}")
        require(len(visiting) < 64, "Dependencies are nested too deeply")
        visiting.add(identifier)
        dependencies = graph[identifier]
        require(isinstance(dependencies, list), "Dependencies must be an array")
        require(len(set(dependencies)) == len(dependencies), "Duplicate dependency")
        for dependency in dependencies:
            visit(dependency)
        visiting.remove(identifier)
        visited.add(identifier)

    for identifier in graph:
        visit(identifier)


def validate_artifact(artifact: dict, public: bool) -> None:
    filename = artifact.get("file_name", "")
    require(len(safe_relative(filename).parts) == 1, "file_name must be a filename")
    size = artifact.get("size", 0)
    require(isinstance(size, int) and not isinstance(size, bool) and 0 < size <= MAX_ARTIFACT_BYTES, "Artifact size must be between 1 byte and 1 TiB")
    require(bool(re.fullmatch(r"[a-f0-9]{64}", artifact.get("sha256", ""))), "Invalid SHA-256")
    require(any(artifact.get(key) for key in ("local_path", "drive_file_id", "url")), "Missing artifact source")
    require(not (artifact.get("drive_file_id") and artifact.get("url")), "Choose drive_file_id or url")
    if artifact.get("local_path"):
        safe_relative(artifact["local_path"])
    if artifact.get("drive_file_id"):
        require(bool(DRIVE_ID_PATTERN.fullmatch(artifact["drive_file_id"])), "Invalid Drive file ID")
    if artifact.get("url"):
        https_url(artifact["url"])
    if public:
        require(bool(artifact.get("drive_file_id") or artifact.get("url")), "A public package needs a download link; run the link command first")
        require(not artifact.get("local_path"), "Public catalogs must not expose local paths")


def validate_install(install: dict) -> None:
    kind = install.get("type")
    require(kind in ("exe", "msi", "zip"), "install.type must be exe, msi or zip")
    if kind in ("exe", "msi"):
        arguments = install.get("silent_args" if kind == "exe" else "arguments", [])
        require(isinstance(arguments, list) and all(isinstance(arg, str) and "\0" not in arg for arg in arguments), "Installer arguments must be an array of strings without NUL")
        require(kind != "exe" or bool(arguments), "EXE installers require --silent-arg (see the vendor documentation)")
    else:
        destination = install.get("destination", {})
        require(destination.get("root") in ROOTS, f"ZIP destination root must be one of {ROOTS}")
        safe_relative(destination.get("path", ""))
        strip = install.get("strip_components", 0)
        require(isinstance(strip, int) and 0 <= strip <= 8, "strip_components must be 0–8")


def validate_source(source: dict) -> None:
    kind = source.get("type")
    require(kind in ("github", "website"), "source.type must be github or website")
    if kind == "github":
        require(bool(re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", source.get("repository", ""))), "Invalid GitHub repository")
        re.compile(source["asset_pattern"])
    else:
        https_url(source["page_url"])
        require(bool(source.get("link_selector")) and bool(source.get("version_selector")), "Website source needs CSS selectors")
        require(bool(source.get("download_hosts")), "Website source needs allowed download hosts")
        for host in source["download_hosts"]:
            require(https_url(f"https://{host}/").hostname == host, "Invalid download host")


def file_metadata(path: Path) -> tuple[int, str]:
    digest = hashlib.sha256()
    size = 0
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
            size += len(chunk)
    return size, digest.hexdigest()


def copy_verified(source: Path, destination: Path, expected: tuple[int, str]) -> None:
    if destination.exists():
        require(file_metadata(destination) == expected, "The destination contains different bytes; it was not overwritten")
        return
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=destination.parent, suffix=".part", delete=False) as stream:
        temporary = Path(stream.name)
    try:
        shutil.copyfile(source, temporary)
        require(file_metadata(temporary) == expected, "Installer changed while copying; try again")
        os.replace(temporary, destination)
    finally:
        temporary.unlink(missing_ok=True)


def initialize(args: argparse.Namespace) -> None:
    root = args.root.resolve()
    require(not (root / "catalog.json").exists(), "catalog.json already exists; init never overwrites it")
    root.mkdir(parents=True, exist_ok=True)
    for name in ("installers", "addons"):
        (root / name).mkdir(exist_ok=True)
    categories = builtin_catalog()["categories"]
    write_catalog(root / "catalog.json", {"schema_version": 1, "title": args.title, "categories": categories, "packages": []})
    print(f"Created {root / 'catalog.json'}")


def add_package(args: argparse.Namespace) -> None:
    root = args.root.resolve()
    path = root / "catalog.json"
    catalog = read_catalog(path)
    source = args.file.resolve(strict=True)
    require(source.is_file(), "Installer must be a file")
    require(bool(ID_PATTERN.fullmatch(args.id)), "Use a-z, 0-9, - and _ for package IDs")
    existing = next((p for p in catalog["packages"] if p["id"] == args.id), None)
    require(existing is None or args.replace, "Package ID already exists. Use --replace to update its entry")
    kind = args.type or source.suffix.lower().lstrip(".")
    require(kind in ("exe", "msi", "zip"), "Supported installers: .exe, .msi, .zip")
    require(source.suffix.lower() == "." + kind, "File extension must match installer type")
    install = make_install_spec(args, kind)
    validate_install(install)
    expected = file_metadata(source)
    require(0 < expected[0] <= MAX_ARTIFACT_BYTES, "Installer must be 1 byte–1 TiB")
    relative = f"{'addons' if args.kind == 'addon' else 'installers'}/{args.id}/{expected[1][:12]}/{source.name}"
    destination = within(root, relative)
    package = {
        "id": args.id, "name": args.name, "version": args.version,
        "publisher": args.publisher, "description": args.description,
        "category": args.category, "kind": args.kind, "enabled": True,
        "tags": args.tag, "depends_on": args.depends_on,
        "artifact": {"file_name": source.name, "size": expected[0], "sha256": expected[1], "local_path": relative},
        "install": install,
    }
    if args.homepage:
        package["homepage"] = args.homepage
    if args.drive_url:
        package["artifact"]["drive_file_id"] = drive_id(args.drive_url)
    if not any(category["id"] == args.category for category in catalog["categories"]):
        require(bool(args.group), "Category not found. Supply --group with its display name")
        category = {"id": args.category, "name": args.group}
        if args.parent:
            category["parent"] = args.parent
        catalog["categories"].append(category)
    catalog["packages"] = [p for p in catalog["packages"] if p["id"] != args.id] + [package]
    validate_catalog(catalog)
    copy_verified(source, destination, expected)
    write_catalog(path, catalog)
    print(f"Added {args.id}: {destination}\nSHA-256: {expected[1]}")


def make_install_spec(args: argparse.Namespace, kind: str) -> dict:
    if kind == "zip":
        return {"type": "zip", "destination": {"root": args.destination_root, "path": args.destination}, "strip_components": args.strip_components}
    admin = args.admin if args.admin is not None else kind == "msi"
    return {"type": kind, "requires_admin": admin, "silent_args" if kind == "exe" else "arguments": args.silent_arg}


def link_package(args: argparse.Namespace) -> None:
    path = args.root.resolve() / "catalog.json"
    catalog = read_catalog(path)
    package = next((p for p in catalog["packages"] if p["id"] == args.id), None)
    require(package is not None and bool(package.get("artifact")), "Package with an installer was not found")
    package["artifact"]["drive_file_id"] = drive_id(args.drive_url)
    package["artifact"].pop("url", None)
    write_catalog(path, catalog)
    print(f"Linked {args.id}")


def publish(args: argparse.Namespace) -> None:
    root = args.root.resolve()
    catalog = read_catalog(root / "catalog.json")
    public = copy.deepcopy(catalog)
    for package in public["packages"]:
        if package.get("artifact"):
            package["artifact"].pop("local_path", None)
    validate_catalog(public, public=True)
    output = root / "catalog.public.json"
    write_catalog(output, public)
    print(f"Published {output}\nShare this file in Google Drive: Anyone with the link / Viewer.")


def validate_command(args: argparse.Namespace) -> None:
    path = args.catalog.resolve()
    catalog = read_catalog(path)
    validate_catalog(catalog, public=args.public)
    if args.verify_files:
        for package in catalog["packages"]:
            artifact = package.get("artifact")
            if package.get("enabled", True) and artifact and artifact.get("local_path"):
                file = within(path.parent, artifact["local_path"])
                require(file_metadata(file) == (artifact["size"], artifact["sha256"]), f"File differs from catalog: {package['id']}")
    print(f"OK: {len(catalog['packages'])} packages, {len(catalog['categories'])} categories")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    commands = result.add_subparsers(dest="command", required=True)
    init = commands.add_parser("init", help="Create an empty catalog in your Drive folder")
    init.add_argument("--root", type=Path, required=True)
    init.add_argument("--title", default="Моя коллекция")
    init.set_defaults(handler=initialize)

    add = commands.add_parser("add", help="Copy an installer, calculate its hash and add a package")
    add.add_argument("--root", type=Path, required=True)
    add.add_argument("--file", type=Path, required=True)
    for name in ("id", "name", "version", "category"):
        add.add_argument(f"--{name}", required=True)
    add.add_argument("--publisher", default="Моя коллекция")
    add.add_argument("--description", default="")
    add.add_argument("--group", help="Display name when creating a new category")
    add.add_argument("--parent", help="Parent category ID for a new group")
    add.add_argument("--kind", choices=("app", "addon"), default="app")
    add.add_argument("--type", choices=("exe", "msi", "zip"))
    add.add_argument("--silent-arg", action="append", default=[], help="Repeat per argument; use --silent-arg=/S")
    add.add_argument("--admin", action=argparse.BooleanOptionalAction, default=None)
    add.add_argument("--depends-on", action="append", default=[])
    add.add_argument("--tag", action="append", default=[])
    add.add_argument("--homepage")
    add.add_argument("--drive-url")
    add.add_argument("--destination-root", choices=ROOTS, default="roaming_app_data")
    add.add_argument("--destination", default="", help="Relative, package-specific ZIP destination folder")
    add.add_argument("--strip-components", type=int, default=0)
    add.add_argument("--replace", action="store_true")
    add.set_defaults(handler=add_package)

    link = commands.add_parser("link", help="Attach a public Google Drive file link to a package")
    link.add_argument("--root", type=Path, required=True)
    link.add_argument("--id", required=True)
    link.add_argument("--drive-url", required=True)
    link.set_defaults(handler=link_package)

    publish_parser = commands.add_parser("publish", help="Create catalog.public.json for other users")
    publish_parser.add_argument("--root", type=Path, required=True)
    publish_parser.set_defaults(handler=publish)

    validate = commands.add_parser("validate", help="Validate a catalog and optionally verify local installers")
    validate.add_argument("--catalog", type=Path, required=True)
    validate.add_argument("--public", action="store_true")
    validate.add_argument("--verify-files", action="store_true")
    validate.set_defaults(handler=validate_command)
    return result


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    try:
        args.handler(args)
    except (ValueError, OSError, KeyError, TypeError, AttributeError, re.error) as error:
        print(f"Error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
