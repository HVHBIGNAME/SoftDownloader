"""Live checks for appearance settings; uses only the isolated test profile."""

import hashlib
import json
import logging
from pathlib import Path
import time
from typing import TYPE_CHECKING

from ui_scenarios import preferences, wait_preferences

if TYPE_CHECKING:
    from capture_ui import Session

LOGGER = logging.getLogger(__name__)


def appearance(session: "Session") -> None:
    session.click(100, 773)
    session.click(303, 142)
    if not preferences(session)["background"]["effects"]:
        session.click(276, 464)
        wait_preferences(session, lambda value: value["background"]["effects"], "Effects did not enable")
    if preferences(session)["reduced_motion"]:
        session.click(276, 510)
        wait_preferences(session, lambda value: not value["reduced_motion"], "Motion did not enable")
    for theme, x in [("light", 387), ("graphite", 467), ("midnight", 548), ("dark", 303)]:
        session.click(x, 258)
        wait_preferences(session, lambda value: value["appearance"]["theme"] == theme, "Theme did not persist")
        session.capture(f"theme-{theme}")
    session.click(430, 310)
    wait_preferences(session, lambda value: value["appearance"]["accent"] == [186, 169, 242], "Accent did not persist")
    session.click(341, 310)
    wait_preferences(session, lambda value: value["appearance"]["accent"] == [134, 185, 232], "Blue accent did not persist")
    for mode, y in [("winter", 532), ("halloween", 580), ("off", 481)]:
        session.click(520, 389)
        session.click(506, y)
        wait_preferences(session, lambda value: value["appearance"]["season"] == mode, f"Season {mode} did not persist")
        session.key(0x1B)
        session.capture(f"season-{mode}")
        if mode == "winter":
            first = session.snapshot().crop((270, 600, 1200, 800))
            time.sleep(1)
            assert first.tobytes() != session.snapshot().crop((270, 600, 1200, 800)).tobytes(), "Snow did not animate"
    sounds(session)
    backgrounds(session)
    session.click(303, 142)
    session.click(276, 510)
    assert preferences(session)["reduced_motion"]
    session.click(276, 510)
    assert not preferences(session)["reduced_motion"]
    session.capture("settings")
    LOGGER.info("Four themes, accents, seasonal effects, sounds and real video verified")


def sounds(session: "Session") -> None:
    session.click(402, 142)
    session.caption = "Мягкие звуки: три варианта, своя громкость и ручное прослушивание."
    if not preferences(session)["sound"]["enabled"]:
        session.click(276, 261)
    for style, y in [("glass", 401), ("wood", 486), ("digital", 573)]:
        session.click(300, y)
        wait_preferences(session, lambda value: value["sound"]["style"] == style, "Sound style did not persist")
    assert preferences(session)["sound"]["volume"] == 0
    session.capture("sounds")
    session.click(276, 261)
    assert not preferences(session)["sound"]["enabled"]
    session.capture("sounds-disabled")
    session.click(276, 261)
    session.click(300, 401)
    LOGGER.info("Sound style and mute preferences verified with volume held at zero")


def backgrounds(session: "Session") -> None:
    session.click(493, 142)
    if preferences(session)["background"]["video"]:
        session.click(490, 290)
    session.capture("background-library")
    session.caption = "Настоящий ночной таймлапс: скачать и включить фоном."
    session.click(1157, 655)
    deadline = time.monotonic() + 135
    while not preferences(session)["background"]["video"] and time.monotonic() < deadline:
        time.sleep(0.25)
    path = Path(preferences(session)["background"]["video"] or "missing")
    assert path.is_file(), "Background download did not finish"
    presets = json.loads((Path(__file__).resolve().parents[1] / "catalog/backgrounds.json").read_text(encoding="utf-8"))
    expected = next(item for item in presets if item["id"] == "night-mountains")
    assert hashlib.sha256(path.read_bytes()).hexdigest() == expected["sha256"]
    time.sleep(2)
    session.click(950, 80)
    crop = (280, 360, 870, 680)
    first = session.snapshot().crop(crop)
    time.sleep(0.7)
    assert first.tobytes() != session.snapshot().crop(crop).tobytes(), "Video did not advance"
    session.capture("background-playing")
    session.click(614, 728)
    assert preferences(session)["background"]["paused"]
    time.sleep(1)
    paused = session.snapshot().crop(crop)
    time.sleep(0.6)
    assert paused.tobytes() == session.snapshot().crop(crop).tobytes(), "Paused video changed"
    session.capture("background-paused")
    session.click(614, 728)
    session.click(100, 151)
    session.search("vscode")
    session.capture("background-catalog")
    session.click(100, 773)
    session.click(490, 290)
    assert preferences(session)["background"]["video"] is None
    LOGGER.info("Downloaded video hash, advancing frames, pause and removal verified")
