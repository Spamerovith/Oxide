# Oxide

Быстрый и лёгкий проводник для Windows на Rust. Выглядит и работает как Проводник Windows 11,
поэтому переучиваться не нужно

![Oxide](Исходники/assets/oxide.png)

## Возможности

- Вкладки в заголовке окна, строка адреса с «хлебными крошками», командная панель, область навигации,
  строка состояния - на привычных местах
- Виды: огромные, крупные, обычные и мелкие значки, список, таблица, плитка, содержимое; компактный вид;
  эскизы изображений и видео
- Контекстное меню в стиле Windows 11 и полное системное меню (Shift+F10)
- Копирование, перемещение, удаление в Корзину выполняет сам Windows: родные окна прогресса и конфликтов
- Мгновенный поиск по всем дискам: индекс хранится на диске и почти не занимает оперативную память,
  а при обновлении не строится с нуля: заново читаются только изменённые папки
- «Свойства» сразу показывают размер папки и сколько в ней файлов и папок, остальные вкладки Windows на месте
- Главная, Этот компьютер, Галерея, Сеть, Корзина (с восстановлением и удалением навсегда), ZIP-архивы как папки
- Те же горячие клавиши, что в Проводнике
- Семь тем: как в Windows, Windows 11 светлая и тёмная, Oxide, Графит, Северная ночь, Бумага
- Русский и английский интерфейс: язык выбирается при первом запуске, поменять можно в параметрах
- Обновления прямо из программы: когда на GitHub выходит новая версия, Oxide предлагает её установить
- Можно открывать папки в Oxide вместо Проводника: с рабочего стола, из ярлыков и «Показать в папке» из браузера

## Установка

Скачайте `Oxide.exe` со страницы [Releases](../../releases) и запустите. Установка не нужна

Требуется Windows 10 или 11 (x64)

Файл не подписан цифровой подписью, поэтому при первом запуске Windows SmartScreen может показать
«Неизвестный издатель»: нажмите «Подробнее» → «Выполнить в любом случае»

## Обновление

Oxide сам проверяет новые версии на этой странице. Когда она выходит, появляется окно с предложением
обновиться, а слева вверху кнопка «Обновить». Новый exe скачивается, проверяется по SHA-256 из релиза
и встаёт на место старого, затем Oxide перезапускается с теми же вкладками

Настройки (`%APPDATA%\Oxide`), закреплённые папки и индекс поиска (`%LOCALAPPDATA%\Oxide`) хранятся
отдельно от exe, поэтому обновление их не трогает. Автопроверку можно выключить в параметрах

Версия 1.1.0 и старше обновляться сами не умеют: один раз скачайте новую версию вручную

## Сборка из исходников

Нужны Rust (stable, MSVC) и Windows SDK

```powershell
cd Исходники
cargo build --release
```

Готовый файл: `Исходники/target/release/oxide.exe`

Чтобы в exe не попали пути вашего компьютера, можно добавить флаги `--remap-path-prefix`
в `Исходники/.cargo/config.toml` (файл в репозиторий не входит)

## Лицензия

Код Oxide распространяется по лицензии [MIT](LICENSE). Сторонние библиотеки и их лицензии перечислены
в [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md)

Oxide - независимый проект. Он не связан с Microsoft, не одобрен и не поддерживается ею

Windows и Windows 11 - товарные знаки Microsoft Corporation. Шрифты и значки Windows программа
не содержит: она берёт их из системы на компьютере пользователя

## English

Oxide is a fast, lightweight file explorer for Windows written in Rust. It looks and works like
Windows 11 File Explorer, so there is nothing to relearn. The interface is available in English and
Russian: pick the language on first launch and change it later in Options. Oxide checks this page for
new versions and updates itself in place, keeping your settings, tabs, pinned folders and search index

Oxide can also open folders instead of File Explorer, including "Show in folder" from browsers (Options, Default file manager)

Properties show the size of a folder right away, and the Recycle Bin can restore items or delete them permanently

Download `Oxide.exe` from [Releases](../../releases) and run it, no installation needed. Requires
Windows 10 or 11 (x64)

Oxide is an independent project and is not affiliated with, endorsed or supported by Microsoft
