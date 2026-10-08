<p align="center">
  <img src="docs/assets/banner.svg" alt="SoftDownloader — один каталог, ваш список, новый компьютер" width="100%">
</p>

<p align="center">
  <a href="https://github.com/HVHBIGNAME/SoftDownloader/releases/latest"><img src="https://img.shields.io/github/v/release/HVHBIGNAME/SoftDownloader?style=flat-square&amp;color=86b9e8&amp;label=release" alt="Последний релиз"></a>
  <a href="https://github.com/HVHBIGNAME/SoftDownloader/actions/workflows/windows.yml"><img src="https://github.com/HVHBIGNAME/SoftDownloader/actions/workflows/windows.yml/badge.svg" alt="Windows CI"></a>
  <img src="https://img.shields.io/badge/Windows-10%20%2F%2011%20x64-859ba8?style=flat-square" alt="Windows 10/11 x64">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-859ba8?style=flat-square" alt="MIT"></a>
</p>

**Нативный менеджер программ для Windows на Rust + egui — один EXE.** Скачайте и запустите: каталог, иконки и интерфейс уже внутри. Выберите софт, проверьте план и установите нужное одной очередью. Сохранённый список поможет собрать такое же рабочее окружение на другом ПК.

<p align="center">
  <a href="https://github.com/HVHBIGNAME/SoftDownloader/releases/latest/download/SoftDownloader.exe"><b>Скачать SoftDownloader.exe</b></a> ·
  <a href="#перенос-списка-программ">Перенос программ</a> ·
  <a href="#видео-использования">Видео</a> ·
  <a href="docs/verification.md">Проверки</a>
</p>

![Компактный каталог программ в SoftDownloader 0.5.0](docs/assets/catalog.png)

## Начать работу

