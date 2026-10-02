# Формат каталога

JSON Schema: [`catalog/schema.json`](../catalog/schema.json). Авторитетная проверка использует ту же Rust-модель, что и приложение:

```powershell
cargo run --bin catalog-check -- "G:\Мой диск\SoftDownloader\catalog.json"
cargo run --bin catalog-check -- --public "G:\Мой диск\SoftDownloader\catalog.public.json"
```

## Пакет с установщиком на Google Drive

```json
{
  "schema_version": 1,
  "title": "Мои дополнения",
  "categories": [{ "id": "network-tools", "name": "HTTP и сеть", "parent": "development" }],
  "packages": [{
    "id": "http-extra",
    "name": "Моя HTTP-утилита",
    "version": "1.0",
    "publisher": "Моя команда",
    "description": "Дополнительная утилита для работы с HTTP Debugger.",
    "category": "network-tools",
    "kind": "addon",
    "depends_on": ["httpdebugger"],
    "tags": ["HTTP", "утилита"],
    "artifact": {
      "file_name": "MyHttpUtility.exe",
      "size": 123456,
      "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
      "drive_file_id": "REPLACE_WITH_YOUR_FILE_ID"
    },
    "install": { "type": "exe", "silent_args": ["/S"], "requires_admin": false }
  }]
}
```

Размер, SHA-256 и ID выше — **заполнители**. Используйте `tools/catalog.py`, чтобы записать реальные значения. `local_path` добавляется для локального режима и удаляется при публикации.

Основные поля:

| Поле | Значение |
|---|---|
| `id` | Стабильный ID: `a-z`, `0-9`, `-`, `_`, до 64 символов |
| `kind` | `app` или `addon` |
| `depends_on` | ID зависимостей, в том числе встроенных программ |
| `enabled` | По умолчанию `true`; `false` оставляет информационную карточку |
| `artifact` | Закреплённый файл: размер, хеш и источник |
| `source` | GitHub, сайт, WinGet или ручная ссылка с инструкцией |
| `install` | EXE, MSI, ZIP, portable EXE, мастер, WinGet или расширение VS Code |
| `detect` | Алиасы, шаблон имени, WinGet ID, локальные пути/PATH и имена MSIX |

Каталог дополняет встроенные категории и программы. Совпадающий `id` переопределяется. Циклы зависимостей и групп, дубликаты внутри одного документа, выходы за пределы папок и неизвестные поля отклоняются.

Встроенная коллекция объединяет `builtin.json` и `extended.json`. Подгруппы выводятся деревом; например, `development → runtimes → java`.

## WinGet

```json
"source": { "type": "winget", "package_id": "Microsoft.VisualStudioCode" },
"install": { "type": "winget" }
```

Пара обязательна; `artifact` не нужен. Используется `winget install --id ID --exact --source winget --silent --accept-source-agreements --accept-package-agreements --disable-interactivity`. Хеши и способ установки берутся из манифеста WinGet. Версия `"Последняя"` означает актуальную версию на момент установки, а не предварительно скачанный файл. При отсутствии WinGet карточка объясняет, как установить Microsoft App Installer.

Проверка всех ID без установки: `catalog-check --check-winget`. Уже обнаруженный managed-пакет пропускается. Отдельные ветки Python/JDK имеют собственные ID и правила обнаружения.

Для Microsoft Store задайте в `source` поле `"repository":"msstore"` и Product ID, например `9PB7GBMCR7N0` для Shutdown PC Timer. По умолчанию репозиторий — `winget`; другие значения отклоняются. В `detect.appx_names` укажите имя установленного MSIX-пакета.

## Расширение VS Code

```json
"depends_on": ["vscode"],
"install": { "type": "vscode_extension", "extension_id": "saoudrizwan.claude-dev" }
```

У такого пакета нет `source` и `artifact`. Сначала устанавливается VS Code, затем вызывается его CLI в режиме `ELECTRON_RUN_AS_NODE`. Обнаружение читает метаданные стандартного каталога `%USERPROFILE%/.vscode/extensions`, не запуская редактор. Пользовательские профили/каталоги расширений и другие редакторы отдельно не обходятся.

## Ручная карточка и штатный мастер

```json
"source": {
  "type": "manual",
  "url": "https://amelabs.net/",
  "instructions": "Скачайте AME Beta ZIP, распакуйте и откройте мастер."
}
```

У ручной карточки нет `install`/`artifact`: ссылка открывается по нажатию, карточка не попадает в очередь установки. Так оформлены AME, Hermes, AMD/NVIDIA, ESET Premium и временно недоступный 2IP StartGuard.

Если сам установщик уже известен и должен показывать свой интерфейс, задайте `"install": {"type":"interactive","requires_admin":true}`. Это EXE без тихих аргументов, например официальный мастер 3uTools. Его завершения ожидает очередь.

## GitHub Releases

```json
"source": {
  "type": "github",
  "repository": "chen08209/FlClash",
  "asset_pattern": "(?i)^FlClash-.*-windows-amd64-setup\\.exe$"
}
```

