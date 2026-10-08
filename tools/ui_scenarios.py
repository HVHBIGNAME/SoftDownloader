"""Interaction checks for capture_ui. These scenarios never confirm installation or removal."""

import json
import logging
import time
from collections.abc import Callable
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from capture_ui import Session

LOGGER = logging.getLogger(__name__)


def silent_profile(profile: Path) -> None:
    if not profile.parent.is_dir():
        raise ValueError("Isolated profile parent must exist")
    profile.mkdir(exist_ok=True)
    path = profile / "settings.json"
    settings = json.loads(path.read_text(encoding="utf-8")) if path.exists() else {}
    sound = settings.setdefault("sound", {})
    sound["enabled"] = True
    sound["volume"] = 0
    path.write_text(json.dumps(settings, ensure_ascii=False, indent=2), encoding="utf-8")


def transfer(session: "Session", example: Path, review: bool) -> None:
    exported = session.profile / f"programs-{time.time_ns()}.json"
    session.caption = "Экспорт сохранит весь список установленных программ."
    session.click(100, 252)
    session.capture("installed")
    time.sleep(2)
    session.click(1186, 43)
    session.choose_file(exported)
    session.capture("export")
    document = json.loads(exported.read_text(encoding="utf-8"))
    exported_ids(exported)
    LOGGER.info("Exported %d programs without machine-specific data", len(document["programs"]))
    session.caption = "Импорт: выберите сохранённый JSON на новом компьютере."
    session.key(ord("O"), control=True)
    session.choose_file(example)
    session.capture("restore")
    time.sleep(3)
    session.click_primary()
    session.caption = "Доступные программы добавлены к выбору. Уже установленное пропускается."
    time.sleep(1)
    session.capture("selection")
    time.sleep(2)
    if review:
        selection_review(session)
    session.click_primary()
    session.caption = "Проверьте план. Установка начнётся только после подтверждения."
    time.sleep(1)
    session.capture("confirmation")
    assert_modal(session, True)
    time.sleep(3)


def preferences(session: "Session") -> dict:
    path = session.profile / "settings.json"
    return json.loads(path.read_text(encoding="utf-8")) if path.exists() else {}


def wait_preferences(session: "Session", matches: Callable[[dict], bool], message: str, timeout: float = 5) -> dict:
    deadline = time.monotonic() + timeout
    while True:
        settings = preferences(session)
        if matches(settings):
            return settings
        if time.monotonic() >= deadline:
            raise AssertionError(message)
        time.sleep(0.05)


def layout(session: "Session", mode: str) -> None:
    session.click(716, 174)
    session.click(760, 238 if mode == "grid" else 272)
    session.key(0x1B)
    if preferences(session).get("catalog_layout", "list") != mode:
        raise RuntimeError(f"The {mode} layout did not persist")


def exported_ids(path: Path) -> set[str]:
    document = json.loads(path.read_text(encoding="utf-8"))
    if document.get("format") != "softdownloader.program-list" or not document["programs"]:
        raise RuntimeError("The exported program set is invalid")
    if any(set(entry) != {"name", "version", "publisher", "package_ids"} for entry in document["programs"]):
        raise RuntimeError("A program set contains unexpected fields")
    return {package for entry in document["programs"] for package in entry["package_ids"]}


def features(session: "Session") -> None:
    session.caption = "Избранное: отмечайте любимые программы звёздочкой, даже если они уже установлены."
    layout(session, "list")
    wanted = {"firefox", "audacity", "inkscape", "vscode"}
    for package in sorted(wanted - set(preferences(session).get("favorites", []))):
        session.search(package)
        session.click(1174, 355)
        if package not in preferences(session).get("favorites", []):
            raise RuntimeError(f"Favorite button failed for {package}")
    saved = set(preferences(session).get("favorites", []))
    if not wanted <= saved:
        raise RuntimeError(f"Favorite buttons did not save the expected IDs: {saved}")
    session.key(0x1B)
    session.capture("compact")
    session.click(100, 201)
    layout(session, "grid")
    session.capture("favorites")
    time.sleep(2)
    session.caption = "Избранное можно сохранить как свой набор для другого компьютера."
    session.search("no-such-package")
    exported = session.profile / f"favorites-{time.time_ns()}.json"
    session.click(1170, 261)
    session.choose_file(exported)
    if exported_ids(exported) != saved:
        raise RuntimeError("Favorites export was incorrectly narrowed by search")
    session.key(0x1B)
    session.click(100, 151)
    layout(session, "list")
    session.click(350, 174)
    session.caption = "К установке: только доступные пакеты, которым нужна установка."
    session.capture("available")
    time.sleep(2)
    session.click(274, 174)
    session.click(716, 174)
    if not preferences(session).get("sort_by_name", False):
        session.click(760, 318)
    session.key(0x1B)
    if not preferences(session).get("sort_by_name"):
        raise RuntimeError("Sorting preference did not persist")
    LOGGER.info("Favorites, both layouts, alphabetic sorting and unfiltered set export verified")


def assert_modal(session: "Session", expected: bool) -> None:
    image = session.snapshot()
    appearance = preferences(session).get("appearance")
    theme = appearance.get("theme", "dark") if appearance is not None else "dark"
    background = {"dark": 21, "light": 242, "graphite": 37, "midnight": 13}[theme]
    modal = image.getpixel((image.width - 10, image.height // 2))[0] < background * 0.75
    if modal != expected:
        raise RuntimeError(f"Expected modal={expected}, observed modal={modal}")


def removal_preview(session: "Session") -> None:
    session.click(100, 252)
    session.caption = "Удаление тоже начинается с проверки списка. Этот предпросмотр будет отменён."
    session.click(282, 262)
    session.click(1125, 797)
    assert_modal(session, True)
    session.capture("removal")
    session.key(0x1B)
    assert_modal(session, False)
    if any((session.profile / "logs").iterdir()):
        raise RuntimeError("Cancelling removal unexpectedly produced queue logs")
    session.click(282, 262)
    LOGGER.info("Removal preview opened and cancelled; no uninstall queue ran")


def selection_review(session: "Session") -> None:
    session.click(1000, 797)
    session.caption = "Весь выбор в одном месте: можно убрать лишнее и сохранить набор."
    session.capture("selection-review")
    saved = session.profile / f"selection-{time.time_ns()}.json"
    session.click(1170, 261)
    session.choose_file(saved)
    before = exported_ids(saved)
    session.click(1010, 261)
    session.capture("selection-cleared")
    session.caption = "Случайно очистили выбор? «Вернуть» или Ctrl+Z восстанавливает его."
    session.key(ord("Z"), control=True)
    restored = session.profile / f"restored-selection-{time.time_ns()}.json"
    session.click(1170, 261)
    session.choose_file(restored)
    if exported_ids(restored) != before:
        raise RuntimeError("Undo did not restore exactly the selected package IDs")
    session.capture("selection-restored")
    LOGGER.info("Selection export and Ctrl+Z restored %d package IDs", len(before))


def narrow_window(session: "Session") -> None:
    from capture_ui import USER

    session.key(0x1B)
    session.click(100, 151)
    session.search("vscode")
    session.click(345, 345)
    USER.SetWindowPos(session.window, None, 50, 40, 1040, 739, 0x54)
    time.sleep(1)
    session.capture("narrow-details")
    session.key(0x1B)
    session.capture("narrow-catalog")
    USER.SetWindowPos(session.window, None, 50, 40, 1296, 879, 0x54)
    time.sleep(0.5)
    LOGGER.info("Minimum-size window and Escape from details captured for visual review")
