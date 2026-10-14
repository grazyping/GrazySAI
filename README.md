# SOsai

Редактор для рисования на Rust по образцу PaintTool SAI 2 — личный проект. Версия 0.4.11 —
что готово и что впереди, в [ROADMAP.md](ROADMAP.md) и на вкладке
[Milestones](https://github.com/grazyping/SOsai/milestones); готовые сборки — в разделе Releases. Linux и Windows,
перо планшета (XPPen Artist 24 Pro), открытие файлов `.sai2`.

## Что умеет

- Открывает `.sai2`: растровые слои, папки, прикрепление, режимы наложения (все 28), слои
  контура и фигур (кривые строятся заново, как в образце).
- Кисть по модели образца: отпечаток, край, непрозрачность, шаг (`docs/13-thin-brushes.md`).
- Кисть, Акварель и Маркер смешивают краску с холстом по правилам образца: смешивание,
  разбавление, стойкость, порог размытия (`docs/16-mixing.md`).
- Стабилизатор как в образце: 1–15 и S-1…S-7 (`docs/15-stabilizer.md`).
- Сохранение `.sai2` (`docs/03-sai2-format.md`).
- Выделение (прямоугольник, лассо, волшебная палочка), перемещение и трансформация (Ctrl+T)
  слоя или выделенного (`docs/17-selection-move.md`).
- Разрешение, размер, отражение и поворот холста; поворот и зеркало вида (`docs/18-canvas-view.md`).
- Пересэмплирование пяти видов, как в образце (`docs/12-resampling.md`).
- Окно в раскладке SAI 2 (`docs/04-ui-layout.md`, `docs/ref/paint-rs-ui.png`).
- Нажим пера: XInput 2 на Linux, Windows Ink на Windows; виртуальное перо и виртуальный
  планшет для проверок без планшета (`docs/14-virtual-pen.md`).

## Сборка

```bash
cargo build --release -p paint-app      # редактор: target/release/SOsai [файл.sai2]
cargo build --release -p paint-cli      # командная строка: сверки, рендер, разбор .sai2
cargo test --workspace --release
```

Сборка под Windows с Linux: цель `x86_64-pc-windows-gnullvm`, компоновщик llvm-mingw
(путь — в `.cargo/config.toml`, поправить под свою машину).

Тесты по файлам пользователя ищут образцы в `~/SAI2` (или в `SAI2_SAMPLES`); если их нет, эти
тесты пропускаются.

## Устройство

| Каталог | Что |
|---|---|
| `crates/paint-core` | Пиксели, плитки, слои, наложение, кисть (`dab.rs`), штрих, векторные фигуры, пересэмплирование |
| `crates/paint-formats` | Чтение `.sai2`: оглавление, слои, пиксели, векторные блоки |
| `crates/paint-app` | Окно редактора (egui) |
| `crates/paint-cli` | Командная строка для сверок с образцом |
| `crates/pen-test` | Журнал событий пера (Windows Ink, WinTab, XInput 2) |
| `crates/virtual-tablet` | Виртуальный планшет через `/dev/uinput` (Linux) |
| `docs/` | План, формат `.sai2`, правила образца своими словами, сверки, интерфейс |
| `tools/` | Проверка виртуальным пером |

## Лицензия

MIT — см. [LICENSE](LICENSE). SOsai — независимый проект и не связан с SYSTEMAX Software
Development, автором PaintTool SAI; названия SAI и PaintTool SAI принадлежат их владельцу.
Код написан заново по наблюдаемому поведению программы и формату файлов; кода PaintTool SAI в
репозитории нет.
