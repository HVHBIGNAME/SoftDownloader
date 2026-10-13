# Материалы оформления

## Звуки интерфейса

С версии 0.7.0 используются оригинальные синтезированные звуки SoftDownloader под лицензией проекта MIT. Генератор: `tools/make_soundscapes.py`. Пять наборов по три события, stereo PCM 44100 Hz / 16 bit: «Аврора», «Кино», «Кристалл», «Тактильные», «Орбита». Длительности 0,16 / 0,78 / 0,48 секунды; пики до 11000/32767, плавная атака/затухание, стереозадержки и отдельные музыкальные интервалы для результата и ошибки. Во время воспроизведения применяется громкость пользователя и ограничитель 14000/32767.

Файл `sounds/LICENSE.txt` относится к прежним записям [Kenney Interface Sounds](https://kenney.nl/assets/interface-sounds), CC0, из версий 0.5–0.6. Идентификаторы старых настроек `glass`, `wood`, `digital` сохраняются, но сами записи заменены.

## Хэллоуин

Новая фоновая сцена `themes/halloween-scene.png` — кадр на второй секунде [Creepy Halloween Pumpkin in Dark Cloud Forest](https://mixkit.co/free-stock-video/creepy-halloween-pumpkin-in-dark-cloud-forest-33884/), Mixkit, [Stock Video Free License](https://mixkit.co/license/#videoFree). Уменьшен до 960×540, используется внутри оформления с туманом, медленным движением и свечением. Для воспроизведения исходного видео доступен отдельный фон «Хэллоуин · огонь в тумане».

Старое изображение `themes/pumpkin.png` и лицензия Twemoji сохранены как материалы прежних версий; приложение их больше не использует.

## Видеофоны

Ролики не входят в EXE: скачиваются по выбору пользователя с сайта автора и сохраняются локально.

- [Snow falling in a pine forest](https://mixkit.co/free-stock-video/snow-falling-in-a-pine-forest-3352/), Mixkit, [Stock Video Free License](https://mixkit.co/license/#videoFree).
- [Matterhorn Mountain Landscape](https://mixkit.co/free-stock-video/matterhorn-mountain-landscape-4281/), Mixkit, [Stock Video Free License](https://mixkit.co/license/#videoFree).
- [Aurora borealis timelapse](https://mixkit.co/free-stock-video/aurora-borealis-timelapse-4033/), dubassy / Mixkit, [Stock Video Free License](https://mixkit.co/license/#videoFree).
- [Sunset in an aerial shot over the sea](https://mixkit.co/free-stock-video/sunset-in-an-aerial-shot-over-the-sea-44373/), Mixkit, [Stock Video Free License](https://mixkit.co/license/#videoFree).
- [Window on a rainy day](https://mixkit.co/free-stock-video/window-on-a-rainy-day-2846/), Mixkit, Stock Video Free License.
- [Foggy autumn forest in the morning](https://mixkit.co/free-stock-video/foggy-autumn-forest-in-the-morning-35013/), Mixkit, Stock Video Free License.
- [Snowing in a foggy forest, slow motion](https://mixkit.co/free-stock-video/snowing-in-a-foggy-forest-slow-motion-35040/), Mixkit, Stock Video Free License.
- [Swiss Alps snow background time-lapse](https://mixkit.co/free-stock-video/swiss-alps-snow-background-time-lapse-4283/), Mixkit, Stock Video Free License.
- [Thunderstorm at night](https://mixkit.co/free-stock-video/thunderstorm-at-night-4422/), Mixkit, Stock Video Free License. Ролик содержит молнии; пауза или уменьшение анимации останавливают воспроизведение.
- Halloween Pumpkin — источник указан выше, Mixkit, Stock Video Free License.
- [Sunny day in a forest seen in detail](https://mixkit.co/free-stock-video/sunny-day-in-a-forest-seen-in-detail-5022/), Mixkit, Stock Video Free License.

В погодном режиме подходящая сцена загружается после включения режима/демонстрации. Размеры и SHA-256 закреплены в `catalog/backgrounds.json`. Пользовательские MP4 никуда не отправляются.

## Погодные данные

- [MET Norway Locationforecast](https://api.met.no/weatherapi/locationforecast/2.0/documentation), Норвежский метеорологический институт, [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).
- [Open-Meteo Geocoding](https://open-meteo.com/en/docs/geocoding-api), данные GeoNames, CC BY 4.0.
- [IPWho](https://ipwho.is/) используется только по нажатию кнопки определения города. IP-адрес не сохраняется в профиле.

Звуки: `python tools/make_soundscapes.py`. Кадр Хэллоуина: `python tools/prepare_ui_assets.py --halloween-poster <путь к mixkit-33884.mp4>` с FFmpeg и проверкой хеша исходника.