`asset_pattern` — регулярное выражение Rust regex. Должен подходить ровно один файл. Версия, размер, URL и SHA-256 получаются из GitHub Releases API; без опубликованного `digest: sha256:…` пакет становится недоступным до исправления рецепта. API используется без токена, с обычным публичным лимитом GitHub. Ошибки источника отображаются в карточке и не мешают другим пакетам.

Поле `version` у такого пакета можно первоначально задать как `"Последняя"`: во время загрузки оно заменится тегом релиза. `install` задаётся отдельно: по расширению невозможно надёжно определить параметры тихой установки.

## Официальный сайт

```json
"source": {
  "type": "website",
  "page_url": "https://www.httpdebugger.com/download",
  "link_selector": "a.download-card__redownload",
  "version_selector": "[itemprop=softwareVersion]",
  "download_hosts": ["www.httpdebugger.com"]
}
```

Читается HTML без выполнения JavaScript. Из ссылки берётся `href`, из версии — атрибут `content` или текст элемента. Разрешены только HTTPS-ссылки на перечисленные домены. Изменение разметки сайта требует обновления селекторов.

Опциональные `version_attribute` и `version_pattern` извлекают версию из атрибута. У 3uTools читается `href` ссылки `a.i4-win-v64`, затем единственная группа шаблона `3uTools_v([0-9.]+)_Setup`. Из `3uTools_v9.10.006_Setup_x64.exe` получается `9.10.006`.

Сайт HTTP Debugger не публикует отдельный SHA-256 в метаданных загрузки. Для `website` проверяется доверенная Authenticode-подпись Windows; фактический SHA-256 сохраняется после скачивания. Неподписанный файл не запускается. Это отличается от проверки по заранее опубликованному хешу. ZIP для такого источника не поддерживается — для ZIP нужен закреплённый хеш или GitHub Releases.

## ZIP и portable EXE

```json
"install": {
  "type": "zip",
  "destination": {
    "root": "roaming_app_data",
    "path": "MyApp/addons/my-addon"
  },
  "strip_components": 1
}
```

Распаковка идёт во временную папку. Перед заменой проверяется маркер владельца, старая версия временно переносится в резервную папку. При ошибке перемещения выполняется откат. Проверяются пути, символические ссылки, дубли имён и лимиты: до 25 000 записей и 8 ГБ распакованных данных. Размер одного скачиваемого файла ограничен 1 ТБ, JSON — 8 МБ.

Для одиночного portable-файла используется `"type":"portable"` с тем же `destination`: EXE копируется в управляемую папку без запуска. Изменение/удаление пакета затрагивает всю эту папку. У запакованного zapret удаляется один верхний уровень (`strip_components: 1`), поэтому `service.bat` располагается прямо в целевой папке.

## Обнаружение установленного ПО

```json
"detect": {
  "names": ["Microsoft Visual Studio Code", "Microsoft Visual Studio Code (User)"],
  "winget_ids": ["Microsoft.VisualStudioCode"],
  "paths": ["%LOCALAPPDATA%/Programs/Microsoft VS Code/Code.exe"],
  "commands": ["code.cmd"],
  "appx_names": []
}
```

- Имя пакета и `names` сравниваются целиком, без проверки по префиксу. `name_pattern` должен быть ограничен `^` и `$`; для нечувствительного регистра используйте `^(?i:...)$`.
- `paths` проверяет конкретные файлы, раскрывая переменные окружения. `commands` ищет только указанные имена файлов в PATH. Пустые файлы-заглушки App Execution Aliases пропускаются; обнаруженный EXE не выполняется.
- `winget_ids` дополняют основной ID источника. Для сопоставления используется JSON `winget export`, а не разбор локализованной таблицы. Условная версия вроде `< 3.3.6` не подменяет известную точную версию.
- `appx_names` — точные имена пакетов из `Get-AppxPackage` для текущего пользователя. Framework, resource, system и non-removable пакеты исключены.
- История сверяется с текущими признаками установки. Пропавшая программа при успешной проверке не остаётся установленной только из-за старой записи. Ошибка отдельного источника отображается отдельно; его неподтверждённые записи сохраняются до успешной проверки.

`catalog-check --inventory` показывает число найденных программ, совпадения с каталогом и предупреждения. Версия `Не определена` означает отсутствие надёжных метаданных. Внешний файл без зарегистрированного удаления отображается с кнопкой открытия папки.

## Удаление

EXE/MSI обнаруживаются в стандартных Uninstall-ветках HKCU/HKLM, 32- и 64-битных представлениях. Используется `QuietUninstallString`, если он есть. MSI-регистрация с `/I{GUID}` преобразуется в `/x {GUID} /qn /norestart`. Командная строка разбирается средствами Windows; оболочка для склейки аргументов не используется.

Для остальных записей используются `winget uninstall`, CLI VS Code или `Remove-AppxPackage` с точным PackageFullName. Управляемые ZIP/portable удаляются по маркеру владельца. Внешние portable-файлы автоматически не удаляются.

При массовом удалении управляемых пакетов дополнения идут раньше основных программ. Если установленное дополнение зависит от выбранной программы, его нужно включить в удаление. Запись о выполненной операции сохраняется после успешного завершения; у обычных Windows-программ дополнительно проверяется исчезновение записи реестра либо необходимость перезагрузки.
