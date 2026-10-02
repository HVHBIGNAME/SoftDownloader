#!/usr/bin/env python3
"""Prepare a SoftDownloader catalog in a Google Drive desktop folder (Python 3.11+)."""

import argparse
from pathlib import Path
import re
import sys

import catalog_model as model


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    commands = result.add_subparsers(dest="command", required=True)
    init = commands.add_parser("init", help="Create an empty catalog in your Drive folder")
    init.add_argument("--root", type=Path, required=True)
    init.add_argument("--title", default="Моя коллекция")
    init.set_defaults(handler=model.initialize)

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
    add.add_argument("--type", choices=tuple(model.INSTALL_EXTENSIONS))
    add.add_argument("--silent-arg", action="append", default=[], help="Repeat per argument; use --silent-arg=/S")
    add.add_argument("--admin", action=argparse.BooleanOptionalAction, default=None)
    add.add_argument("--depends-on", action="append", default=[])
    add.add_argument("--tag", action="append", default=[])
    add.add_argument("--homepage")
    add.add_argument("--drive-url")
    add.add_argument("--destination-root", choices=model.ROOTS, default="roaming_app_data")
    add.add_argument("--destination", default="", help="Relative, package-specific ZIP/portable folder")
    add.add_argument("--strip-components", type=int, default=0)
    add.add_argument("--replace", action="store_true")
    add.set_defaults(handler=model.add_package)

    link = commands.add_parser("link", help="Attach a public Google Drive file link to a package")
    link.add_argument("--root", type=Path, required=True)
    link.add_argument("--id", required=True)
    link.add_argument("--drive-url", required=True)
    link.set_defaults(handler=model.link_package)

    publish = commands.add_parser("publish", help="Create catalog.public.json for other users")
    publish.add_argument("--root", type=Path, required=True)
    publish.set_defaults(handler=model.publish)

    validate = commands.add_parser("validate", help="Validate a catalog and optionally verify local installers")
    validate.add_argument("--catalog", type=Path, required=True)
    validate.add_argument("--public", action="store_true")
    validate.add_argument("--verify-files", action="store_true")
    validate.set_defaults(handler=model.validate_command)
    return result


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    try:
        print(args.handler(args))
    except (ValueError, OSError, KeyError, TypeError, AttributeError, re.error) as error:
        print(f"Error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
