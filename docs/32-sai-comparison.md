# Сравнение с SAI 2017: функция за функцией

Цель — чтобы GrazySAI работал как оригинал. Здесь каждая команда SAI 2 (сборка 2017-01-19,
полный перечень — `07-sai2-reference.md`, раздел 6) и как она сделана у нас. Обновляется при
каждой сверке; найденные расхождения идут в дорожную карту.

Обозначения: **✔** — есть и сверено с живой SAI (или с её кодом); **≈** — есть, но работает
иначе или не сверено; **✘** — нет. «Своё» — сверх SAI.

## Файл

| SAI | GrazySAI | |
|---|---|---|
| New, Open, Save, Save As | Создать, Открыть, Сохранить, Сохранить как (+ `.ora`, `.psd`) | ✔ |
| Create Canvas from Clipboard | Холст из буфера обмена (Ctrl+B) | ✔ |
| Recent | Недавние (10) | ✔ |
| Export | Экспорт: PNG, JPEG, BMP, TGA, TIFF, WebP, QOI, PSD, OpenRaster | ≈ (у SAI — свой список) |
| Миниатюра `thum` в файле | пишется, размер — по правилу SAI | ✔ |
| Always Use FileViewer, Open/Save by FileViewer — свой обозреватель файлов с миниатюрами | системное окно | ✘ |
| Quit | Выход | ✔ |

## Правка

| SAI | GrazySAI | |
|---|---|---|
| Undo, Redo (100 шагов) | то же, история на диске сверх 1 ГБ | ✔ |
| Cut, Copy, Paste, Copy by Selection Bounds | то же, системный буфер | ✔ |
| Select All | Выделить всё | ✔ |

## Холст

| SAI | GrazySAI | |
|---|---|---|
| Change Canvas Resolution, Change Canvas Size | Разрешение и размер холста | ✔ |
| Trim Canvas by Selection | Обрезать по выделению | ✔ |
| Reverse Horizontal/Vertical, Rotate 90° | то же | ✔ |
| Background (белый, чёрный, прозрачный: шашки, белый, чёрный, цвет), Background Color | то же, по коду SAI, пишется в файл | ✔ |
| Layer Effect Color: Normal, Vivid, Deep, Multiply, Ver.1 Like | хранится в файле, меню нет, на вид не влияет | ✘ |

## Слой

| SAI | GrazySAI | |
|---|---|---|
| New Normal / Linework Layer, New Layer Folder | то же (+ слой фигур) | ✔ |
| New 1VP/2VP/3VP Perspective Ruler, Perspective Grid — слои | своя линейка перспективы (не слой, в файл не пишется) | ≈ |
| Copy, Delete, Erase Layer | то же | ✔ |
| Transfer into Next, Merge Layers, Merge Visible, Flatten | то же | ✔ |
| Fill, Outline | Заливка, Обводка (сверено) | ✔ |
| Rasterize, Convert Luminance to Opacity | то же | ✔ |
| Transform Layer (режимы, перспектива 0–100) | то же | ✔ |
| Reverse Layer, Rotate Layer 90° | то же | ✔ |
| Layer Property | Свойства слоя | ✔ |
| Nudge (стрелки) | Alt+стрелки (Shift — на 10) | ≈ (клавиши иные) |
| Cut & Nudge | — | ✘ |
| Preserve Opacity, Protect Painting / Moving / Modifying, Clipping, Selection Source | то же | ✔ |
| Select Forward/Backward Layer | — | ✘ |
| Open/Close Layer Folder | папки всегда раскрыты | ✘ |
| Bring Forward / Send Backward | Поднять / Опустить | ✔ |
| Bring to Front / Send to Back | — | ✘ |
| Change Visibility, Show Alone | глаз в списке; «показать только этот» нет | ≈ |
| Mark Layer Folder for Visibility Change | — | ✘ |
| Layer Mask: New, Apply | то же | ✔ |

## Выделение

