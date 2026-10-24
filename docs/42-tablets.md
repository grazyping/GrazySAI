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
