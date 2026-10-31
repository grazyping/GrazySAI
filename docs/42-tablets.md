# Графические планшеты

[English](42-tablets.en.md) | **Русский**

Как GrazySAI получает перо и что нужно от драйвера. Сведено по документации производителей,
ядра Linux и обсуждениям пользователей (октябрь 2026).

## Откуда программа берёт перо

| Система | Способ | Что даёт | Как включить |
|---|---|---|---|
| Windows | **WinTab** (`Wintab32.dll` ставит драйвер) | нажим, наклон, поворот, обратный конец пера | «Настройки → Windows: перо через WinTab» (пока выключено по умолчанию: на настоящем планшете ещё не проверено — напишите, работает ли у вас) |
| Windows | **Windows Ink** (система) | нажим | по умолчанию; в драйвере — галочка «Windows Ink» |
| Linux, X11 и XWayland | **XInput 2** | нажим, наклон, поворот, обратный конец пера | ничего не нужно |
| Linux, Wayland напрямую | `zwp_tablet_v2` | — | пока нет: окно открывается через XWayland |
| FreeBSD | XInput 2 (как Linux) | нажим, наклон | см. раздел «Системы» дорожной карты |

Если работают оба способа Windows, берётся WinTab, пока от него идут пакеты; без них — Windows Ink.
В строке состояния видно, откуда сейчас нажим: «перо (WinTab)», «перо (Windows Ink)», «перо
(XInput 2)» или «мышь».

Наклон и поворот идут в динамику кисти: входы «Наклон пера», «Куда наклонено перо», «Поворот
пера» (панель кисти → «Динамика»).

## Бренды и драйверы

| Бренд | Линейки | Windows: WinTab / Ink | Linux |
|---|---|---|---|
| Wacom | One, Intuos, Intuos Pro, Cintiq, Cintiq Pro, Movink | оба (WinTab — родной для Wacom) | драйвер `wacom` в ядре и `xf86-input-wacom` / libinput |
| XP-Pen | Deco, Deco Pro, Artist, Artist Pro, Innovator | оба; Ink — галочка в драйвере | `hid-uclogic` в ядре (модели UGEE v2, Artist 24 Pro — с 6.19) и свой драйвер XP-Pen |
| Huion | Inspiroy, Kamvas, Kamvas Pro | оба; Huion советует, чтобы режим драйвера совпадал с режимом программы | `hid-uclogic` / DIGImend и свой драйвер Huion |
| Gaomon | S, M, PD | оба | `hid-uclogic` (многие модели — те же контроллеры, что у Huion) |
| UGEE | M, S, U, Ugee 2150 | оба | `hid-uclogic` (с Linux 5.1 и новее) |
| Veikk | A, VK, Studio | оба | свой драйвер Veikk; в ядре — частично |
| Xencelabs | Pen Tablet, Pen Display | оба | свой драйвер Xencelabs |
| Parblo, Artisul, Simbans, BOSTO | разные | обычно оба | чаще всего `hid-uclogic` или общий HID |

Совет для Windows: если нажима нет — переключить в драйвере «Windows Ink» (вкл/выкл) и
перезапустить драйвер и GrazySAI; в GrazySAI можно выключить WinTab в настройках.

## Что ставят драйверы (проверено по самим драйверам, октябрь 2026)

Скачаны и разобраны официальные драйверы Linux: XP-Pen 4.0.15, Huion 15.0.0.162, UGEE 4.3.4,
Veikk 3.5.10, Xencelabs 1.3.0 (Gaomon 16.0.0.49 — ссылка не отдала файл). Все они читают планшет
сами и создают виртуальное перо через `/dev/uinput`; в X11 его оси называются «Abs Pressure»,
«Abs Tilt X», «Abs Tilt Y» — ровно те, что читает GrazySAI. Значит, с фирменным драйвером и без
него (драйвер ядра `wacom` / `hid-uclogic`) нажим и наклон приходят одинаково.

Windows: драйвер XP-Pen 4.0.19 поставлен на тестовый ПК — он кладёт `Wintab32.dll`; GrazySAI
с включённым WinTab загружает её и без подключённого планшета пишет «WinTab есть, но планшет не
найден — Windows Ink».

Распакованы без запуска драйверы для Windows: Wacom 6.4.12-3, Gaomon 16.0.0.194, XP-Pen 4.0.19. У Wacom
и Gaomon `Wintab32.dll` (x86 и x64; у Wacom ещё ARM64) отдаёт все функции, которые вызывает GrazySAI, включая
`WTInfoW` и `WTOpenW`; в установщике XP-Pen 4.0.19 библиотека упакована глубже и прочитана только на
тестовом ПК. Драйвер Gaomon держит список программ в `WintabMapMode.ini` (там есть SAI и SAI2, GrazySAI
нет — работает режим по умолчанию). Единицы наклона задаёт драйвер, поэтому GrazySAI теперь берёт их из
`DVC_ORIENTATION`, а не считает их десятыми градуса; обратный конец пера определяется ещё и по `TPS_INVERT`.
С настоящим планшетом под Windows это не проверено. Драйвер Huion для Windows не скачан: страница не
отдаёт прямых ссылок.

Wacom: драйвер X11 `xf86-input-wacom` 1.2.3 (проверено по исходникам) подписывает оси так же — «Abs Pressure», «Abs Tilt X/Y», а поворот Art Pen и колёсико — «Abs Rotation Z» и «Abs Wheel», их GrazySAI тоже читает. Драйверы Windows Wacom, Huion и Gaomon скачиваются только со страниц выбора модели; прямых ссылок нет, поэтому не разобраны.

## Проверенные модели

| Модель | Система | Способ | Что проверено |
|---|---|---|---|
| XP-Pen Artist 24 Pro | Linux (X11, XWayland) | XInput 2 | нажим, обратный конец пера |
| Виртуальное перо (`PAINT_VIRTUAL_PEN`) | Linux, Windows (Wine) | Windows Ink | нажим, касания, ластик |
| — | Windows под Wine, без планшета | WinTab | программа запускается, без планшета остаётся на Windows Ink; с настоящим планшетом WinTab ещё не проверен |

Другие модели — по отзывам: пишите в [Issues](https://github.com/grazyping/GrazySAI/issues),
какая модель и что в строке состояния.

## Источники

- Huion: [нажим не работает в программе](https://support.huion.com/en/support/solutions/articles/44002618549), [настройки драйвера V15](https://support.huion.com/en/support/solutions/articles/44002356041)
- Krita: [WinTab и Windows Ink](https://krita-artists.org/t/difference-between-wintab-and-windows-ink-input/12781)
- Ядро Linux: [XP-Pen Artist 24 Pro в 6.19](https://www.phoronix.com/news/Linux-6.19-XP-PEN-Artist-24-Pro), [новые планшеты UC-Logic в 5.1](https://www.phoronix.com/news/Linux-5.1-UC-Logic-Additions), [XP-Pen Deco L](https://www.phoronix.com/news/Linux-5.20-XP-PEN-Deco-L), [DIGImend](https://github.com/DIGImend/digimend-kernel-drivers/releases)