| SAI | GrazySAI | |
|---|---|---|
| Deselect, Invert, Show/Hide Selection | то же | ✔ |
| Dilate / Erode Selection (окно) | Расширить / Сузить на 1–25 px, углы как у SAI | ✔ |
| Dilate / Erode Selection 1px (сразу) | — | ✘ |
| Region from Pixel Opacity: Set / Add / Delete / Cull | по слою с Ctrl/Shift/Alt в списке слоёв | ≈ (нет пунктов меню) |
| Select CPs in Selection, Select Strokes with Selection, Deselect All CPs | — | ✘ |

## Линейка

| SAI | GrazySAI | |
|---|---|---|
| Show/Hide, Straight, Ellipse, Reset, Lock Pivot | то же, сверено | ✔ |
| Перспективные линейки и сетки (точки схода, плоскости) | своя линейка 1–3 точки, без сетки | ≈ |

## Фильтр

| SAI | GrazySAI | |
|---|---|---|
| Hue/Saturation, Brightness/Contrast, Gaussian Blur (в 2017 работают только они) | то же, по коду SAI | ✔ |
| остальные пункты SAI 2017 — «Not Implemented» | фильтры как в Clip Studio Paint | своё |

## Вид

| SAI | GrazySAI | |
|---|---|---|
| New View, New Floating View | то же | ✔ |
| Close View | Закрыть (Ctrl+W) закрывает вид | ✔ |
| Close All Views | — | ✘ |
| Zoom In / Out, Display at 100% | то же | ✔ |
| Zoom In/Out at Cursor Position | колесо — к курсору; клавиш нет | ≈ |
| Reverse Horizontal, Rotate CCW/CW, Reset Position / Angle | то же | ✔ |
| Select Previous / Next View | — | ✘ |
| Scroll View Up/Down/Left/Right | — | ✘ |

## Окно

| SAI | GrazySAI | |
|---|---|---|
| Scaling of UI, Brush Size Circle, Dot Cursor, Full Screen, Show All Panels | то же | ✔ |
| Панели справа/слева, HSV/HSL Mode, Swatches Size | то же, по коду SAI | ✔ |
| Show: Navigator, Layer, Color, Tool Panel, Quick Bar, View Selector, Status Bar, Scroll Bar | только «все панели» разом | ✘ |
| Show: Color Circle, RGB Slider, HSV Slider, Color Mixer, Swatches | кнопки панели цвета | ≈ |
| Scratch Pad — черновик для смешивания цветов | — | ✘ |
| Separate User Interface Panel (плавающие панели) | — | ✘ |

## Помощь

| SAI | GrazySAI | |
|---|---|---|
| Shortcut Keys | Быстрые клавиши (переназначаются) | ✔ |
| Options (память, история, перо и др.) | — | ✘ |

## Цвет и кисть (клавиши)

| SAI | GrazySAI | |
|---|---|---|
| Exchange FG/BG | X | ✔ |
| Switch to Transparent | — | ✘ |
| Pick Color from Merged / Current Layer | Alt + щелчок; «Только текущий слой» у пипетки | ≈ (нет отдельных клавиш) |
| Switch to Straight Line Mode | кнопка «Прямая линия», Shift + нажатие | ✔ |
| Temporarily Ignore Ruler | «Не замечать линейку» | ✔ |
| Previous / Next Size in Brush Size List | [ и ] | ✔ |
| Change Brush Density to 10 … 100 | — | ✘ |

## Кисти

| SAI | GrazySAI | |
|---|---|---|
| Карандаш, Аэрограф, Кисть, Акварель, Маркер, Ластик, двоичное перо | по коду SAI, сверено вживую | ✔ |
| Акварель с «Усилением плотности» | ближе к SAI, правило подобрано по замерам (причина в коде не найдена) | ≈ |
| Двоичное перо при наклоне 0,4 | лишняя точка SAI не повторена | ≈ |
| Форма, текстура, щетина | по коду SAI, сверено | ✔ |
| Текст: набор на холсте, настройки на панели | как в SAI | ✔ |
