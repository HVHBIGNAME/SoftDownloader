# Каталог на Google Диске

## 1. Создать рабочую папку

В Google Drive for desktop создайте папку, например `G:\Мой диск\SoftDownloader`. Буква диска и название «Мой диск» могут отличаться.

Из папки проекта:

```powershell
$root = "G:\Мой диск\SoftDownloader"
python tools/catalog.py init --root "$root" --title "Моя коллекция"
```

Получится:

```text
SoftDownloader/
  catalog.json          рабочий каталог
  catalog.public.json   появляется после publish
  installers/           установщики программ
  addons/               дополнительные пакеты
```

Официальные программы уже встроены в приложение. В этой папке достаточно хранить свои дополнения. Встроенные ID перечислены в [`builtin.json`](../catalog/builtin.json) и [`extended.json`](../catalog/extended.json).

## 2. Добавить установщик

### EXE

```powershell
python tools/catalog.py add --root "$root" `
  --file "C:\Installers\MyTool.exe" --id my-tool `
  --name "Мой инструмент" --version "1.0" --publisher "Моя команда" `
  --description "Краткое описание программы" `
  --category utilities --silent-arg=/S --admin
```

`/S` — пример для некоторых NSIS-установщиков, а не универсальный флаг. Inno Setup обычно использует `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-`. Qt Installer Framework использует свои CLI-команды. Перед публикацией сверяйтесь с документацией программы.

Каждый параметр передаётся отдельно: `--silent-arg=/VERYSILENT --silent-arg=/NORESTART`. Для установки от текущего пользователя есть `--no-admin`.

### MSI

```powershell
python tools/catalog.py add --root "$root" `
  --file "C:\Installers\App.msi" --id my-app --name "Моя программа" `
  --version "2.0" --category utilities
```

Для MSI приложение само добавляет `/i`, `/qn`, `/norestart` и путь журнала. Дополнительные MSI-свойства можно передать через `--silent-arg=PROPERTY=value`.

### Утилита для другой программы

```powershell
python tools/catalog.py add --root "$root" `
  --file "C:\Installers\MyHttpUtility.exe" --id http-extra `
  --name "Моя HTTP-утилита" --version "1.0" `
  --kind addon --depends-on httpdebugger --category network-tools `
  --silent-arg=/S
```

Утилита появится в разделе **Аддоны** и в карточке HTTP Debugger. Основная программа будет установлена первой. Несколько зависимостей задаются повторением `--depends-on`.

### ZIP-аддон

```powershell
python tools/catalog.py add --root "$root" `
  --file "C:\Installers\MyAddon.zip" --id my-addon --name "Мой аддон" `
  --version "1.0" --kind addon --category utilities --depends-on my-app `
  --destination-root roaming_app_data --destination "MyApp\addons\my-addon" `
  --strip-components 1
```

`strip-components 1` убирает одну верхнюю папку из путей архива. Если файлы уже лежат в корне ZIP, используйте `0`.

Корни назначения: `roaming_app_data`, `local_app_data`, `documents`. Назначение должно быть **отдельной папкой конкретного аддона**. Приложение записывает маркер владения и допускает замену/удаление только собственного пакета. Не указывайте общую папку `addons`, если в ней находятся другие плагины.

### Portable или установщик с мастером

Для копирования самостоятельного EXE без запуска:

```powershell
python tools/catalog.py add --root "$root" `
  --file "C:\Installers\PortableTool.exe" --id portable-tool --name "Portable Tool" `
  --version "1.0" --category utilities --type portable `
  --destination-root local_app_data --destination "SoftDownloader/apps/portable-tool"
```

Чтобы запускать штатный мастер, используйте `--type interactive --admin`. Например, так можно добавить свой официальный ESET Premium Live Installer в локальный каталог под ID `eset-premium`: запись переопределит встроенную ручную карточку. После добавления при необходимости перенесите в JSON поле `detect` из встроенной карточки, чтобы сохранить её алиасы обнаружения.

## 3. Поделиться установщиком

После `add` утилита выведет путь скопированного файла. Дождитесь окончания синхронизации Google Drive. В веб-интерфейсе Диска откройте доступ к **этому файлу**: «Все, у кого есть ссылка» → «Читатель».

```powershell
python tools/catalog.py link --root "$root" --id my-tool `
  --drive-url "https://drive.google.com/file/d/FILE_ID/view?usp=sharing"
```

Можно передать только ID файла. Ссылка на папку вместо файла не подойдёт. Разрешите скачивание для читателей в настройках доступа Google Drive.

## 4. Проверить и опубликовать

```powershell
python tools/catalog.py validate --catalog "$root\catalog.json" --verify-files
python tools/catalog.py publish --root "$root"
python tools/catalog.py validate --catalog "$root\catalog.public.json" --public
```

`publish` создаёт `catalog.public.json`, убирает локальные пути и проверяет наличие сетевого источника у активных пакетов. Откройте доступ «Все, у кого есть ссылка» к этому JSON-файлу.

В SoftDownloader: **Настройки → Дополнительный каталог → ссылка на catalog.public.json → Сохранить и подключить**.

На своём ПК можно указать локальный `catalog.json`: свои установщики будут читаться из синхронизируемой папки, официальные — из официальных источников.

## 5. Обновить пакет

Повторите `add` с тем же `id`, новой версией и `--replace`. Новые байты попадут в отдельную папку по хешу, предыдущий файл сохранится. Привяжите ссылку нового файла через `link`, затем выполните `publish`.

Утилита перезаписывает существующий JSON на месте, чтобы сохранять файловую идентичность для Drive for desktop. После обновления убедитесь, что публичная ссылка открывает новую версию. Если при синхронизации Google выдал новый ID, загрузите новую версию в существующий файл через **Управление версиями** в веб-интерфейсе Drive либо обновите ссылку в приложении.

## Группы

Встроенные группы включают `vpn`, `development`, `editors`, `runtimes`, `java`, `network-tools`, `utilities`, `creative`, `3d`, `design`, `media`, `gaming` и другие. `init` переносит весь актуальный список. При добавлении новой группы:

```powershell
python tools/catalog.py add --root "$root" --file "C:\Installers\MyTool.exe" `
  --id design-helper --name "Design Helper" --version "1.0" `
  --category design-addons --group "Дополнения для дизайна" --parent design `
  --silent-arg=/S
```

## Если скачивание не работает

- Страница входа Google: проверьте доступ «Все, у кого есть ссылка» у JSON и установщика.
- Лимит скачиваний: Google Drive ограничивает популярные файлы независимо от объёма хранилища; 5 ТБ места не означают неограниченный трафик. Повторите позже или смените источник конкретного файла.
- Не совпадает SHA-256: файл изменился после создания записи. Повторно добавьте его и опубликуйте каталог.
- Локальный файл недоступен: дождитесь синхронизации или включите «Доступен офлайн» в Google Drive for desktop.
- Активные пакеты типа `exe` без параметров тихой установки отклоняются: добавьте корректные флаги или выберите `interactive` для обычного мастера.
