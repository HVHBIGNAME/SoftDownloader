# SoftDownloader

Нативный менеджер программ для **Windows 10/11 x64** на Rust + egui. **97 пакетов в 22 группах:** WinGet, Microsoft Store, GitHub, сайты разработчиков и свои дополнения с Google Диска.

![Каталог SoftDownloader](docs/assets/catalog.png)

## Что умеет

- **Готовая коллекция:** браузеры, мессенджеры, ИИ-редакторы, инструменты разработки, VPN, медиа, игровые лаунчеры и системные утилиты.
- **Поиск, группы и подгруппы.** VPN, разработка, утилиты; группы дополняются вашим каталогом.
- **Связанные дополнения:** у программы отображаются её аддоны и дополнительные утилиты. Зависимости устанавливаются первыми.
- **Одиночная и массовая установка:** WinGet, EXE/MSI, ZIP, portable EXE и расширения VS Code. Очередь, прогресс и повтор неудачных задач.
- **Обнаружение и удаление:** реестр Windows, Microsoft Store, WinGet, расширения VS Code и управляемые пакеты. CLI/portable также ищутся в известных папках и PATH.
- **Точное сопоставление:** алиасы, локализованные названия, ветки Python/JDK и архитектуры Visual C++. Если версия неизвестна, это явно показано.
- **Контроль файлов:** SHA-256 из GitHub или вашего каталога; для официального сайта без опубликованного хеша — проверка Authenticode средствами Windows.
- **Значки программ:** собственная иконка приложения в Windows и реальные иконки установленных программ, считанные из самих EXE. Ничего не скачивается и не запускается.
- **Google Drive:** публичные ссылки для пользователей и локальная синхронизируемая папка для владельца коллекции.

### Что включено

| Группа | Примеры |
|---|---|
| Общение и браузеры | Discord, Telegram, AyuGram, Thunderbird, Brave, Chrome, Firefox |
| ИИ | Cursor, Trae, Antigravity, Claude Desktop, Claude Code/CLI, Cline, Hermes |
| Разработка | VS Code, Visual Studio Community 2026, Sublime Text, Notepad++, DB Browser, Git, Docker |
| Языки и среды | Python 3.13/3.14, Node.js LTS, Go, Rust, Visual C++ x64/x86, Temurin JDK 8/11/17/21/25 |
| VPN и сеть | Amnezia, FlClash, Happ, Proton, Mullvad, Windscribe, WireGuard, IVPN, WARP, Hiddify, Tailscale, Proxifier, TgWsProxy, zapret |
| Медиа и творчество | Spotify, VLC, OBS, Audacity, Blender, GIMP, Inkscape, CapCut, DroidCam |
| Игры и устройства | Steam, WeMod, LabyMod, Modrinth, Prism/Prism Cracked, LiquidLauncher, Minecraft, Meta Horizon Link, Sideloadly, 3uTools, iTunes |
| Утилиты | WinRAR, Bandizip, 7-Zip, CrystalDiskInfo/Mark, System Informer, ShareX, Twinkle Tray, Wireshark, OP Auto Clicker, Shutdown Timer Classic и другие |

**81 пакет использует WinGet (80 из `winget`, один из `msstore`), 9 — прямые загрузки, Cline — Marketplace.** У шести карточек есть кнопка **«САЙТ»** и инструкция: Hermes, AME Wizard Beta, AMD Software, NVIDIA App, ESET Premium и 2IP StartGuard. Эти карточки не входят в массовую установку. Страница 2IP при проверке отвечала HTTP 503.

Claude Code и Claude CLI объединены в одну карточку. System Informer — продолжение Process Hacker. Shutdown PC Timer доступен из Microsoft Store; Shutdown Timer Classic добавлен отдельно. Полный список и правила обнаружения: [`catalog/extended.json`](catalog/extended.json).

## Запуск

