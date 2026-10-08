# Материалы оформления

## Звуки интерфейса

[Interface Sounds](https://kenney.nl/assets/interface-sounds), Kenney, лицензия [CC0](https://creativecommons.org/publicdomain/zero/1.0/).

Используются короткие записи switch_002, switch_003 и click_001. Переведены из OGG в mono PCM, 22050 Hz, 16 bit. Отфильтрованы частоты ниже 70 Hz и выше 900–1600 Hz, добавлены плавные края; каждый щелчок не длиннее 100 ms. Пики приведены к 2200 из 32767 (около −23,5 dBFS), в плеере действует дополнительный ограничитель. Завершение и ошибка используют два тихих касания, без тональных сигналов. Идентификаторы `glass`, `wood`, `digital` сохраняются для совместимости настроек. Исходные условия находятся в `sounds/LICENSE.txt`.

## Хэллоуин

Изображение Jack-O-Lantern из [Twemoji 14.0.2](https://github.com/twitter/twemoji/tree/v14.0.2), copyright Twitter, Inc. and other contributors. Лицензия [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/), копия в `themes/LICENSE-TWEMOJI.txt`. PNG используется без изменения исходного файла; в интерфейсе меняются размер и прозрачность.

## Видеофоны

Ролики не входят в EXE: скачиваются по выбору пользователя с сайта автора и сохраняются локально.

- [Snow falling in a pine forest](https://mixkit.co/free-stock-video/snow-falling-in-a-pine-forest-3352/), Mixkit, [Stock Video Free License](https://mixkit.co/license/#videoFree).
- [Matterhorn Mountain Landscape](https://mixkit.co/free-stock-video/matterhorn-mountain-landscape-4281/), Mixkit, [Stock Video Free License](https://mixkit.co/license/#videoFree).

Подготовка звуков и графики воспроизводится через `python tools/prepare_ui_assets.py` с FFmpeg. Пользовательские MP4 остаются на компьютере пользователя и никуда не отправляются.