1. Скачайте **[SoftDownloader.exe](https://github.com/HVHBIGNAME/SoftDownloader/releases/latest/download/SoftDownloader.exe)** и запустите его.
2. Дождитесь проверки установленного ПО. Найдите программы через поиск или группы и отметьте нужные.
3. Нажмите **«Проверить и установить»**, проверьте зависимости и подтвердите установку. Результаты появятся в очереди.

Для пакетов WinGet нужен [Установщик приложений Microsoft](https://apps.microsoft.com/detail/9nblggh4nns1). EXE/MSI при необходимости откроют штатный мастер или запрос UAC.

Само приложение поставляется одним файлом, работает с системными библиотеками Windows и хранит настройки/кэш в AppData. Установка Rust, Python или отдельного runtime для его запуска не требуется. Файл `.sha256` в релизе — необязательная проверка скачивания; видео и документация доступны отдельно.

## Каталог под себя

- **Список или карточки** — переключаются в меню **Вид**. Выбор, статус установки и избранное занимают одинаковые фиксированные места.
- **Избранное** — звёздочка рядом с программой сохраняет её в вашей коллекции, включая уже установленное ПО.
- **Все / Доступные / Выбрано** — быстрые фильтры. Поиск **Ctrl+K / Ctrl+F**, группы и сортировка по имени помогают сузить список.
- **Проверить выбор** — соберите весь набор в одном месте. Его можно сохранить для другого компьютера; случайную очистку отменяют **Вернуть** или **Ctrl+Z**.

![Избранное в режиме карточек](docs/assets/favorites.png)

## Темы, звуки и видеофон

Настройки разделены на **Оформление**, **Звуки**, **Видеофон** и **Источники и данные**.

| Настройка | Что доступно |
|---|---|
| **Четыре темы** | Тёмная, светлая, серая и ночная. Пять готовых акцентов и свой цвет; контраст текста сохраняется при переключении. |
| **Мягкие звуки** | Три набора коротких приглушённых касаний, громкость и отдельное прослушивание. Пики ограничены, громкость по умолчанию — 18%. |
| **Праздники** | Падающий снег в декабре и январе, тыква с мягким свечением в октябре. Можно выбрать сезон вручную или отключить эффекты. |
| **Настоящее видео** | Свой MP4 либо скачиваемые ролики Mixkit: заснеженный лес и ночные горы. Предпросмотр, приглушение, пауза и воспроизведение по кругу без звука. |

Видео скачивается только по вашему выбору и хранится отдельно от EXE. Фон и эффекты приостанавливаются в неактивном окне и при включённом **Уменьшить анимацию**.

| Светлая тема | Видеофон с предпросмотром |
|---|---|
| ![Светлая тема и свой акцент](docs/assets/theme-light.png) | ![Настоящий таймлапс гор в настройках](docs/assets/background-playing.png) |

**[Настройка оформления и звуков](docs/appearance.md)** · **[Источники материалов и лицензии](assets/CREDITS.md)**

## Перенос списка программ

**На текущем ПК:** `Установлено → Экспорт списка` → сохраните JSON-файл.

**На новом ПК:** `Импорт списка` или **Ctrl+O** → выберите файл → проверьте найденные программы → **«Добавить к выбору»** → **«Проверить и установить»**.

- В экспорт попадает полный обнаруженный список, включая программы вне каталога.
- Доступные отсутствующие программы отмечаются в предпросмотре; выбор можно изменить.
- Уже установленные пропускаются. Неизвестные и недоступные записи видны с пояснением.
- Используются рецепты текущего каталога. Сохранённые версии служат справкой; зависимости добавляются автоматически.
- **Сохранить набор…** в избранном или просмотре выбора экспортирует именно вашу коллекцию, независимо от поискового фильтра.

**[Подробности и формат списка](docs/program-lists.md)** · **[Пример JSON](docs/examples/workstation.softdownloader.json)**

| Предпросмотр импорта | Выбор перед установкой |
|---|---|
| ![Предпросмотр списка программ](docs/assets/restore.png) | ![Выбранные программы из импортированного списка](docs/assets/selection.png) |

## Видео использования

[![Короткая запись интерфейса](docs/assets/workflow.gif)](https://github.com/HVHBIGNAME/SoftDownloader/releases/download/v0.5.0/SoftDownloader-0.5.0-demo.mp4)

**[Открыть / скачать полное видео MP4 · 58 секунд](https://github.com/HVHBIGNAME/SoftDownloader/releases/download/v0.5.0/SoftDownloader-0.5.0-demo.mp4)** — запись настоящего окна: список и карточки, избранное, экспорт набора, отмена удаления, импорт примера, очистка и восстановление выбора, подтверждение плана. Звука в записи нет; реальные установщики не запускаются.

## Возможности

| | |
|---|---|
| **Один файл** | `SoftDownloader.exe` со встроенными каталогом, шрифтами и иконками. Сборка оптимизирована по размеру: full LTO, `opt-level=z`, статический CRT. |
| **97 пакетов · 22 группы** | Браузеры, общение, разработка, ИИ, VPN, медиа, игры и системные утилиты. |
| **Одна очередь** | WinGet, Microsoft Store, EXE/MSI, ZIP, portable EXE и расширения VS Code. Повтор незавершённых задач и отмена загрузок. |
| **Учёт установленного** | Реестр Windows, Store, WinGet, VS Code, известные папки и PATH. Удаление по одной программе или списком. |
| **Компактный интерфейс** | Segoe UI, ровные строки и карточки, фиксированные статусы, четыре темы. Виртуализированные списки, короткие переходы и уведомления без сдвига содержимого. |
| **Свои наборы** | Избранное, просмотр всего выбора, экспорт коллекций и отмена очистки. Режим каталога и сортировка сохраняются. |
| **Локальные иконки** | Извлечение из установленных программ в фоне, кэш удачных и неудачных чтений. Для остальных — цветные плитки. |
| **Кэш и таймауты** | Каталог сохраняется на час. Явное обновление обходит кэш; успешные ответы сохраняются даже при таймауте других источников. |
| **Проверка файлов** | SHA-256 из каталога/GitHub; для официального сайта без опубликованного хеша — Authenticode средствами Windows. |

<details>
<summary><b>Что есть в каталоге</b></summary>

| Группа | Примеры |
|---|---|
| Общение и браузеры | Discord, Telegram, AyuGram, Thunderbird, Brave, Chrome, Firefox |
| ИИ | Cursor, Trae, Antigravity, Claude Desktop, Claude Code/CLI, Cline, Hermes |
| Разработка | VS Code, Visual Studio Community 2026, Sublime Text, Notepad++, Git, Docker, DB Browser |
| Языки и среды | Python 3.13/3.14, Node.js LTS, Go, Rust, Visual C++ x64/x86, Temurin JDK 8/11/17/21/25 |
| VPN и сеть | Amnezia, FlClash, Happ, Proton, Mullvad, Windscribe, WireGuard, WARP, Hiddify, Tailscale, Proxifier, TgWsProxy, zapret |
| Медиа и творчество | Spotify, VLC, OBS, Audacity, Blender, GIMP, Inkscape, CapCut, DroidCam |
| Игры и устройства | Steam, WeMod, LabyMod, Modrinth, Prism, LiquidLauncher, Minecraft, Meta Horizon Link, Sideloadly, 3uTools, iTunes |
| Утилиты | WinRAR, Bandizip, 7-Zip, CrystalDiskInfo/Mark, System Informer, ShareX, Twinkle Tray, Wireshark, OP Auto Clicker, таймеры выключения |

81 пакет использует WinGet: 80 из `winget`, один из `msstore`. Ещё 9 — прямые загрузки, Cline — Marketplace. У шести ручных карточек есть ссылка и инструкция: Hermes, AME Wizard Beta, AMD Software, NVIDIA App, ESET Premium и 2IP StartGuard.

Полные записи и правила обнаружения: [`catalog/extended.json`](catalog/extended.json). Claude Code и Claude CLI объединены; System Informer — продолжение Process Hacker. TgWsProxy и zapret требуют настройки после распаковки. Подробности установки и удаления — в [документации](docs/catalog.md).

</details>

| Установленные программы | Настройки |
|---|---|
| ![Установленное ПО и экспорт](docs/assets/installed.png) | ![Настройки оформления и анимации](docs/assets/settings.png) |

## Google Диск — для дополнительного софта

Основной каталог получает программы из WinGet, GitHub и с сайтов разработчиков. **Google Диск предназначен для дополнительных пакетов, которые будут добавлены позже.** Адрес этой коллекции встраивается в приложение; обычному пользователю не нужно подключать свой Диск.

В `0.5.0` канал дополнений подготовлен, публичный каталог ещё не задан. Его адрес задаётся владельцем сборки в [`catalog/channel.json`](catalog/channel.json) или через `SOFTDOWNLOADER_CATALOG_URL` при сборке. Дополнения объединяются с основным каталогом, а стабильные ID сохраняют совместимость экспортированных списков.

Настройка собственного источника находится в свёрнутом разделе настроек для владельца коллекции. **[Подготовка и публикация дополнений](docs/google-drive.md)** · **[Формат каталога](docs/catalog.md)**

## Сборка и проверки

Нужны Rust **1.88+**, MSVC toolchain и Visual Studio Build Tools с компонентом Desktop development with C++.

```powershell
cargo run --release

cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --all-features --locked
python -m unittest discover -s tools/tests -v

cargo build --release --locked
powershell -ExecutionPolicy Bypass -File tools/package.ps1
python tools/verify_standalone.py --file dist/SoftDownloader.exe
```

Отдельный профиль: `SoftDownloader.exe --data-dir "C:\Temp\SoftDownloader-test"`.
Локальный каталог дополнений: `SoftDownloader.exe --catalog "G:\Мой диск\SoftDownloader\catalog.json"`.

Обычный `cargo build --release` собирает только приложение: `target/release/softdownloader.exe`. Скрипт выпуска копирует его в `dist/SoftDownloader.exe` и записывает контрольную сумму.

Служебный CLI собирается отдельно: `cargo run --features catalog-tools --bin catalog-check -- --resolve`. Флаги `--check-winget` и `--inventory` проверяют ID и обнаружение программ, `--video clip.mp4 --frame frame.png` — нативное декодирование и перемотку видео. CLI предназначен разработчику и в пользовательский релиз не входит. Интеграционные тесты установки/удаления используют изолированные фиктивные пакеты. **[Результаты проверок 0.5.0](docs/verification.md)**.

Скриншоты и видео воспроизводятся через [`tools/capture_ui.py`](tools/capture_ui.py), медиа README — через [`tools/prepare_docs_media.py`](tools/prepare_docs_media.py), иконка — через [`tools/make_branding.py`](tools/make_branding.py). Этим инструментам нужен Pillow; видео также использует FFmpeg. GUI-прогоны принудительно устанавливают нулевую громкость в отдельном тестовом профиле. Исходные звуки и графика подготавливаются [`tools/prepare_ui_assets.py`](tools/prepare_ui_assets.py).

### Основные модули

```text
src/program_list.rs    экспорт, валидация и сопоставление списков
src/catalog_filter.rs  поиск, избранное и просмотр выбора
src/preferences.rs     темы, сезоны, звуки и видеофон
src/backgrounds.rs     каталог и проверка скачиваемых видео
src/video/             нативный Windows Media Foundation decoder
src/catalog_cache.rs   кэш, обновление и восстановление метаданных
src/config.rs          встроенный канал дополнений
src/catalog.rs         каталог, категории и зависимости
src/discovery.rs       GitHub Releases и официальные сайты
src/inventory.rs       обнаружение и сопоставление программ
src/engine.rs          фоновые задачи и последовательная очередь
src/ui/                интерфейс и фоновые иконки
src/system/            WinGet, Store, VS Code и Windows API
src/installer/         установщики, подписи, ZIP и portable
src/uninstall/         реестр и способы удаления
tools/catalog.py      подготовка дополнительных пакетов
```

Настройки, история, кэш и журналы хранятся локально; папка открывается из настроек. Исходный код — [MIT](LICENSE); материалы оформления — по [указанным лицензиям](assets/CREDITS.md). Сторонние программы распространяются на условиях своих разработчиков.