Готовая сборка формируется в [GitHub Actions](https://github.com/HVHBIGNAME/SoftDownloader/actions) как артефакт `SoftDownloader-windows-x64`. После публикации тега `v*` ZIP появится также в [Releases](https://github.com/HVHBIGNAME/SoftDownloader/releases).

Распакуйте ZIP и запустите **`SoftDownloader.exe`**. Для карточек WinGet нужен [Установщик приложений Microsoft](https://apps.microsoft.com/detail/9nblggh4nns1); проверить доступность можно командой `winget --version`. Для своих дополнений откройте **Настройки → Дополнительный каталог** и вставьте ссылку на `catalog.public.json` с Google Диска.

Каталог и список установленного обновляются в фоне. До завершения проверки ПК установка временно недоступна. Большая коллекция отрисовывает только видимые строки карточек.

## Иконки и сетевые запросы

**Иконки.** У приложения есть собственная иконка для Проводника, панели задач и запуска из меню «Пуск»: `assets/SoftDownloader.ico` встраивается в EXE на этапе сборки. Внутри интерфейса карточки и список установленного показывают настоящие иконки программ — они читаются из исполняемых файлов через API Windows (`PrivateExtractIconsW` и `GetDIBits`). Путь берётся из `DisplayIcon` или `InstallLocation` в реестре, а для portable — из управляемой папки пакета. Чтение выполняется в фоновом потоке по 64 пикселя и кэшируется, поэтому список из сотен программ не тормозит. Если иконки нет, показывается цветная плитка с буквами. Ничего не скачивается из интернета и ни один файл не запускается.

**Меньше сетевых запросов.** Каталог сохраняется локально и повторно используется в течение часа, поэтому приложение не опрашивает GitHub и сайты при каждом запуске. Если обновление не удалось, показывается последний рабочий каталог вместо пустого окна. У каждого источника свой таймаут 8 секунд, на все источники — общий бюджет 20 секунд; источники, не ответившие вовремя, помечаются в карточке. Подключение к серверу ограничено 5 секундами, чтение ответа — 30. Перечисление установленных программ WinGet и Store ограничено 25 секундами и отменяется, если пользователь запросил новую проверку.

### Из исходников

Требуются Rust 1.88+ с MSVC toolchain и Visual Studio Build Tools с компонентом Desktop development with C++.

```powershell
cargo run --release
```

Локальная папка Google Drive или отдельный профиль:

```powershell
cargo run -- --catalog "G:\Мой диск\SoftDownloader\catalog.json"
cargo run -- --data-dir "C:\Temp\SoftDownloader-test"
```

## Свой Google Диск

Папка Google Drive for desktop синхронизирует установщики. `tools/catalog.py` подготавливает метаданные; нужен Python 3.11+, дополнительные Python-пакеты не требуются.

```powershell
$root = "G:\Мой диск\SoftDownloader"
python tools/catalog.py init --root "$root"

python tools/catalog.py add --root "$root" `
  --file "C:\Installers\MyUtility.exe" `
  --id my-utility --name "Моя утилита" --version "1.0" `
  --category utilities --silent-arg=/S
```

Для утилиты, связанной с HTTP Debugger, добавьте `--kind addon --depends-on httpdebugger --category network-tools`. Параметры тихой установки задаются по документации конкретного установщика.

Затем откройте доступ **«Все, у кого есть ссылка / Читатель»** к скопированному установщику и привяжите его ссылку:

```powershell
python tools/catalog.py link --root "$root" --id my-utility `
  --drive-url "https://drive.google.com/file/d/FILE_ID/view"
python tools/catalog.py publish --root "$root"
```

Откройте такой же доступ к `catalog.public.json`. Его ссылку пользователи вставляют в настройки приложения.

**[Полная инструкция Google Drive](docs/google-drive.md)** · **[Формат каталога и официальные источники](docs/catalog.md)**

## Установка и удаление

EXE запускается с параметрами из каталога. MSI использует `/qn /norestart`; администратор запрашивается через штатное окно UAC. Код `3010`/`1641` отображается как необходимость перезагрузки.

WinGet получает точный ID из источника `winget` и использует проверку файла и параметры установки из манифеста. Актуальная версия и размер определяются самим менеджером. Обнаруженная WinGet-программа пропускается при установке; автоматическое обновление всех уже установленных программ не выполняется. Cline устанавливается в обычный профиль VS Code, а подключение модели настраивается пользователем.

Удаление использует `QuietUninstallString` или тихий MSI-деинсталлятор. Если программа зарегистрировала только обычное удаление, откроется её штатный мастер — такие строки помечены **«МАСТЕР»**. UWP/MSIX удаляются для текущего пользователя; системные, служебные и неудаляемые пакеты исключены из списка.

ZIP и portable EXE получают собственную управляемую папку в AppData или Documents. Кнопка **«Папка»** открывает её. Обновление заменяет содержимое управляемой папки; удаление удаляет её целиком. Внешний portable, найденный только по файлу, отмечен **«ВРУЧНУЮ»**: для него доступно открытие папки.

TgWsProxy требует настройки подключения после запуска. Для zapret распаковываются файлы; стратегия и служба настраиваются через `service.bat`. Созданную вручную службу удалите этим же скриптом перед удалением файлов пакета. Prism Cracked использует обычную ZIP-сборку без `portable.txt`, в отдельной папке программы.

Остановка очереди отменяет скачивание и следующие задачи. Уже запущенный установщик или деинсталлятор завершает работу. Проверенные файлы с опубликованным SHA-256 остаются в кэше для повторного использования.

## Проверки и сборка

Подробности: [результаты проверок](docs/verification.md).

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
python -m unittest discover -s tools/tests -v

# Проверить реальные официальные источники (нужен интернет)
cargo run --bin catalog-check -- --resolve

# Подтвердить WinGet ID без установки программ
cargo run --bin catalog-check -- --check-winget

# Проверить обнаружение на своём ПК
cargo run --bin catalog-check -- --inventory

# Собрать переносимый ZIP
cargo build --release --bins --locked
powershell -ExecutionPolicy Bypass -File tools/package.ps1
```

Установленные программы на рабочем ПК не изменяются тестами: полный цикл установки/удаления проверяется на изолированном ZIP-пакете. Проверка источников скачивает только метаданные; `catalog-check --download-only ID --cache DIR --resolve` отдельно скачивает и проверяет установщик без запуска.

## Структура

```text
src/catalog.rs          модель каталога и зависимости
src/discovery.rs        GitHub Releases и разбор официальных страниц
src/inventory.rs        сопоставление источников установленного ПО
src/system/             WinGet, Microsoft Store, VS Code и иконки
src/ui/icons.rs        фоновое чтение и кэш иконок программ
src/network.rs          HTTPS и Google Drive
src/transfer.rs         загрузка, кэш и SHA-256
src/installer/          EXE/MSI, Authenticode, ZIP и portable
src/uninstall/          реестр Windows и способы удаления
src/engine.rs           фоновая очередь
src/ui/                 нативный интерфейс
catalog/builtin.json    официальный каталог
catalog/extended.json   расширенная коллекция и правила обнаружения
assets/SoftDownloader.ico  иконка приложения, встраивается в EXE при сборке
tools/catalog.py        подготовка дополнительных пакетов
tools/catalog_model.py  проверка метаданных и копирование файлов
```

Каталог с Google Диска дополняет встроенный. Запись с таким же `id` заменяет встроенную — так можно закрепить версию или изменить параметры установки. При распространении своей сборки можно задать `SOFTDOWNLOADER_CATALOG_URL` во время `cargo build`; ссылка станет источником дополнений по умолчанию.

Настройки, история, кэш и журналы находятся в локальной папке данных пользователя. Точный путь доступен в настройках приложения. Доступ к Google Drive выполняется по публичным ссылкам; сервисный аккаунт и ключ Google Cloud не нужны.

Лицензия кода: [MIT](LICENSE). У сторонних программ собственные лицензии и условия использования.
