//! Окно редактора: холст, панели по образцу, рисование на выбранном слое, открытие `.sai2`.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod assets;
mod i18n;
mod file_viewer;
mod icons;
mod pen;
mod ruler;
mod sai_ui;
mod keys;
mod settings;
mod update;
mod files;
mod virtual_pen;
mod clip;
mod self_test;
mod plugins;
mod recording;
mod scene3d;
mod lassofill;

use i18n::{tr, trf};
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use paint_core::tile::{Key, TILE};
use paint_core::mix::MixParams;
use paint_core::selection::Combine;
use paint_brush::{TexChoice, ToolKind};
use settings::SettingsFile;
use paint_core::{
    BlendMode, BrushParams, Document, InputPoint, Layer, LiveStroke, Stabilizer, Stroke,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Диапазоны полоски размера кисти, как в SAI 2: имя, наименьший и наибольший размер.
const SIZE_RANGES: [(&str, f32, f32); 5] = [("×0.1", 0.1, 10.0), ("×1", 1.0, 100.0), ("×5", 5.0, 500.0), ("×10", 10.0, 1000.0), ("×50", 50.0, 5000.0)];

/// Плитка показа: 256×256 точек, то есть 4×4 плитки ядра. Одна текстура на плитку.
const DT: i32 = 256;

/// Режимы в том же порядке, что в списке образца.
const MODES: [(BlendMode, &str); 28] = [
    (BlendMode::Normal, "Обычный"),
    (BlendMode::Multiply, "Умножение"),
    (BlendMode::Screen, "Экран"),
    (BlendMode::Overlay, "Перекрытие"),
    (BlendMode::Shade, "Тень"),
    (BlendMode::Shine, "Сияние"),
    (BlendMode::ShadeShine, "Тень/Сияние"),
    (BlendMode::Burn, "Затемнение"),
    (BlendMode::Dodge, "Осветление"),
    (BlendMode::BurnDodge, "Затемнение/Осветление"),
    (BlendMode::Darken, "Темнее"),
    (BlendMode::ColorBurn, "Затемнение основы"),
    (BlendMode::LinearBurn, "Линейный затемнитель"),
    (BlendMode::DarkerColor, "Темнее цвет"),
    (BlendMode::Lighten, "Светлее"),
    (BlendMode::ColorDodge, "Осветление основы"),
    (BlendMode::LinearDodge, "Линейный осветлитель"),
    (BlendMode::LighterColor, "Светлее цвет"),
    (BlendMode::SoftLight, "Мягкий свет"),
    (BlendMode::HardLight, "Жёсткий свет"),
    (BlendMode::VividLight, "Яркий свет"),
    (BlendMode::LinearLight, "Линейный свет"),
    (BlendMode::PinLight, "Точечный свет"),
    (BlendMode::HardMix, "Жёсткое смешение"),
    (BlendMode::Difference, "Разница"),
    (BlendMode::Exclusion, "Исключение"),
    (BlendMode::Subtract, "Вычитание"),
    (BlendMode::Divide, "Деление"),
];

/// Сетка размеров кисти — таблица SAI 2017 целиком (0,6…5000).
const SIZES: [f32; 50] = [
    0.6, 0.8, 1.0, 1.3, 1.6, 2.0, 2.5, 3.0, 3.5, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 12.0, 14.0,
    16.0, 20.0, 25.0, 30.0, 35.0, 40.0, 50.0, 60.0, 70.0, 80.0, 100.0, 120.0, 160.0, 200.0, 250.0,
    300.0, 350.0, 400.0, 450.0, 500.0, 600.0, 700.0, 800.0, 1000.0, 1200.0, 1600.0, 2000.0, 2500.0,
    3000.0, 3500.0, 4000.0, 5000.0,
];

/// Настройки «Окно → Показать»: ключи и названия (по порядку `WindowOpts::show`).
const SHOW_KEYS: [&str; 7] = ["show_navigator", "show_layers", "show_color", "show_tools", "show_quickbar", "show_tabs", "show_status"];
const SHOW_NAMES: [&str; 7] = ["Навигатор", "Слои", "Цвет", "Инструменты", "Быстрая панель", "Вкладки видов", "Строка состояния"];

/// Пункты «Файл → Экспорт»: сведённые картинки, затем форматы со слоями.
const EXPORTS: [(&str, &str); 10] = [
    ("PNG (сведённая картинка)", "png"),
    ("JPEG (качество 92)", "jpg"),
    ("BMP", "bmp"),
    ("TGA", "tga"),
    ("TIFF", "tiff"),
    ("WebP (без потерь)", "webp"),
    ("QOI", "qoi"),
    ("PSD (со слоями)", "psd"),
    ("PSB (большой PSD, со слоями)", "psb"),
    ("OpenRaster (.ora, со слоями)", "ora"),
];

/// Следующее свободное имя с номером: `рисунок.sai2` → `рисунок_001.sai2`, `рисунок_009.sai2` →
/// `рисунок_010.sai2` (ширина номера сохраняется, не меньше трёх цифр); занятые номера пропускаются.
fn numbered_path(path: &std::path::Path) -> PathBuf {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned());
    let (base, mut n, width) = match stem.rsplit_once('_') {
        Some((b, d)) if !d.is_empty() && d.len() <= 9 && d.bytes().all(|c| c.is_ascii_digit()) => (b.to_string(), d.parse::<u64>().unwrap_or(0), d.len().max(3)),
        _ => (stem.clone(), 0, 3),
    };
    loop {
        n += 1;
        let name = match &ext {
            Some(e) => format!("{base}_{n:0width$}.{e}"),
            None => format!("{base}_{n:0width$}"),
        };
        let candidate = path.with_file_name(name);
        if !candidate.exists() {
            return candidate;
        }
    }
}

/// Пиксельная сетка: линии между пикселями холста в видимой части окна (при большом увеличении).
fn draw_pixel_grid(painter: &egui::Painter, xf: ViewXf, rect: Rect, w: f32, h: f32) {
    let corners = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()].map(|p| xf.to_canvas(p));
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for c in corners {
        (x0, y0, x1, y1) = (x0.min(c.x), y0.min(c.y), x1.max(c.x), y1.max(c.y));
    }
    let (x0, y0) = (x0.floor().max(0.0), y0.floor().max(0.0));
    let (x1, y1) = (x1.ceil().min(w), y1.ceil().min(h));
    if x1 <= x0 || y1 <= y0 || (x1 - x0) * (y1 - y0) > 400_000.0 {
        return;
    }
    let stroke = egui::Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(128, 128, 128, 90));
    let mut x = x0;
    while x <= x1 {
        painter.line_segment([xf.to_screen(Vec2::new(x, y0)), xf.to_screen(Vec2::new(x, y1))], stroke);
        x += 1.0;
    }
    let mut y = y0;
    while y <= y1 {
        painter.line_segment([xf.to_screen(Vec2::new(x0, y)), xf.to_screen(Vec2::new(x1, y))], stroke);
        y += 1.0;
    }
}

/// Расширение формата со слоями, в который программа сохраняет (`sai2`, `ora`, `psd`, `psb`).
fn layered_ext(path: &std::path::Path) -> Option<&'static str> {
    let e = path.extension()?.to_string_lossy().to_ascii_lowercase();
    ["sai2", "ora", "psd", "psb"].into_iter().find(|x| *x == e)
}

#[derive(Clone, Copy, PartialEq)]
enum Tool {
    /// Кисть из сетки инструментов слоя (`PaintApp::brush`).
    Brush,
    Hand,
    Picker,
    Zoom,
    /// Выделение прямоугольником (M).
    Marquee,
    /// Лассо (L).
    Lasso,
    /// Волшебная палочка (W).
    Wand,
    /// Заливка: связная область похожего цвета (допуск — общий с палочкой).
    Fill,
    /// Перемещение слоя или выделенного (то же — Ctrl + перетаскивание).
    Move,
    /// Поворот вида перетаскиванием вокруг середины окна (как инструмент SAI).
    RotateView,
    /// Текст: щелчок — новая надпись, по текстовому слою — правка (docs/30-text-layers.md).
    Text,
}

/// Надпись, которая сейчас правится в окне «Текст».
struct TextEditing {
    /// Слой надписи (новый слой вставлен сразу, без записи в историю).
    path: Vec<usize>,
    x: f64,
    y: f64,
    text: String,
    /// Курсор — номер знака.
    caret: usize,
    /// Слой создан этой правкой (при отмене исчезает).
    created: bool,
    /// Кривые и пиксели слоя до правки — вернуть перед записью шага.
    orig: Option<(Option<paint_core::layer::VectorData>, paint_core::tile::ColorMap)>,
    /// Остальное из файла (флаги, матрица) — сохраняется как было.
    base: Option<paint_formats::sai2_text::TextLayer>,
    /// Текст или настройки менялись.
    changed: bool,
}

/// Перевод координат между экраном и холстом с учётом масштаба, поворота и зеркала вида.
/// `offset` — точка холста в левом верхнем углу области просмотра без поворота; поворот и
/// зеркало — вокруг середины области.
#[derive(Clone, Copy)]
struct ViewXf {
    mid: Pos2,
    center: Vec2,
    zoom: f32,
    cos: f32,
    sin: f32,
    flip: bool,
}

impl ViewXf {
    fn new(rect: Rect, zoom: f32, offset: Vec2, angle: f32, flip: bool) -> Self {
        let half = rect.size() / 2.0;
        let (sin, cos) = angle.sin_cos();
        Self { mid: rect.min + half, center: offset + half / zoom, zoom, cos, sin, flip }
    }

    fn to_screen(self, c: Vec2) -> Pos2 {
        let mut d = (c - self.center) * self.zoom;
        if self.flip {
            d.x = -d.x;
        }
        self.mid + Vec2::new(self.cos * d.x - self.sin * d.y, self.sin * d.x + self.cos * d.y)
    }

    fn vec_to_canvas(self, d: Vec2) -> Vec2 {
        let mut v = Vec2::new(self.cos * d.x + self.sin * d.y, -self.sin * d.x + self.cos * d.y);
        if self.flip {
            v.x = -v.x;
        }
        v / self.zoom
    }

    fn to_canvas(self, p: Pos2) -> Vec2 {
        self.center + self.vec_to_canvas(p - self.mid)
    }
}

/// Инструмент слоя контура.
#[derive(Clone, Copy, PartialEq)]
enum LineTool {
    Pen,
    Eraser,
    Weight,
    Color,
    Edit,
    Pressure,
}

/// Идёт правка кривых слоя контура: рабочая копия кривых, исходные кривые и пиксели (для
/// отмены одной записью), захваченный узел.
struct LineEdit {
    path: Vec<usize>,
    shapes: Vec<paint_formats::sai2_vector::Shape>,
    orig_pixels: paint_core::tile::ColorMap,
    node: Option<(usize, usize, usize)>,
    start_y: f32,
    start_weight: f32,
    changed: bool,
    /// Точка нажатия; узел сдвинулся (иначе щелчок по узлу — выделить его или снять).
    start: [f64; 2],
    moved: bool,
    /// Захвачен выделенный узел — едут все выделенные (как в SAI): узел и его место до сдвига.
    group: Vec<((usize, usize, usize), [f64; 2])>,
    /// Кривые менялись — слой ещё не перерисован.
    stale: bool,
}

/// Выделенные узлы слоя контура (Select CPs in Selection и т. п. в SAI). Действуют, пока
/// кривые слоя те же (`raw` — отпечаток данных слоя).
struct CpSelection {
    path: Vec<usize>,
    raw: u64,
    nodes: std::collections::BTreeSet<(usize, usize, usize)>,
}

/// Отпечаток данных кривых слоя (для кэша показа и проверки выделения узлов).
fn raw_hash(raw: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    raw.hash(&mut h);
    h.finish()
}

/// Узел на холсте (с учётом сдвига пути).
fn node_pos(shapes: &[paint_formats::sai2_vector::Shape], (si, pi, ni): (usize, usize, usize)) -> Option<[f64; 2]> {
    let p = shapes.get(si)?.paths.get(pi)?;
    let c = p.points.get(ni)?;
    Some([c.point[0] + p.offset[0], c.point[1] + p.offset[1]])
}

/// Опорная картинка: своё окно поверх холста, непрозрачность, масштаб; пипетка или Alt + щелчок берёт цвет.
struct RefImage {
    id: u64,
    name: String,
    size: [usize; 2],
    rgba: Vec<u8>,
    tex: Option<egui::TextureHandle>,
    opacity: f32,
    /// Масштаб картинки в окне (вначале — чтобы большая сторона была не больше 360 px).
    scale: f32,
}

/// Какой фильтр открыт.
#[derive(Clone, Copy, PartialEq)]
enum FilterKind {
    HueSaturation,
    BrightnessContrast,
    GaussianBlur,
    /// «Резкость» и «Сильная резкость» (как в Clip Studio Paint) — без настроек.
    Sharpen,
    SharpenMore,
    /// «Нерезкая маска»: радиус, сила, порог.
    Unsharp,
    /// «Изменение толщины линий» и «Удаление пыли» (Коррекция в CSP).
    LineWidth,
    Dust,
    /// «Эффекты» и «Отрисовка» как в CSP.
    Aberration,
    Noise,
    Mosaic,
    Crystallize,
    RetroFilm,
    Perlin,
    /// «Карандашный рисунок» и «Художественный» — свои версии (у CSP способ закрыт).
    Pencil,
    Artistic,
    /// «Размытие» как в CSP.
    Average,
    Blur,
    BlurStrong,
    Motion,
    Radial,
    Spin,
    Smoothing,
    Lens,
    /// «Искажение» как в CSP.
    Distort(paint_core::filters::DistortKind),
    /// «Уровни», «Кривые» (цветокоррекция) и «Медиана» (коррекция).
    Levels,
    Curves,
    Median,
}

/// Пиксели слоя после фильтра (с учётом выделения); `color` — основной цвет (для «Удаления пыли»).
fn filter_result(f: &FilterDialog, w: u32, h: u32, color: Color32) -> paint_core::tile::ColorMap {
    use paint_core::filters::{gaussian_blur, map_pixels};
    let sel = &*f.sel;
    let amount = |x: i32, y: i32| sel.amount(x, y);
    match f.kind {
        FilterKind::HueSaturation => {
            let hs = f.hs;
            map_pixels(&f.original, &amount, &|p| hs.apply(p))
        }
        FilterKind::BrightnessContrast => {
            let bc = f.bc;
            map_pixels(&f.original, &amount, &|p| bc.apply(p))
        }
        FilterKind::GaussianBlur => gaussian_blur(&f.original, w, h, f.radius, &amount),
        FilterKind::Levels => {
            let l = f.levels;
            map_pixels(&f.original, &amount, &|p| l.apply(p))
        }
        FilterKind::Curves => {
            let c = f.curve;
            map_pixels(&f.original, &amount, &|p| paint_core::filters::curves(&c, p))
        }
        FilterKind::Median => paint_core::filters::median(&f.original, w, h, f.median_r, &amount),
        FilterKind::Sharpen | FilterKind::SharpenMore => {
            let s = if f.kind == FilterKind::Sharpen { 0.5 } else { 1.0 };
            paint_core::filters::sharpen(&f.original, w, h, s, &amount)
        }
        FilterKind::Unsharp => paint_core::filters::unsharp_mask(&f.original, w, h, f.radius, f.amount, f.threshold, &amount),
        FilterKind::LineWidth => paint_core::filters::adjust_line_width(&f.original, w, h, f.width_px, f.thicken, &amount),
        FilterKind::Dust => {
            use paint_core::filters::DustMode;
            let c = color;
            let mode = match f.dust_mode {
                1 => DustMode::FromWhite,
                2 => DustMode::GapsSurrounding,
                3 => DustMode::GapsColor([c.r(), c.g(), c.b()]),
                _ => DustMode::FromTransparency,
            };
            paint_core::filters::remove_dust(&f.original, w, h, f.dust_size, mode, &amount)
        }
        FilterKind::Aberration => paint_core::filters::chromatic_aberration(&f.original, w, h, f.ca_radial, f.ca_px, f.ca_angle, &amount),
        FilterKind::Noise => paint_core::filters::noise(&f.original, w, h, f.noise_color, f.noise_amount, &amount),
        FilterKind::Mosaic => paint_core::filters::mosaic(&f.original, w, h, f.tile, &amount),
        FilterKind::Crystallize => paint_core::filters::crystallize(&f.original, w, h, f.cell, f.randomness, f.tiling, &amount),
        FilterKind::RetroFilm => paint_core::filters::retro_film(&f.original, w, h, f.retro, &amount),
        FilterKind::Perlin => paint_core::filters::perlin_noise(&f.original, w, h, f.perlin, &amount),
        FilterKind::Average => paint_core::filters::average(&f.original, w, h, &amount),
        FilterKind::Blur | FilterKind::BlurStrong => {
            paint_core::filters::box_blur(&f.original, w, h, if f.kind == FilterKind::Blur { 1 } else { 2 }, &amount)
        }
        FilterKind::Motion => paint_core::filters::motion_blur(&f.original, w, h, f.motion, &amount),
        FilterKind::Radial => {
            let c = paint_core::filters::content_center(&f.original, w, h);
            paint_core::filters::radial_blur(&f.original, w, h, c, f.radial, &amount)
        }
        FilterKind::Spin => {
            let c = paint_core::filters::content_center(&f.original, w, h);
            paint_core::filters::spin_blur(&f.original, w, h, c, f.spin_angle, f.spin_dir, &amount)
        }
        FilterKind::Smoothing => paint_core::filters::smoothing(&f.original, w, h, &amount),
        FilterKind::Lens => paint_core::filters::lens_blur(&f.original, w, h, f.lens_radius, f.lens_sides, f.lens_highlight, &amount),
        FilterKind::Pencil => paint_core::filters::pencil_drawing(&f.original, w, h, f.pencil, &amount),
        FilterKind::Artistic => paint_core::filters::artistic(&f.original, w, h, f.artistic, &amount),
        FilterKind::Distort(_) => paint_core::filters::distort(&f.original, w, h, &f.distort, &amount),
    }
}

/// Ноутбуки с двумя видеокартами (Windows): драйверы NVIDIA и AMD находят эти имена в таблице
/// экспорта программы (её задаёт `build.rs`) и рисуют на дискретной карте, а не на встроенной.
#[cfg(windows)]
#[no_mangle]
#[used]
pub static NvOptimusEnablement: u32 = 1;
#[cfg(windows)]
#[no_mangle]
#[used]
pub static AmdPowerXpressRequestHighPerformance: u32 = 1;

/// Точки пера за кадр: если их меньше `want`, путь от прошлой точки `last` через `moves`
/// делится на `want` равных по длине шагов (нажим — плавно от прошлого к `pressure`).
fn resample_moves(last: (Pos2, f32), moves: &[Pos2], pressure: f32, want: usize) -> Vec<(Pos2, f32)> {
    if moves.len() >= want {
        return moves.iter().map(|&p| (p, pressure)).collect();
    }
    let mut path = vec![last.0];
    path.extend_from_slice(moves);
    let lens: Vec<f32> = path.windows(2).map(|w| w[0].distance(w[1])).collect();
    let total: f32 = lens.iter().sum();
    if total <= 0.0 {
        return moves.iter().map(|&p| (p, pressure)).collect();
    }
    let mut out = Vec::with_capacity(want);
    let (mut seg, mut before) = (0, 0.0);
    for k in 1..=want {
        let t = k as f32 / want as f32;
        let at = total * t;
        while seg + 1 < lens.len() && before + lens[seg] < at {
            before += lens[seg];
            seg += 1;
        }
        let f = if lens[seg] > 0.0 { ((at - before) / lens[seg]).clamp(0.0, 1.0) } else { 1.0 };
        out.push((path[seg] + (path[seg + 1] - path[seg]) * f, last.1 + (pressure - last.1) * t));
    }
    out
}

/// Чтение файла: сбой разбора (испорченный файл) — ошибка, а не падение программы со всеми
/// вкладками.
fn read_guarded<T>(f: impl FnOnce() -> Result<T, Box<dyn std::error::Error>>) -> Result<T, String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r.map_err(|e| e.to_string()),
        Err(_) => Err(String::from(tr("файл повреждён"))),
    }
}

/// Открытый фильтр: настройки, слой и его исходные пиксели (для предпросмотра и отмены).
/// Копия дешёвая (пиксели и выделение — общие): её считает фоновый поток предпросмотра.
#[derive(Clone)]
struct FilterDialog {
    kind: FilterKind,
    hs: paint_core::filters::HueSaturation,
    bc: paint_core::filters::BrightnessContrast,
    levels: paint_core::filters::Levels,
    curve: paint_core::curve::PressureCurve,
    median_r: u32,
    radius: f64,
    /// Нерезкая маска: сила (1 = 100 %) и порог 0–255.
    amount: f64,
    threshold: u8,
    /// Толщина линий: на сколько px и утолщить ли; пыль: размер (точек) и что делать (0–3).
    width_px: u32,
    thicken: bool,
    dust_size: u32,
    dust_mode: u8,
    /// Эффекты: аберрация (по кругу, px, угол), шум (цветной, сила), мозаика, кристаллизация
    /// (ячейка, разброс, бесшовно), ретро-плёнка, шум Перлина.
    ca_radial: bool,
    ca_px: f64,
    ca_angle: f64,
    noise_color: bool,
    noise_amount: f64,
    tile: u32,
    cell: u32,
    randomness: f64,
    tiling: bool,
    retro: paint_core::filters::RetroFilm,
    perlin: paint_core::filters::Perlin,
    /// Размытие: в движении, радиальное, вращением (угол и направление), линза (радиус, углы, блики).
    motion: paint_core::filters::Streak,
    radial: paint_core::filters::Streak,
    spin_angle: f64,
    spin_dir: u8,
    lens_radius: f64,
    lens_sides: u32,
    lens_highlight: f64,
    /// Искажение: середина, радиус, сила, волна, полярный режим.
    distort: paint_core::filters::Distort,
    pencil: paint_core::filters::Pencil,
    artistic: paint_core::filters::Artistic,
    preview: bool,
    path: Vec<usize>,
    original: Arc<paint_core::tile::ColorMap>,
    /// Выделение на момент открытия (фильтр действует по нему).
    sel: Arc<paint_core::selection::Selection>,
    /// Настройки изменились — пересчитать предпросмотр.
    dirty: bool,
}

/// Замер кадров во время штриха (`PAINT_FRAMES`): длительность кадров и счёта штриха, мс.
#[derive(Default)]
struct FrameStats {
    frames: Vec<f32>,
    paint: Vec<f32>,
    cur_paint: f32,
    moves: usize,
    /// Сумма за штрих: пересчёт плиток и подготовка текстур, мс.
    render: f32,
    upload: f32,
    add: f32,
}

/// Кадр идёт, пока жив: при выходе из `update` пишет длительность, если рисуется штрих.
struct FrameTimer(std::rc::Rc<std::cell::RefCell<FrameStats>>, Instant);

impl Drop for FrameTimer {
    fn drop(&mut self) {
        let mut s = self.0.borrow_mut();
        let paint = std::mem::take(&mut s.cur_paint);
        s.frames.push(self.1.elapsed().as_secs_f32() * 1000.0);
        s.paint.push(paint);
    }
}

/// Предпросмотр фильтра, который считается в фоне: номер настроек и ответ потока.
struct FilterJob {
    gen: u64,
    rx: std::sync::mpsc::Receiver<paint_core::tile::ColorMap>,
}

/// «Помощь → Настройки…», как Options в SAI 2017, плюс память истории и копия для восстановления.
#[derive(Clone, Copy, PartialEq)]
struct Options {
    /// Двойной щелчок по слою открывает его свойства.
    dbl_props: bool,
    /// Колесо мыши масштабирует наоборот.
    swap_zoom: bool,
    /// Клавиши размера кисти меняют его на четверть, а не по списку размеров.
    size_relative: bool,
    /// Удержание клавиши инструмента, цвета или функции (зеркало вида, «не замечать линейку»,
    /// прямая линия) дольше `spring_ms` — временное переключение до отпускания (shift behavior).
    spring_tools: bool,
    spring_color: bool,
    spring_funcs: bool,
    spring_ms: u32,
    /// Повторное нажатие клавиши того же инструмента возвращает прежний.
    duplex_revert: bool,
    /// Ctrl + перетаскивание не двигает слой столько мс после сочетания с модификатором.
    move_guard_ms: u32,
    /// Память истории до переноса шагов на диск, МБ; копия для восстановления — раз в N минут.
    history_mb: u32,
    recovery_min: u32,
    /// Кривая нажима пера (прямая — нажим как есть).
    pressure_curve: paint_core::curve::PressureCurve,
    /// Правый щелчок по холсту — палитра у курсора (недавние цвета, образцы, кисти).
    popup_palette: bool,
    /// PSD и PSB — 16 бит на канал (с профилем sRGB) вместо 8.
    psd16: bool,
    /// PSD и PSB: слои с режимами SAI (их нет в PSD) — обычными слоями с готовым видом.
    psd_exact: bool,
    /// «Минимальный нажим для мазка» (SAI 2024+): слабее — перо не рисует, сильнее — нажим
    /// отсчитывается от этого порога. 0..0.9.
    min_pressure: f32,
}

impl Default for Options {
    fn default() -> Self {
        Self { dbl_props: true, swap_zoom: false, size_relative: false, spring_tools: false, spring_color: false, spring_funcs: false, spring_ms: 250, duplex_revert: false, move_guard_ms: 300, history_mb: 1024, recovery_min: 3, pressure_curve: paint_core::curve::PressureCurve::default(), popup_palette: false, psd16: false, psd_exact: false, min_pressure: 0.0 }
    }
}

/// Что вернуть, когда отпустят удерживаемую клавишу (временное переключение).
#[derive(Clone, Copy)]
enum SpringUndo {
    Tool(Tool, usize),
    Color,
    Flip,
    IgnoreRuler,
    Straight,
}

/// Черновик (Scratch Pad в SAI): маленький белый холст на панели цвета — рисовать текущей
/// кистью, чтобы смешать цвет, и брать с него цвет (Alt + щелчок или правая кнопка).
struct ScratchPad {
    doc: Document,
    live: Option<LiveStroke>,
    tex: Option<egui::TextureHandle>,
    dirty: bool,
    /// Физических пикселей экрана на точку интерфейса: черновик рисуется в пикселях экрана, как
    /// в SAI (при масштабе Windows 125–150 % иначе картинка растягивается и теряет чёткость).
    scale: f32,
}

impl ScratchPad {
    /// Размер в точках интерфейса.
    const W: u32 = 220;
    const H: u32 = 110;

    fn new(scale: f32) -> Self {
        let (w, h) = ((Self::W as f32 * scale).round() as u32, (Self::H as f32 * scale).round() as u32);
        Self { doc: Document::new(w.max(1), h.max(1)), live: None, tex: None, dirty: true, scale }
    }
}

/// Холст во вкладке, пока он не активен (у активного эти поля — в `PaintApp`).
struct Tab {
    doc: Document,
    file: Option<PathBuf>,
    ruler: ruler::Ruler,
    notes: Vec<String>,
    zoom: f32,
    offset: Vec2,
    selected: Vec<usize>,
    dirty: bool,
    had_vectors: bool,
    view_angle: f32,
    view_flip: bool,
    rec: Option<paint_formats::record::Recorder>,
}

impl Tab {
    fn title(&self) -> String {
        self.file.as_ref().and_then(|f| f.file_name()).map_or_else(|| tr("НовыйХолст1.sai2").to_string(), |n| n.to_string_lossy().into_owned())
    }
}

/// Плавающий вид холста («New Floating View» в SAI): свои масштаб, сдвиг, поворот и зеркало.
#[derive(Clone, Copy)]
struct FloatView {
    zoom: f32,
    offset: Vec2,
    angle: f32,
    flip: bool,
}

/// Вкладка внизу — вид холста: у одного холста может быть несколько («Новый вид» в SAI), у
/// каждого свои масштаб, сдвиг, поворот и зеркало.
#[derive(Clone, Copy)]
struct TabView {
    canvas: usize,
    view: FloatView,
}

/// Меню «Окно», как в SAI 2017.
#[derive(Clone, Copy, PartialEq)]
struct WindowOpts {
    ui_scale: f32,
    /// Цвет и инструменты справа (по умолчанию слева).
    tools_right: bool,
    /// Навигатор и слои справа (по умолчанию справа).
    layers_right: bool,
    fullscreen: bool,
    /// Tab: спрятать панели, оставить холст.
    panels_hidden: bool,
    dot_cursor: bool,
    brush_circle: bool,
    /// Список размеров кисти: только числа (без кружков) и над настройками кисти (SAI 2).
    size_numbers: bool,
    size_list_top: bool,
    /// Режим ползунков HSV: 0 — V-HSV (как SAI по умолчанию), 1 — HSL, 2 — HSV.
    hsv_mode: u8,
    /// Размер образцов: 0 — маленький (10 px), 1 — средний (12), 2 — большой (14, по умолчанию).
    swatch_size: u8,
    /// «Окно → Показать» (как в SAI): навигатор, слои, цвет, инструменты, быстрая панель,
    /// вкладки видов, строка состояния.
    show: [bool; 7],
    /// «Окно → Отдельные панели» (Separate User Interface Panel в SAI): навигатор, слои, цвет,
    /// инструменты — каждая в своём окне (номера — как у `show`).
    separate: [bool; 4],
}

impl Default for WindowOpts {
    fn default() -> Self {
        Self { ui_scale: 1.0, tools_right: false, layers_right: false, fullscreen: false, panels_hidden: false, dot_cursor: false, brush_circle: true, size_numbers: false, size_list_top: false, hsv_mode: 0, swatch_size: 2, show: [true; 7], separate: [false; 4] }
    }
}

/// Окно «Обводка…» слоя (как в SAI 2017): ширина 1–25, внутри / по центру / снаружи.
struct OutlineDialog {
    path: Vec<usize>,
    original: paint_core::tile::ColorMap,
    width: u32,
    /// 0 — внутри, 1 — по центру, 2 — снаружи.
    position: u8,
    canvas_edges: bool,
    preview: bool,
    dirty: bool,
}

impl OutlineDialog {
    /// На сколько наружу и внутрь.
    fn spread(&self) -> (u32, u32) {
        match self.position {
            0 => (0, self.width),
            1 => (self.width / 2, self.width.div_ceil(2)),
            _ => (self.width, 0),
        }
    }
}

/// Окно «Расширить/Сузить выделение…» (как в SAI 2017: 1–25 px, предпросмотр).
struct GrowDialog {
    grow: bool,
    n: u32,
    preview: bool,
    original: paint_core::selection::Selection,
    dirty: bool,
}

/// Настройки инструмента «Градиент» (как в SAI 2017).
#[derive(Clone, Copy, PartialEq, Default)]
struct GradientOpts {
    circle: bool,
    to_transparent: bool,
    reverse: bool,
    s_curve: bool,
}

/// Градиент до OK: концы можно двигать, настройки — менять (как в SAI).
struct GradientEdit {
    path: Vec<usize>,
    original: paint_core::tile::ColorMap,
    g: paint_core::filters::Gradient,
    /// Какой конец тянут: 0 — начало, 1 — конец.
    drag: Option<u8>,
    dirty: bool,
}

/// Диалог изменения холста.
struct CanvasDialog {
    resample: bool,
    size: [u32; 2],
    keep_ratio: bool,
    mode: paint_core::resample::Mode,
    anchor: (u8, u8),
}

/// Окно «Менеджер ресурсов»: раздел (`None` — наборы образцов), выбранный ресурс, поля ввода.
#[derive(Default)]
struct AssetsWindow {
    kind: Option<assets::Kind>,
    sel: Option<String>,
    name: String,
    group: String,
    image: Option<assets::ImageType>,
    confirm_delete: bool,
}

/// «Срезать крючки» (Anti-Hook): точки мазка придерживаются на длину крючка — в начале, пока
/// не ясно, был ли крючок, и в конце, чтобы при отрыве пера срезать загиб.
struct HookBuf {
    /// Длина крючка в точках холста.
    range: f32,
    start_done: bool,
    buf: Vec<InputPoint>,
    /// Уже отданные точки (последние ~2 длины крючка) — по ним видно основное направление.
    done: Vec<InputPoint>,
}

fn path_len(pts: &[InputPoint]) -> f32 {
    pts.windows(2).map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y)).sum()
}

/// Режим трансформации, как в панели SAI 2017: Freeform, Resize, Deform, Rotate.
#[derive(Clone, Copy, PartialEq, Default)]
enum TransformKind {
    /// Всё: стороны и углы — размер, снаружи — поворот, Ctrl + угол — деформация.
    #[default]
    Free,
    /// Только размер и сдвиг.
    Resize,
    /// Углы двигаются по отдельности без Ctrl.
    Deform,
    /// Только поворот и сдвиг.
    Rotate,
    /// Деформация сеткой (SAI 2 Alpha 2026): узлы тянут картинку мягко.
    Lattice,
    /// Деформация сеткой: картинка проходит через узлы.
    Mesh,
}

impl TransformKind {
    fn grid(self) -> Option<paint_core::gridwarp::GridKind> {
        match self {
            TransformKind::Lattice => Some(paint_core::gridwarp::GridKind::Lattice),
            TransformKind::Mesh => Some(paint_core::gridwarp::GridKind::Mesh),
            _ => None,
        }
    }
}

/// За какую часть рамки трансформации тянут.
#[derive(Clone, Copy, PartialEq)]
enum Handle {
    Move,
    Rotate,
    /// Угол или середина стороны: −1 / 0 / +1 по осям рамки.
    Scale(i8, i8),
    /// Угол свободной деформации (0 — левый верхний, далее по часовой).
    Corner(usize),
    /// Узел сетки деформации (по строкам).
    Node(usize),
}

/// Идёт трансформация (Ctrl+T). Рамка — прямоугольник источника `src`, переведённый в
/// «местные» края `l, r, t, b` (от центра источника), повёрнутый на `angle` и сдвинутый на `shift`.
struct Transform {
    path: Vec<usize>,
    original: paint_core::tile::ColorMap,
    /// Маска слоя до трансформации — трансформируется вместе со слоем.
    orig_mask: Option<paint_core::tile::MaskMap>,
    orig_sel: paint_core::selection::Selection,
    src: [f64; 4],
    edges: [f64; 4],
    angle: f64,
    shift: [f64; 2],
    drag: Option<(Handle, Vec2)>,
    mode: paint_core::resample::Mode,
    dirty: bool,
    /// Свободная деформация: углы рамки на холсте (левый верхний, правый верхний, правый нижний,
    /// левый нижний); `None` — обычная рамка.
    quad: Option<[Vec2; 4]>,
    /// Доля перспективы свободной деформации, 0..=1 (0 — билинейно, как SAI по умолчанию).
    perspective: f64,
    /// Деформация сеткой («Решётка», «Сетка»); `None` — рамка.
    grid: Option<paint_core::gridwarp::GridWarp>,
}

impl Transform {
    /// Отображение источника в результат: обычная рамка — аффинно, свободная — перспективой.
    fn warp(&self) -> paint_core::selection::Warp {
        use paint_core::selection::Warp;
        match self.quad {
            None => Warp::Affine(self.matrix()),
            Some(q) => Warp::Quad { src: self.src, quad: q.map(|p| [p.x as f64, p.y as f64]), perspective: self.perspective },
        }
    }

    /// Рамка ещё не тронута (сетку можно ставить прямо на источник).
    fn untouched(&self) -> bool {
        let [x0, y0, x1, y1] = self.src;
        let [cx, cy] = self.center();
        self.quad.is_none() && self.angle == 0.0 && self.shift == [0.0, 0.0] && self.edges == [x0 - cx, y0 - cy, x1 - cx, y1 - cy]
    }

    /// Углы рамки на холсте по часовой, начиная с левого верхнего.
    fn corners(&self) -> [Vec2; 4] {
        if let Some(g) = &self.grid {
            let (c, r) = (g.cols, g.rows);
            return [g.point(0, 0), g.point(c, 0), g.point(c, r), g.point(0, r)].map(|p| Vec2::new(p[0] as f32, p[1] as f32));
        }
        match self.quad {
            Some(q) => q,
            None => {
                let [l, t, r, b] = self.edges;
                [self.world(l, t), self.world(r, t), self.world(r, b), self.world(l, b)]
            }
        }
    }

    /// Ручки на холсте.
    fn handle_points(&self) -> Vec<(Handle, Vec2)> {
        if let Some(g) = &self.grid {
            return g.points.iter().enumerate().map(|(i, p)| (Handle::Node(i), Vec2::new(p[0] as f32, p[1] as f32))).collect();
        }
        match self.quad {
            Some(q) => (0..4).map(|i| (Handle::Corner(i), q[i])).collect(),
            None => self.handles().into_iter().map(|(h, u, v)| (h, self.world(u, v))).collect(),
        }
    }

    /// Точка внутри рамки.
    fn contains(&self, p: Vec2) -> bool {
        let c = self.corners();
        let side = |a: Vec2, b: Vec2| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        let s: Vec<f32> = (0..4).map(|i| side(c[i], c[(i + 1) % 4])).collect();
        s.iter().all(|&v| v >= 0.0) || s.iter().all(|&v| v <= 0.0)
    }
    fn center(&self) -> [f64; 2] {
        [(self.src[0] + self.src[2]) / 2.0, (self.src[1] + self.src[3]) / 2.0]
    }

    /// Матрица: точка источника → точка результата.
    fn matrix(&self) -> [f64; 6] {
        let [x0, y0, x1, y1] = self.src;
        let [l, t, r, b] = self.edges;
        let su = (r - l) / (x1 - x0).max(1e-9);
        let sv = (b - t) / (y1 - y0).max(1e-9);
        let (ou, ov) = (l - x0 * su, t - y0 * sv);
        let (s, c) = self.angle.sin_cos();
        let [cx, cy] = self.center();
        [c * su, -s * sv, c * ou - s * ov + cx + self.shift[0], s * su, c * sv, s * ou + c * ov + cy + self.shift[1]]
    }

    /// Точка рамки по местным координатам (u, v) → холст.
    fn world(&self, u: f64, v: f64) -> Vec2 {
        let (s, c) = self.angle.sin_cos();
        let [cx, cy] = self.center();
        Vec2::new((cx + self.shift[0] + c * u - s * v) as f32, (cy + self.shift[1] + s * u + c * v) as f32)
    }

    /// Точка холста → местные координаты рамки.
    fn local(&self, p: Vec2) -> [f64; 2] {
        let (s, c) = self.angle.sin_cos();
        let [cx, cy] = self.center();
        let (dx, dy) = (p.x as f64 - cx - self.shift[0], p.y as f64 - cy - self.shift[1]);
        [c * dx + s * dy, -s * dx + c * dy]
    }

    /// Ручки: (вид, местные координаты).
    fn handles(&self) -> Vec<(Handle, f64, f64)> {
        let [l, t, r, b] = self.edges;
        let (mx, my) = ((l + r) / 2.0, (t + b) / 2.0);
        vec![
            (Handle::Scale(-1, -1), l, t),
            (Handle::Scale(0, -1), mx, t),
            (Handle::Scale(1, -1), r, t),
            (Handle::Scale(1, 0), r, my),
            (Handle::Scale(1, 1), r, b),
            (Handle::Scale(0, 1), mx, b),
            (Handle::Scale(-1, 1), l, b),
            (Handle::Scale(-1, 0), l, my),
        ]
    }

    fn drag_to(&mut self, handle: Handle, from: Vec2, to: Vec2) {
        if let Some(g) = self.grid.as_mut() {
            match handle {
                Handle::Node(i) => {
                    if let Some(p) = g.points.get_mut(i) {
                        *p = [to.x as f64, to.y as f64];
                    }
                }
                _ => {
                    for p in g.points.iter_mut() {
                        p[0] += (to.x - from.x) as f64;
                        p[1] += (to.y - from.y) as f64;
                    }
                }
            }
            self.dirty = true;
            return;
        }
        if let Some(q) = self.quad.as_mut() {
            match handle {
                Handle::Corner(i) => q[i] = to,
                _ => {
                    for p in q.iter_mut() {
                        *p += to - from;
                    }
                }
            }
            self.dirty = true;
            return;
        }
        match handle {
            Handle::Corner(_) | Handle::Node(_) => {}
            Handle::Move => {
                self.shift[0] += (to.x - from.x) as f64;
                self.shift[1] += (to.y - from.y) as f64;
            }
            Handle::Rotate => {
                let [l, t, r, b] = self.edges;
                let mid = self.world((l + r) / 2.0, (t + b) / 2.0);
                let a0 = ((from.y - mid.y) as f64).atan2((from.x - mid.x) as f64);
                let a1 = ((to.y - mid.y) as f64).atan2((to.x - mid.x) as f64);
                self.angle += a1 - a0;
                // центр рамки остаётся на месте
                let after = self.world((l + r) / 2.0, (t + b) / 2.0);
                self.shift[0] += (mid.x - after.x) as f64;
                self.shift[1] += (mid.y - after.y) as f64;
            }
            Handle::Scale(hx, hy) => {
                let [u, v] = self.local(to);
                match hx {
                    -1 => self.edges[0] = u,
                    1 => self.edges[2] = u,
                    _ => {}
                }
                match hy {
                    -1 => self.edges[1] = v,
                    1 => self.edges[3] = v,
                    _ => {}
                }
            }
        }
        self.dirty = true;
    }
}

/// Идёт перемещение: исходные пиксели слоя и выделение, точка начала, текущий сдвиг.
/// Слой, который тянут в списке.
struct LayerDrag {
    path: Vec<usize>,
    /// Ширина строки — шире ярлык у курсора не бывает.
    width: f32,
}

/// Цвет подсказок перетаскивания слоя — темнее рамки выбранного, чтобы читалось сразу.
const DROP_INK: Color32 = Color32::from_rgb(70, 70, 190);

/// Подсказка, куда упадёт слой: полоса между строками или подсвеченная папка.
enum DropMark {
    Line(f32, f32, f32),
    Into(Rect),
}

struct MoveDrag {
    path: Vec<usize>,
    start: Vec2,
    original: paint_core::tile::ColorMap,
    orig_mask: Option<paint_core::tile::MaskMap>,
    orig_sel: paint_core::selection::Selection,
    offset: (i32, i32),
}

/// Выделение, которое сейчас тянут мышью или пером.
struct SelDrag {
    /// Точки в координатах холста: у прямоугольника — начало и текущая, у лассо — весь путь.
    points: Vec<Vec2>,
    mode: Combine,
    /// Обводка заливки лассо (а не выделения).
    fill: bool,
}

/// Инструмент рисования в сетке «кистей слоя», как у образца: кисть (`paint_brush::Brush` —
/// общая с мастерской) и своя клавиша.
#[derive(Clone)]
struct BrushTool {
    brush: paint_brush::Brush,
    /// Своя клавиша инструмента («Shortcut Key» в SAI).
    key: Option<keys::Binding>,
    /// «Запомнить настройки» (SAI 2020+): к ним можно вернуться.
    reserved: Option<paint_brush::Brush>,
    /// Настройки до последнего «Вернуть» — чтобы отменить возврат.
    before_restore: Option<paint_brush::Brush>,
    /// Группа инструментов (вкладка над сеткой, как в SAI 2020+).
    group: usize,
}

impl std::ops::Deref for BrushTool {
    type Target = paint_brush::Brush;
    fn deref(&self) -> &paint_brush::Brush {
        &self.brush
    }
}

impl std::ops::DerefMut for BrushTool {
    fn deref_mut(&mut self) -> &mut paint_brush::Brush {
        &mut self.brush
    }
}

/// Новый инструмент сетки из вида.
trait NewTool {
    fn new_tool(self) -> BrushTool;
}

impl NewTool for ToolKind {
    fn new_tool(self) -> BrushTool {
        BrushTool { brush: self.new_brush(), key: None, reserved: None, before_restore: None, group: 0 }
    }
}

/// Параметры инструмента сетки из настроек (ключи с приставкой `prefix`, например `brush.3.`).
fn read_tool(b: &mut BrushTool, s: &settings::Settings, prefix: &str) {
    paint_brush::read(&mut b.brush, s, prefix)
}

/// Параметры инструмента сетки в настройки.
fn write_tool(b: &BrushTool, s: &mut settings::Settings, prefix: &str) {
    paint_brush::write(&b.brush, s, prefix)
}

impl BrushTool {
    /// Имя в ячейке: своё или имя вида на языке интерфейса.
    fn title(&self) -> String {
        self.custom.clone().unwrap_or_else(|| tr(self.name).to_string())
    }

    /// Полное имя: своё или вида («Карандаш», а не «Каранд.» из ячейки).
    fn full_name(&self) -> String {
        self.custom.clone().unwrap_or_else(|| tr(self.kind.label()).to_string())
    }
}

/// Окно параметров команды плагина: плагин, команда, значения полей.
type PluginAsk = (usize, usize, Vec<(String, f64)>);

/// Пункт меню «Плагины»: подпись, подсказка (имя плагина), плагин, команда.
type MenuItem = (String, String, usize, usize);

/// Имена файлов форм (`blotmap`), текстур (`brushtex`), щетин (`bristle`) и отпечатков (`tips`).
type GrainNames = (Vec<String>, Vec<String>, Vec<String>, Vec<String>);

/// Сетка по умолчанию — как у нового SAI 2017, плюс градиент.
fn brush_tools() -> Vec<BrushTool> {
    use ToolKind::*;
    let mut out: Vec<BrushTool> = [Pencil, AirBrush, Brush, Water, Marker, Eraser, SelPen, SelErs, Gradient].map(ToolKind::new_tool).into();
    // вторая группа — двоичные, как вкладка Binary в SAI 2
    for k in [Binary, PixelPen, BinEraser, BinSelPen, BinSelErs, Bucket] {
        let mut t = k.new_tool();
        t.group = 1;
        out.push(t);
    }
    out
}

/// Группы инструментов по умолчанию.
fn tool_groups() -> Vec<String> {
    vec![String::from(tr("Основные")), String::from(tr("Двоичные"))]
}

/// Действие с ячейкой сетки инструментов (меню по правому щелчку).
#[derive(Clone, Copy, PartialEq)]
enum GridAction {
    Add(ToolKind),
    Duplicate,
    Delete,
    Property,
    /// Сохранить в файл кисти `.gsbrush`.
    Save,
    /// Запомнить настройки, вернуть запомненные, отменить возврат.
    Reserve,
    Restore,
    UndoRestore,
    /// Перенести в группу.
    ToGroup(usize),
}

#[derive(Clone, Copy, PartialEq)]
enum LayerAction {
    Duplicate,
    Delete,
    Clear,
    Merge,
    Transfer,
    Up,
    Down,
}

/// Число ячеек образцов цвета.
const SWATCHES: usize = 104;
/// Сколько недавних цветов помнить.
const RECENT_COLORS: usize = 16;


/// Масштабы в списке быстрой панели, %.
const ZOOMS: [f32; 14] = [
    6.25, 12.5, 25.0, 33.3, 50.0, 66.7, 100.0, 150.0, 200.0, 300.0, 400.0, 800.0, 1600.0, 3200.0,
];

struct DisplayTile {
    /// Готовые для текстуры точки (перевод из RGBA — в потоках пересчёта, не в кадре).
    pixels: Vec<Color32>,
    texture: Option<egui::TextureHandle>,
    /// Какие кусочки TILE×TILE изменились с прошлой загрузки (бит на кусочек, строками): на
    /// видеокарту идут только они (`set_partial`), а не вся плитка.
    dirty: u64,
    /// Фильтр, с которым текстура загружена: сменился масштаб — загрузить заново.
    options: Option<egui::TextureOptions>,
}

/// Кусочков TILE×TILE в стороне экранной плитки.
const SUB: i32 = DT / TILE;

struct PaintApp {
    doc: Document,
    file: Option<PathBuf>,
    open_path: String,
    /// Папка текстур бумаги (`papertex` из SAI) и найденные в ней имена.
    paper_dir: String,
    paper_names: Option<Vec<String>>,
    /// Имена форм (`blotmap`) и текстур кисти (`brushtex`) из папки SAI.
    grain_names: Option<GrainNames>,
    /// Кисти из мастерской (`brushes/dev/`): когда смотрели и время отметки `.changed`.
    dev_checked: Instant,
    dev_stamp: Option<std::time::SystemTime>,
    /// Плагины: установленные, ошибки загрузки, отметка `plugins/dev/.changed`.
    plugins: Vec<plugins::Installed>,
    plugin_errors: Vec<String>,
    plugins_stamp: Option<std::time::SystemTime>,
    /// Окно параметров команды: плагин, команда, значения.
    plugin_ask: Option<PluginAsk>,
    /// Окно установки: проверенный пакет и путь к нему.
    plugin_install: Option<(paint_plugin::Plugin, PathBuf)>,
    show_plugins: bool,
    /// Журнал последних запусков (для окна «Плагины»).
    plugin_log: Vec<String>,
    /// Ошибка своей динамики из плагина у текущего мазка (показывается после него).
    dyn_error: Option<paint_plugin::DynError>,
    /// Водяной знак: настройки, добавлять ли при экспорте картинки, открыто ли окно, предпросмотр.
    watermark: paint_formats::watermark::Watermark,
    watermark_on: bool,
    show_watermark: bool,
    watermark_preview: Option<egui::TextureHandle>,
    /// Запись процесса рисования (у активного холста), окно выгрузки и идущая выгрузка.
    rec: Option<paint_formats::record::Recorder>,
    rec_export: Option<recording::ExportDialog>,
    rec_job: Option<recording::Job>,
    /// Правка выбранного слоя 3D и видно ли окно «3D».
    s3d: Option<scene3d::Edit3d>,
    show_3d: bool,
    s3d_mode: scene3d::MouseMode,
    /// Перетаскивание слоя в списке: какой слой и строки списка (путь, прямоугольник, папка ли).
    layer_drag: Option<LayerDrag>,
    /// Линейка (прямая или эллипс) и её правка Ctrl + перетаскиванием: что взяли и где было.
    ruler: ruler::Ruler,
    ruler_drag: Option<(ruler::Grip, [f32; 2])>,
    /// Кисть рисует в маске выбранного слоя, а не в слое.
    mask_edit: bool,
    /// Мазок в маске: штрих (его покрытие), маска до мазка, к какому значению ведёт мазок.
    mask_stroke: Option<(Stroke, paint_core::layer::LayerMask, u16)>,
    /// Виды панели цвета: круг, ползунки RGB, ползунки HSV.
    color_views: [bool; 6],
    /// Черновик панели цвета (Scratch Pad в SAI).
    scratch: ScratchPad,
    /// Свой обозреватель файлов (FileViewer в SAI): открытое окно, «всегда использовать»,
    /// закладки и последняя папка.
    viewer: Option<file_viewer::FileViewer>,
    always_viewer: bool,
    viewer_marks: Vec<PathBuf>,
    viewer_dir: Option<PathBuf>,
    viewer_view: (u8, u8),
    /// «Настройки»: действующие и окно правки (копия до OK).
    opts: Options,
    opts_edit: Option<Options>,
    /// Удерживаемая клавиша: какая, когда нажата, что вернуть.
    spring: Option<(egui::Key, Instant, SpringUndo)>,
    /// Инструмент до последнего переключения клавишей (для повторного нажатия).
    prev_tool: (Tool, usize),
    /// Когда было последнее сочетание с Ctrl или Alt (защита от случайного сдвига слоя).
    last_mod_key: Option<Instant>,
    /// Смеситель цвета: четыре строки — два края и метка взятого цвета.
    mixer: [[Color32; 2]; 4],
    mixer_pos: [Option<f32>; 4],
    /// Образцы цвета и выбранная ячейка.
    swatches: Vec<Option<Color32>>,
    /// Наборы образцов (SAI 2020+): имя и цвета; текущий набор — в `swatches`.
    swatch_sets: Vec<(String, Vec<Option<Color32>>)>,
    swatch_set: usize,
    /// Окно «Менеджер ресурсов».
    assets: Option<AssetsWindow>,
    /// Группы ресурсов: «вид/имя» → группа.
    asset_groups: std::collections::BTreeMap<String, String>,
    /// Недавние цвета мазков, новые первыми.
    recent_colors: Vec<Color32>,
    swatch_sel: usize,
    layer_rects: Vec<(Vec<usize>, Rect, bool)>,
    status: String,
    notes: Vec<String>,
    zoom: f32,
    /// Точка холста, которая показана в левом верхнем углу области просмотра.
    offset: Vec2,
    fit_pending: bool,
    tool: Tool,
    brushes: Vec<BrushTool>,
    /// Группы инструментов (вкладки над сеткой) и открытая вкладка.
    tool_groups: Vec<String>,
    tool_group: usize,
    /// Переименование группы: номер и новое имя.
    group_rename: Option<(usize, String)>,
    /// Инструмент при прошлом показе вкладок: выбран другой (клавишей) — открыть его группу.
    tab_brush: usize,
    /// Диапазон полоски размера кисти (номер в `SIZE_RANGES`).
    size_range: u8,
    /// Миниатюра навигатора: уменьшенная копия холста (с усреднением, как в SAI — тонкие линии
    /// не рвутся), её масштаб и размер; плитки ядра, изменённые с прошлого показа.
    nav: Option<(f32, [usize; 2], Vec<Color32>, egui::TextureHandle)>,
    nav_dirty: HashSet<Key>,
    /// Когда смотрели занятость диска и что увидели.
    disk_seen: Option<(Instant, Option<(u64, u64)>)>,
    brush: usize,
    /// Основной цвет в HSV (тон хранится и у серых), фоновый — RGB.
    hsv: [f32; 3],
    back: Color32,
    stabilizer: paint_core::stabilizer::Level,
    selected: Vec<usize>,
    live: Option<LiveStroke>,
    smoother: Stabilizer,
    display: HashMap<(i32, i32), DisplayTile>,
    queue: VecDeque<Key>,
    queued: HashSet<Key>,
    pen: pen::PenFeed,
    /// Сила последнего касания пера (Windows Ink); `None` — в этом сеансе пера ещё не было.
    touch_force: Option<f32>,
    touch_down: bool,
    /// Текущий мазок начат пером (касанием): до его конца нажим берётся от пера, даже когда
    /// касание уже кончилось — иначе последняя точка при отрыве шла бы с нажимом мыши 1,0.
    stroke_by_touch: bool,
    /// Последний использованный нажим — для строки состояния.
    last_pressure: f32,
    last_source: &'static str,
    /// Область холста на экране в прошлом кадре — для навигатора.
    view: Rect,
    show_open: bool,
    show_new: bool,
    new_size: [u32; 2],
    /// Заготовки размеров холста (SAI 2018+): имя, ширина, высота; выбранная — по номеру.
    canvas_presets: Vec<(String, u32, u32)>,
    preset_sel: Option<usize>,
    show_about: bool,
    /// Есть несохранённые изменения.
    dirty: bool,
    /// В открытом файле были слои контура или фигур (их кривые при сохранении станут растром).
    had_vectors: bool,
    show_save_as: bool,
    /// Выход: окно вопроса, сохранение по очереди (какой холст ждём), выход разрешён.
    exit_ask: bool,
    exit_saving: bool,
    exit_waiting: Option<usize>,
    exit_ok: bool,
    /// Инструмент «Текст»: шрифт и размер по умолчанию, открытая правка, строка поиска шрифта.
    text_font: String,
    text_size: f32,
    /// «Прямая линия» на быстрой панели: каждый штрих — прямая (как в SAI).
    straight: bool,
    /// Идёт «Закрыть все виды».
    closing_all: bool,
    /// «Прозрачный цвет» (клетка у квадратов цвета): кисть стирает. Выбор цвета его снимает.
    transparent: bool,
    prev_hsv: [f32; 3],
    /// «Показать только этот слой»: видимость слоёв до него.
    solo: Option<Vec<(Vec<usize>, bool)>>,
    /// Режим выделения с панели (Shift/Alt его перебивают) и пипетка только по текущему слою.
    sel_mode: Combine,
    pick_layer: bool,
    /// Инструмент «Поворот вида»: прошлая точка перетаскивания.
    rot_last: Option<Pos2>,
    /// Прямой штрих: откуда и ластиком ли; последняя точка штриха и конец прошлого (Shift +
    /// нажатие ведёт прямую от него).
    line_from: Option<(InputPoint, bool)>,
    last_point: Option<InputPoint>,
    last_end: Option<InputPoint>,
    text_bold: bool,
    text_italic: bool,
    text_vertical: bool,
    /// Прокрутить список шрифтов к выбранному (при показе панели текста).
    font_scroll: bool,
    text_edit: Option<TextEditing>,
    font_filter: String,
    /// Системное окно «Открыть» / «Сохранить», ждущее ответа.
    file_dialog: Option<files::Pending>,
    /// Для окон, которые открываются не из кадра (клавиши, «Сохранить»).
    ctx: Option<egui::Context>,
    save_path: String,
    /// Миниатюры слоёв по пути в дереве; сбрасываются после правок.
    thumbs: HashMap<Vec<usize>, egui::TextureHandle>,
    /// Виртуальное перо для проверок без планшета (`PAINT_VIRTUAL_PEN`).
    virtual_pen: Option<virtual_pen::VirtualPen>,
    /// Кадров после конца сценария до сохранения результата (`PAINT_VIRTUAL_PEN_OUT`).
    virtual_done: u32,
    /// Сводка по нажиму от пера за сеанс: (источник, мин., макс., число замеров) — для проверок.
    pen_stats: HashMap<&'static str, (f32, f32, u32)>,
    started: Instant,
    /// Для `PAINT_TIMING`: когда начали показ холста целиком (после открытия или нового холста).
    timing: Option<Instant>,
    sel_drag: Option<SelDrag>,
    /// Многоугольное лассо: вершины, пока многоугольник не закрыт (Enter, двойной щелчок).
    sel_poly: Option<SelDrag>,
    /// Лассо: многоугольником и со сглаженным краем (настройки инструмента, как в SAI).
    lasso_polygon: bool,
    lasso_aa: bool,
    grow_dialog: Option<GrowDialog>,
    outline_dialog: Option<OutlineDialog>,
    /// Последние ширина и положение обводки (окно открывается с ними, как в SAI).
    outline_last: (u32, u8, bool),
    /// Прошлые «Расширить/Сузить выделение» (px) и окно холста (пропорции, пересчёт, привязка).
    grow_last: u32,
    canvas_last: (bool, paint_core::resample::Mode, (u8, u8)),
    gradient: Option<GradientEdit>,
    grad_opts: GradientOpts,
    transform_kind: TransformKind,
    /// Плавающий вид, если открыт.
    float_view: Option<FloatView>,
    /// Какой вид сейчас рисуется (0 — главный, 1 — плавающий) и какой принимает ввод.
    drawing_view: u8,
    input_view: u8,
    /// Вкладки холстов: на месте активной — `None` (её поля — в самом `PaintApp`).
    tabs: Vec<Option<Tab>>,
    active_tab: usize,
    /// Вкладки внизу (виды холстов) и активная; у каждого холста хотя бы один вид.
    views: Vec<TabView>,
    active_view: usize,
    /// Закрыть вкладку с несохранёнными изменениями — спросить.
    close_ask: Option<usize>,
    /// Настройки меню «Окно».
    win: WindowOpts,
    /// Нажата Tab (`Some(shift)`), перехвачена до egui в `raw_input_hook`.
    tab_pressed: Option<bool>,
    /// Окно «Цвет фона холста»: выбираемый цвет.
    bg_color_edit: Option<[u8; 3]>,
    /// Недавние файлы, сверху — последний.
    recent: Vec<PathBuf>,
    /// Системный буфер обмена и клавиша «Вставить».
    clip: clip::Clip,
    /// Где лежало последнее скопированное у нас (x, y, ширина, высота) — вставка на то же место.
    clip_own: Option<(i32, i32, u32, u32)>,
    /// Окно «Свойства слоя»: слой и новое имя.
    layer_props: Option<(Vec<usize>, String)>,
    /// Окно «Свойства» инструмента сетки: номер, имя, вторая строка.
    tool_props: Option<(usize, String, String, Option<keys::Binding>)>,
    /// Окно «Свойства» инструмента ждёт клавишу.
    tool_key_capture: bool,
    /// Поиск в окне «Клавиши».
    keys_filter: String,
    /// Ползунок «Перспектива» трансформации, 0–100.
    transform_perspective: u8,
    /// Деление сетки деформации (столбцы, строки) и «Смешивание» складок.
    grid_div: [u8; 2],
    grid_blend: bool,
    /// Спросить перед переводом в оттенки серого.
    ask_gray: bool,
    /// Нажим ниже «минимального нажима для мазка» (по последнему `pressure`).
    below_min: bool,
    /// Перо коснулось слабее порога: мазок начнётся, когда нажим его превысит (ластик ли).
    pending_start: Option<bool>,
    /// Shift после начала мазка: точка, от которой мазок идёт прямо, и ось (`true` — по
    /// горизонтали), когда она выбрана.
    shift_lock: Option<(InputPoint, Option<bool>)>,
    /// «Срезать крючки»: придержанные точки мазка.
    hook: Option<HookBuf>,
    /// Образец мазка текущей кисти: ключ (настройки текстом) и картинка.
    brush_preview: Option<(String, egui::TextureHandle)>,
    move_drag: Option<MoveDrag>,
    transform: Option<Transform>,
    /// Мазок пера или ластика выделения и его путь на холсте (для показа во время мазка).
    sel_stroke: Option<(Stroke, Vec<Vec2>)>,
    /// Мазок пера на слое контура: точки с нажимом (станут кривой при отрыве пера).
    line_stroke: Option<Vec<paint_core::vectorize::StrokePoint>>,
    line_tool: LineTool,
    line_edit: Option<LineEdit>,
    /// Вид фигуры и фигура, которую сейчас тянут (углы рамки на холсте).
    shape_kind: paint_formats::linework::ShapeKind,
    shape_drag: Option<(Vec2, Vec2)>,
    /// Диалог «Изменить разрешение» / «Изменить размер»: новые размеры, сохранять пропорции,
    /// способ пересэмплирования, привязка.
    canvas_dialog: Option<CanvasDialog>,
    filter: Option<FilterDialog>,
    /// Настройки последнего закрытого фильтра (без пикселей) — следующее окно открывается с ними.
    filter_last: Option<FilterDialog>,
    frame_stats: Option<std::rc::Rc<std::cell::RefCell<FrameStats>>>,
    /// Точки штриха, ещё не нарисованные (см. `drain_points`).
    pending_points: VecDeque<InputPoint>,
    /// Прошлая точка пера на экране и её нажим (для `resample_moves`).
    raw_last: Option<(Pos2, f32)>,
    /// Область списка слоёв — от неё считаются id «папка открыта» (для клавиши Alt+\).
    layers_ui: Option<egui::Id>,
    /// Пикселей экрана на точку интерфейса (масштаб Windows, «Масштаб интерфейса»): «100 %» вида
    /// — пиксель холста на пиксель экрана, как в SAI.
    ppp: f32,
    cp_sel: Option<CpSelection>,
    /// Разобранные кривые текущего слоя контура для показа узлов: слой, отпечаток, кривые.
    line_view: Option<(Vec<usize>, u64, Vec<paint_formats::sai2_vector::Shape>)>,
    /// Фоновый предпросмотр фильтра; номер последних настроек и номер показанного на слое.
    filter_job: Option<FilterJob>,
    filter_gen: u64,
    filter_shown: Option<u64>,
    /// Свои клавиши, окно «Клавиши» и действие, которому сейчас назначают клавишу.
    keymap: keys::Keymap,
    show_keys: bool,
    key_capture: Option<keys::Action>,
    /// Восстановление после сбоя: когда последний раз писали копию; копия пишется сейчас;
    /// найдена копия прошлого запуска (путь копии и исходного файла).
    recovery_at: Instant,
    recovery_busy: std::sync::Arc<std::sync::atomic::AtomicBool>,
    recovery_dirty: bool,
    recovery_found: Option<(PathBuf, String)>,
    /// Поворот вида (радианы) и зеркало вида по горизонтали — только показ, рисунок не меняется.
    view_angle: f32,
    view_flip: bool,
    /// Граница выделения для показа; пересчитывается после правок выделения.
    sel_outline: Vec<[(i32, i32); 2]>,
    sel_dirty: bool,
    show_selection: bool,
    /// Допуск волшебной палочки, 0..=255.
    wand_tolerance: u8,
    /// Палочка и заливка: закрывать щели контура до 2·N px; расширять под линию на N px.
    fill_gap: u32,
    fill_expand: u32,
    /// Настройки заливки лассо.
    lassofill: lassofill::Settings,
    /// Обновление с GitHub: состояние фоновой проверки и загрузки.
    update: update::Shared,
    show_update: bool,
    check_updates: bool,
    /// Тёмная тема интерфейса.
    dark_theme: bool,
    /// Окно истории открыто.
    show_history: bool,
    /// Пиксельная сетка при увеличении от 800 %.
    pixel_grid: bool,
    /// Бесшовный режим: холст показан плиткой 3×3, мазки переходят через края.
    wrap_around: bool,
    /// Опорные картинки поверх холста.
    refs: Vec<RefImage>,
    next_ref: u64,
    /// Где открыта палитра по правому щелчку (точка экрана) и в каком кадре.
    popup_at: Option<Pos2>,
    popup_frame: u64,
    /// Симметрия: 0 — нет, 1 — ось по вертикали (лево ↔ право), 2 — по горизонтали, 3 — обе.
    symmetry: u8,
    /// Симметрия по кругу (`symmetry == 4`): частей 1–20, с отражением, угол первой оси (°),
    /// центр (`None` — середина холста).
    sym_n: u8,
    sym_mirror: bool,
    sym_angle: f32,
    sym_center: Option<[f32; 2]>,
}

impl PaintApp {
    fn new(path: Option<PathBuf>, pen: pen::PenFeed) -> Self {
        let mut app = Self {
            doc: new_document(1200, 900),
            file: None,
            open_path: path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            paper_dir: String::new(),
            paper_names: None,
            grain_names: None,
            dev_checked: Instant::now(),
            dev_stamp: None,
            plugins: Vec::new(),
            plugin_errors: Vec::new(),
            plugins_stamp: None,
            plugin_ask: None,
            plugin_install: None,
            show_plugins: false,
            plugin_log: Vec::new(),
            dyn_error: None,
            watermark: Default::default(),
            watermark_on: false,
            show_watermark: false,
            watermark_preview: None,
            rec: None,
            rec_export: None,
            rec_job: None,
            s3d: None,
            show_3d: false,
            s3d_mode: scene3d::MouseMode::Camera,
            layer_drag: None,
            ruler: ruler::Ruler::default(),
            ruler_drag: None,
            color_views: [true, false, true, false, false, false],
            scratch: ScratchPad::new(1.0),
            viewer: None,
            always_viewer: false,
            viewer_marks: Vec::new(),
            viewer_dir: None,
            viewer_view: (0, 0),
            opts: Options::default(),
            opts_edit: None,
            spring: None,
            prev_tool: (Tool::Brush, 0),
            last_mod_key: None,
            mixer: [[Color32::WHITE; 2]; 4],
            mixer_pos: [None; 4],
            swatches: vec![None; SWATCHES],
            swatch_sets: vec![(String::from(tr("Образцы")), vec![None; SWATCHES])],
            swatch_set: 0,
            assets: None,
            asset_groups: Default::default(),
            recent_colors: Vec::new(),
            swatch_sel: 0,
            mask_edit: false,
            mask_stroke: None,
            layer_rects: Vec::new(),
            status: String::from(tr("Новый холст 1200×900")),
            notes: Vec::new(),
            zoom: 1.0,
            offset: Vec2::ZERO,
            fit_pending: true,
            tool: Tool::Brush,
            brushes: brush_tools(),
            tool_groups: tool_groups(),
            tool_group: 0,
            group_rename: None,
            tab_brush: usize::MAX,
            size_range: 1,
            nav: None,
            nav_dirty: HashSet::new(),
            disk_seen: None,
            // PAINT_BRUSH — номер кисти в сетке при запуске (для проверок виртуальным пером)
            brush: std::env::var("PAINT_BRUSH").ok().and_then(|v| v.parse().ok()).filter(|&i: &usize| i != 5 && i < 8).unwrap_or(0),
            hsv: [0.0, 0.0, 0.08],
            back: Color32::WHITE,
            stabilizer: paint_core::stabilizer::Level::Plain(3),
            selected: vec![0],
            live: None,
            smoother: Stabilizer::new(3),
            display: HashMap::new(),
            queue: VecDeque::new(),
            queued: HashSet::new(),
            pen,
            touch_force: None,
            touch_down: false,
            stroke_by_touch: false,
            last_pressure: 1.0,
            last_source: tr("мышь"),
            view: Rect::NOTHING,
            show_open: false,
            show_new: false,
            new_size: [1200, 900],
            canvas_presets: vec![
                ("512 × 512".into(), 512, 512),
                ("1200 × 900".into(), 1200, 900),
                ("A4, 300 dpi".into(), 2480, 3508),
                ("4096 × 4096".into(), 4096, 4096),
            ],
            preset_sel: None,
            show_about: false,
            dirty: false,
            had_vectors: false,
            show_save_as: false,
            exit_ask: false,
            exit_saving: false,
            exit_waiting: None,
            exit_ok: false,
            text_font: String::new(),
            text_size: 20.0,
            straight: false,
            closing_all: false,
            transparent: false,
            prev_hsv: [0.0; 3],
            solo: None,
            sel_mode: Combine::Replace,
            pick_layer: false,
            rot_last: None,
            line_from: None,
            last_point: None,
            last_end: None,
            text_bold: false,
            text_italic: false,
            text_vertical: false,
            font_scroll: true,
            text_edit: None,
            font_filter: String::new(),
            file_dialog: None,
            ctx: None,
            save_path: String::new(),
            thumbs: HashMap::new(),
            virtual_pen: virtual_pen::VirtualPen::from_env(),
            virtual_done: 0,
            pen_stats: HashMap::new(),
            started: Instant::now(),
            timing: None,
            sel_drag: None,
            sel_poly: None,
            lasso_polygon: false,
            lasso_aa: true,
            grow_dialog: None,
            outline_dialog: None,
            outline_last: (1, 0, false),
            grow_last: 1,
            canvas_last: (true, paint_core::resample::Mode::Bicubic, (1, 1)),
            gradient: None,
            grad_opts: GradientOpts::default(),
            transform_kind: TransformKind::Free,
            tool_props: None,
            tool_key_capture: false,
            keys_filter: String::new(),
            clip: clip::Clip::new(),
            recent: Vec::new(),
            bg_color_edit: None,
            win: WindowOpts::default(),
            tabs: vec![None],
            float_view: None,
            drawing_view: 0,
            input_view: 0,
            active_tab: 0,
            views: vec![TabView { canvas: 0, view: FloatView { zoom: 1.0, offset: Vec2::ZERO, angle: 0.0, flip: false } }],
            active_view: 0,
            close_ask: None,
            tab_pressed: None,
            clip_own: None,
            layer_props: None,
            transform_perspective: 0,
            grid_div: [2, 2],
            grid_blend: false,
            below_min: false,
            ask_gray: false,
            pending_start: None,
            shift_lock: None,
            hook: None,
            brush_preview: None,
            move_drag: None,
            transform: None,
            sel_stroke: None,
            line_stroke: None,
            line_tool: LineTool::Pen,
            line_edit: None,
            shape_kind: paint_formats::linework::ShapeKind::Rectangle,
            shape_drag: None,
            canvas_dialog: None,
            filter: None,
            filter_last: None,
            pending_points: VecDeque::new(),
            raw_last: None,
            layers_ui: None,
            ppp: 1.0,
            cp_sel: None,
            line_view: None,
            frame_stats: std::env::var_os("PAINT_FRAMES").map(|_| Default::default()),
            filter_job: None,
            filter_gen: 0,
            filter_shown: None,
            keymap: keys::defaults(),
            show_keys: false,
            key_capture: None,
            recovery_at: Instant::now(),
            recovery_busy: Default::default(),
            recovery_dirty: false,
            recovery_found: None,
            view_angle: 0.0,
            view_flip: false,
            sel_outline: Vec::new(),
            sel_dirty: false,
            show_selection: true,
            wand_tolerance: 10,
            fill_gap: 0,
            fill_expand: 0,
            lassofill: Default::default(),
            update: std::sync::Arc::new(std::sync::Mutex::new(update::State::Idle)),
            show_update: false,
            check_updates: true,
            dark_theme: false,
            show_history: false,
            pixel_grid: false,
            wrap_around: false,
            refs: Vec::new(),
            next_ref: 0,
            popup_at: None,
            popup_frame: 0,
            symmetry: 0,
            sym_n: 6,
            sym_mirror: true,
            sym_angle: 0.0,
            sym_center: None,
        };
        app.load_settings();
        app.recovery_found = recovery_files().filter(|(f, _)| f.exists()).map(|(f, info)| {
            let original = std::fs::read_to_string(&info).unwrap_or_default();
            (f, original.trim().to_string())
        });
        match path {
            Some(p) => app.open(p),
            None => app.invalidate_all(),
        }
        // Для проверок: PAINT_RULER=straight|ellipse — показать линейку при запуске.
        match std::env::var("PAINT_RULER").as_deref() {
            Ok("straight") => app.ruler.visible = true,
            Ok("ellipse") => {
                app.ruler.visible = true;
                app.ruler.kind = ruler::Kind::Ellipse;
            }
            _ => {}
        }
        // Для проверок вида на разных языках: PAINT_SHOW_DIALOG — открыть окно при запуске.
        match std::env::var("PAINT_SHOW_DIALOG").as_deref() {
            Ok("new") => app.show_new = true,
            Ok("open") => app.show_open = true,
            Ok("save_as") => app.show_save_as = true,
            Ok("keys") => app.show_keys = true,
            Ok("about") => app.show_about = true,
            Ok("update") => app.show_update = true,
            Ok(d @ ("resolution" | "size")) => {
app.open_canvas_dialog(d == "resolution")
            }
            Ok("blur") => app.open_filter(FilterKind::GaussianBlur),
            Ok("bc") => app.open_filter(FilterKind::BrightnessContrast),
            Ok("lens") => app.open_filter(FilterKind::Lens),
            Ok("wave") => app.open_filter(FilterKind::Distort(paint_core::filters::DistortKind::Wave)),
            Ok("twirl") => app.open_filter(FilterKind::Distort(paint_core::filters::DistortKind::Twirl)),
            Ok("polar") => app.open_filter(FilterKind::Distort(paint_core::filters::DistortKind::Polar)),
            Ok("pencil") => app.open_filter(FilterKind::Pencil),
            Ok("artistic") => app.open_filter(FilterKind::Artistic),
            _ => {}
        }
        // Для проверок: PAINT_TOOL=move|marquee|lasso|wand — инструмент при запуске;
        // PAINT_SELECT=x0,y0,x1,y1 — готовое выделение прямоугольником.
        // для замеров: размер кисти (PAINT_BRUSH_SIZE) и новый холст ШxВ (PAINT_CANVAS)
        if let Some(v) = std::env::var("PAINT_BRUSH_SIZE").ok().and_then(|v| v.parse::<f32>().ok()) {
            let b = app.brush;
            app.brushes[b].params.size = v.clamp(0.1, 5000.0);
        }
        if let Some((w, h)) = std::env::var("PAINT_CANVAS").ok().and_then(|v| v.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))) {
            app.new_canvas(w, h);
        }
        app.tool = match std::env::var("PAINT_TOOL").as_deref() {
            Ok("move") => Tool::Move,
            Ok("fill") => Tool::Fill,
            Ok("marquee") => Tool::Marquee,
            Ok("lasso") => Tool::Lasso,
            Ok("wand") => Tool::Wand,
            _ => app.tool,
        };
        // PAINT_VIEW=угол[,flip] — поворот и зеркало вида при запуске (для проверок)
        if let Ok(v) = std::env::var("PAINT_VIEW") {
            let mut it = v.split(',');
            app.view_angle = it.next().and_then(|a| a.trim().parse::<f32>().ok()).unwrap_or(0.0).to_radians();
            app.view_flip = it.next().is_some_and(|f| f.trim() == "flip");
        }
        // PAINT_REFERENCE=файл — опорная картинка при запуске (для проверок)
        if let Some(p) = std::env::var_os("PAINT_REFERENCE") {
            app.add_reference(std::path::Path::new(&p));
        }
        // PAINT_ZOOM=k — масштаб при запуске (для проверок), без вписывания в окно
        if let Some(z) = std::env::var("PAINT_ZOOM").ok().and_then(|v| v.parse::<f32>().ok()).filter(|z| *z > 0.0) {
            app.zoom = z;
            app.fit_pending = false;
        }
        // PAINT_LAYER=n — выбрать слой n верхнего уровня; PAINT_LINE_TOOL — инструмент контура
        if let Some(n) = std::env::var("PAINT_LAYER").ok().and_then(|v| v.parse::<usize>().ok()) {
            if n < app.doc.layers.len() {
                app.selected = vec![n];
            }
        }
        app.line_tool = match std::env::var("PAINT_LINE_TOOL").as_deref() {
            Ok("eraser") => LineTool::Eraser,
            Ok("weight") => LineTool::Weight,
            Ok("color") => LineTool::Color,
            Ok("edit") => LineTool::Edit,
            Ok("pressure") => LineTool::Pressure,
            _ => LineTool::Pen,
        };
        // PAINT_LINEWORK=1 — начать на новом слое контура; =shapes — на новом слое фигур (=rect|ellipse|triangle)
        match std::env::var("PAINT_LINEWORK").as_deref() {
            Ok("shapes") | Ok("rect") => app.add_shape_layer(),
            Ok("ellipse") => {
                app.add_shape_layer();
                app.shape_kind = paint_formats::linework::ShapeKind::Ellipse;
            }
            Ok("triangle") => {
                app.add_shape_layer();
                app.shape_kind = paint_formats::linework::ShapeKind::Triangle;
            }
            Ok(_) => app.add_linework_layer(),
            Err(_) => {}
        }
        // PAINT_FILTER=hs|bc|gauss — открыть фильтр при запуске (для проверок)
        match std::env::var("PAINT_FILTER").as_deref() {
            Ok("hs") => app.open_filter(FilterKind::HueSaturation),
            Ok("bc") => app.open_filter(FilterKind::BrightnessContrast),
            Ok("gauss") => app.open_filter(FilterKind::GaussianBlur),
            _ => {}
        }
        if std::env::var_os("PAINT_TRANSFORM").is_some() {
            if let Ok(v) = std::env::var("PAINT_SELECT") {
                let n: Vec<f64> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                if n.len() == 4 {
                    app.apply_selection(&paint_core::selection::rect(n[0], n[1], n[2], n[3]), Combine::Replace);
                }
            }
            app.begin_transform();
            // PAINT_TRANSFORM=free — сразу свободная деформация
            if std::env::var("PAINT_TRANSFORM").as_deref() == Ok("free") {
                if let Some(t) = app.transform.as_mut() {
                    t.quad = Some(t.corners());
                }
            }
        } else if let Ok(v) = std::env::var("PAINT_SELECT") {
            let n: Vec<f64> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            if n.len() == 4 {
                app.apply_selection(&paint_core::selection::rect(n[0], n[1], n[2], n[3]), Combine::Replace);
            }
        }
        app
    }

    /// Настройки прошлого запуска: кисти, цвета, сглаживание, допуск палочки.
    fn load_settings(&mut self) {
        // своя папка программы (текстуры и отпечатки из кистей CSP, GIMP) — всегда, даже без папки SAI
        paint_core::paper::set_user_dir(settings::dir());
        self.reload_plugins();
        let s = settings::Settings::load();
        // свой список инструментов сетки (без него — сетка по умолчанию)
        if let Some(n) = s.f32("tools.count").map(|v| v as usize).filter(|&n| n > 0 && n < 1000) {
            let tools: Vec<BrushTool> = (0..n)
                .filter_map(|i| {
                    let mut t = ToolKind::from_key(s.str(&format!("tool.{i}.kind"))?)?.new_tool();
                    t.custom = s.str(&format!("tool.{i}.name")).filter(|v| !v.is_empty()).map(str::to_string);
                    t.extra = s.str(&format!("tool.{i}.extra")).unwrap_or("").to_string();
                    t.key = s.str(&format!("tool.{i}.key")).and_then(keys::Binding::parse);
                    t.group = s.f32(&format!("tool.{i}.group")).map_or(0, |v| v as usize);
                    Some(t)
                })
                .collect();
            if tools.len() == n {
                self.brushes = tools;
                // до групп (1.1) все инструменты — в одной группе
                self.tool_groups = vec![String::from(tr("Основные"))];
            }
        }
        if let Some(n) = s.f32("tool_groups.count").map(|v| v as usize).filter(|&n| (1..=64).contains(&n)) {
            self.tool_groups = (0..n).map(|g| s.str(&format!("tool_groups.{g}")).unwrap_or("").to_string()).collect();
        }
        let ng = self.tool_groups.len();
        for b in self.brushes.iter_mut() {
            b.group = b.group.min(ng - 1);
        }
        self.tool_group = s.f32("tool_group").map_or(0, |v| v as usize).min(ng - 1);
        for (i, b) in self.brushes.iter_mut().enumerate() {
            read_tool(b, &s, &format!("brush.{i}."));
            if s.bool(&format!("brush.{i}.has_reserved")) == Some(true) {
                let mut r = b.kind.new_brush();
                paint_brush::read(&mut r, &s, &format!("brush.{i}.reserved."));
                b.reserved = Some(r);
            }
        }
        if let Some(i) = s.f32("brush").map(|v| v as usize).filter(|&i| i < self.brushes.len()) {
            if std::env::var_os("PAINT_BRUSH").is_none() {
                self.brush = i;
            }
        }
        for c in 0..3 {
            s.read_f32(&format!("color.{c}"), &mut self.hsv[c]);
        }
        if let (Some(r), Some(g), Some(b)) = (s.f32("back.r"), s.f32("back.g"), s.f32("back.b")) {
            self.back = Color32::from_rgb(r as u8, g as u8, b as u8);
        }
        if let Some(l) = s.str("stabilizer") {
            if let Some(v) = paint_core::stabilizer::Level::all().into_iter().find(|v| v.label() == l) {
                self.stabilizer = v;
            }
        }
        for (a, id, _, _) in keys::ALL {
            if let Some(v) = s.str(&format!("key.{id}")) {
                self.keymap.insert(a, if v == "none" { None } else { keys::Binding::parse(v) });
            }
        }
        s.read_bool("check_updates", &mut self.check_updates);
        s.read_bool("dark_theme", &mut self.dark_theme);
        s.read_bool("pixel_grid", &mut self.pixel_grid);
        s.read_bool("wrap_around", &mut self.wrap_around);
        {
            let wm = &mut self.watermark;
            s.read_bool("wm.on", &mut self.watermark_on);
            if let Some(v) = s.str("wm.text") {
                wm.text = v.to_string();
            }
            if let Some(v) = s.str("wm.image") {
                wm.image = (!v.is_empty()).then(|| PathBuf::from(v));
            }
            if let Some(v) = s.str("wm.font").filter(|v| !v.is_empty()) {
                wm.font = v.to_string();
            }
            if let Some(n) = s.str("wm.color").and_then(|v| u32::from_str_radix(v, 16).ok()) {
                wm.color = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
            }
            s.read_f32("wm.opacity", &mut wm.opacity);
            s.read_f32("wm.size", &mut wm.size);
            s.read_f32("wm.margin", &mut wm.margin);
            if let Some(p) = s.str("wm.place").and_then(paint_formats::watermark::Place::from_key) {
                wm.place = p;
            }
        }
        if let Some(v) = s.f32("symmetry") {
            self.symmetry = (v as u8).min(4);
        }
        if let Some(v) = s.f32("sym_n") {
            self.sym_n = (v as u8).clamp(1, 20);
        }
        if let Some(v) = s.f32("sym_mirror") {
            self.sym_mirror = v != 0.0;
        }
        if let Some(v) = s.f32("sym_angle") {
            self.sym_angle = v.clamp(-360.0, 360.0);
        }
        sai_ui::set_dark(self.dark_theme);
        if let Some(v) = s.bool("key_hints") {
            keys::set_hints(v);
        }
        if let Some(v) = s.f32("ui_scale") {
            self.win.ui_scale = v.clamp(1.0, 2.0);
        }
        s.read_bool("tools_right", &mut self.win.tools_right);
        for (i, k) in SHOW_KEYS.iter().enumerate() {
            s.read_bool(k, &mut self.win.show[i]);
        }
        for (i, k) in SHOW_KEYS[..4].iter().enumerate() {
            s.read_bool(&format!("separate_{k}"), &mut self.win.separate[i]);
        }
        s.read_bool("viewer.always", &mut self.always_viewer);
        if let Some(m) = s.str("viewer.marks") {
            self.viewer_marks = m.split('\u{1f}').filter(|x| !x.is_empty()).map(PathBuf::from).collect();
        }
        self.viewer_dir = s.str("viewer.dir").map(PathBuf::from);
        if let Some(v) = s.str("viewer.view").and_then(|v| v.parse::<u16>().ok()) {
            self.viewer_view = ((v >> 8) as u8, v as u8);
        }
        let o = &mut self.opts;
        for (k, v) in [("opt.dbl_props", &mut o.dbl_props), ("opt.swap_zoom", &mut o.swap_zoom), ("opt.size_relative", &mut o.size_relative), ("opt.spring_tools", &mut o.spring_tools), ("opt.spring_color", &mut o.spring_color), ("opt.spring_funcs", &mut o.spring_funcs), ("opt.duplex_revert", &mut o.duplex_revert), ("opt.popup_palette", &mut o.popup_palette), ("opt.psd16", &mut o.psd16), ("opt.psd_exact", &mut o.psd_exact)] {
            s.read_bool(k, v);
        }
        for (k, v) in [("opt.spring_ms", &mut o.spring_ms), ("opt.move_guard_ms", &mut o.move_guard_ms), ("opt.history_mb", &mut o.history_mb), ("opt.recovery_min", &mut o.recovery_min)] {
            if let Some(n) = s.str(k).and_then(|t| t.parse().ok()) {
                *v = n;
            }
        }
        if let Some(v) = s.f32("opt.min_pressure") {
            o.min_pressure = v.clamp(0.0, 0.9);
        }
        if let Some(v) = s.str("opt.pressure_curve") {
            let n: Vec<f32> = v.split(',').filter_map(|t| t.trim().parse().ok()).collect();
            if let [x1, y1, x2, y2] = n[..] {
                o.pressure_curve.points = [[x1, y1], [x2, y2]];
            }
        }
        paint_core::history_disk::set_ram_budget(self.opts.history_mb as usize * (1 << 20));
        s.read_bool("layers_right", &mut self.win.layers_right);
        s.read_bool("dot_cursor", &mut self.win.dot_cursor);
        s.read_bool("brush_circle", &mut self.win.brush_circle);
        s.read_bool("size_numbers", &mut self.win.size_numbers);
        s.read_bool("size_list_top", &mut self.win.size_list_top);
        if let Some(v) = s.f32("hsv_mode") {
            self.win.hsv_mode = (v as u8).min(2);
        }
        if let Some(v) = s.f32("swatch_size") {
            self.win.swatch_size = (v as u8).min(2);
        }
        if let Some(n) = s.f32("canvas_preset.count").map(|v| v as usize).filter(|&n| n <= 200) {
            self.canvas_presets = (0..n)
                .filter_map(|i| {
                    let v = s.str(&format!("canvas_preset.{i}"))?;
                    let mut it = v.rsplitn(3, '|');
                    let h: u32 = it.next()?.parse().ok()?;
                    let w: u32 = it.next()?.parse().ok()?;
                    Some((it.next()?.to_string(), w.clamp(1, 16000), h.clamp(1, 16000)))
                })
                .collect();
        }
        if let (Some(w), Some(h)) = (s.f32("new_width"), s.f32("new_height")) {
            self.new_size = [(w as u32).clamp(1, 16000), (h as u32).clamp(1, 16000)];
        }
        if let (Some(c), Some(r)) = (s.f32("grid_cols"), s.f32("grid_rows")) {
            self.grid_div = [c.clamp(1.0, 32.0) as u8, r.clamp(1.0, 32.0) as u8];
        }
        if let Some(v) = s.bool("grid_blend") {
            self.grid_blend = v;
        }
        if let Some(v) = s.f32("transform_perspective") {
            self.transform_perspective = v.clamp(0.0, 100.0) as u8;
        }
        s.read_bool("lasso_polygon", &mut self.lasso_polygon);
        s.read_bool("lasso_aa", &mut self.lasso_aa);
        s.read_bool("gradient_circle", &mut self.grad_opts.circle);
        s.read_bool("gradient_transparent", &mut self.grad_opts.to_transparent);
        s.read_bool("gradient_reverse", &mut self.grad_opts.reverse);
        s.read_bool("gradient_s_curve", &mut self.grad_opts.s_curve);
        self.lassofill.load(&s);
        if let Some(t) = s.f32("fill_gap") {
            self.fill_gap = t.clamp(0.0, 20.0) as u32;
        }
        if let Some(t) = s.f32("fill_expand") {
            self.fill_expand = t.clamp(0.0, 10.0) as u32;
        }
        if let Some(t) = s.f32("wand_tolerance") {
            self.wand_tolerance = t.clamp(0.0, 255.0) as u8;
        }
        self.recent = (0..10).filter_map(|i| s.str(&format!("recent.{i}")).filter(|v| !v.is_empty()).map(PathBuf::from)).collect();
        if let Some(p) = s.str("open_path") {
            if self.open_path.is_empty() {
                self.open_path = p.to_string();
            }
        }
        for (i, v) in self.color_views.iter_mut().enumerate() {
            s.read_bool(&format!("color_view.{i}"), v);
        }
        let hex = |v: &str| u32::from_str_radix(v, 16).ok().map(|n| Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8));
        for (r, row) in self.mixer.iter_mut().enumerate() {
            for (e, c) in row.iter_mut().enumerate() {
                if let Some(v) = s.str(&format!("mixer.{r}.{e}")).and_then(hex) {
                    *c = v;
                }
            }
        }
        self.recent_colors = (0..RECENT_COLORS).filter_map(|i| s.str(&format!("recent.{i}")).and_then(hex)).collect();
        for (i, c) in self.swatches.iter_mut().enumerate() {
            *c = s.str(&format!("swatch.{i}")).and_then(hex);
        }
        if let Some(n) = s.f32("swatch_set.count").map(|v| v as usize).filter(|&n| (1..=100).contains(&n)) {
            self.swatch_sets = (0..n)
                .map(|k| {
                    let name = s.str(&format!("swatch_set.{k}.name")).unwrap_or("").to_string();
                    let mut cols: Vec<Option<Color32>> = s.str(&format!("swatch_set.{k}.colors")).unwrap_or("").split(',').map(hex).collect();
                    cols.resize(SWATCHES, None);
                    (name, cols)
                })
                .collect();
            self.swatch_set = s.f32("swatch_set.cur").map_or(0, |v| v as usize).min(n - 1);
        }
        self.swatch_sets[self.swatch_set].1 = self.swatches.clone();
        for (k, v) in s.prefixed("asset_group.") {
            self.asset_groups.insert(k, v);
        }
        if let Some(p) = s.str("paper_dir") {
            self.paper_dir = p.to_string();
            self.set_paper_dir();
        }
    }

    /// Раз в 3 минуты, если были правки, записать копию холста в фоне (`recovery.sai2`).
    fn autosave(&mut self) {
        use std::sync::atomic::Ordering;
        if self.dirty {
            self.recovery_dirty = true;
        }
        // PAINT_AUTOSAVE_SECS — другой промежуток (для проверок)
        let every = std::env::var("PAINT_AUTOSAVE_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(self.opts.recovery_min.max(1) as u64 * 60);
        if !self.recovery_dirty || self.recovery_at.elapsed() < Duration::from_secs(every) {
            return;
        }
        if self.live.is_some() || self.transform.is_some() || self.filter.is_some() || self.move_drag.is_some() {
            return;
        }
        let Some((file, info)) = recovery_files() else { return };
        if self.recovery_busy.swap(true, Ordering::SeqCst) {
            return;
        }
        self.recovery_at = Instant::now();
        self.recovery_dirty = false;
        let doc = self.doc.snapshot();
        let original = self.file.as_ref().map(|f| f.display().to_string()).unwrap_or_default();
        let busy = self.recovery_busy.clone();
        std::thread::spawn(move || {
            if let Some(d) = file.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            if paint_formats::sai2_write::save(&doc, &file).is_ok() {
                let _ = std::fs::write(&info, original);
            }
            busy.store(false, Ordering::SeqCst);
        });
    }

    fn save_settings(&self) {
        let mut s = settings::Settings::load();
        s.set("tools.count", self.brushes.len());
        for (i, b) in self.brushes.iter().enumerate() {
            s.set(&format!("tool.{i}.kind"), b.kind.key());
            s.set(&format!("tool.{i}.name"), b.custom.as_deref().unwrap_or(""));
            s.set(&format!("tool.{i}.extra"), &b.extra);
            s.set(&format!("tool.{i}.key"), b.key.map_or(String::new(), |k| k.text()));
            s.set(&format!("tool.{i}.group"), b.group);
        }
        for (i, b) in self.brushes.iter().enumerate() {
            write_tool(b, &mut s, &format!("brush.{i}."));
            s.set(&format!("brush.{i}.has_reserved"), b.reserved.is_some() as u8);
            if let Some(r) = &b.reserved {
                paint_brush::write(r, &mut s, &format!("brush.{i}.reserved."));
            }
        }
        s.set("brush", self.brush);
        s.set("tool_groups.count", self.tool_groups.len());
        for (g, n) in self.tool_groups.iter().enumerate() {
            s.set(&format!("tool_groups.{g}"), n);
        }
        s.set("tool_group", self.tool_group);
        for c in 0..3 {
            s.set(&format!("color.{c}"), self.hsv[c]);
        }
        s.set("back.r", self.back.r());
        s.set("back.g", self.back.g());
        s.set("back.b", self.back.b());
        s.set("stabilizer", self.stabilizer.label());
        s.set("wand_tolerance", self.wand_tolerance);
        self.lassofill.save(&mut s);
        s.set("fill_gap", self.fill_gap);
        s.set("fill_expand", self.fill_expand);
        s.set("transform_perspective", self.transform_perspective);
        s.set("grid_cols", self.grid_div[0]);
        s.set("grid_rows", self.grid_div[1]);
        s.set("grid_blend", self.grid_blend as u8);
        let w = &self.win;
        s.set("ui_scale", w.ui_scale);
        s.set("tools_right", w.tools_right as u8);
        for (i, k) in SHOW_KEYS.iter().enumerate() {
            s.set(k, w.show[i] as u8);
        }
        for (i, k) in SHOW_KEYS[..4].iter().enumerate() {
            s.set(&format!("separate_{k}"), w.separate[i] as u8);
        }
        s.set("viewer.always", self.always_viewer as u8);
        s.set("viewer.marks", self.viewer_marks.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\u{1f}"));
        if let Some(d) = &self.viewer_dir {
            s.set("viewer.dir", d.display());
        }
        s.set("viewer.view", ((self.viewer_view.0 as u16) << 8) | self.viewer_view.1 as u16);
        let o = self.opts;
        for (k, v) in [("opt.dbl_props", o.dbl_props), ("opt.swap_zoom", o.swap_zoom), ("opt.size_relative", o.size_relative), ("opt.spring_tools", o.spring_tools), ("opt.spring_color", o.spring_color), ("opt.spring_funcs", o.spring_funcs), ("opt.duplex_revert", o.duplex_revert), ("opt.popup_palette", o.popup_palette), ("opt.psd16", o.psd16), ("opt.psd_exact", o.psd_exact)] {
            s.set(k, v as u8);
        }
        for (k, v) in [("opt.spring_ms", o.spring_ms), ("opt.move_guard_ms", o.move_guard_ms), ("opt.history_mb", o.history_mb), ("opt.recovery_min", o.recovery_min)] {
            s.set(k, v);
        }
        let [[x1, y1], [x2, y2]] = o.pressure_curve.points;
        s.set("opt.pressure_curve", format!("{x1:.3},{y1:.3},{x2:.3},{y2:.3}"));
        s.set("opt.min_pressure", o.min_pressure);
        s.set("layers_right", w.layers_right as u8);
        s.set("dot_cursor", w.dot_cursor as u8);
        s.set("brush_circle", w.brush_circle as u8);
        s.set("size_numbers", w.size_numbers as u8);
        s.set("size_list_top", w.size_list_top as u8);
        s.set("hsv_mode", w.hsv_mode);
        s.set("swatch_size", w.swatch_size);
        s.set("canvas_preset.count", self.canvas_presets.len());
        for (i, (n, w, h)) in self.canvas_presets.iter().enumerate() {
            s.set(&format!("canvas_preset.{i}"), format!("{}|{w}|{h}", n.replace('|', "/")));
        }
        s.set("new_width", self.new_size[0]);
        s.set("new_height", self.new_size[1]);
        s.set("lasso_polygon", self.lasso_polygon);
        s.set("lasso_aa", self.lasso_aa);
        s.set("gradient_circle", self.grad_opts.circle);
        s.set("gradient_transparent", self.grad_opts.to_transparent);
        s.set("gradient_reverse", self.grad_opts.reverse);
        s.set("gradient_s_curve", self.grad_opts.s_curve);
        s.set("check_updates", self.check_updates as u8);
        s.set("dark_theme", self.dark_theme as u8);
        s.set("pixel_grid", self.pixel_grid as u8);
        s.set("wrap_around", self.wrap_around as u8);
        {
            let wm = &self.watermark;
            s.set("wm.on", self.watermark_on as u8);
            s.set("wm.text", &wm.text);
            s.set("wm.image", wm.image.as_ref().map(|p| p.display().to_string()).unwrap_or_default());
            s.set("wm.font", &wm.font);
            s.set("wm.color", format!("{:02x}{:02x}{:02x}", wm.color[0], wm.color[1], wm.color[2]));
            s.set("wm.opacity", wm.opacity);
            s.set("wm.size", wm.size);
            s.set("wm.margin", wm.margin);
            s.set("wm.place", wm.place.key());
        }
        s.set("symmetry", self.symmetry);
        s.set("sym_n", self.sym_n);
        s.set("sym_mirror", self.sym_mirror as u8);
        s.set("sym_angle", self.sym_angle);
        for (a, id, _, _) in keys::ALL {
            let v = self.keymap.get(&a).copied().flatten().map_or_else(|| "none".to_string(), |b| b.text());
            s.set(&format!("key.{id}"), v);
        }
        s.set("open_path", &self.open_path);
        for i in 0..10 {
            s.set(&format!("recent.{i}"), self.recent.get(i).map_or(String::new(), |p| p.display().to_string()));
        }
        s.set("paper_dir", &self.paper_dir);
        for (i, v) in self.color_views.iter().enumerate() {
            s.set(&format!("color_view.{i}"), *v as u8);
        }
        let hex = |c: Color32| format!("{:02x}{:02x}{:02x}", c.r(), c.g(), c.b());
        for (r, row) in self.mixer.iter().enumerate() {
            for (e, c) in row.iter().enumerate() {
                s.set(&format!("mixer.{r}.{e}"), hex(*c));
            }
        }
        for i in 0..RECENT_COLORS {
            match self.recent_colors.get(i) {
                Some(c) => s.set(&format!("recent.{i}"), hex(*c)),
                None => s.remove(&format!("recent.{i}")),
            }
        }
        for (i, c) in self.swatches.iter().enumerate() {
            match c {
                Some(c) => s.set(&format!("swatch.{i}"), hex(*c)),
                None => s.remove(&format!("swatch.{i}")),
            }
        }
        let mut sets = self.swatch_sets.clone();
        if let Some(cur) = sets.get_mut(self.swatch_set) {
            cur.1 = self.swatches.clone();
        }
        s.set("swatch_set.count", sets.len());
        s.set("swatch_set.cur", self.swatch_set);
        for (k, (name, cols)) in sets.iter().enumerate() {
            s.set(&format!("swatch_set.{k}.name"), name);
            s.set(&format!("swatch_set.{k}.colors"), cols.iter().map(|c| c.map_or(String::new(), hex)).collect::<Vec<_>>().join(","));
        }
        for (k, g) in &self.asset_groups {
            s.set(&format!("asset_group.{k}"), g);
        }
        s.save();
    }

    /// Форма и текстура инструмента `tool` для нового штриха (файлы ищутся в папке SAI).
    /// Копировать (и вырезать) рисунок текущего слоя в пределах выделения, как Ctrl+C в SAI;
    /// без выделения — весь рисунок слоя. `bounds` — «Копировать по границам выделения»:
    /// прямоугольник выделения целиком, без мягкого края.
    fn copy(&mut self, cut: bool, bounds: bool) {
        let Some(layer) = self.doc.layer(&self.selected) else { return };
        if layer.children.is_some() {
            self.status = String::from(tr("Копируется рисунок обычного слоя"));
            return;
        }
        let sel = &self.doc.selection;
        let whole = sel.is_empty();
        let area = if whole { paint_core::selection::content_bounds(&layer.pixels) } else { sel.bounds() };
        let Some((x0, y0, x1, y1)) = area.map(|(a, b, c, d)| (a.max(0), b.max(0), c.min(self.doc.width as i32), d.min(self.doc.height as i32))).filter(|b| b.0 < b.2 && b.1 < b.3) else {
            self.status = String::from(tr("Нечего копировать"));
            return;
        };
        let (w, h) = ((x1 - x0) as u32, (y1 - y0) as u32);
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in y0..y1 {
            for x in x0..x1 {
                let p = layer.pixels.get(x, y);
                let k = if whole || bounds { paint_core::pixel::ONE } else { sel.amount(x, y) };
                let p = p.map(|c| ((c as u32 * k + paint_core::pixel::ONE / 2) / paint_core::pixel::ONE) as u16);
                rgba.extend(paint_core::pixel::to_rgba8(p));
            }
        }
        self.clip.set_image(&clip::Image { width: w, height: h, rgba });
        self.clip_own = Some((x0, y0, w, h));
        if cut {
            self.layer_action(LayerAction::Clear);
        }
        self.status = trf("Скопировано {0}×{1}", &[&w, &h]);
    }

    /// Вставить картинку из буфера новым слоем над текущим, как Ctrl+V в SAI: своё — на то же
    /// место, чужое — посередине холста. Потом — инструмент «Перемещение».
    fn paste(&mut self) {
        let Some(img) = self.clip.get_image() else {
            self.status = String::from(tr("В буфере обмена нет картинки"));
            return;
        };
        let (x, y) = match self.clip_own {
            Some((x, y, w, h)) if (w, h) == (img.width, img.height) => (x, y),
            _ => ((self.doc.width as i32 - img.width as i32) / 2, (self.doc.height as i32 - img.height as i32) / 2),
        };
        if self.transform.is_some() {
            self.finish_transform(true);
        }
        let n = count_layers(&self.doc.layers) + 1;
        let mut layer = Layer::new(&trf("Слой{0}", &[&n]));
        for (i, px) in img.rgba.as_chunks::<4>().0.iter().enumerate() {
            if px[3] > 0 {
                let (dx, dy) = ((i as u32 % img.width) as i32, (i as u32 / img.width) as i32);
                layer.pixels.set(x + dx, y + dy, paint_core::pixel::from_rgba8(*px));
            }
        }
        let mut path = self.selected.clone();
        let index = path.pop().map_or(0, |i| i + 1);
        path.push(index);
        let keys = self.doc.insert_layer(&path, layer);
        self.selected = path;
        self.after_structure(keys);
        self.tool = Tool::Move;
        self.dirty = true;
        self.status = trf("Вставлено {0}×{1}", &[&img.width, &img.height]);
    }

    /// «Холст из буфера обмена»: новый холст размером с картинку буфера.
    fn canvas_from_clipboard(&mut self) {
        let Some(img) = self.clip.get_image() else {
            self.status = String::from(tr("В буфере обмена нет картинки"));
            return;
        };
        self.new_canvas(img.width, img.height);
        for (i, px) in img.rgba.as_chunks::<4>().0.iter().enumerate() {
            if px[3] > 0 {
                let (x, y) = ((i as u32 % img.width) as i32, (i as u32 / img.width) as i32);
                self.doc.layers[0].pixels.set(x, y, paint_core::pixel::from_rgba8(*px));
            }
        }
        self.invalidate_all();
    }

    /// Правый щелчок по сетке инструментов: добавить, копия, удалить, свойства.
    fn grid_action(&mut self, i: usize, act: GridAction) {
        match act {
            GridAction::ToGroup(g) => {
                self.brushes[i].group = g;
            }
            GridAction::Add(k) => {
                let mut t = k.new_tool();
                t.group = self.tool_group;
                self.brushes.push(t);
                self.brush = self.brushes.len() - 1;
                self.tool = Tool::Brush;
            }
            GridAction::Duplicate => {
                let src = &self.brushes[i];
                let copy = BrushTool { brush: src.brush.clone(), key: None, reserved: src.reserved.clone(), before_restore: None, group: src.group };
                self.brushes.insert(i + 1, copy);
                if self.brush > i {
                    self.brush += 1;
                }
            }
            GridAction::Delete => {
                if self.brushes.len() > 1 {
                    self.brushes.remove(i);
                    if self.brush > i || self.brush >= self.brushes.len() {
                        self.brush = self.brush.saturating_sub(1);
                    }
                }
            }
            GridAction::Save => self.ask_save_brush(Some(i)),
            GridAction::Reserve => {
                let t = &mut self.brushes[i];
                t.reserved = Some(t.brush.clone());
                self.status = String::from(tr("Настройки инструмента запомнены"));
            }
            GridAction::Restore => {
                let t = &mut self.brushes[i];
                if let Some(r) = t.reserved.clone() {
                    // имя, подпись и вид — те же; возвращаются настройки
                    let (custom, extra) = (t.brush.custom.clone(), t.brush.extra.clone());
                    t.before_restore = Some(std::mem::replace(&mut t.brush, r));
                    t.brush.custom = custom;
                    t.brush.extra = extra;
                    self.status = String::from(tr("Настройки инструмента возвращены"));
                }
            }
            GridAction::UndoRestore => {
                let t = &mut self.brushes[i];
                if let Some(b) = t.before_restore.take() {
                    t.brush = b;
                    self.status = String::from(tr("Возврат настроек отменён"));
                }
            }
            GridAction::Property => {
                let t = &self.brushes[i];
                self.tool_props = Some((i, t.title(), t.extra.clone(), t.key));
            }
        }
    }

    /// Вкладки групп инструментов над сеткой (SAI 2020+): выбор, правый щелчок — новая группа,
    /// переименовать, удалить (инструменты удалённой группы переходят в первую).
    fn tool_group_tabs(&mut self, ui: &mut egui::Ui) {
        if self.tab_brush != self.brush {
            self.tab_brush = self.brush;
            if let Some(b) = self.brushes.get(self.brush) {
                self.tool_group = b.group.min(self.tool_groups.len().saturating_sub(1));
            }
        }
        let mut act: Option<(usize, u8)> = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (g, name) in self.tool_groups.iter().enumerate() {
                let r = ui.selectable_label(self.tool_group == g, egui::RichText::new(name).size(11.0));
                if r.clicked() {
                    act = Some((g, 0));
                }
                r.context_menu(|ui| {
                    if ui.button(tr("Новая группа")).clicked() {
                        act = Some((g, 1));
                        ui.close_menu();
                    }
                    if ui.button(tr("Переименовать…")).clicked() {
                        act = Some((g, 2));
                        ui.close_menu();
                    }
                    if ui.add_enabled(self.tool_groups.len() > 1, egui::Button::new(tr("Удалить группу"))).clicked() {
                        act = Some((g, 3));
                        ui.close_menu();
                    }
                });
            }
        });
        match act {
            Some((g, 0)) => self.tool_group = g,
            Some((_, 1)) => {
                self.tool_groups.push(trf("Группа {0}", &[&(self.tool_groups.len() + 1)]));
                self.tool_group = self.tool_groups.len() - 1;
            }
            Some((g, 2)) => self.group_rename = Some((g, self.tool_groups[g].clone())),
            Some((g, 3)) if self.tool_groups.len() > 1 => {
                self.tool_groups.remove(g);
                for b in self.brushes.iter_mut() {
                    if b.group == g {
                        b.group = 0;
                    } else if b.group > g {
                        b.group -= 1;
                    }
                }
                self.tool_group = self.tool_group.min(self.tool_groups.len() - 1);
            }
            _ => {}
        }
        if let Some((g, mut name)) = self.group_rename.take() {
            let mut keep = true;
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(120.0));
                if ui.button("OK").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    if let (Some(n), false) = (self.tool_groups.get_mut(g), name.trim().is_empty()) {
                        *n = name.trim().to_string();
                    }
                    keep = false;
                }
            });
            if keep {
                self.group_rename = Some((g, name));
            }
        }
    }

    /// Вид текущего инструмента сетки.
    fn kind(&self) -> ToolKind {
        self.brushes[self.brush].kind
    }

    /// Ластик для обратного конца пера: первый ластик сетки (без него — текущий инструмент).
    fn eraser_index(&self) -> usize {
        self.brushes.iter().position(|b| b.kind == ToolKind::Eraser).unwrap_or(self.brush)
    }

    /// Перо или ластик выделения.
    fn sel_tool(&self) -> bool {
        matches!(self.kind(), ToolKind::SelPen | ToolKind::SelErs | ToolKind::BinSelPen | ToolKind::BinSelErs)
    }

    /// Ластик выделения (обычный или двоичный).
    fn sel_eraser(&self) -> bool {
        matches!(self.kind(), ToolKind::SelErs | ToolKind::BinSelErs)
    }

    fn grain_for(&self, tool: usize) -> Option<paint_core::stroke::Grain> {
        let paper = self.doc.layer(&self.selected).and_then(|l| l.paper.as_ref()).and_then(|p| Some((p.texture.clone()?, p.scale)));
        self.brushes[tool].stroke_grain(paper)
    }

    /// Кривые текущего слоя контура (разобранные, с кэшем по отпечатку данных).
    fn line_shapes(&mut self) -> Option<(u64, &[paint_formats::sai2_vector::Shape])> {
        let layer = self.doc.layer(&self.selected)?;
        let v = layer.vector.as_ref().filter(|v| &v.kind == b"liwk")?;
        let h = raw_hash(&v.raw);
        let fresh = self.line_view.as_ref().is_some_and(|(p, k, _)| *p == self.selected && *k == h);
        if !fresh {
            let shapes = paint_formats::sai2_vector::parse(&v.raw, true).ok()?;
            self.line_view = Some((self.selected.clone(), h, shapes));
        }
        self.line_view.as_ref().map(|(_, h, s)| (*h, s.as_slice()))
    }

    /// Узлы слоя контура поверх холста, как в SAI: кривые тонкой линией, узлы — точками (концы
    /// — жёлтыми ромбами), выделенные узлы — крупными кружками. Видны с «Правкой» и «Нажимом»
    /// или когда на слое выделены узлы.
    fn draw_line_nodes(&mut self, painter: &egui::Painter, xf: ViewXf) {
        let editing = matches!(self.line_tool, LineTool::Edit | LineTool::Pressure);
        let live = self.line_edit.as_ref().filter(|e| e.path == self.selected).map(|e| e.shapes.clone());
        let (hash, shapes) = match live {
            Some(sh) => (None, sh),
            None => match self.line_shapes() {
                Some((h, sh)) => (Some(h), sh.to_vec()),
                None => return,
            },
        };
        let sel: Vec<(usize, usize, usize)> = match &self.cp_sel {
            Some(c) if c.path == self.selected && (hash.is_none() || hash == Some(c.raw)) => c.nodes.iter().copied().collect(),
            _ => Vec::new(),
        };
        if !editing && sel.is_empty() {
            return;
        }
        let s = |p: [f64; 2]| xf.to_screen(Vec2::new(p[0] as f32, p[1] as f32));
        let curve = egui::Stroke::new(1.0_f32, Color32::from_rgb(220, 110, 210));
        for sh in &shapes {
            for p in &sh.paths {
                let o = p.offset;
                let at = |q: [f64; 2]| s([q[0] + o[0], q[1] + o[1]]);
                let n = p.points.len();
                let segs = if p.closed { n } else { n.saturating_sub(1) };
                for i in 0..segs {
                    let (a, b) = (&p.points[i], &p.points[(i + 1) % n]);
                    painter.add(egui::epaint::CubicBezierShape::from_points_stroke([at(a.point), at(a.handle_out), at(b.handle_in), at(b.point)], false, Color32::TRANSPARENT, curve));
                }
            }
        }
        let green = Color32::from_rgb(40, 200, 60);
        for (si, sh) in shapes.iter().enumerate() {
            for (pi, p) in sh.paths.iter().enumerate() {
                let n = p.points.len();
                for (ni, c) in p.points.iter().enumerate() {
                    let q = s([c.point[0] + p.offset[0], c.point[1] + p.offset[1]]);
                    if sel.contains(&(si, pi, ni)) {
                        painter.circle_filled(q, 4.5, green);
                        painter.circle_stroke(q, 4.5, egui::Stroke::new(1.0_f32, Color32::from_gray(40)));
                    } else if !p.closed && (ni == 0 || ni + 1 == n) {
                        let d = 3.5;
                        painter.add(egui::Shape::convex_polygon(vec![q + Vec2::new(0.0, -d), q + Vec2::new(d, 0.0), q + Vec2::new(0.0, d), q + Vec2::new(-d, 0.0)], Color32::from_rgb(250, 215, 0), egui::Stroke::NONE));
                    } else {
                        painter.circle_filled(q, 2.5, green);
                    }
                }
            }
        }
    }

    /// «Выделить узлы в выделении» (`whole` — «Выделить штрихи по выделению»: пути целиком,
    /// если выделение задевает их кривую) и «Снять выделение узлов» (`None`), как в SAI. Рамка
    /// выделения после выбора снимается — так делает SAI.
    fn select_cps(&mut self, whole: Option<bool>) {
        let Some(whole) = whole else {
            self.cp_sel = None;
            return;
        };
        let inside = |sel: &paint_core::selection::Selection, q: [f64; 2]| sel.amount(q[0].floor() as i32, q[1].floor() as i32) * 2 > paint_core::pixel::ONE;
        let sel_mask = self.doc.selection.clone();
        if sel_mask.is_empty() {
            self.status = String::from(tr("Нет выделения"));
            return;
        }
        let path = self.selected.clone();
        let Some((raw, shapes)) = self.line_shapes().map(|(h, s)| (h, s.to_vec())) else { return };
        let mut nodes = std::collections::BTreeSet::new();
        for (si, sh) in shapes.iter().enumerate() {
            for (pi, p) in sh.paths.iter().enumerate() {
                let o = p.offset;
                let abs = |q: [f64; 2]| [q[0] + o[0], q[1] + o[1]];
                let hit_cp: Vec<usize> = (0..p.points.len()).filter(|&ni| inside(&sel_mask, abs(p.points[ni].point))).collect();
                if !whole {
                    nodes.extend(hit_cp.into_iter().map(|ni| (si, pi, ni)));
                    continue;
                }
                // путь целиком, если выделения касается узел или точка кривой
                let n = p.points.len();
                let segs = if p.closed { n } else { n.saturating_sub(1) };
                let touched = !hit_cp.is_empty()
                    || (0..segs).any(|i| {
                        let (a, b) = (&p.points[i], &p.points[(i + 1) % n]);
                        let (p0, p1, p2, p3) = (abs(a.point), abs(a.handle_out), abs(b.handle_in), abs(b.point));
                        (1..32).any(|k| {
                            let t = k as f64 / 32.0;
                            let u = 1.0 - t;
                            let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
                            inside(&sel_mask, [w[0] * p0[0] + w[1] * p1[0] + w[2] * p2[0] + w[3] * p3[0], w[0] * p0[1] + w[1] * p1[1] + w[2] * p2[1] + w[3] * p3[1]])
                        })
                    });
                if touched {
                    nodes.extend((0..n).map(|ni| (si, pi, ni)));
                }
            }
        }
        self.status = trf("Выделено узлов: {0}", &[&nodes.len()]);
        self.cp_sel = Some(CpSelection { path, raw, nodes });
        self.deselect();
    }

    /// Рамка правящейся надписи и мигающий курсор (как в SAI).
    fn draw_text_box(&self, painter: &egui::Painter, xf: ViewXf) {
        let Some(e) = self.text_edit.as_ref() else { return };
        let carets = paint_formats::sai2_text::caret_positions(&self.text_now(e));
        let s = |p: [f64; 2]| xf.to_screen(Vec2::new(p[0] as f32, p[1] as f32));
        let pts: Vec<Pos2> = carets.iter().flat_map(|c| [s(c[0]), s(c[1])]).collect();
        let mut r = Rect::from_points(&pts).expand(4.0);
        if r.width() < 12.0 {
            r = r.expand2(Vec2::new(6.0, 0.0));
        }
        painter.rect_stroke(r, 0.0, egui::Stroke::new(1.0_f32, Color32::from_gray(150)));
        let t = painter.ctx().input(|i| i.time);
        if (t * 2.0) as i64 % 2 == 0 {
            if let Some(c) = carets.get(e.caret.min(carets.len() - 1)) {
                painter.line_segment([s(c[0]), s(c[1])], egui::Stroke::new(1.5_f32, Color32::from_rgb(220, 180, 0)));
            }
        }
        painter.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    }

    /// Шрифт по умолчанию — первый из привычных, что есть в системе.
    fn ensure_text_font(&mut self) {
        if self.text_font.is_empty() {
            let fonts = paint_formats::sai2_text::system_fonts();
            self.text_font = ["Noto Sans", "DejaVu Sans", "Segoe UI", "Arial"]
                .iter()
                .find(|f| fonts.iter().any(|n| n == *f))
                .map(|f| f.to_string())
                .or_else(|| fonts.first().cloned())
                .unwrap_or_default();
        }
    }

    /// Щелчок инструментом «Текст», как в SAI: внутри рамки правящейся надписи — курсор туда;
    /// по выбранному текстовому слою — его правка; иначе сразу новый текстовый слой в этой точке.
    /// Набор идёт прямо на холсте, настройки — на панели инструмента.
    fn begin_text(&mut self, x: f64, y: f64) {
        use paint_formats::sai2_text;
        self.ensure_text_font();
        if let Some(i) = self.text_edit.as_ref().and_then(|e| text_hit(&self.text_now(e), x, y)) {
            if let Some(e) = self.text_edit.as_mut() {
                e.caret = i;
            }
            return;
        }
        self.finish_text(true);
        let existing = self
            .doc
            .layer(&self.selected)
            .and_then(|l| l.vector.as_ref())
            .filter(|v| &v.kind == b"text")
            .and_then(|v| sai2_text::parse(&v.raw))
            .filter(|t| text_hit(t, x, y).is_some());
        if let Some(t) = existing {
            let layer = self.doc.layer(&self.selected).unwrap();
            let orig = (layer.vector.clone(), layer.pixels.clone());
            if let Some(first) = t.runs.first() {
                self.text_font = first.font.clone();
                self.text_size = first.size;
                self.text_bold = first.style & sai2_text::STYLE_BOLD != 0;
                self.text_italic = first.style & sai2_text::STYLE_ITALIC != 0;
            }
            self.text_vertical = t.flags[0] & sai2_text::FLAG_VERTICAL != 0;
            let text = t.text();
            self.text_edit = Some(TextEditing { path: self.selected.clone(), x: t.x, y: t.y, caret: text.chars().count(), text, created: false, orig: Some(orig), base: Some(t), changed: false });
            return;
        }
        // новый слой «Текст» над текущим — сразу, как в SAI
        let mut path = self.selected.clone();
        let index = path.pop().map_or(0, |i| i + 1);
        path.push(index);
        let keys = self.doc.insert_layer_quiet(&path, Layer::new(tr("Текст")));
        self.selected = path.clone();
        self.after_structure_quiet(keys);
        self.text_edit = Some(TextEditing { path, x, y, text: String::new(), caret: 0, created: true, orig: None, base: None, changed: false });
        self.text_refresh();
    }

    /// Надпись, как она выглядит сейчас: текст правки и настройки панели.
    fn text_now(&self, e: &TextEditing) -> paint_formats::sai2_text::TextLayer {
        use paint_formats::sai2_text::{self, TextLayer, TextRun};
        let c = self.color();
        let style = if self.text_bold { sai2_text::STYLE_BOLD } else { 0 } | if self.text_italic { sai2_text::STYLE_ITALIC } else { 0 };
        let run = TextRun { font: self.text_font.clone(), size: self.text_size, color: [c.r(), c.g(), c.b()], style, text: e.text.clone() };
        let mut layer = e.base.clone().unwrap_or_else(|| TextLayer::new(e.x, e.y, run.clone()));
        let vflag = if self.text_vertical { sai2_text::FLAG_VERTICAL } else { 0 };
        // без изменений — слой как был (в том числе по отрезкам)
        let same = !e.changed
            && e.base.as_ref().is_some_and(|b| b.text() == e.text && b.flags[0] & sai2_text::FLAG_VERTICAL == vflag && b.runs.first().is_some_and(|r| r.font == run.font && r.size == run.size && r.style == style && r.color == run.color));
        if !same {
            layer.runs = vec![run];
            layer.flags[0] = (layer.flags[0] & !sai2_text::FLAG_VERTICAL) | vflag;
        }
        layer
    }

    /// Перерисовать правящуюся надпись на холсте (без записи в историю).
    fn text_refresh(&mut self) {
        use paint_formats::sai2_text;
        let Some(e) = self.text_edit.as_ref() else { return };
        let layer = self.text_now(e);
        let mut pixels = paint_core::tile::ColorMap::new();
        sai2_text::render(&layer, &mut pixels);
        let vector = paint_core::layer::VectorData { kind: *b"text", raw: sai2_text::to_bytes(&layer) };
        let path = e.path.clone();
        let keys = self.doc.set_vector_quiet(&path, Some(vector), pixels);
        self.invalidate(keys);
        self.thumbs.clear();
    }

    /// Что-то в надписи поменялось (текст или настройка панели) — перерисовать.
    fn text_changed(&mut self) {
        if let Some(e) = self.text_edit.as_mut() {
            e.changed = true;
        }
        self.text_refresh();
    }

    /// Закончить набор: живая правка снимается и записывается одним шагом отмены (`commit`),
    /// или надпись возвращается как была. Пустая новая надпись исчезает.
    fn finish_text(&mut self, commit: bool) {
        use paint_formats::sai2_text;
        let Some(e) = self.text_edit.take() else { return };
        // слой за время правки мог исчезнуть или смениться — тогда правка просто бросается
        let ours = self.doc.layer(&e.path).is_some_and(|l| l.vector.as_ref().is_some_and(|v| &v.kind == b"text") || (e.created && l.vector.is_none()));
        if !ours {
            return;
        }
        let layer = self.text_now(&e);
        let keep = commit && !e.text.is_empty() && (e.created || e.changed);
        if e.created {
            let (_, keys) = self.doc.remove_layer_quiet(&e.path);
            self.invalidate(keys);
        } else if let Some((v, px)) = e.orig.clone() {
            let keys = self.doc.set_vector_quiet(&e.path, v, px);
            self.invalidate(keys);
        }
        if keep {
            let mut pixels = paint_core::tile::ColorMap::new();
            sai2_text::render(&layer, &mut pixels);
            let vector = paint_core::layer::VectorData { kind: *b"text", raw: sai2_text::to_bytes(&layer) };
            if e.created {
                let mut l = Layer::new(tr("Текст"));
                l.vector = Some(vector);
                l.pixels = pixels;
                let keys = self.doc.insert_layer(&e.path, l);
                self.selected = e.path.clone();
                self.after_structure(keys);
            } else {
                let keys = self.doc.set_vector_layer(&e.path, vector, pixels);
                self.invalidate(keys);
            }
            self.dirty = true;
        } else if e.created {
            // пустая надпись: выбран снова слой под ней
            let mut p = e.path.clone();
            if let Some(last) = p.last_mut() {
                *last = last.saturating_sub(1);
            }
            if self.doc.layer(&p).is_some() {
                self.selected = p;
            }
            self.after_structure_quiet(Vec::new());
        }
        self.text_font = layer.runs.first().map_or(self.text_font.clone(), |r| r.font.clone());
        self.thumbs.clear();
    }

    /// Клавиши при наборе: буквы и вставка — в текст, стрелки, Home/End, Backspace/Delete,
    /// Enter — новая строка, Esc — закончить.
    fn text_keys(&mut self, ctx: &egui::Context) {
        let Some(e) = self.text_edit.as_mut() else { return };
        let events = ctx.input(|i| i.events.clone());
        let mut changed = false;
        let mut done = false;
        let byte = |t: &str, c: usize| t.char_indices().nth(c).map_or(t.len(), |(b, _)| b);
        for ev in events {
            match ev {
                egui::Event::Text(t) | egui::Event::Paste(t) => {
                    let t: String = t.chars().filter(|c| !c.is_control() || *c == '\n').collect();
                    if !t.is_empty() {
                        let at = byte(&e.text, e.caret);
                        e.text.insert_str(at, &t);
                        e.caret += t.chars().count();
                        changed = true;
                    }
                }
                egui::Event::Key { key, pressed: true, modifiers, .. } => {
                    let n = e.text.chars().count();
                    let lines: Vec<&str> = e.text.split('\n').collect();
                    // строка и место в ней
                    let (mut row, mut col, mut left) = (0, 0, e.caret);
                    for (r, l) in lines.iter().enumerate() {
                        let len = l.chars().count();
                        if left <= len {
                            (row, col) = (r, left);
                            break;
                        }
                        left -= len + 1;
                    }
                    let start_of = |r: usize| lines[..r].iter().map(|l| l.chars().count() + 1).sum::<usize>();
                    match key {
                        egui::Key::Escape => done = true,
                        egui::Key::Enter => {
                            let at = byte(&e.text, e.caret);
                            e.text.insert(at, '\n');
                            e.caret += 1;
                            changed = true;
                        }
                        egui::Key::Backspace if e.caret > 0 => {
                            let at = byte(&e.text, e.caret - 1);
                            e.text.remove(at);
                            e.caret -= 1;
                            changed = true;
                        }
                        egui::Key::Delete if e.caret < n => {
                            let at = byte(&e.text, e.caret);
                            e.text.remove(at);
                            changed = true;
                        }
                        egui::Key::ArrowLeft => e.caret = e.caret.saturating_sub(1),
                        egui::Key::ArrowRight => e.caret = (e.caret + 1).min(n),
                        egui::Key::Home if modifiers.command => e.caret = 0,
                        egui::Key::End if modifiers.command => e.caret = n,
                        egui::Key::Home => e.caret = start_of(row),
                        egui::Key::End => e.caret = start_of(row) + lines[row].chars().count(),
                        egui::Key::ArrowUp if row > 0 => e.caret = start_of(row - 1) + col.min(lines[row - 1].chars().count()),
                        egui::Key::ArrowDown if row + 1 < lines.len() => e.caret = start_of(row + 1) + col.min(lines[row + 1].chars().count()),
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        if changed {
            self.text_changed();
        }
        if done {
            self.finish_text(true);
        }
    }

    /// Настройки текста на панели инструмента (как в SAI): вертикально, цвет, размер, начертание,
    /// список шрифтов. Меняют правящуюся надпись сразу.
    fn text_panel(&mut self, ui: &mut egui::Ui, lw: f32, w: f32) {
        use paint_formats::sai2_text;
        self.ensure_text_font();
        let mut ch = false;
        sai_ui::row(ui, tr("Раскладка"), lw, true, |ui| {
            ch |= ui.checkbox(&mut self.text_vertical, tr("Вертикально")).changed();
        });
        let c = self.color();
        sai_ui::row(ui, tr("Цвет"), lw, true, |ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::new(32.0, 18.0), egui::Sense::hover());
            ui.painter().rect_filled(r, 0.0, c);
            ui.painter().rect_stroke(r, 0.0, egui::Stroke::new(1.0_f32, sai_ui::border()));
        });
        sai_ui::row(ui, tr("Размер"), lw, true, |ui| {
            ch |= ui.add(egui::DragValue::new(&mut self.text_size).range(1.0..=999.0).speed(0.5).suffix(" px")).changed();
        });
        sai_ui::row(ui, tr("Начертание"), lw, true, |ui| {
            ch |= ui.checkbox(&mut self.text_bold, tr("Жирный")).changed();
            ch |= ui.checkbox(&mut self.text_italic, tr("Курсив")).changed();
        });
        sai_ui::row(ui, tr("Шрифт"), lw, true, |ui| {
            ui.add(egui::TextEdit::singleline(&mut self.font_filter).hint_text(tr("Поиск")).desired_width(ui.available_width() - 4.0));
        });
        let fonts = sai2_text::system_fonts();
        let f = self.font_filter.to_lowercase();
        // список, как у SAI: белый, имена в строку (длинные обрезаются)
        egui::Frame::none().stroke(egui::Stroke::new(1.0_f32, sai_ui::border())).fill(sai_ui::gray(255)).show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("font_list").max_height(240.0).auto_shrink([false, true]).show(ui, |ui| {
                ui.set_width(w - 4.0);
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                ui.spacing_mut().item_spacing.y = 0.0;
                for name in fonts.iter().filter(|n| f.is_empty() || n.to_lowercase().contains(&f)) {
                    let sel = *name == self.text_font;
                    let r = ui.add(egui::SelectableLabel::new(sel, egui::RichText::new(name).size(13.0))).on_hover_text(name);
                    if sel && self.font_scroll {
                        r.scroll_to_me(Some(egui::Align::Center));
                        self.font_scroll = false;
                    }
                    if r.clicked() && !sel {
                        self.text_font = name.clone();
                        ch = true;
                    }
                }
            });
        });
        // цвет надписи — основной цвет: сменили на круге — надпись тоже
        let col = [c.r(), c.g(), c.b()];
        if self.text_edit.is_some() && self.text_edit.as_ref().map(|e| self.text_now(e).runs.first().map(|r| r.color)) != Some(Some(col)) {
            ch = true;
        }
        if ch && self.text_edit.is_some() {
            self.text_changed();
        }
    }

    /// Окно «Обновление»: что нашлось, заметки выпуска, загрузка и перезапуск.
    fn update_window(&mut self, ctx: &egui::Context) {
        use update::State;
        let state = self.update.lock().unwrap().clone();
        let mut open = true;
        let mut close = false;
        egui::Window::new(tr("Обновление"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.label(trf("Установлена версия {0}.", &[&update::current()]));
                let release = match &state {
                    State::Available(r) | State::Downloading(r) | State::Installed(r) => Some(r.clone()),
                    State::Failed(_, r) => r.clone(),
                    _ => None,
                };
                match &state {
                    State::Idle | State::Checking => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(tr("Проверка…"));
                        });
                    }
                    State::UpToDate => {
                        ui.label(tr("Это последняя версия."));
                    }
                    State::Failed(err, _) => {
                        ui.colored_label(sai_ui::header(), tr("Не получилось:"));
                        ui.add(egui::Label::new(egui::RichText::new(err).weak()).wrap());
                    }
                    _ => {}
                }
                if let Some(r) = &release {
                    ui.strong(trf("Новая версия: {0}", &[&r.version]));
                    let notes = update::notes_for(&r.notes, i18n::code());
                    if !notes.is_empty() {
                        egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                            ui.add(egui::Label::new(notes.replace("**", "")).wrap());
                        });
                    }
                    ui.add_space(4.0);
                    match &state {
                        State::Downloading(_) => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(tr("Загрузка и установка…"));
                            });
                        }
                        State::Installed(_) => {
                            ui.label(tr("Готово: новая версия откроется при следующем запуске."));
                            ui.horizontal(|ui| {
                                let restart = ui
                                    .add_enabled(!self.dirty, egui::Button::new(tr("Перезапустить сейчас")))
                                    .on_disabled_hover_text(tr("Сначала сохраните холст"));
                                if restart.clicked() && update::restart() {
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                }
                                if ui.button(tr("Позже")).clicked() {
                                    close = true;
                                }
                            });
                        }
                        _ => {
                            ui.horizontal(|ui| {
                                if r.asset.is_some() && ui.button(tr("Скачать и установить")).clicked() {
                                    update::install(&self.update, r.clone(), ctx);
                                }
                                if ui.button(tr("Страница выпуска")).clicked() {
                                    update::open_url(&r.page);
                                }
                                if ui.button(tr("Позже")).clicked() {
                                    close = true;
                                }
                            });
                        }
                    }
                } else if ui.button(tr("Страница выпусков")).clicked() {
                    update::open_url(&format!("{}/releases", update::REPO));
                }
            });
        self.show_update = open && !close;
    }

    /// Образец мазка текущей кисти над её настройками (как в SAI 2018+): пересчитывается, когда
    /// меняются настройки.
    fn brush_preview_ui(&mut self, ui: &mut egui::Ui, width: f32) {
        let b = &self.brushes[self.brush].brush;
        let mut kv = paint_brush::Kv::default();
        paint_brush::write(b, &mut kv, "");
        let key = format!("{}{}", b.kind.key(), kv.to_text());
        let (w, h) = ((width.max(80.0) * self.ppp) as u32, (40.0 * self.ppp) as u32);
        let key = format!("{key}{w}");
        if self.brush_preview.as_ref().is_none_or(|(k, _)| *k != key) {
            let mut pb = b.clone();
            // ластик, смешивание и сдвиг краски на пустом образце не видны — рисуются как кисть
            pb.params.erase = false;
            pb.params.mix = None;
            // мазок на шашке прозрачности, как в SAI 2
            let layer = pb.preview_layer_rgba(w, h);
            let cell = (6.0 * self.ppp).max(4.0) as u32;
            let mut rgba = Vec::with_capacity(layer.len());
            for (i, p) in layer.as_chunks::<4>().0.iter().enumerate() {
                let (x, y) = (i as u32 % w, i as u32 / w);
                let bg = if (x / cell + y / cell).is_multiple_of(2) { 255u32 } else { 228 };
                let a = p[3] as u32;
                rgba.extend((0..3).map(|c| ((p[c] as u32 * a + bg * (255 - a)) / 255) as u8));
                rgba.push(255);
            }
            let img = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
            let tex = ui.ctx().load_texture("brush_preview", img, egui::TextureOptions::LINEAR);
            self.brush_preview = Some((key, tex));
        }
        if let Some((_, tex)) = &self.brush_preview {
            ui.add(egui::Image::new((tex.id(), Vec2::new(w as f32 / self.ppp, h as f32 / self.ppp))));
        }
    }

    /// Цвет мазка и случайности кисти-разброса: своё зерно у каждого мазка; цвет — с разбросом
    /// на весь мазок или (флажок «К каждой форме») свой у каждой формы.
    fn stroke_color(&self, stroke: &mut Stroke) -> [u8; 3] {
        let c = self.color();
        let fg = [c.r(), c.g(), c.b()];
        let Some(sc) = stroke.params.scatter else { return fg };
        stroke.seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
        let bg = [self.back.r(), self.back.g(), self.back.b()];
        if sc.each_shape {
            stroke.shape_colors = Some((fg, bg));
            fg
        } else {
            sc.color(fg, bg, stroke.seed)
        }
    }

    /// Сделать текущим набор образцов `k` (цвета прежнего набора запоминаются).
    fn switch_swatch_set(&mut self, k: usize) {
        if k >= self.swatch_sets.len() {
            return;
        }
        self.swatch_sets[self.swatch_set].1 = self.swatches.clone();
        self.swatch_set = k;
        self.swatches = self.swatch_sets[k].1.clone();
        self.swatches.resize(SWATCHES, None);
        self.save_settings();
    }

    /// Окно «Менеджер ресурсов» (SAI 2020+): наборы образцов, бумаги, формы и текстуры кисти,
    /// отпечатки, черновики; группы; новый ресурс из слоя.
    fn assets_window(&mut self, ctx: &egui::Context) {
        let Some(mut w) = self.assets.take() else { return };
        let mut open = true;
        let mut status: Option<String> = None;
        egui::Window::new(tr("Менеджер ресурсов")).open(&mut open).resizable(true).default_width(560.0).show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(170.0);
                    if ui.selectable_label(w.kind.is_none(), tr("Наборы образцов")).clicked() {
                        w = AssetsWindow::default();
                    }
                    for k in assets::Kind::ALL {
                        if ui.selectable_label(w.kind == Some(k), tr(k.label())).clicked() {
                            w = AssetsWindow { kind: Some(k), ..Default::default() };
                        }
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    match w.kind {
                        None => self.swatch_sets_ui(ui, &mut w),
                        Some(k) => {
                            if let Some(s) = self.asset_files_ui(ui, &mut w, k) {
                                status = Some(s);
                            }
                        }
                    }
                });
            });
        });
        if let Some(s) = status {
            self.status = s;
        }
        if open {
            self.assets = Some(w);
        }
    }

    /// Наборы образцов в менеджере ресурсов.
    fn swatch_sets_ui(&mut self, ui: &mut egui::Ui, w: &mut AssetsWindow) {
        let mut pick = None;
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            for (k, (name, cols)) in self.swatch_sets.iter().enumerate() {
                let n = if k == self.swatch_set { self.swatches.iter().flatten().count() } else { cols.iter().flatten().count() };
                let label = format!("{}{name} ({n})", if k == self.swatch_set { "● " } else { "" });
                if ui.selectable_label(w.sel.as_deref() == Some(&k.to_string()), label).clicked() {
                    w.sel = Some(k.to_string());
                    w.name = name.clone();
                }
                let _ = &mut pick;
            }
        });
        ui.horizontal(|ui| {
            if ui.button(tr("Новый набор")).clicked() {
                self.swatch_sets.push((trf("Набор {0}", &[&(self.swatch_sets.len() + 1)]), vec![None; SWATCHES]));
                pick = Some(self.swatch_sets.len() - 1);
            }
            let sel = w.sel.as_deref().and_then(|v| v.parse::<usize>().ok()).filter(|&k| k < self.swatch_sets.len());
            if let Some(k) = sel {
                if ui.button(tr("Сделать текущим")).clicked() {
                    pick = Some(k);
                }
                if ui.add_enabled(self.swatch_sets.len() > 1, egui::Button::new(tr("Удалить"))).clicked() {
                    if k == self.swatch_set {
                        self.swatch_set = 0;
                    } else if k < self.swatch_set {
                        self.swatch_set -= 1;
                    }
                    let cur = if k == self.swatch_set && k == 0 { 1 } else { self.swatch_set };
                    self.swatch_sets.remove(k);
                    self.swatch_set = cur.min(self.swatch_sets.len() - 1);
                    self.swatches = self.swatch_sets[self.swatch_set].1.clone();
                    self.swatches.resize(SWATCHES, None);
                    w.sel = None;
                    self.save_settings();
                }
            }
        });
        if let Some(k) = w.sel.as_deref().and_then(|v| v.parse::<usize>().ok()).filter(|&k| k < self.swatch_sets.len()) {
            ui.horizontal(|ui| {
                ui.label(tr("Название:"));
                ui.add(egui::TextEdit::singleline(&mut w.name).desired_width(200.0));
                if ui.button(tr("Переименовать")).clicked() && !w.name.trim().is_empty() {
                    self.swatch_sets[k].0 = w.name.trim().to_string();
                    self.save_settings();
                }
            });
        }
        if let Some(k) = pick {
            self.switch_swatch_set(k);
            w.sel = Some(k.to_string());
        }
    }

    /// Ресурсы-файлы вида `k` в менеджере ресурсов; возвращает сообщение для строки состояния.
    fn asset_files_ui(&mut self, ui: &mut egui::Ui, w: &mut AssetsWindow, k: assets::Kind) -> Option<String> {
        let mut msg = None;
        let items = k.items();
        let group_of = |n: &str| self.asset_groups.get(&format!("{}/{n}", k.dir())).cloned().unwrap_or_default();
        let mut groups: Vec<String> = items.iter().map(|n| group_of(n)).collect();
        groups.sort();
        groups.dedup();
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            if items.is_empty() {
                ui.weak(tr("Своих ресурсов этого вида пока нет"));
            }
            for g in &groups {
                ui.strong(if g.is_empty() { tr("[ Без группы ]").to_string() } else { g.clone() });
                for n in items.iter().filter(|n| group_of(n) == *g) {
                    if ui.selectable_label(w.sel.as_deref() == Some(n.as_str()), format!("   {n}")).clicked() {
                        w.sel = Some(n.clone());
                        w.name = n.clone();
                        w.group = g.clone();
                        w.confirm_delete = false;
                    }
                }
            }
        });
        if let Some(sel) = w.sel.clone().filter(|n| items.contains(n)) {
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(tr("Название:"));
                ui.add(egui::TextEdit::singleline(&mut w.name).desired_width(200.0));
                if ui.button(tr("Переименовать")).clicked() {
                    match assets::rename(k, &sel, &w.name) {
                        Ok(n) => {
                            if let Some(g) = self.asset_groups.remove(&format!("{}/{sel}", k.dir())) {
                                self.asset_groups.insert(format!("{}/{n}", k.dir()), g);
                            }
                            w.sel = Some(n);
                            self.grain_names = None;
                            self.save_settings();
                        }
                        Err(e) => msg = Some(e),
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(tr("Группа:"));
                ui.add(egui::TextEdit::singleline(&mut w.group).desired_width(200.0));
                if ui.button(tr("Перенести")).clicked() {
                    let key = format!("{}/{sel}", k.dir());
                    if w.group.trim().is_empty() {
                        self.asset_groups.remove(&key);
                    } else {
                        self.asset_groups.insert(key, w.group.trim().to_string());
                    }
                    self.save_settings();
                }
            });
            ui.horizontal(|ui| {
                if k == assets::Kind::Scratch && ui.button(tr("Открыть в черновике")).clicked() {
                    msg = Some(self.load_scratch(&sel));
                }
                if !w.confirm_delete {
                    if ui.button(tr("Удалить…")).clicked() {
                        w.confirm_delete = true;
                    }
                } else {
                    ui.colored_label(Color32::from_rgb(190, 50, 30), trf("Удалить «{0}» с диска?", &[&sel]));
                    if ui.button(tr("Да")).clicked() {
                        msg = Some(match assets::delete(k, &sel) {
                            Ok(()) => trf("«{0}» удалён", &[&sel]),
                            Err(e) => e,
                        });
                        self.asset_groups.remove(&format!("{}/{sel}", k.dir()));
                        w.sel = None;
                        w.confirm_delete = false;
                        self.grain_names = None;
                        self.save_settings();
                    }
                    if ui.button(tr("Нет")).clicked() {
                        w.confirm_delete = false;
                    }
                }
            });
        }
        ui.separator();
        // новый ресурс: черновик — из черновика, остальное — из текущего слоя
        if k == assets::Kind::Scratch {
            ui.horizontal(|ui| {
                ui.label(tr("Название:"));
                ui.add(egui::TextEdit::singleline(&mut w.name).desired_width(200.0));
                if ui.button(tr("Сохранить текущий черновик")).clicked() {
                    let d = &self.scratch.doc;
                    msg = Some(match assets::create(k, &w.name, d.width as usize, d.height as usize, &d.flatten(), assets::ImageType::Alpha) {
                        Ok(n) => trf("Черновик сохранён: «{0}»", &[&n]),
                        Err(e) => e,
                    });
                }
            });
            return msg;
        }
        ui.label(tr("Новый из текущего слоя (выделенная часть или всё нарисованное):"));
        ui.horizontal(|ui| {
            use assets::ImageType as T;
            let cur = w.image.unwrap_or(T::BlackOnWhite);
            for (t, name) in [(T::BlackOnWhite, tr("Чёрное на белом")), (T::WhiteOnBlack, tr("Белое на чёрном")), (T::Alpha, tr("По прозрачности"))] {
                if ui.radio(cur == t, name).clicked() {
                    w.image = Some(t);
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label(tr("Название:"));
            ui.add(egui::TextEdit::singleline(&mut w.name).desired_width(200.0));
            if ui.button(tr("Создать")).clicked() {
                msg = Some(match self.layer_rgba() {
                    Some((lw, lh, rgba)) => match assets::create(k, &w.name, lw, lh, &rgba, w.image.unwrap_or(assets::ImageType::BlackOnWhite)) {
                        Ok(n) => {
                            self.grain_names = None;
                            w.sel = Some(n.clone());
                            trf("Ресурс создан: «{0}»", &[&n])
                        }
                        Err(e) => e,
                    },
                    None => tr("На слое нечего взять").to_string(),
                });
            }
        });
        msg
    }

    /// Пиксели текущего слоя (8 бит, без предумножения) в границах выделения или нарисованного.
    fn layer_rgba(&self) -> Option<(usize, usize, Vec<u8>)> {
        let layer = self.doc.layer(&self.selected)?;
        let sel = &self.doc.selection;
        let (x0, y0, x1, y1) = if sel.mask.tiles.is_empty() { paint_core::selection::content_bounds(&layer.pixels)? } else { sel.bounds()? };
        let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
        if w == 0 || h == 0 || w > 4096 || h > 4096 {
            return None;
        }
        let mut out = Vec::with_capacity(w * h * 4);
        for y in y0..y1 {
            for x in x0..x1 {
                let mut p = paint_core::pixel::to_rgba8(layer.pixels.get(x, y));
                if !sel.mask.tiles.is_empty() {
                    p[3] = ((p[3] as u32 * sel.amount(x, y)) / paint_core::pixel::ONE) as u8;
                }
                out.extend_from_slice(&p);
            }
        }
        Some((w, h, out))
    }

    /// Открыть сохранённый черновик (вписать в размер черновика).
    fn load_scratch(&mut self, name: &str) -> String {
        let Some(p) = assets::Kind::Scratch.path(name) else { return tr("нет папки настроек").to_string() };
        match paint_core::io::load_image(&p) {
            Ok((iw, ih, rgba)) => {
                let d = &mut self.scratch.doc;
                d.layers[0].pixels = paint_core::tile::ColorMap::new();
                for y in 0..ih.min(d.height) {
                    for x in 0..iw.min(d.width) {
                        let i = ((y * iw + x) * 4) as usize;
                        let c = [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]];
                        d.layers[0].pixels.set(x as i32, y as i32, paint_core::pixel::from_rgba8(c));
                    }
                }
                self.scratch.dirty = true;
                trf("Черновик «{0}» открыт", &[&name])
            }
            Err(e) => e.to_string(),
        }
    }

    /// Поменять основной и фоновый цвет (X, стрелка у квадратов цвета).
    fn swap_colors(&mut self) {
        let fore = self.color();
        self.hsv = sai_ui::rgb_to_hsv(self.back);
        self.back = fore;
    }

    /// Основной цвет из RGB; у серого тон не определён — прежний остаётся.
    /// Оси симметрии — по середине холста.
    fn mirror(&self) -> paint_core::Mirror {
        let (cx, cy) = (self.doc.width as f32 / 2.0, self.doc.height as f32 / 2.0);
        let radial = (self.symmetry == 4).then(|| {
            let [x, y] = self.sym_center.unwrap_or([cx, cy]);
            paint_core::Radial { cx: x, cy: y, angle: self.sym_angle.to_radians(), n: self.sym_n, mirror: self.sym_mirror }
        });
        paint_core::Mirror {
            radial,
            x: (self.symmetry < 4 && self.symmetry & 1 != 0).then_some(cx),
            y: (self.symmetry < 4 && self.symmetry & 2 != 0).then_some(cy),
            wrap: self.wrap_around.then_some((self.doc.width as f32, self.doc.height as f32)),
        }
    }

    /// Цвет мазка — в начало недавних (без повторов).
    fn push_recent(&mut self, c: Color32) {
        self.recent_colors.retain(|&x| x != c);
        self.recent_colors.insert(0, c);
        self.recent_colors.truncate(RECENT_COLORS);
        // Как в SAI: использованный цвет сам ложится в первую пустую клетку образцов
        // (занятые клетки не трогаются).
        if !self.swatches.contains(&Some(c)) {
            if let Some(slot) = self.swatches.iter_mut().find(|s| s.is_none()) {
                *slot = Some(c);
            }
        }
    }

    /// Кисти Clip Studio Paint — новыми инструментами в конец сетки (docs/33-csp-brushes.md).
    /// Текстура кисти пишется в свою папку программы `brushtex` как BMP.
    fn import_csp(&mut self, path: &std::path::Path) {
        match paint_formats::csp::load(path) {
            Ok(b) => self.add_imported(&b, "CSP"),
            Err(e) => self.status = trf("Кисть CSP не открылась: {0}", &[&e]),
        }
    }

    /// Спросить, куда сохранить кисть `i` (или весь набор при `None`).
    fn ask_save_brush(&mut self, i: Option<usize>) {
        let p = self.brush_default_path(i);
        let ask = i.map_or(files::Ask::SaveBrushSet, files::Ask::SaveBrush);
        if !self.native_dialog(ask, &p) {
            self.save_brush_file(p, i);
        }
    }

    /// Куда сохранять кисти без окна выбора: папка настроек, `brushes/`.
    fn brush_default_path(&self, i: Option<usize>) -> PathBuf {
        let dir = settings::dir().unwrap_or_else(|| std::env::current_dir().unwrap_or_default()).join("brushes");
        let _ = std::fs::create_dir_all(&dir);
        match i {
            Some(i) => dir.join(format!("{}.gsbrush", paint_brush::safe_name(&self.brushes[i].full_name()))),
            None => dir.join(format!("{}.gsbrushes", tr("Мои кисти"))),
        }
    }

    /// Динамика инструмента `i` в мазок: связи и (если выбрана) своя динамика из плагина.
    fn attach_dynamics(&mut self, st: &mut Stroke, i: usize) {
        st.dynamics = self.brushes[i].dynamics.clone();
        let Some(spec) = self.brushes[i].dyn_plugin.clone() else { return };
        let Some((id, name)) = spec.split_once('/') else { return };
        match self.plugins.iter().find(|p| p.plugin.id == id) {
            Some(p) => match paint_plugin::dynamics_hook(&p.plugin, name) {
                Ok((hook, err)) => {
                    st.dyn_hook = Some(hook);
                    self.dyn_error = Some(err);
                }
                Err(e) => self.status = trf("Ошибка плагина «{0}»: {1}", &[&p.plugin.name, &e]),
            },
            None => self.status = trf("Динамика «{0}»: плагин {1} не установлен", &[&name, &id]),
        }
    }

    /// Окно «Водяной знак»: текст или картинка, место, размер, непрозрачность; предпросмотр на
    /// уменьшенном рисунке.
    fn watermark_window(&mut self, ctx: &egui::Context) {
        use paint_formats::watermark::Place;
        let mut open = true;
        let mut changed = false;
        let mut pick_image = false;
        egui::Window::new(tr("Водяной знак")).open(&mut open).resizable(false).show(ctx, |ui| {
            ui.spacing_mut().slider_width = 180.0;
            ui.checkbox(&mut self.watermark_on, tr("Добавлять при экспорте картинки и в запись процесса"));
            let wm = &mut self.watermark;
            ui.horizontal(|ui| {
                ui.label(tr("Текст"));
                changed |= ui.add_enabled(wm.image.is_none(), egui::TextEdit::singleline(&mut wm.text).desired_width(220.0)).changed();
            });
            ui.horizontal(|ui| {
                ui.label(tr("Картинка"));
                ui.label(wm.image.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| tr("[ нет — текст ]").to_string()));
                if ui.small_button(tr("Выбрать…")).clicked() {
                    pick_image = true;
                }
                if wm.image.is_some() && ui.small_button("×").clicked() {
                    wm.image = None;
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label(tr("Место"));
                let name = |p: Place| match p {
                    Place::BottomRight => tr("справа внизу"),
                    Place::BottomLeft => tr("слева внизу"),
                    Place::TopRight => tr("справа вверху"),
                    Place::TopLeft => tr("слева вверху"),
                    Place::Center => tr("посередине"),
                    Place::Tiled => tr("по всей картинке"),
                };
                egui::ComboBox::from_id_salt("wmplace").selected_text(name(wm.place)).show_ui(ui, |ui| {
                    for p in Place::ALL {
                        changed |= ui.selectable_value(&mut wm.place, p, name(p)).changed();
                    }
                });
            });
            changed |= ui.add(egui::Slider::new(&mut wm.size, 0.01..=0.3).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)).text(tr("Размер"))).changed();
            changed |= ui.add(egui::Slider::new(&mut wm.opacity, 0.05..=1.0).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)).text(tr("Непрозрачность"))).changed();
            changed |= ui.add(egui::Slider::new(&mut wm.margin, 0.0..=0.1).custom_formatter(|v, _| format!("{:.1} %", v * 100.0)).text(tr("Отступ"))).changed();
            if wm.image.is_none() {
                let mut c = Color32::from_rgb(wm.color[0], wm.color[1], wm.color[2]);
                ui.horizontal(|ui| {
                    ui.label(tr("Цвет"));
                    if ui.color_edit_button_srgba(&mut c).changed() {
                        wm.color = [c.r(), c.g(), c.b()];
                        changed = true;
                    }
                });
            }
            if self.watermark_preview.is_none() || changed {
                // предпросмотр: рисунок, уменьшенный до 360 px по большей стороне
                let (w, h) = (self.doc.width, self.doc.height);
                let k = (360.0 / w.max(h) as f32).min(1.0);
                let (pw, ph) = (((w as f32 * k) as u32).max(1), ((h as f32 * k) as u32).max(1));
                let full = self.doc.flatten();
                let mut img = vec![0u8; (pw * ph * 4) as usize];
                for y in 0..ph {
                    for x in 0..pw {
                        let (sx, sy) = ((x as f32 / k) as u32, (y as f32 / k) as u32);
                        let si = ((sy.min(h - 1) * w + sx.min(w - 1)) * 4) as usize;
                        img[((y * pw + x) * 4) as usize..][..4].copy_from_slice(&full[si..si + 4]);
                    }
                }
                paint_formats::watermark::apply(&mut img, pw, ph, wm);
                self.watermark_preview = Some(ctx.load_texture("wm_preview", egui::ColorImage::from_rgba_unmultiplied([pw as usize, ph as usize], &img), egui::TextureOptions::LINEAR));
            }
            if let Some(t) = &self.watermark_preview {
                ui.image((t.id(), t.size_vec2()));
            }
            ui.label(tr("Знак добавляется только в экспорт (PNG, JPEG, WebP и др.) и в запись процесса; в сам рисунок и в .sai2 он не пишется."));
        });
        if pick_image {
            let dir = self.file.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default().join("x"));
            if !self.native_dialog(files::Ask::Watermark, &dir) {
                self.status = tr("Окно выбора файла недоступно; перетащите файл в окно").to_string();
            }
        }
        if !open {
            self.show_watermark = false;
            self.save_settings();
        }
    }

    /// Перечитать установленные плагины.
    fn reload_plugins(&mut self) {
        let (list, errors) = plugins::load_all();
        self.plugins = list;
        self.plugin_errors = errors;
    }

    /// Меню «Плагины»: команды (подменю по «/» в пути), управление, папка.
    fn plugins_menu(&mut self, ui: &mut egui::Ui) {
        let mut run: Option<(usize, usize)> = None;
        // команды разных плагинов с одним подменю — вместе: «Подменю» → [(подпись, плагин, команда)]
        let mut groups: Vec<(String, Vec<MenuItem>)> = Vec::new();
        for (pi, inst) in self.plugins.iter().enumerate() {
            for (ci, c) in inst.plugin.commands.iter().enumerate() {
                let parts: Vec<&str> = c.menu.split('/').map(str::trim).filter(|p| !p.is_empty()).collect();
                let (group, label) = match parts.as_slice() {
                    [] => (String::new(), c.menu.clone()),
                    [one] => (String::new(), one.to_string()),
                    [g, rest @ ..] => (g.to_string(), rest.join(" / ")),
                };
                let label = if inst.dev { format!("{label} ✱") } else { label };
                let item = (label, inst.plugin.name.clone(), pi, ci);
                match groups.iter_mut().find(|(g, _)| *g == group) {
                    Some((_, items)) => items.push(item),
                    None => groups.push((group, vec![item])),
                }
            }
        }
        let item = |ui: &mut egui::Ui, (label, hint, pi, ci): &MenuItem, run: &mut Option<(usize, usize)>| {
            if ui.button(label).on_hover_text(hint).clicked() {
                *run = Some((*pi, *ci));
                ui.close_menu();
            }
        };
        for (group, items) in &groups {
            if group.is_empty() {
                for it in items {
                    item(ui, it, &mut run);
                }
            } else {
                ui.menu_button(group, |ui| {
                    for it in items {
                        item(ui, it, &mut run);
                    }
                });
            }
        }
        if !self.plugins.is_empty() {
            ui.separator();
        }
        if ui.button(tr("Управление плагинами…")).clicked() {
            self.show_plugins = true;
            ui.close_menu();
        }
        if ui.button(tr("Открыть папку плагинов")).clicked() {
            if let Some(d) = plugins::dir() {
                let _ = std::fs::create_dir_all(&d);
                update::open_url(&d.display().to_string());
            }
            ui.close_menu();
        }
        if let Some((pi, ci)) = run {
            let c = &self.plugins[pi].plugin.commands[ci];
            let values: Vec<(String, f64)> = c.params.iter().map(|p| (p.id.clone(), p.default)).collect();
            if c.params.is_empty() {
                self.run_plugin(pi, ci, &values);
            } else {
                self.plugin_ask = Some((pi, ci, values));
            }
        }
    }

    /// Выполнить команду плагина над текущим слоем; всё, что плагин сделал, отменяется по слоям.
    fn run_plugin(&mut self, pi: usize, ci: usize, values: &[(String, f64)]) {
        if self.doc.layer(&self.selected).is_none_or(|l| l.children.is_some() || l.vector.is_some()) {
            self.status = tr("Плагин работает с обычным слоем — выберите его").to_string();
            return;
        }
        let (plugin, command) = (self.plugins[pi].plugin.clone(), self.plugins[pi].plugin.commands[ci].clone());
        let c = self.color();
        let sel = self.doc.selection.clone();
        let mut canvas = plugins::DocCanvas { doc: &mut self.doc, path: self.selected.clone(), color: [c.r(), c.g(), c.b()], touched: Vec::new(), created: 0 };
        let result = paint_plugin::run(&plugin, &command, values, &mut canvas, paint_plugin::Limits::default());
        let (touched, path, color) = (std::mem::take(&mut canvas.touched), canvas.path.clone(), canvas.color);
        for (p, orig) in touched {
            self.doc.move_commit(&p, orig, None, sel.clone());
        }
        self.selected = path;
        if color != [c.r(), c.g(), c.b()] {
            self.set_color(Color32::from_rgb(color[0], color[1], color[2]));
        }
        self.invalidate_all();
        self.thumbs.clear();
        self.dirty = true;
        let name = command.menu.rsplit('/').next().unwrap_or(&command.menu).to_string();
        match result {
            Ok(r) => {
                self.status = trf("«{0}»: готово за {1} с", &[&name, &format!("{:.2}", r.elapsed.as_secs_f32())]);
                self.plugin_log.extend(r.log.into_iter().map(|l| format!("{}: {l}", plugin.name)));
            }
            Err(e) => {
                self.status = trf("Ошибка плагина «{0}»: {1}", &[&plugin.name, &e.lines().next().unwrap_or("")]);
                self.plugin_log.push(format!("{}: {e}", plugin.name));
                self.show_plugins = true;
            }
        }
        let n = self.plugin_log.len();
        if n > 200 {
            self.plugin_log.drain(..n - 200);
        }
    }

    /// Окна плагинов: параметры команды, установка, управление.
    fn plugin_windows(&mut self, ctx: &egui::Context) {
        let mut go: Option<PluginAsk> = None;
        let mut close_ask = false;
        if let Some((pi, ci, values)) = self.plugin_ask.as_mut() {
            let c = self.plugins.get(*pi).and_then(|p| p.plugin.commands.get(*ci)).cloned();
            match c {
                None => close_ask = true,
                Some(c) => {
                    let title = c.menu.rsplit('/').next().unwrap_or(&c.menu).to_string();
                    egui::Window::new(title).collapsible(false).resizable(false).show(ctx, |ui| {
                        ui.spacing_mut().slider_width = 180.0;
                        for (p, (_, v)) in c.params.iter().zip(values.iter_mut()) {
                            match &p.kind {
                                paint_plugin::ParamKind::Slider { min, max, step } => {
                                    let mut sl = egui::Slider::new(v, *min..=*max).text(&p.label);
                                    if *step > 0.0 {
                                        sl = sl.step_by(*step);
                                    }
                                    ui.add(sl);
                                }
                                paint_plugin::ParamKind::Check => {
                                    let mut b = *v >= 0.5;
                                    if ui.checkbox(&mut b, &p.label).changed() {
                                        *v = b as u8 as f64;
                                    }
                                }
                                paint_plugin::ParamKind::Choice(opts) => {
                                    let cur = (*v as usize).min(opts.len().saturating_sub(1));
                                    egui::ComboBox::from_label(&p.label).selected_text(opts.get(cur).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                                        for (i, o) in opts.iter().enumerate() {
                                            if ui.selectable_label(i == cur, o).clicked() {
                                                *v = i as f64;
                                            }
                                        }
                                    });
                                }
                            }
                        }
                        ui.horizontal(|ui| {
                            if ui.button(tr("ОК")).clicked() {
                                go = Some((*pi, *ci, values.clone()));
                            }
                            if ui.button(tr("Отмена")).clicked() {
                                close_ask = true;
                            }
                        });
                    });
                }
            }
        }
        if close_ask || go.is_some() {
            self.plugin_ask = None;
        }
        if let Some((pi, ci, values)) = go {
            self.run_plugin(pi, ci, &values);
        }
        // установка
        let mut install = None;
        let mut close_install = false;
        if let Some((p, path)) = &self.plugin_install {
            egui::Window::new(tr("Установить плагин")).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.heading(&p.name);
                ui.label(format!("{} {} · {}", p.id, p.version, p.author));
                if !p.description.is_empty() {
                    ui.label(&p.description);
                }
                ui.separator();
                ui.label(tr("Команды:"));
                for c in &p.commands {
                    ui.label(format!("  • {}", c.menu));
                }
                ui.separator();
                ui.label(tr("Плагин работает в песочнице: без доступа к файлам, сети и системе; меняет только открытый рисунок, и всё это отменяется."));
                ui.horizontal(|ui| {
                    if ui.button(tr("Установить")).clicked() {
                        install = Some(path.clone());
                    }
                    if ui.button(tr("Отмена")).clicked() {
                        close_install = true;
                    }
                });
            });
        }
        if let Some(path) = install {
            self.status = match plugins::install(&path) {
                Ok(p) => trf("Плагин «{0}» установлен: меню «Плагины»", &[&p.name]),
                Err(e) => trf("Плагин не открылся: {0}", &[&e]),
            };
            self.reload_plugins();
            close_install = true;
        }
        if close_install {
            self.plugin_install = None;
        }
        // управление
        if self.show_plugins {
            let mut open = true;
            let mut remove = None;
            egui::Window::new(tr("Плагины")).open(&mut open).default_width(420.0).show(ctx, |ui| {
                if self.plugins.is_empty() {
                    ui.label(tr("Плагинов нет. Установить — «Файл → Открыть» файл .gsplugin или перетащить его в окно."));
                }
                for (i, p) in self.plugins.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(&p.plugin.name).strong());
                        ui.label(format!("{} {}", p.plugin.version, if p.dev { tr("(из мастерской)") } else { "" }));
                        if !p.dev && ui.small_button(tr("Удалить")).clicked() {
                            remove = Some(i);
                        }
                    });
                    if !p.plugin.description.is_empty() {
                        ui.label(&p.plugin.description);
                    }
                }
                if !self.plugin_errors.is_empty() {
                    ui.separator();
                    ui.label(tr("Не загрузились:"));
                    for e in &self.plugin_errors {
                        ui.colored_label(Color32::from_rgb(190, 40, 40), e);
                    }
                }
                if !self.plugin_log.is_empty() {
                    ui.separator();
                    ui.label(tr("Журнал:"));
                    egui::ScrollArea::vertical().max_height(180.0).stick_to_bottom(true).show(ui, |ui| {
                        for l in &self.plugin_log {
                            ui.monospace(l);
                        }
                    });
                }
            });
            if let Some(i) = remove {
                let _ = std::fs::remove_file(&self.plugins[i].path);
                self.reload_plugins();
            }
            self.show_plugins = open;
        }
    }

    /// Кисти из мастерской: раз в секунду смотрим отметку `brushes/dev/.changed`; изменилась —
    /// кисти из `brushes/dev/` встают в сетку (кисть с тем же именем и подписью «Studio»
    /// заменяется, клавиша остаётся).
    fn check_studio(&mut self, ctx: &egui::Context) {
        if self.dev_checked.elapsed() < Duration::from_secs(1) {
            ctx.request_repaint_after(Duration::from_secs(1));
            return;
        }
        self.dev_checked = Instant::now();
        ctx.request_repaint_after(Duration::from_secs(1));
        if let Some(d) = plugins::dir() {
            let ps = std::fs::metadata(d.join("dev").join(".changed")).and_then(|m| m.modified()).ok();
            if ps.is_some() && ps != self.plugins_stamp {
                let first = self.plugins_stamp.is_none();
                self.plugins_stamp = ps;
                if !first || self.started.elapsed() > Duration::from_secs(3) {
                    self.reload_plugins();
                    self.status = trf("Плагины обновлены из мастерской: {0}", &[&self.plugins.iter().filter(|p| p.dev).map(|p| p.plugin.name.clone()).collect::<Vec<_>>().join(", ")]);
                }
            }
        }
        let Some(dir) = settings::dir().map(|d| d.join("brushes").join("dev")) else { return };
        let stamp = std::fs::metadata(dir.join(".changed")).and_then(|m| m.modified()).ok();
        if stamp.is_none() || stamp == self.dev_stamp {
            return;
        }
        let first = self.dev_stamp.is_none();
        self.dev_stamp = stamp;
        // при запуске прошлые кисти мастерской не подхватываются — только новые экспорты
        if first && self.started.elapsed() < Duration::from_secs(3) {
            return;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else { return };
        let mut files: Vec<(std::time::SystemTime, PathBuf)> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "gsbrush"))
            .filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p)))
            .collect();
        files.sort();
        // только то, что изменилось за последние секунды перед отметкой
        let since = stamp.and_then(|t| t.checked_sub(Duration::from_secs(5)));
        let mut names = Vec::new();
        for (_, p) in files.into_iter().filter(|(t, _)| since.is_none_or(|s| *t >= s)) {
            let Ok(f) = paint_formats::pack::read(&p) else { continue };
            for mut brush in paint_brush::from_files(&f, tr("Кисть")) {
                brush.extra = "Studio".into();
                let name = brush.custom.clone().unwrap_or_default();
                match self.brushes.iter().position(|b| b.extra == "Studio" && b.custom == brush.custom) {
                    Some(i) => {
                        self.brushes[i].brush = brush;
                        self.brush = i;
                    }
                    None => {
                        self.brushes.push(BrushTool { brush, key: None, reserved: None, before_restore: None, group: 0 });
                        self.brush = self.brushes.len() - 1;
                    }
                }
                names.push(name);
            }
        }
        if !names.is_empty() {
            self.tool = Tool::Brush;
            self.grain_names = None;
            self.status = trf("Из мастерской: {0}", &[&names.join(", ")]);
            ctx.request_repaint();
        }
    }

    /// Сохранить кисть `i` или все инструменты сетки (`None`) в файл.
    fn save_brush_file(&mut self, path: PathBuf, i: Option<usize>) {
        let list: Vec<(&paint_brush::Brush, String)> = match i {
            Some(i) => vec![(&self.brushes[i].brush, self.brushes[i].full_name())],
            None => self.brushes.iter().map(|b| (&b.brush, b.full_name())).collect(),
        };
        self.status = match paint_brush::save(&path, &list) {
            Ok(()) => trf("Сохранено: {0}", &[&path.display()]),
            Err(e) => trf("Не удалось сохранить {0}: {1}", &[&path.display(), &e]),
        };
    }

    /// Открыть файл кисти или набора: кисти встают в конец сетки, их картинки — в свою папку.
    fn open_brush_file(&mut self, path: &std::path::Path) {
        let files = match paint_formats::pack::read(path) {
            Ok(f) => f,
            Err(e) => {
                self.status = trf("Кисть не открылась: {0}", &[&e]);
                return;
            }
        };
        let list = paint_brush::from_files(&files, tr("Кисть"));
        if list.is_empty() {
            self.status = tr("В файле нет кистей GrazySAI").to_string();
            return;
        }
        let added = list.len();
        self.brushes.extend(list.into_iter().map(|brush| BrushTool { brush, key: None, reserved: None, before_restore: None, group: 0 }));
        self.brush = self.brushes.len() - 1;
        self.tool = Tool::Brush;
        self.grain_names = None;
        self.status = trf("Кистей добавлено в сетку: {0}", &[&added]);
    }

    /// Пресет Krita `.kpp` или набор `.bundle` — новыми инструментами в конец сетки.
    fn import_krita(&mut self, path: &std::path::Path) {
        let bundle = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("bundle"));
        let r = if bundle { paint_formats::krita::load_bundle(path) } else { paint_formats::krita::load_kpp(path).map(|b| vec![b]) };
        match r {
            Ok(b) => self.add_imported(&b, "Krita"),
            Err(e) => self.status = trf("Кисть Krita не открылась: {0}", &[&e]),
        }
    }

    /// Чужие кисти (CSP, Krita) — инструментами сетки; `from` — подпись во второй строке ячейки.
    fn add_imported(&mut self, brushes: &[paint_formats::brush_tip::ImportedBrush], from: &str) {
        let mut skipped: Vec<&str> = Vec::new();
        for b in brushes {
            let kind = if b.erase {
                ToolKind::Eraser
            } else if b.mix.is_some() {
                ToolKind::Brush
            } else {
                ToolKind::Pencil
            };
            let mut t = kind.new_tool();
            let step = paint_core::brush::HARDNESS_STEPS.iter().copied().min_by(|x, y| (x - b.hardness).abs().total_cmp(&(y - b.hardness).abs())).unwrap_or(1.0);
            t.params = BrushParams {
                size: b.size,
                min_size: b.size_pressure.unwrap_or(1.0),
                size_by_pressure: b.size_pressure.is_some(),
                density: b.density,
                min_density: b.density_pressure.unwrap_or(1.0),
                density_by_pressure: b.density_pressure.is_some(),
                hardness: step,
                spacing: b.spacing.unwrap_or(0.0),
                mix: b.mix.map(|(blend, dilute)| MixParams::new(blend, dilute, 0.5)),
                ..t.params
            };
            t.custom = Some(b.name.clone());
            t.extra = from.to_string();
            if let (Some(tex), Some(dir)) = (&b.texture, paint_core::paper::user_dir("brushtex")) {
                let name: String = format!("{from} {}", b.name).chars().map(|c| if c.is_alphanumeric() || " -_().".contains(c) { c } else { '_' }).collect();
                if std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(dir.join(format!("{name}.bmp")), tex.to_bmp())).is_ok() {
                    t.grain.tex = TexChoice::File(name);
                    t.grain.tex_strength = b.texture_density;
                    t.grain.tex_scale = b.texture_scale;
                }
            }
            if let Some(tip) = &b.tip {
                let cells: Vec<paint_core::Stamp> = std::iter::once(tip.clone()).chain(b.tip_more.iter().cloned()).collect();
                if let Ok(name) = paint_core::paper::save_tips(&format!("{from} {}", b.name), &cells) {
                    t.grain.tip = Some(name);
                    t.grain.tip_random = b.tip_random;
                }
            }
            t.grain.tip_angle = b.tip_angle;
            t.grain.tip_ratio = b.tip_ratio;
            t.grain.tip_follow = b.tip_follow;
            for s in &b.skipped {
                if !skipped.contains(s) {
                    skipped.push(s);
                }
            }
            self.brushes.push(t);
        }
        self.brush = self.brushes.len() - 1;
        self.tool = Tool::Brush;
        self.grain_names = None;
        let added = match brushes {
            [one] => trf("Кисть {0} «{1}» добавлена в сетку", &[&from, &one.name]),
            many => trf("Кистей {0} добавлено в сетку: {1}", &[&from, &many.len()]),
        };
        let label = |s: &str| match s {
            "распыление" => tr("распыление"),
            "двойная кисть" => tr("двойная кисть"),
            "форма кончика из картинки" => tr("форма кончика из картинки"),
            "смена цвета по мазку" => tr("смена цвета по мазку"),
            "кайма акварели" => tr("кайма акварели"),
            "толщина кончика" => tr("толщина кончика"),
            "текстура (картинки нет в файле)" => tr("текстура (картинки нет в файле)"),
            "движок кисти (взяты только общие настройки)" => tr("движок кисти (взяты только общие настройки)"),
            "сплюснутый кончик" => tr("сплюснутый кончик"),
            "кончик (файла нет рядом с пресетом и в папках Krita)" => tr("кончик (файла нет рядом с пресетом и в папках Krita)"),
            "вид кончика (векторный или текстовый)" => tr("вид кончика (векторный или текстовый)"),
            "текстура" => tr("текстура"),
            _ => s,
        }
        .to_string();
        let skipped: Vec<String> = skipped.iter().map(|s| label(s)).collect();
        self.status = if skipped.is_empty() { added } else { format!("{added}. {}", trf("Нет в GrazySAI, пропущено: {0}", &[&skipped.join(", ")])) };
    }

    /// Палитра GIMP или Krita — в образцы цвета (вместо прежних, по порядку).
    fn load_palette(&mut self, path: &std::path::Path) {
        match paint_formats::palette::load(path) {
            Ok(p) => {
                let n = p.colors.len().min(SWATCHES);
                for (i, slot) in self.swatches.iter_mut().enumerate() {
                    *slot = p.colors.get(i).map(|(c, _)| Color32::from_rgb(c[0], c[1], c[2]));
                }
                self.color_views[4] = true;
                self.status = trf("Палитра «{0}»: {1} цветов в образцах", &[&p.name, &n]);
            }
            Err(e) => self.status = trf("Палитра не открылась: {0}", &[&e]),
        }
    }

    fn set_color(&mut self, c: Color32) {
        let hsv = sai_ui::rgb_to_hsv(c);
        let hue = if hsv[1] == 0.0 { self.hsv[0] } else { hsv[0] };
        self.hsv = [hue, hsv[1], hsv[2]];
    }

    fn color(&self) -> Color32 {
        sai_ui::hsv_to_rgb(self.hsv[0], self.hsv[1], self.hsv[2])
    }

    fn file_name(&self) -> String {
        self.file.as_ref().and_then(|f| f.file_name()).map_or_else(
            || tr("НовыйХолст1.sai2").to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    }

    fn open(&mut self, path: PathBuf) {
        let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
        if ext == "gpl" || ext == "kpl" {
            self.load_palette(&path);
            return;
        }
        if ext == "gbr" || ext == "gih" || ext == "abr" {
            self.add_tip(&path);
            return;
        }
        if ext == "gsplugin" {
            match paint_plugin::load(&path) {
                Ok(p) => self.plugin_install = Some((p, path.clone())),
                Err(e) => self.status = trf("Плагин не открылся: {0}", &[&e]),
            }
            return;
        }
        if ext == "gsbrush" || ext == "gsbrushes" {
            self.open_brush_file(&path);
            return;
        }
        if paint_3d::MODEL_EXTS.contains(&ext.as_str()) {
            self.s3d_new_layer(&path);
            return;
        }
        if ext == "kpp" || ext == "bundle" {
            self.import_krita(&path);
            return;
        }
        if ext == "sut" || ext == "bfps" {
            self.import_csp(&path);
            return;
        }
        if ext == "psd" || ext == "psb" || ext == "ora" || paint_core::io::IMAGE_EXTS.contains(&ext.as_str()) {
            let started = Instant::now();
            let loaded = read_guarded(|| {
                if ext == "psd" || ext == "psb" {
                    paint_formats::psd::load(&path)
                } else if ext == "ora" {
                    paint_formats::ora::load(&path)
                } else {
                    paint_core::io::load_image(&path).map(|(w, h, rgba)| Document::from_rgba(w, h, &rgba))
                }
            });
            match loaded {
                Ok(doc) => {
                    self.prepare_tab();
                    self.stop_recording();
                    self.doc = doc;
                    self.had_vectors = false;
                    self.notes.clear();
                    self.status = trf(
                        "Открыт {0} — {1}×{2}, слоёв {3}, за {4} с",
                        &[&path.display(), &self.doc.width, &self.doc.height, &self.doc.layers.len(), &format!("{:.1}", started.elapsed().as_secs_f32())],
                    );
                    self.remember_recent(&path);
                    self.file = Some(path);
                    self.selected = first_raster(&self.doc.layers).unwrap_or_default();
                    self.live = None;
                    self.fit_pending = true;
                    self.ruler.placed = false;
                    self.dirty = false;
                    self.invalidate_all();
                    self.resume_recording();
                }
                Err(e) => self.status = trf("Не удалось открыть {0}: {1}", &[&path.display(), &e]),
            }
            return;
        }
        let started = Instant::now();
        match read_guarded(|| paint_formats::sai2_doc::load(&path).map_err(Into::into)) {
            Ok(loaded) => {
                self.prepare_tab();
                // кривые слоёв контура и фигур теперь сохраняются как были
                self.had_vectors = false;
                self.stop_recording();
                self.doc = loaded.document;
                self.notes = loaded.warnings;
                self.status = trf(
                    "Открыт {0} — {1}×{2}, слоёв {3}, за {4} с",
                    &[&path.display(), &self.doc.width, &self.doc.height, &loaded.info.layers.len(), &format!("{:.1}", started.elapsed().as_secs_f32())],
                );
                self.remember_recent(&path);
                self.file = Some(path);
                self.selected = first_raster(&self.doc.layers).unwrap_or_default();
                self.live = None;
                self.fit_pending = true;
                self.ruler.placed = false;
                self.dirty = false;
                self.invalidate_all();
                self.resume_recording();
            }
            Err(e) => self.status = trf("Не удалось открыть {0}: {1}", &[&path.display(), &e]),
        }
    }

    /// Забрать поля активного холста во вкладку (на их месте — пустой холст).
    fn take_tab(&mut self) -> Tab {
        self.s3d_finish();
        if let Some(r) = self.rec.as_mut() {
            r.capture(&mut self.doc);
        }
        Tab {
            doc: std::mem::replace(&mut self.doc, Document::new(1, 1)),
            file: self.file.take(),
            ruler: self.ruler,
            notes: std::mem::take(&mut self.notes),
            zoom: self.zoom,
            offset: self.offset,
            selected: std::mem::take(&mut self.selected),
            dirty: self.dirty,
            had_vectors: self.had_vectors,
            view_angle: self.view_angle,
            view_flip: self.view_flip,
            rec: self.rec.take(),
        }
    }

    /// Сделать вкладку активной: её поля — в `PaintApp`.
    fn put_tab(&mut self, t: Tab) {
        self.doc = t.doc;
        self.file = t.file;
        self.ruler = t.ruler;
        self.notes = t.notes;
        self.zoom = t.zoom;
        self.offset = t.offset;
        self.selected = t.selected;
        self.dirty = t.dirty;
        self.had_vectors = t.had_vectors;
        self.view_angle = t.view_angle;
        self.view_flip = t.view_flip;
        self.rec = t.rec;
        self.live = None;
        self.thumbs.clear();
        self.sel_dirty = true;
        self.invalidate_all();
    }

    /// Закончить начатое на холсте (штрих, трансформация, окна с предпросмотром) перед сменой вкладки.
    fn settle(&mut self) {
        if self.transform.is_some() {
            self.finish_transform(true);
        }
        self.finish_filter(false);
        self.finish_gradient(true);
        if let Some(d) = self.outline_dialog.take() {
            let keys = self.doc.set_layer_pixels(&d.path, d.original);
            self.invalidate(keys);
        }
        if let Some(d) = self.grow_dialog.take() {
            self.doc.selection = d.original;
        }
        self.sel_poly = None;
        self.finish_text(true);
        self.layer_props = None;
    }

    /// Перейти на вкладку `i`.
    fn switch_tab(&mut self, i: usize) {
        if i == self.active_tab || i >= self.tabs.len() || self.live.is_some() {
            return;
        }
        self.settle();
        let cur = self.take_tab();
        self.tabs[self.active_tab] = Some(cur);
        let t = self.tabs[i].take().expect("tab slot");
        self.active_tab = i;
        self.put_tab(t);
    }

    fn cur_view(&self) -> FloatView {
        FloatView { zoom: self.zoom, offset: self.offset, angle: self.view_angle, flip: self.view_flip }
    }

    fn set_view(&mut self, v: FloatView) {
        self.zoom = v.zoom;
        self.offset = v.offset;
        self.view_angle = v.angle;
        self.view_flip = v.flip;
    }

    /// Перейти на вкладку-вид `j` (другой холст — через `switch_tab`).
    fn switch_view(&mut self, j: usize) {
        if j == self.active_view || j >= self.views.len() || self.live.is_some() {
            return;
        }
        self.views[self.active_view].view = self.cur_view();
        let v = self.views[j];
        if v.canvas != self.active_tab {
            self.switch_tab(v.canvas);
            if self.active_tab != v.canvas {
                return;
            }
        }
        self.active_view = j;
        self.set_view(v.view);
    }

    /// «Новый вид» (как в SAI): ещё одна вкладка того же холста рядом с текущей.
    fn new_view(&mut self) {
        let v = TabView { canvas: self.active_tab, view: self.cur_view() };
        self.views[self.active_view] = v;
        self.views.insert(self.active_view + 1, v);
        self.active_view += 1;
    }

    /// Закрыть вкладку-вид: если у холста есть другие виды — только её, иначе сам холст.
    fn request_close_view(&mut self, j: usize) {
        let Some(v) = self.views.get(j).copied() else { return };
        if self.live.is_some() {
            return;
        }
        let Some(other) = (0..self.views.len()).find(|&k| k != j && self.views[k].canvas == v.canvas) else {
            self.request_close_tab(v.canvas);
            return;
        };
        if j == self.active_view {
            self.switch_view(other);
        }
        self.views.remove(j);
        if self.active_view > j {
            self.active_view -= 1;
        }
    }

    /// Убрать виды закрытого холста `i` и сдвинуть номера холстов после него.
    fn drop_views_of(&mut self, i: usize) {
        let before = self.views[..self.active_view].iter().filter(|v| v.canvas == i).count();
        let active_gone = self.views[self.active_view].canvas == i;
        self.views.retain(|v| v.canvas != i);
        for v in &mut self.views {
            if v.canvas > i {
                v.canvas -= 1;
            }
        }
        if active_gone {
            self.active_view = self.views.iter().position(|v| v.canvas == self.active_tab).unwrap_or(0);
            self.set_view(self.views[self.active_view].view);
        } else {
            self.active_view -= before;
        }
    }

    /// Перед новым холстом или открытием: если текущий холст уже в деле — новая вкладка
    /// (как в SAI каждый холст — своя вкладка), иначе — на месте пустого.
    fn prepare_tab(&mut self) {
        let pristine = self.file.is_none() && !self.dirty && !self.doc.can_undo();
        if pristine {
            return;
        }
        self.settle();
        self.views[self.active_view].view = self.cur_view();
        let cur = self.take_tab();
        self.tabs[self.active_tab] = Some(cur);
        self.tabs.push(None);
        self.active_tab = self.tabs.len() - 1;
        self.views.push(TabView { canvas: self.active_tab, view: self.cur_view() });
        self.active_view = self.views.len() - 1;
        self.thumbs.clear();
        self.dirty = false;
        self.had_vectors = false;
        self.view_angle = 0.0;
        self.view_flip = false;
        self.ruler = ruler::Ruler::default();
    }

    /// Закрыть вкладку `i` (без вопросов); последняя закрытая — пустой новый холст.
    fn close_tab(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        if i != self.active_tab {
            self.tabs.remove(i);
            if self.active_tab > i {
                self.active_tab -= 1;
            }
            self.drop_views_of(i);
            return;
        }
        self.settle();
        if self.tabs.len() == 1 {
            self.new_canvas_in_place(self.doc.width.max(1), self.doc.height.max(1));
            self.dirty = false;
            self.views.truncate(1);
            self.active_view = 0;
            return;
        }
        self.stop_recording();
        self.tabs.remove(i);
        let next = i.min(self.tabs.len() - 1);
        let t = self.tabs[next].take().expect("tab slot");
        self.active_tab = next;
        self.put_tab(t);
        self.drop_views_of(i);
    }

    /// Закрыть вкладку, спросив, если в ней есть несохранённое.
    fn request_close_tab(&mut self, i: usize) {
        let dirty = if i == self.active_tab { self.dirty } else { self.tabs.get(i).and_then(|t| t.as_ref()).is_some_and(|t| t.dirty) };
        if dirty {
            self.close_ask = Some(i);
        } else {
            self.close_tab(i);
        }
    }

    /// Запомнить файл в «Недавних» (как «Recent» в SAI): сверху, без повторов, до 10.
    fn remember_recent(&mut self, path: &std::path::Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(10);
    }

    fn new_canvas(&mut self, w: u32, h: u32) {
        self.prepare_tab();
        self.new_canvas_in_place(w, h);
    }

    fn new_canvas_in_place(&mut self, w: u32, h: u32) {
        self.stop_recording();
        self.doc = new_document(w, h);
        self.had_vectors = false;
        self.file = None;
        self.notes.clear();
        self.selected = vec![0];
        self.live = None;
        self.fit_pending = true;
        self.ruler.placed = false;
        self.status = trf("Новый холст {0}×{1}", &[&w, &h]);
        self.invalidate_all();
    }

/// Сохранить в `.sai2`: в тот же файл или, если его нет или в нём были кривые, — спросить путь.
    fn save(&mut self) {
        if self.filter.is_some() {
            self.finish_filter(true);
        }
        if self.transform.is_some() {
            self.finish_transform(true);
        }
        match self.file.clone() {
            Some(f) if !self.had_vectors && layered_ext(&f).is_some() => self.save_to(f),
            _ => self.ask_save_as(),
        }
    }

    /// «Сохранить с номером»: следующий свободный номер рядом с текущим файлом.
    fn save_numbered(&mut self) {
        if self.filter.is_some() {
            self.finish_filter(true);
        }
        if self.transform.is_some() {
            self.finish_transform(true);
        }
        match self.file.clone() {
            Some(f) if !self.had_vectors && layered_ext(&f).is_some() => self.save_to(numbered_path(&f)),
            _ => self.ask_save_as(),
        }
    }

    fn ask_save_as(&mut self) {
        let base = self.file.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default().join(tr("НовыйХолст1.sai2")));
        let suggested = if self.had_vectors {
            let stem = base.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            base.with_file_name(format!("{stem}-paint-rs.sai2"))
        } else {
            base.with_extension(layered_ext(&base).unwrap_or("sai2"))
        };
        self.save_path = suggested.display().to_string();
        if self.always_viewer {
            self.open_viewer(file_viewer::Mode::Save);
            return;
        }
        if !self.native_dialog(files::Ask::SaveAs, &suggested) {
            self.show_save_as = true;
        }
    }

    /// «Открыть…»: системное окно, без него — встроенное.
    /// Открыть свой обозреватель файлов.
    fn open_viewer(&mut self, mode: file_viewer::Mode) {
        let dir = self
            .file
            .as_ref()
            .and_then(|f| f.parent().map(|p| p.to_path_buf()))
            .or_else(|| self.viewer_dir.clone())
            .unwrap_or_else(file_viewer::home);
        let name = if mode == file_viewer::Mode::Save {
            self.file.as_ref().and_then(|f| f.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| tr("НовыйХолст1.sai2").to_string())
        } else {
            String::new()
        };
        let mut v = file_viewer::FileViewer::new(mode, dir, name, self.viewer_marks.clone());
        (v.view, v.sort) = self.viewer_view;
        self.viewer = Some(v);
    }

    /// Окно обозревателя и его ответ.
    fn viewer_window(&mut self, ctx: &egui::Context) {
        let Some(v) = self.viewer.as_mut() else { return };
        let out = v.show(ctx).or_else(|| v.overwrite_window(ctx));
        let Some(out) = out else { return };
        let v = self.viewer.take().unwrap();
        self.viewer_marks = v.bookmarks.clone();
        self.viewer_dir = Some(v.dir.clone());
        self.viewer_view = (v.view, v.sort);
        self.save_settings();
        match out {
            file_viewer::Outcome::Open(p) => {
                self.open_path = p.display().to_string();
                self.open(p);
            }
            file_viewer::Outcome::Save(p) => self.save_to(p),
            file_viewer::Outcome::Cancel => {}
        }
    }

    fn ask_open(&mut self) {
        if self.always_viewer {
            self.open_viewer(file_viewer::Mode::Open);
            return;
        }
        let base = self
            .file
            .clone()
            .or_else(|| (!self.open_path.trim().is_empty()).then(|| PathBuf::from(self.open_path.trim())))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default().join("x"));
        if !self.native_dialog(files::Ask::Open, &base) {
            self.show_open = true;
        }
    }

    /// Открыть системное окно, если можно; `false` — нужно встроенное.
    fn native_dialog(&mut self, ask: files::Ask, suggested: &std::path::Path) -> bool {
        if !files::native() {
            return false;
        }
        if self.file_dialog.is_some() {
            return true; // одно окно за раз
        }
        let Some(ctx) = self.ctx.clone() else { return false };
        self.file_dialog = Some(files::ask(ask, suggested, &ctx));
        true
    }

    /// Ответ системного окна, если пришёл.
    fn poll_file_dialog(&mut self) {
        let Some(answer) = self.file_dialog.as_ref().and_then(|d| d.poll()) else { return };
        let ask = self.file_dialog.take().map(|d| d.ask).unwrap_or(files::Ask::Open);
        match (answer, ask) {
            (files::Answer::Path(p), files::Ask::Open) => {
                self.open_path = p.display().to_string();
                self.open(p);
            }
            (files::Answer::Path(p), files::Ask::SaveAs) => {
                let ext = layered_ext(&p).unwrap_or("sai2");
                self.save_to(files::with_ext(p, ext));
            }
            (files::Answer::Path(p), files::Ask::Export(ext)) => self.export_to(files::with_ext(p, ext), ext),
            (files::Answer::Path(p), files::Ask::RecordExport(zip)) => self.run_record_export(files::with_ext(p, if zip { "zip" } else { "png" })),
            (files::Answer::Unavailable, files::Ask::RecordExport(zip)) => {
                let p = self.record_export_default(zip);
                self.run_record_export(p);
            }
            (files::Answer::Path(p), files::Ask::Model3d(add)) => {
                if add {
                    self.s3d_add_model(&p);
                } else {
                    self.s3d_new_layer(&p);
                }
            }
            (files::Answer::Unavailable, files::Ask::Model3d(_)) => self.status = String::from(tr("Окно выбора файла недоступно; перетащите файл модели в окно")),
            (files::Answer::Path(p), files::Ask::Reference) => self.add_reference(&p),
            (files::Answer::Path(p), files::Ask::Tip) => self.add_tip(&p),
            (files::Answer::Path(p), files::Ask::Watermark) => {
                self.watermark.image = Some(p);
                self.watermark_preview = None;
            }
            (files::Answer::Path(p), files::Ask::SaveBrush(i)) => self.save_brush_file(files::with_ext(p, "gsbrush"), Some(i)),
            (files::Answer::Path(p), files::Ask::SaveBrushSet) => self.save_brush_file(files::with_ext(p, "gsbrushes"), None),
            (files::Answer::Unavailable, files::Ask::Open) => self.show_open = true,
            (files::Answer::Unavailable, files::Ask::SaveAs) => self.show_save_as = true,
            (files::Answer::Unavailable, files::Ask::Export(ext)) => {
                let path = self.export_default(ext);
                self.export_to(path, ext);
            }
            (files::Answer::Unavailable, files::Ask::Reference) => self.status = String::from(tr("Перетащите картинку в окно с Ctrl")),
            (files::Answer::Unavailable, files::Ask::SaveBrush(i)) => {
                let p = self.brush_default_path(Some(i));
                self.save_brush_file(p, Some(i));
            }
            (files::Answer::Unavailable, files::Ask::SaveBrushSet) => {
                let p = self.brush_default_path(None);
                self.save_brush_file(p, None);
            }
            (files::Answer::Unavailable, files::Ask::Tip) => self.status = String::from(tr("Перетащите картинку в окно с Shift — она станет отпечатком кисти")),
            (files::Answer::Cancelled, _) | (_, files::Ask::SavePlugin) | (_, files::Ask::OpenWasm) | (_, files::Ask::Watermark) => {}
        }
    }

    fn save_to(&mut self, path: PathBuf) {
        if self.transform.is_some() {
            self.finish_transform(true);
        }
        let started = Instant::now();
        let result: Result<(), Box<dyn std::error::Error>> = match layered_ext(&path) {
            Some("ora") => paint_formats::ora::save(&self.doc, &path),
            Some("psd") => paint_formats::psd::save_with(&self.doc, &path, paint_formats::psd::SaveOptions { big: false, deep: self.opts.psd16, exact: self.opts.psd_exact }),
            Some("psb") => paint_formats::psd::save_with(&self.doc, &path, paint_formats::psd::SaveOptions { big: true, deep: self.opts.psd16, exact: self.opts.psd_exact }),
            _ => paint_formats::sai2_write::save(&self.doc, &path).map_err(Into::into),
        };
        match result {
            Ok(()) => {
                self.status = trf("Сохранено: {0} (за {1} с)", &[&path.display(), &format!("{:.1}", started.elapsed().as_secs_f32())]);
                self.remember_recent(&path);
                self.recording_saved_as(&path);
                self.file = Some(path);
                self.dirty = false;
                self.had_vectors = false;
            }
            Err(e) => self.status = trf("Не удалось сохранить {0}: {1}", &[&path.display(), &e]),
        }
    }

    /// Экспорт: спросить, куда (`jpg` и `png` — сведённая картинка, `psd` — слои).
    fn export_as(&mut self, ext: &'static str) {
        if self.transform.is_some() {
            self.finish_transform(true);
        }
        if self.filter.is_some() {
            self.finish_filter(true);
        }
        let path = self.export_default(ext);
        if !self.native_dialog(files::Ask::Export(ext), &path) {
            self.export_to(path, ext);
        }
    }

    /// Куда экспорт по умолчанию: рядом с файлом или в рабочую папку.
    fn export_default(&self, ext: &str) -> PathBuf {
        match &self.file {
            Some(f) => f.with_extension(format!("export.{ext}")),
            None => std::env::current_dir().unwrap_or_default().join(format!("grazysai-export.{ext}")),
        }
    }

    fn export_to(&mut self, path: PathBuf, ext: &str) {
        let result: Result<(), Box<dyn std::error::Error>> = match ext {
            "psd" => paint_formats::psd::save_with(&self.doc, &path, paint_formats::psd::SaveOptions { big: false, deep: self.opts.psd16, exact: self.opts.psd_exact }),
            "psb" => paint_formats::psd::save_with(&self.doc, &path, paint_formats::psd::SaveOptions { big: true, deep: self.opts.psd16, exact: self.opts.psd_exact }),
            "ora" => paint_formats::ora::save(&self.doc, &path),
            _ => {
                let mut img = self.doc.flatten();
                if self.watermark_on {
                    paint_formats::watermark::apply(&mut img, self.doc.width, self.doc.height, &self.watermark);
                }
                paint_core::io::save_image(&path, self.doc.width, self.doc.height, &img)
            }
        };
        self.status = match result {
            Ok(()) => trf("Сохранено: {0}", &[&path.display()]),
            Err(e) => trf("Не удалось сохранить {0}: {1}", &[&path.display(), &e]),
        };
    }

    fn invalidate_all(&mut self) {
        if std::env::var_os("PAINT_TIMING").is_some() {
            self.timing = Some(Instant::now());
        }
        self.sel_dirty = true;
        self.transform = None;
        if let Some(f) = self.filter.take() {
            self.filter_last = Some(FilterDialog { original: Default::default(), sel: Default::default(), ..f });
        }
        self.move_drag = None;
        self.thumbs.clear();
        self.display.clear();
        self.queue.clear();
        self.queued.clear();
        let (tw, th) = (
            (self.doc.width as i32 + TILE - 1) / TILE,
            (self.doc.height as i32 + TILE - 1) / TILE,
        );
        for ty in 0..th {
            for tx in 0..tw {
                self.queue.push_back((tx, ty));
                self.queued.insert((tx, ty));
            }
        }
    }

    /// Пересчитать весь холст после смены свойств слоя (непрозрачность, режим, видимость…):
    /// прежняя картинка остаётся на экране, пока плитки не пересчитаны, — без мигания.
    fn refresh_all(&mut self) {
        let (tw, th) = (
            (self.doc.width as i32 + TILE - 1) / TILE,
            (self.doc.height as i32 + TILE - 1) / TILE,
        );
        let keys: Vec<Key> = (0..th).flat_map(|ty| (0..tw).map(move |tx| (tx, ty))).collect();
        self.invalidate(keys);
    }

    fn invalidate(&mut self, keys: impl IntoIterator<Item = Key>) {
        for key in keys {
            if self.queued.insert(key) {
                self.queue.push_front(key); // свежие правки показываем первыми
            }
        }
    }

    /// Пересчитывает часть ожидающих плиток, не дольше `budget`.
    fn render_pending(&mut self, budget: Duration) {
        let started = Instant::now();
        let (w, h) = (self.doc.width as i32, self.doc.height as i32);
        let threads = paint_core::threads();
        loop {
            // пачка плиток — по нескольку на поток; свежие правки стоят в начале очереди
            let mut batch = Vec::new();
            while batch.len() < threads * 4 {
                let Some(key) = self.queue.pop_front() else { break };
                self.queued.remove(&key);
                if key.0 < 0 || key.1 < 0 || key.0 * TILE >= w || key.1 * TILE >= h {
                    continue;
                }
                batch.push(key);
            }
            if batch.is_empty() {
                if let Some(t) = self.timing.take() {
                    eprintln!("timing: холст {}×{} показан целиком за {:.2} с", w, h, t.elapsed().as_secs_f32());
                }
                break;
            }
            let doc = &self.doc;
            let chunk = batch.len().div_ceil(threads);
            let rendered: Vec<(Key, Vec<Color32>)> = std::thread::scope(|scope| {
                let handles: Vec<_> = batch
                    .chunks(chunk)
                    .map(|keys| {
                        scope.spawn(move || {
                            keys.iter()
                                .map(|k| (*k, doc.render_tile(*k).as_chunks::<4>().0.iter().map(|p| Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3])).collect::<Vec<_>>()))
                                .collect::<Vec<_>>()
                        })
                    })
                    .collect();
                handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
            });
            for (key, rgba) in rendered {
                let dkey = (key.0 * TILE / DT, key.1 * TILE / DT);
                let tile = self.display.entry(dkey).or_insert_with(|| DisplayTile {
                    pixels: vec![Color32::TRANSPARENT; (DT * DT) as usize],
                    texture: None,
                    dirty: 0,
                    options: None,
                });
                let (ox, oy) = (key.0 * TILE - dkey.0 * DT, key.1 * TILE - dkey.1 * DT);
                for y in 0..TILE {
                    let dst = ((oy + y) * DT + ox) as usize;
                    let src = (y * TILE) as usize;
                    tile.pixels[dst..dst + TILE as usize].copy_from_slice(&rgba[src..src + TILE as usize]);
                }
                tile.dirty |= 1 << ((oy / TILE) * SUB + ox / TILE);
                self.nav_dirty.insert(key);
            }
            if started.elapsed() > budget {
                break;
            }
        }
    }

    fn can_paint(&self) -> bool {
        self.doc
            .layer(&self.selected)
            .is_some_and(|l| l.children.is_none() && l.visible && l.paintable())
    }

    /// Текущий нажим и признак «ластик на обратном конце пера».
    fn pressure(&mut self) -> (f32, bool) {
        let fresh = self.pen.lock().ok().and_then(|s| {
            s.at.filter(|t| t.elapsed() < Duration::from_millis(250))
                .map(|_| (s.pressure, s.eraser))
        });
        let m = self.opts.min_pressure;
        let gate = |p: f32| ((p - m) / (1.0 - m)).max(0.0);
        let (raw, pressure, eraser, source) = match (fresh, self.touch_force) {
            (Some((p, e)), _) => (p, gate(self.opts.pressure_curve.apply(p)), e, tr("перо (XInput 2)")),
            (None, Some(f)) if self.touch_down || (self.stroke_by_touch && (self.live.is_some() || self.line_stroke.is_some() || self.sel_stroke.is_some() || self.mask_stroke.is_some())) => (f, gate(self.opts.pressure_curve.apply(f)), false, tr("перо (Windows Ink)")),
            _ => (1.0, 1.0, false, tr("мышь")),
        };
        // нажим слабее порога — перо будто не касается (мышь не в счёт)
        self.below_min = m > 0.0 && self.opts.pressure_curve.apply(raw) < m;
        self.last_pressure = pressure;
        self.last_source = source;
        let e = self.pen_stats.entry(source).or_insert((f32::MAX, f32::MIN, 0));
        *e = (e.0.min(pressure), e.1.max(pressure), e.2 + 1);
        (pressure, eraser)
    }

    /// Точка штриха на линейке, если линейка показана и не отключена.
    /// `first` — начало штриха (перспектива выбирает направление по первому движению).
    fn snap_to_ruler(&mut self, p: InputPoint, first: bool) -> InputPoint {
        if !self.ruler.active() {
            return p;
        }
        let [x, y] = if matches!(self.ruler.kind, ruler::Kind::Perspective(_)) {
            self.ruler.snap_perspective([p.x, p.y], first, self.zoom)
        } else if self.ruler.starts_from_stroke() {
            self.ruler.snap_from([p.x, p.y], first)
        } else {
            self.ruler.snap([p.x, p.y])
        };
        InputPoint { x, y, ..p }
    }

    /// Показать или спрятать линейку; при первом показе на холсте — поставить посреди вида.
    fn toggle_ruler(&mut self) {
        self.ruler.visible = !self.ruler.visible;
        if self.ruler.visible && !self.ruler.placed {
            self.reset_ruler();
        }
    }

    /// Середина вида и его меньшая сторона в точках холста.
    fn view_frame(&self) -> ([f32; 2], f32) {
        let xf = ViewXf::new(self.view, self.zoom, self.offset, self.view_angle, self.view_flip);
        let c = xf.to_canvas(self.view.center());
        ([c.x, c.y], self.view.width().min(self.view.height()).max(100.0) / self.zoom)
    }

    /// Показать линейку перспективы с `n` точками схода (точки — посреди вида, если их число
    /// сменилось или линейка ещё не ставилась).
    fn show_perspective(&mut self, n: u8) {
        let kind = ruler::Kind::Perspective(n);
        let changed = self.ruler.kind != kind;
        self.ruler.kind = kind;
        if !self.ruler.placed {
            self.reset_ruler();
        } else if changed {
            let (c, min) = self.view_frame();
            self.ruler.place_vps(c, min);
        }
        self.ruler.visible = true;
    }

    fn reset_ruler(&mut self) {
        let (c, min) = self.view_frame();
        self.ruler.reset(c, min);
    }

    fn begin_stroke(&mut self, p: InputPoint, eraser_end: bool) {
        // точки прошлого штриха (если его сбросили без `end_stroke`) — не в этот
        self.pending_points.clear();
        self.stroke_by_touch = self.touch_down;
        let shap = self.doc.layer(&self.selected).is_some_and(|l| {
            l.visible && !l.lock_paint && !l.lock_all && l.vector.as_ref().is_some_and(|v| &v.kind == b"shap")
        });
        if shap && self.filter.is_none() {
            // слой фигур: протяжка задаёт рамку фигуры
            self.shape_drag = Some((Vec2::new(p.x, p.y), Vec2::new(p.x, p.y)));
            return;
        }
        let liwk = self.doc.layer(&self.selected).is_some_and(|l| {
            l.visible && !l.lock_paint && !l.lock_all && l.vector.as_ref().is_some_and(|v| &v.kind == b"liwk")
        });
        let tool = if eraser_end { LineTool::Eraser } else { self.line_tool };
        if liwk && tool != LineTool::Pen && self.filter.is_none() {
            let layer = self.doc.layer(&self.selected).unwrap();
            let raw = &layer.vector.as_ref().unwrap().raw;
            match paint_formats::sai2_vector::parse(raw, true) {
                Ok(shapes) => {
                    let reach = 6.0 / self.zoom as f64;
                    let node = matches!(tool, LineTool::Edit | LineTool::Pressure)
                        .then(|| paint_formats::linework::hit_node(&shapes, p.x as f64, p.y as f64, reach))
                        .flatten();
                    let start_weight = node
                        .and_then(|(si, pi, ni)| shapes.get(si)?.paths.get(pi)?.points.get(ni).map(|c| c.weight))
                        .unwrap_or(1.0);
                    let raw_now = raw_hash(raw);
                    let group = match (&self.cp_sel, node) {
                        (Some(sel), Some(n)) if sel.path == self.selected && sel.raw == raw_now && sel.nodes.contains(&n) && tool == LineTool::Edit => {
                            sel.nodes.iter().filter_map(|&k| Some((k, node_pos(&shapes, k)?))).collect()
                        }
                        _ => Vec::new(),
                    };
                    self.line_edit = Some(LineEdit {
                        path: self.selected.clone(),
                        shapes,
                        orig_pixels: layer.pixels.clone(),
                        node,
                        start_y: p.y,
                        start_weight,
                        changed: false,
                        start: [p.x as f64, p.y as f64],
                        moved: false,
                        group,
                        stale: false,
                    });
                    self.line_apply(tool, p);
                }
                Err(e) => self.status = trf("Кривые слоя не прочитаны: {0}", &[&e]),
            }
            return;
        }
        let p = self.snap_to_ruler(p, true);
        if liwk && !eraser_end && !self.sel_tool() && self.filter.is_none() {
            // перо слоя контура: точки копятся, по отрыву становятся кривой
            self.smoother = Stabilizer::with_level(self.stabilizer);
            let q = self.smoother.push(p);
            self.line_stroke = Some(vec![paint_core::vectorize::StrokePoint { at: [q.x as f64, q.y as f64], pressure: q.pressure as f64 }]);
            return;
        }
        if self.filter.is_some() {
            self.status = String::from(tr("Сначала закройте окно фильтра (OK или «Отмена»)"));
            return;
        }
        if self.mask_edit && !self.sel_tool() {
            // кисть рисует в маске: тёмный цвет скрывает слой, светлый и ластик — показывают
            let Some(orig) = self.doc.layer(&self.selected).and_then(|l| l.mask.clone()) else {
                self.mask_edit = false;
                self.status = String::from(tr("У слоя нет маски"));
                return;
            };
            let mut params = if eraser_end { self.brushes[self.eraser_index()].params } else { self.brushes[self.brush].params };
            params.mix = None;
            let erase = eraser_end || matches!(self.kind(), ToolKind::Eraser | ToolKind::BinEraser);
            let c = self.color();
            let lum = (c.r() as u32 * 77 + c.g() as u32 * 150 + c.b() as u32 * 29) >> 8;
            let target = if erase { 0 } else { ((255 - lum.min(255)) * 0x4000 / 255) as u16 };
            self.smoother = Stabilizer::with_level(self.stabilizer);
            self.stroke_by_touch = self.touch_down;
            let mut st = Stroke::new(params);
            st.mirror = self.mirror();
            st.set_grain(self.grain_for(if eraser_end { self.eraser_index() } else { self.brush }));
            let tool_index = if eraser_end { self.eraser_index() } else { self.brush };
            self.attach_dynamics(&mut st, tool_index);
            st.add_point(self.smoother.push(p));
            let keys = self.doc.mask_stroke_preview(&self.selected.clone(), &orig.hide, &st.mask, target);
            self.invalidate(keys);
            self.mask_stroke = Some((st, orig, target));
            return;
        }
        if !eraser_end && self.sel_tool() {
            // перо и ластик выделения рисуют не в слой, а в выделение
            self.smoother = Stabilizer::with_level(self.stabilizer);
            let mut s = Stroke::new(self.brushes[self.brush].params);
            s.mirror = self.mirror();
            let q = self.smoother.push(p);
            s.add_point(q);
            self.sel_stroke = Some((s, vec![Vec2::new(q.x, q.y)]));
            return;
        }
        if !self.can_paint() {
            let kind = self.doc.layer(&self.selected).and_then(|l| l.vector.as_ref().map(|v| v.kind));
            self.status = if self.doc.layer(&self.selected).is_some_and(|l| l.scene3d.is_some()) {
                String::from(tr("Это слой 3D: рисуйте на слое выше (в окне «3D» мышь — «рисование»); «Слой → Растеризовать слой» — сделать обычным"))
            } else if kind == Some(*b"text") {
                String::from(tr("Это текстовый слой: кистью он не правится (как в SAI). «Слой → Растеризовать слой» — сделать обычным"))
            } else if kind == Some(*b"pers") || kind == Some(*b"grid") {
                String::from(tr("Это линейка перспективы из более новой SAI: она сохранится в файле как была"))
            } else if kind.is_some() {
                String::from(tr("Это слой контура или фигур: кистью он не правится (как в SAI). «Слой → Растеризовать слой» — сделать обычным"))
            } else {
                String::from(tr("Рисовать можно на видимом обычном слое без запрета рисования — выберите его в списке справа"))
            };
            return;
        }
        let mut params = if eraser_end {
            self.brushes[self.eraser_index()].params
        } else {
            self.brushes[self.brush].params
        };
        // «прозрачный цвет», как в SAI: кисть стирает своей формой
        if self.transparent && !eraser_end {
            params.erase = true;
            params.mix = None;
        }
        self.stroke_by_touch = self.touch_down;
        self.smoother = Stabilizer::with_level(self.stabilizer);
        let c = self.color();
        if !params.erase {
            self.push_recent(c);
        }
        let mut stroke = Stroke::new(params);
        stroke.mirror = self.mirror();
        stroke.set_grain(self.grain_for(if eraser_end { self.eraser_index() } else { self.brush }));
        let tool_index = if eraser_end { self.eraser_index() } else { self.brush };
        self.attach_dynamics(&mut stroke, tool_index);
        // прямая, как в SAI: кнопка «Прямая линия» — весь штрих прямой от точки нажатия;
        // Shift + нажатие — прямая от конца прошлого штриха
        let shift = self.ctx.as_ref().is_some_and(|c| c.input(|i| i.modifiers.shift));
        self.line_from = match self.last_end {
            Some(e) if shift => Some((e, eraser_end)),
            _ if self.straight => Some((p, eraser_end)),
            _ => None,
        };
        self.last_point = Some(p);
        let rgb = self.stroke_color(&mut stroke);
        if self.line_from.is_some() {
            self.live = Some(LiveStroke::new(self.selected.clone(), stroke, rgb));
            self.straight_to(p);
            return;
        }
        let mut live = LiveStroke::new(self.selected.clone(), stroke, rgb);
        let q = self.smoother.push(p);
        self.shift_lock = None;
        let anti = self.brushes[tool_index].anti_hook;
        if anti > 0.0 {
            self.hook = Some(HookBuf { range: anti / self.zoom, start_done: false, buf: vec![q], done: Vec::new() });
        } else {
            self.hook = None;
            live.stroke.add_point(q);
        }
        let keys = self.doc.live_update(&mut live);
        self.invalidate(keys);
        self.live = Some(live);
    }

    /// Прямой штрих: убрать прежний вид и провести заново от начала до `p` (нажим — плавно от
    /// нажима в начале к текущему).
    fn straight_to(&mut self, p: InputPoint) {
        let (Some((a, eraser)), Some(old)) = (self.line_from, self.live.take()) else { return };
        let keys = self.doc.live_cancel(old);
        self.invalidate(keys);
        let mut params = self.brushes[if eraser { self.eraser_index() } else { self.brush }].params;
        if self.transparent && !eraser {
            params.erase = true;
            params.mix = None;
        }
        let mut stroke = Stroke::new(params);
        stroke.set_grain(self.grain_for(if eraser { self.eraser_index() } else { self.brush }));
        let tool_index = if eraser { self.eraser_index() } else { self.brush };
        self.attach_dynamics(&mut stroke, tool_index);
        let rgb = self.stroke_color(&mut stroke);
        let mut live = LiveStroke::new(self.selected.clone(), stroke, rgb);
        live.stroke.add_point(a);
        if (p.x - a.x).hypot(p.y - a.y) > 0.01 {
            live.stroke.add_point(p);
        }
        let keys = self.doc.live_update(&mut live);
        self.invalidate(keys);
        self.live = Some(live);
    }

    fn continue_stroke(&mut self, p: InputPoint) {
        if let Some((_, b)) = self.shape_drag.as_mut() {
            *b = Vec2::new(p.x, p.y);
            return;
        }
        if self.line_edit.is_some() {
            let tool = if self.pen.lock().is_ok_and(|s| s.eraser) { LineTool::Eraser } else { self.line_tool };
            self.line_apply(tool, p);
            return;
        }
        let p = self.snap_to_ruler(p, false);
        if let Some(pts) = self.line_stroke.as_mut() {
            let q = self.smoother.push(p);
            pts.push(paint_core::vectorize::StrokePoint { at: [q.x as f64, q.y as f64], pressure: q.pressure as f64 });
            return;
        }
        if let Some((s, path)) = self.sel_stroke.as_mut() {
            let q = self.smoother.push(p);
            s.add_point(q);
            path.push(Vec2::new(q.x, q.y));
            return;
        }
        if let Some((st, orig, target)) = self.mask_stroke.as_mut() {
            st.add_point(self.smoother.push(p));
            let path = self.selected.clone();
            let keys = self.doc.mask_stroke_preview(&path, &orig.hide, &st.mask, *target);
            self.invalidate(keys);
            return;
        }
        if self.live.is_some() {
            self.last_point = Some(p);
        }
        if self.line_from.is_some() {
            self.straight_to(p);
            return;
        }
        if self.live.is_none() {
            return;
        }
        // Shift после начала мазка (SAI 2026): дальше — прямо по горизонтали или вертикали
        let shift = self.ctx.as_ref().is_some_and(|c| c.input(|i| i.modifiers.shift));
        let p = if shift {
            let (a, axis) = self.shift_lock.get_or_insert((p, None));
            let (dx, dy) = (p.x - a.x, p.y - a.y);
            if axis.is_none() && dx.hypot(dy) > 4.0 / self.zoom {
                *axis = Some(dx.abs() >= dy.abs());
            }
            match *axis {
                Some(true) => InputPoint { y: a.y, ..p },
                Some(false) => InputPoint { x: a.x, ..p },
                None => *a,
            }
        } else {
            self.shift_lock = None;
            p
        };
        // точка — в очередь: рисуется в `drain_points` столько, сколько успевается за кадр
        let q = self.smoother.push(p);
        self.hook_feed(q);
    }

    /// Точка мазка — в очередь рисования, а при «Срезать крючки» — сначала в придержку.
    fn hook_feed(&mut self, q: InputPoint) {
        let Some(h) = self.hook.as_mut() else {
            self.pending_points.push_back(q);
            return;
        };
        h.buf.push(q);
        if !h.start_done {
            if path_len(&h.buf) < 3.0 * h.range {
                return;
            }
            // крючок в начале — тот же поиск с другого конца
            let rev: Vec<[f32; 2]> = h.buf.iter().rev().map(|p| [p.x, p.y]).collect();
            let k = paint_brush::hook_len(&rev, h.range);
            h.buf.drain(..k);
            h.start_done = true;
        }
        while h.buf.len() > 2 && path_len(&h.buf[1..]) > h.range {
            let q = h.buf.remove(0);
            h.done.push(q);
            self.pending_points.push_back(q);
        }
        while h.done.len() > 2 && path_len(&h.done[1..]) > 2.5 * h.range {
            h.done.remove(0);
        }
    }

    /// Перо поднято: срезать крючки на концах и отдать придержанные точки.
    fn hook_flush(&mut self) {
        let Some(mut h) = self.hook.take() else { return };
        h.buf.extend(self.smoother.finish());
        let pts: Vec<[f32; 2]> = h.buf.iter().map(|p| [p.x, p.y]).collect();
        if !h.start_done {
            let rev: Vec<[f32; 2]> = pts.iter().rev().copied().collect();
            let k = paint_brush::hook_len(&rev, h.range).min(h.buf.len().saturating_sub(1));
            h.buf.drain(..k);
        }
        // крючок в конце: ищется по придержанным точкам вместе с уже отданными перед ними
        let pts: Vec<[f32; 2]> = h.done.iter().chain(h.buf.iter()).map(|p| [p.x, p.y]).collect();
        let k = paint_brush::hook_len(&pts, h.range).min(h.buf.len().saturating_sub(1));
        h.buf.truncate(h.buf.len() - k);
        self.pending_points.extend(h.buf);
    }

    /// Нарисовать точки из очереди штриха; `budget` — не дольше (окно не замирает на огромных
    /// кистях: штрих догоняет перо за несколько кадров, как в SAI), `None` — все.
    fn drain_points(&mut self, budget: Option<Duration>) {
        if self.pending_points.is_empty() {
            return;
        }
        let Some(mut live) = self.live.take() else {
            self.pending_points.clear();
            return;
        };
        let started = Instant::now();
        // смешивающей кисти — после каждой точки, как прежде (маска жёсткости зависит от того,
        // сколько отпечатков уже известно); обычной — пачкой, плитки накладываются один раз
        let per = if live.stroke.is_mixing() { 1 } else { 8 };
        let mut keys = Vec::new();
        while !self.pending_points.is_empty() {
            let t0 = Instant::now();
            let mut n = 0;
            while n < per {
                let Some(p) = self.pending_points.pop_front() else { break };
                live.stroke.add_point(p);
                n += 1;
            }
            let t = Instant::now();
            keys.extend(self.doc.live_update(&mut live));
            if let Some(s) = &self.frame_stats {
                let mut s = s.borrow_mut();
                s.add += (t - t0).as_secs_f32() * 1000.0;
                s.cur_paint += t.elapsed().as_secs_f32() * 1000.0;
                s.moves += n;
            }
            if budget.is_some_and(|b| started.elapsed() > b) {
                break;
            }
        }
        self.invalidate(keys);
        self.live = Some(live);
        if !self.pending_points.is_empty() {
            if let Some(ctx) = &self.ctx {
                ctx.request_repaint();
            }
        }
    }

    /// `PAINT_FRAMES`: итог штриха в stderr — кадры, среднее, 95 % и худший, доля счёта штриха.
    fn report_frames(&mut self) {
        let Some(s) = &self.frame_stats else { return };
        let mut s = s.borrow_mut();
        if s.frames.is_empty() {
            return;
        }
        let mut f = s.frames.clone();
        f.sort_by(|a, b| a.total_cmp(b));
        let n = f.len();
        let avg = f.iter().sum::<f32>() / n as f32;
        let paint = s.paint.iter().sum::<f32>() / n as f32;
        let slow = f.iter().filter(|v| **v > 33.0).count();
        eprintln!(
            "FRAMES n={n} moves={} avg={avg:.1} p95={:.1} max={:.1} >33ms={slow} paint_avg={paint:.1} paint_max={:.1} render_avg={:.1} upload_avg={:.1} add_avg={:.1}",
            s.moves,
            f[(n * 95 / 100).min(n - 1)],
            f[n - 1],
            s.paint.iter().cloned().fold(0.0, f32::max),
            s.render / n as f32,
            s.upload / n as f32,
            s.add / n as f32
        );
        *s = FrameStats::default();
    }

    fn end_stroke(&mut self) {
        self.pending_start = None;
        self.shift_lock = None;
        if self.live.is_some() {
            self.hook_flush();
        }
        // перо поднято — остаток очереди дорисовать
        self.drain_points(None);
        self.report_frames();
        if let Some((a, b)) = self.shape_drag.take() {
            self.commit_shape(a, b);
            return;
        }
        if let Some(e) = self.line_edit.take() {
            // показ шёл без истории — вернуть исходные пиксели и записать правку одной записью
            let keys = self.doc.set_layer_pixels(&e.path, e.orig_pixels);
            self.invalidate(keys);
            if let (Some(n), false, LineTool::Edit) = (e.node, e.moved, self.line_tool) {
                // щелчок по узлу «Правкой» — выделить его или снять (как режим Select в SAI)
                let raw = self.doc.layer(&e.path).and_then(|l| l.vector.as_ref()).map(|v| raw_hash(&v.raw)).unwrap_or(0);
                let sel = match self.cp_sel.take() {
                    Some(s) if s.path == e.path && s.raw == raw => s,
                    _ => CpSelection { path: e.path.clone(), raw, nodes: Default::default() },
                };
                let mut sel = sel;
                if !sel.nodes.remove(&n) {
                    sel.nodes.insert(n);
                }
                self.cp_sel = Some(sel);
            }
            if e.changed {
                let raw = paint_formats::sai2_vector::write(&e.shapes, true);
                // узлы только сдвинулись — их выделение остаётся в силе
                if let Some(sel) = self.cp_sel.as_mut().filter(|s| s.path == e.path && matches!(self.line_tool, LineTool::Edit | LineTool::Pressure)) {
                    sel.raw = raw_hash(&raw);
                }
                let pixels = paint_formats::linework::render_shapes(&e.shapes);
                let keys = self.doc.set_vector_layer(&e.path, paint_core::layer::VectorData { kind: *b"liwk", raw }, pixels);
                self.invalidate(keys);
                self.thumbs.clear();
                self.dirty = true;
            }
            return;
        }
        if let Some(mut pts) = self.line_stroke.take() {
            for q in self.smoother.finish() {
                pts.push(paint_core::vectorize::StrokePoint { at: [q.x as f64, q.y as f64], pressure: q.pressure as f64 });
            }
            self.commit_line(&pts);
            return;
        }
        if let Some((mut st, orig, target)) = self.mask_stroke.take() {
            for q in self.smoother.finish() {
                st.add_point(q);
            }
            let path = self.selected.clone();
            let keys = self.doc.mask_stroke_preview(&path, &orig.hide, &st.mask, target);
            self.invalidate(keys);
            self.doc.mask_stroke_commit(&path, orig);
            self.thumbs.clear();
            self.dirty = true;
            return;
        }
        if let Some((mut s, _)) = self.sel_stroke.take() {
            for q in self.smoother.finish() {
                s.add_point(q);
            }
            let mode = if self.sel_eraser() { Combine::Subtract } else { Combine::Add };
            self.apply_selection(&s.mask, mode);
            return;
        }
        let Some(mut live) = self.live.take() else {
            return;
        };
        self.thumbs.clear();
        self.dirty = true;
        // прямой штрих сглаживателю не отдавался
        if self.line_from.take().is_none() {
            for p in self.smoother.finish() {
                live.stroke.add_point(p);
            }
        }
        self.last_end = self.last_point;
        let keys = self.doc.live_finish(live);
        self.invalidate(keys);
    }

    /// Инструмент слоя контура в точке `p` (ластик, толщина, цвет — по пути под точкой; узлы и
    /// нажим — по захваченному узлу). Показ — без истории.
    fn line_apply(&mut self, tool: LineTool, p: InputPoint) {
        use paint_formats::linework as lw;
        let reach = 4.0 / self.zoom as f64;
        let size = self.brushes[self.brush].params.size;
        let c = self.color();
        let Some(e) = self.line_edit.as_mut() else { return };
        let (x, y) = (p.x as f64, p.y as f64);
        let mut now = false;
        match tool {
            LineTool::Eraser => {
                while let Some((si, pi)) = lw::hit(&e.shapes, x, y, reach) {
                    lw::delete_path(&mut e.shapes, si, pi);
                    now = true;
                }
            }
            LineTool::Weight => {
                if let Some((si, _)) = lw::hit(&e.shapes, x, y, reach) {
                    lw::set_size(&mut e.shapes, si, size);
                    now = true;
                }
            }
            LineTool::Color => {
                if let Some((si, _)) = lw::hit(&e.shapes, x, y, reach) {
                    lw::set_color(&mut e.shapes, si, [c.r(), c.g(), c.b()]);
                    now = true;
                }
            }
            LineTool::Edit => {
                let (dx, dy) = (x - e.start[0], y - e.start[1]);
                // дрожь пера при щелчке — не сдвиг (щелчок выделяет узел)
                e.moved |= dx.hypot(dy) > 2.0 / self.zoom as f64;
                if let (Some(n), true) = (e.node, e.moved) {
                    if e.group.is_empty() {
                        lw::move_node(&mut e.shapes, n, x, y);
                    } else {
                        for &(k, at) in &e.group {
                            lw::move_node(&mut e.shapes, k, at[0] + dx, at[1] + dy);
                        }
                    }
                    now = true;
                }
            }
            LineTool::Pressure => {
                if let Some(n) = e.node {
                    // вверх — сильнее, вниз — слабее; 100 px — весь размах
                    let w = e.start_weight - (p.y - e.start_y) / 100.0;
                    lw::set_node_pressure(&mut e.shapes, n, w);
                    now = true;
                }
            }
            LineTool::Pen => {}
        }
        e.changed |= now;
        // слой перерисуется раз за кадр (`render_line_edit`), а не на каждое движение пера
        e.stale |= now;
    }

    /// Показ правки контура: слой — по рабочим кривым, если они менялись с прошлого кадра.
    fn render_line_edit(&mut self) {
        let Some(e) = self.line_edit.as_mut().filter(|e| e.stale) else { return };
        e.stale = false;
        let pixels = paint_formats::linework::render_shapes(&e.shapes);
        let path = e.path.clone();
        let keys = self.doc.set_layer_pixels(&path, pixels);
        self.invalidate(keys);
    }

    /// Фигура в рамке a–b на слое фигур (одна запись истории).
    fn commit_shape(&mut self, a: Vec2, b: Vec2) {
        if (a.x - b.x).abs() < 1.0 || (a.y - b.y).abs() < 1.0 {
            return;
        }
        let path = self.selected.clone();
        let Some(layer) = self.doc.layer(&path) else { return };
        let Some(v) = &layer.vector else { return };
        let c = self.color();
        let (x0, x1) = (a.x.min(b.x) as f64, a.x.max(b.x) as f64);
        let (y0, y1) = (a.y.min(b.y) as f64, a.y.max(b.y) as f64);
        let raw = match paint_formats::linework::append_shape(&v.raw, self.shape_kind, x0, y0, x1, y1, [c.r(), c.g(), c.b()]) {
            Ok(r) => r,
            Err(e) => {
                self.status = trf("Не удалось добавить фигуру: {0}", &[&e]);
                return;
            }
        };
        match paint_formats::sai2_vector::parse(&raw, false) {
            Ok(shapes) => {
                let pixels = paint_formats::linework::render_layer_shapes(&shapes, false);
                let keys = self.doc.set_vector_layer(&path, paint_core::layer::VectorData { kind: *b"shap", raw }, pixels);
                self.invalidate(keys);
                self.thumbs.clear();
                self.dirty = true;
            }
            Err(e) => self.status = trf("Не удалось добавить фигуру: {0}", &[&e]),
        }
    }

    /// Раскраска по контуру (`paint_core::colorize`): подсказки — цветные пятна текущего слоя,
    /// контур — сведение остальных видимых слоёв. Результат — новый слой «Умножение» под текущим
    /// (одна запись истории); слой подсказок не меняется.
    fn colorize_by_lines(&mut self) {
        let Some(hints) = self.doc.layer(&self.selected).filter(|l| l.children.is_none()) else { return };
        let (w, h) = (self.doc.width as usize, self.doc.height as usize);
        let mut hint_rgba = vec![0u8; w * h * 4];
        if let Some((x0, y0, x1, y1)) = paint_core::selection::content_bounds(&hints.pixels) {
            for y in y0.max(0)..y1.min(h as i32) {
                for x in x0.max(0)..x1.min(w as i32) {
                    let i = (y as usize * w + x as usize) * 4;
                    hint_rgba[i..i + 4].copy_from_slice(&paint_core::pixel::to_rgba8(hints.pixels.get(x, y)));
                }
            }
        }
        let mut probe = paint_core::Document::new(self.doc.width, self.doc.height);
        probe.effect_color = self.doc.effect_color;
        probe.layers = self.doc.layers.clone();
        if let Some(l) = probe.layer_mut(&self.selected) {
            l.visible = false;
        }
        let Some((rgba, n)) = paint_core::colorize::colorize(&probe.flatten_clear(), &hint_rgba, w, h, 3) else {
            self.status = tr("Нет подсказок: поставьте на текущем слое цветные пятна в областях, которые надо залить").to_string();
            return;
        };
        let mut layer = Layer::new(tr("Раскраска"));
        // «Умножение»: линии контура видны сквозь заливку, где бы ни лежал слой
        layer.mode = paint_core::blend::BlendMode::Multiply;
        for (i, c) in rgba.as_chunks::<4>().0.iter().enumerate() {
            if c[3] != 0 {
                layer.pixels.set((i % w) as i32, (i / w) as i32, paint_core::pixel::from_rgba8(*c));
            }
        }
        let path = self.selected.clone();
        let keys = self.doc.insert_layer(&path, layer);
        self.selected = path;
        self.after_structure(keys);
        self.status = trf("Раскраска: {0} цветов, новый слой под подсказками", &[&n]);
    }

    /// Новый слой фигур над текущим.
    fn add_shape_layer(&mut self) {
        let mut path = self.selected.clone();
        let index = path.pop().map_or(0, |i| i + 1);
        let n = count_layers(&self.doc.layers) + 1;
        let mut layer = Layer::new(&trf("Фигуры{0}", &[&n]));
        layer.vector = Some(paint_formats::linework::empty_shapes());
        path.push(index);
        let keys = self.doc.insert_layer(&path, layer);
        self.selected = path;
        self.after_structure(keys);
    }

    /// Штрих пера на слое контура → кривая, добавленная к кривым слоя (одна запись истории).
    fn commit_line(&mut self, pts: &[paint_core::vectorize::StrokePoint]) {
        let Some((nodes, closed)) = paint_core::vectorize::vectorize(pts, 1.0) else {
            self.status = String::from(tr("Штрих слишком короткий для кривой"));
            return;
        };
        let path = self.selected.clone();
        let Some(layer) = self.doc.layer(&path) else { return };
        let Some(v) = &layer.vector else { return };
        let b = &self.brushes[self.brush].params;
        let c = self.color();
        let step = paint_core::brush::HARDNESS_STEPS.iter().position(|h| (h - b.hardness).abs() < 0.01).unwrap_or(4) as u32;
        let st = paint_formats::linework::LineStyle { rgb: [c.r(), c.g(), c.b()], size: b.size, density: b.density, hardness: step };
        let raw = match paint_formats::linework::append(&v.raw, &nodes, closed, &st) {
            Ok(r) => r,
            Err(e) => {
                self.status = trf("Не удалось добавить кривую: {0}", &[&e]);
                return;
            }
        };
        let mut probe = layer.clone();
        probe.vector = Some(paint_core::layer::VectorData { kind: *b"liwk", raw: raw.clone() });
        match paint_formats::linework::render(&probe) {
            Ok(pixels) => {
                let keys = self.doc.set_vector_layer(&path, paint_core::layer::VectorData { kind: *b"liwk", raw }, pixels);
                self.invalidate(keys);
                self.thumbs.clear();
                self.dirty = true;
                self.status = trf("Кривая: узлов {0}", &[&nodes.len()]);
            }
            Err(e) => self.status = trf("Не удалось нарисовать кривую: {0}", &[&e]),
        }
    }

    fn undo(&mut self) {
        self.s3d_finish();
        if self.gradient.is_some() {
            self.finish_gradient(false);
            return;
        }
        if self.filter.is_some() {
            self.finish_filter(false);
            return;
        }
        if self.transform.is_some() {
            self.finish_transform(false);
            return;
        }
        let size = (self.doc.width, self.doc.height);
        if let Some(keys) = self.doc.undo() {
            self.after_structure(keys);
        }
        if size != (self.doc.width, self.doc.height) {
            self.fit_pending = true;
            self.invalidate_all();
        }
        self.sel_dirty = true;
    }

    fn redo(&mut self) {
        self.s3d_finish();
        if self.transform.is_some() {
            return;
        }
        let size = (self.doc.width, self.doc.height);
        if let Some(keys) = self.doc.redo() {
            self.after_structure(keys);
        }
        if size != (self.doc.width, self.doc.height) {
            self.fit_pending = true;
            self.invalidate_all();
        }
        self.sel_dirty = true;
    }

    /// Соединить новую область с выделением и обновить показ.
    fn apply_selection(&mut self, area: &paint_core::tile::MaskMap, mode: Combine) {
        self.doc.select(area, mode);
        self.sel_dirty = true;
        self.status = if self.doc.selection.is_empty() {
            tr("Выделение снято").to_string()
        } else {
            let (x0, y0, x1, y1) = self.doc.selection.bounds().unwrap_or_default();
            trf("Выделение {0}×{1} в ({2}, {3})", &[&(x1 - x0), &(y1 - y0), &x0, &y0])
        };
    }

    fn deselect(&mut self) {
        self.doc.deselect();
        self.sel_dirty = true;
    }

    /// Выбрать слой выше или ниже в списке (как «Select Forward/Backward Layer» в SAI).
    fn select_neighbor_layer(&mut self, up: bool) {
        let mut flat = Vec::new();
        fn walk(layers: &[Layer], path: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
            for (i, l) in layers.iter().enumerate() {
                path.push(i);
                out.push(path.clone());
                if let Some(ch) = &l.children {
                    walk(ch, path, out);
                }
                path.pop();
            }
        }
        walk(&self.doc.layers, &mut Vec::new(), &mut flat);
        // в списке сверху вниз — обратный порядок обхода
        flat.reverse();
        let Some(i) = flat.iter().position(|p| *p == self.selected) else { return };
        let j = if up { i.checked_sub(1) } else { (i + 1 < flat.len()).then_some(i + 1) };
        if let Some(j) = j {
            self.selected = flat[j].clone();
        }
    }

    /// Слой на передний (`front`) или задний план своей стопки.
    fn layer_to_end(&mut self, front: bool) {
        let Some((&last, parent)) = self.selected.split_last() else { return };
        let len = if parent.is_empty() { self.doc.layers.len() } else { self.doc.layer(parent).and_then(|l| l.children.as_ref()).map_or(0, Vec::len) };
        let delta = if front { len as isize - 1 - last as isize } else { -(last as isize) };
        if let Some((p, keys)) = self.doc.move_layer(&self.selected.clone(), delta) {
            self.selected = p;
            self.after_structure(keys);
        }
    }

    /// «Показать только этот слой» (Show Alone в SAI): остальные прячутся; повтор — всё как было.
    fn show_alone(&mut self) {
        let keep = self.selected.clone();
        self.show_alone_of(keep);
    }

    /// «Открыть/закрыть папку слоёв» (Alt+\, как в SAI): текущая папка, а у слоя — его папка.
    fn toggle_folder(&mut self, ctx: &egui::Context) {
        let Some(id) = self.layers_ui else { return };
        let Some(path) = (1..=self.selected.len()).rev().map(|n| self.selected[..n].to_vec()).find(|p| self.doc.layer(p).is_some_and(|l| l.children.is_some())) else { return };
        let fid = id.with(("folder", path.clone()));
        let open = ctx.data_mut(|d| {
            let v = d.get_persisted_mut_or(fid, false);
            *v = !*v;
            *v
        });
        // папка закрылась, а текущим был слой внутри — текущей становится сама папка
        if !open && self.selected.len() > path.len() {
            self.selected = path;
        }
    }

    /// Отмеченная папка (Ctrl + щелчок по глазу), в которой лежит текущий слой (или он сам).
    fn marked_folder(&self) -> Option<Vec<usize>> {
        (1..=self.selected.len()).rev().map(|n| self.selected[..n].to_vec()).find(|p| self.doc.layer(p).is_some_and(|l| l.children.is_some() && l.marked))
    }

    /// «Отметить папку для видимости» (Mark Layer Folder for Visibility Change в SAI): текущая
    /// папка, а у слоя — его папка; повтор снимает отметку.
    fn mark_folder(&mut self) {
        let path = (1..=self.selected.len()).rev().map(|n| self.selected[..n].to_vec()).find(|p| self.doc.layer(p).is_some_and(|l| l.children.is_some()));
        match path.and_then(|p| self.doc.layer_mut(&p)) {
            Some(l) => l.marked = !l.marked,
            None => self.status = String::from(tr("Текущий слой не в папке — отмечать нечего")),
        }
    }

    /// «Показать/скрыть отмеченную папку» и «Показать только отмеченную папку» (как в SAI).
    fn marked_folder_visibility(&mut self, alone: bool) {
        let Some(path) = self.marked_folder() else {
            self.status = String::from(tr("Нет отмеченной папки над текущим слоем (Ctrl + щелчок по глазу папки)"));
            return;
        };
        if alone {
            self.show_alone_of(path);
        } else if let Some(l) = self.doc.layer_mut(&path) {
            l.visible = !l.visible;
            self.invalidate_all();
            self.dirty = true;
        }
    }

    fn show_alone_of(&mut self, keep: Vec<usize>) {
        fn walk(layers: &mut [Layer], path: &mut Vec<usize>, keep: &[usize], saved: &mut Vec<(Vec<usize>, bool)>) {
            for (i, l) in layers.iter_mut().enumerate() {
                path.push(i);
                saved.push((path.clone(), l.visible));
                // видны сам слой, его папки-родители и всё внутри него
                l.visible = keep.starts_with(path) || path.starts_with(keep);
                if let Some(ch) = l.children.as_mut() {
                    walk(ch, path, keep, saved);
                }
                path.pop();
            }
        }
        if let Some(saved) = self.solo.take() {
            for (p, v) in saved {
                if let Some(l) = self.doc.layer_mut(&p) {
                    l.visible = v;
                }
            }
        } else {
            let mut saved = Vec::new();
            walk(&mut self.doc.layers, &mut Vec::new(), &keep, &mut saved);
            self.solo = Some(saved);
        }
        self.invalidate_all();
    }

    /// «Закрыть все виды» (Close All Views в SAI): закрываются все вкладки по очереди; о
    /// несохранённом спрашивается, «Отмена» останавливает. Остаётся пустой холст.
    fn close_all_views(&mut self) {
        self.closing_all = true;
        self.close_all_step();
    }

    /// Следующий шаг «Закрыть все виды» (пока окно вопроса закрыто).
    fn close_all_step(&mut self) {
        while self.closing_all && self.close_ask.is_none() {
            let pristine = self.views.len() == 1 && self.file.is_none() && !self.dirty && !self.doc.can_undo();
            if pristine {
                self.closing_all = false;
                break;
            }
            let before = (self.views.len(), self.tabs.len());
            self.request_close_view(self.active_view);
            // последний холст закрывается в новый пустой — дальше закрывать нечего
            if self.close_ask.is_none() && (self.views.len(), self.tabs.len()) == before {
                self.closing_all = false;
            }
        }
    }

    /// Повернуть слой на 90° (`cw`) или отразить (`horizontal`) — из меню «Слой» и с панели.
    fn layer_turn(&mut self, cw: Option<bool>, horizontal: Option<bool>) {
        if self.transform.is_some() {
            self.finish_transform(true);
        }
        let path = self.selected.clone();
        let keys = match (cw, horizontal) {
            (Some(cw), _) => self.doc.rotate_layer(&path, cw),
            (_, Some(h)) => self.doc.flip_layer(&path, h),
            _ => return,
        };
        self.invalidate(keys);
        self.sel_dirty = true;
        self.thumbs.clear();
        self.dirty = true;
    }

    /// Кнопки на панели для инструментов выделения, перемещения, вида и пипетки (по образцу
    /// параметров инструментов Krita и GIMP; в SAI этих кнопок нет — только сочетания клавиш).
    fn tool_buttons(&mut self, ui: &mut egui::Ui) {
        let b = Vec2::splat(22.0);
        match self.tool {
            Tool::Marquee | Tool::Lasso | Tool::Wand => {
                ui.horizontal(|ui| {
                    ui.label(tr("Режим:"));
                    for (m, g, tip) in [
                        (Combine::Replace, "sel_replace", tr("Заменить")),
                        (Combine::Add, "sel_add", tr("Добавить (Shift)")),
                        (Combine::Subtract, "sel_sub", tr("Вычесть (Alt)")),
                        (Combine::Intersect, "sel_int", tr("Пересечь (Shift+Alt)")),
                    ] {
                        if sai_ui::icon_button(ui, b, self.sel_mode == m, true, tip, |p, r, c| sai_ui::glyph(p, r, c, g)).clicked() {
                            self.sel_mode = m;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    if ui.small_button(tr("Всё")).on_hover_text(tr("Выделить всё")).clicked() {
                        self.select_all();
                    }
                    if ui.small_button(tr("Снять")).on_hover_text(tr("Снять выделение")).clicked() {
                        self.deselect();
                    }
                    if ui.small_button(tr("Инвертировать")).clicked() {
                        self.invert_selection();
                    }
                });
            }
            Tool::Move => {
                ui.horizontal(|ui| {
                    ui.label(tr("Слой:"));
                    let tips = [tr("Отразить слой по горизонтали"), tr("Отразить слой по вертикали"), tr("Повернуть слой на 90° против часовой"), tr("Повернуть слой на 90° по часовой")];
                    for (i, (g, tip)) in ["flip_h", "flip_v", "ccw", "cw"].into_iter().zip(tips).enumerate() {
                        if sai_ui::icon_button(ui, b, false, true, tip, |p, r, c| sai_ui::glyph(p, r, c, g)).clicked() {
                            match i {
                                0 => self.layer_turn(None, Some(true)),
                                1 => self.layer_turn(None, Some(false)),
                                2 => self.layer_turn(Some(false), None),
                                _ => self.layer_turn(Some(true), None),
                            }
                        }
                    }
                });
            }
            Tool::Zoom | Tool::Hand | Tool::RotateView => {
                ui.horizontal(|ui| {
                    if sai_ui::icon_button(ui, b, false, true, tr("Мельче"), |p, r, c| sai_ui::glyph(p, r, c, "minus")).clicked() {
                        self.set_zoom(self.zoom / 1.5);
                    }
                    if sai_ui::icon_button(ui, b, false, true, tr("Крупнее"), |p, r, c| sai_ui::glyph(p, r, c, "plus")).clicked() {
                        self.set_zoom(self.zoom * 1.5);
                    }
                    if ui.small_button("100%").clicked() {
                        self.set_zoom(1.0 / self.ppp);
                    }
                    if ui.small_button(tr("По окну")).clicked() {
                        self.fit_pending = true;
                    }
                });
                ui.horizontal(|ui| {
                    for (g, tip, step) in [("ccw", tr("Повернуть вид против часовой"), -15.0), ("cw", tr("Повернуть вид по часовой"), 15.0), ("reset", tr("Сбросить угол вида"), 0.0)] {
                        if sai_ui::icon_button(ui, b, false, true, tip, |p, r, c| sai_ui::glyph(p, r, c, g)).clicked() {
                            self.rotate_view(step);
                        }
                    }
                    if sai_ui::icon_button(ui, b, self.view_flip, true, tr("Зеркало вида"), |p, r, c| sai_ui::glyph(p, r, c, "flip")).clicked() {
                        self.view_flip = !self.view_flip;
                    }
                    if ui.small_button(tr("Сбросить положение")).clicked() {
                        self.reset_view_position();
                    }
                });
            }
            Tool::Picker => {
                ui.checkbox(&mut self.pick_layer, tr("Только текущий слой"));
            }
            _ => return,
        }
        ui.add_space(4.0);
    }

    fn select_all(&mut self) {
        self.doc.select_all();
        self.sel_dirty = true;
    }

    fn invert_selection(&mut self) {
        self.doc.invert_selection();
        self.sel_dirty = true;
    }

    fn begin_move(&mut self, at: Vec2) {
        if self.filter.is_some() {
            return;
        }
        let Some(layer) = self.doc.layer(&self.selected) else { return };
        if layer.children.is_some() || !layer.visible || !layer.movable() {
            self.status = String::from(tr("Двигать можно видимый обычный слой без запрета перемещения"));
            return;
        }
        self.move_drag = Some(MoveDrag {
            path: self.selected.clone(),
            start: at,
            original: layer.pixels.clone(),
            orig_mask: self.doc.mask_snapshot(&self.selected),
            orig_sel: self.doc.selection.clone(),
            offset: (0, 0),
        });
    }

    fn update_move(&mut self, at: Vec2) {
        let Some(d) = self.move_drag.as_mut() else { return };
        let off = ((at.x - d.start.x).round() as i32, (at.y - d.start.y).round() as i32);
        if off == d.offset {
            return;
        }
        d.offset = off;
        let keys = self.doc.move_preview(&d.path, &d.original, d.orig_mask.as_ref(), &d.orig_sel, off.0, off.1);
        self.status = trf("Сдвиг: {0}, {1}", &[&off.0, &off.1]);
        self.sel_dirty = true;
        self.invalidate(keys);
    }

    fn end_move(&mut self) {
        let Some(d) = self.move_drag.take() else { return };
        self.doc.move_commit(&d.path, d.original, d.orig_mask, d.orig_sel);
        self.thumbs.clear();
        self.dirty = true;
    }

    /// Начать трансформацию (Ctrl+T): выделенное или всё содержимое текущего слоя.
    fn begin_transform(&mut self) {
        if self.filter.is_some() {
            return;
        }
        let Some(layer) = self.doc.layer(&self.selected) else { return };
        if layer.children.is_some() || !layer.visible || !layer.movable() {
            self.status = String::from(tr("Трансформировать можно видимый обычный слой без запрета перемещения"));
            return;
        }
        let sel = self.doc.selection.clone();
        let bounds = if sel.mask.tiles.is_empty() { paint_core::selection::content_bounds(&layer.pixels) } else { sel.bounds() };
        let Some((x0, y0, x1, y1)) = bounds else {
            self.status = String::from(tr("На слое нечего трансформировать"));
            return;
        };
        let src = [x0 as f64, y0 as f64, x1 as f64, y1 as f64];
        let (cx, cy) = ((src[0] + src[2]) / 2.0, (src[1] + src[3]) / 2.0);
        self.transform = Some(Transform {
            path: self.selected.clone(),
            original: layer.pixels.clone(),
            orig_mask: self.doc.mask_snapshot(&self.selected),
            orig_sel: sel,
            src,
            edges: [src[0] - cx, src[1] - cy, src[2] - cx, src[3] - cy],
            angle: 0.0,
            shift: [0.0, 0.0],
            drag: None,
            mode: paint_core::resample::Mode::Bilinear,
            dirty: false,
            quad: None,
            perspective: self.transform_perspective as f64 / 100.0,
            grid: None,
        });
        self.ensure_grid();
        self.status = String::from(tr("Трансформация: углы и стороны — размер, Ctrl + угол — свободная деформация, внутри — сдвиг, снаружи — поворот; Enter — применить, Esc — отмена"));
    }

    /// Режим «Решётка» или «Сетка»: поставить сетку на источник (если рамку уже двигали —
    /// сначала применить её, дальше деформировать результат), или снять её в других режимах.
    fn ensure_grid(&mut self) {
        let kind = self.transform_kind.grid();
        let (cols, rows, blend) = (self.grid_div[0] as usize, self.grid_div[1] as usize, self.grid_blend);
        let Some(t) = self.transform.as_mut() else { return };
        match kind {
            None => {
                if t.grid.take().is_some() {
                    // деформацию сеткой применить — дальше рамка работает с её итогом
                    self.finish_transform(true);
                    self.begin_transform();
                }
            }
            Some(k) => {
                let same = t.grid.as_ref().is_some_and(|g| g.kind == k && g.cols == cols && g.rows == rows);
                if same {
                    if let Some(g) = t.grid.as_mut() {
                        if g.blend != blend {
                            g.blend = blend;
                            t.dirty = true;
                        }
                    }
                    return;
                }
                let moved = t.grid.as_ref().is_some_and(|g| *g != paint_core::gridwarp::GridWarp::new(g.src, g.cols, g.rows, g.kind)) || !t.untouched();
                if moved {
                    self.finish_transform(true);
                    self.begin_transform();
                    return;
                }
                let mut g = paint_core::gridwarp::GridWarp::new(t.src, cols, rows, k);
                g.blend = blend;
                t.grid = Some(g);
                t.dirty = true;
            }
        }
    }

    /// Пересчитать показ трансформации (во время протяжки — быстрым билинейным способом).
    fn update_transform(&mut self, fast: bool) {
        let Some(t) = self.transform.as_mut() else { return };
        t.dirty = false;
        let mode = if fast { paint_core::resample::Mode::Bilinear } else { t.mode };
        let keys = match &t.grid {
            Some(g) => self.doc.grid_preview(&t.path, &t.original, t.orig_mask.as_ref(), &t.orig_sel, g, mode),
            None => self.doc.warp_preview(&t.path, &t.original, t.orig_mask.as_ref(), &t.orig_sel, t.warp(), mode),
        };
        self.sel_dirty = true;
        self.invalidate(keys);
    }

    fn finish_transform(&mut self, apply: bool) {
        if apply {
            self.update_transform(false);
        }
        let Some(t) = self.transform.take() else { return };
        if apply {
            self.doc.move_commit(&t.path, t.original, t.orig_mask, t.orig_sel);
            self.dirty = true;
            self.status = String::from(tr("Трансформация применена"));
        } else {
            let keys = self.doc.move_preview(&t.path, &t.original, t.orig_mask.as_ref(), &t.orig_sel, 0, 0);
            self.invalidate(keys);
            self.status = String::from(tr("Трансформация отменена"));
        }
        self.sel_dirty = true;
        self.thumbs.clear();
    }

    /// Действие над всем холстом: применить, показать заново (размер мог измениться).
    fn canvas_op(&mut self, f: impl FnOnce(&mut Document) -> Vec<Key>) {
        if self.transform.is_some() {
            self.finish_transform(true);
        }
        f(&mut self.doc);
        self.dirty = true;
        self.fit_pending = true;
        self.invalidate_all();
        self.status = trf("Холст {0}×{1}", &[&self.doc.width, &self.doc.height]);
    }

    /// Открыть фильтр для текущего слоя.
    fn open_filter(&mut self, kind: FilterKind) {
        if self.transform.is_some() {
            self.finish_transform(true);
        }
        if self.filter.is_some() {
            return;
        }
        let Some(layer) = self.doc.layer(&self.selected) else { return };
        if layer.children.is_some() || !layer.paintable() {
            self.status = String::from(tr("Фильтр применяется к обычному слою без запрета рисования"));
            return;
        }
        self.filter = Some(FilterDialog {
            kind,
            hs: Default::default(),
            bc: Default::default(),
            levels: Default::default(),
            curve: Default::default(),
            median_r: 1,
            radius: if kind == FilterKind::Unsharp { 2.0 } else { 1.0 },
            amount: 1.0,
            threshold: 0,
            width_px: 1,
            thicken: true,
            dust_size: 8,
            dust_mode: 0,
            ca_radial: true,
            ca_px: 5.0,
            ca_angle: 0.0,
            noise_color: false,
            noise_amount: 0.2,
            tile: 10,
            cell: 20,
            randomness: 0.7,
            tiling: false,
            retro: paint_core::filters::RetroFilm { preset: 0, effect: 0, intensity: 0.6, noise: 0.15 },
            perlin: paint_core::filters::Perlin { scale: 64.0, amplitude: 0.7, attenuation: 0.5, repetition: 4, offset: [0.0, 0.0] },
            motion: paint_core::filters::Streak { strength: 20.0, angle: 0.0, dir: 0, smooth: true },
            radial: paint_core::filters::Streak { strength: 0.1, angle: 0.0, dir: 0, smooth: true },
            spin_angle: 10.0,
            spin_dir: 0,
            lens_radius: 10.0,
            lens_sides: 0,
            lens_highlight: 0.3,
            distort: {
                use paint_core::filters::DistortKind as D;
                let (w, h) = (self.doc.width as f64, self.doc.height as f64);
                let k = if let FilterKind::Distort(k) = kind { k } else { D::Pinch };
                let amount = match k {
                    D::Twirl => 90.0,
                    _ => 0.5,
                };
                paint_core::filters::Distort { kind: k, center: (w / 2.0, h / 2.0), radius: w.min(h) * 0.4, amount, amplitude: 10.0, wavelength: 40.0, angle: 0.0, shape: 0, to_polar: true }
            },
            pencil: paint_core::filters::Pencil { thickness: 3.0, darkness: 0.5, hatching: 0.3, color: false },
            artistic: paint_core::filters::Artistic { radius: 4, levels: 6, edges: 0.6 },
            preview: true,
            path: self.selected.clone(),
            original: Arc::new(layer.pixels.clone()),
            sel: Arc::new(self.doc.selection.clone()),
            dirty: true,
        });
        // прошлые настройки фильтров (окно открывается с ними, как в SAI)
        if let Some(last) = self.filter_last.take() {
            let Some(f) = self.filter.take() else { return };
            let distort = paint_core::filters::Distort {
                kind: f.distort.kind,
                // середина — от прежнего холста, если на этом она вне его
                ..if last.distort.center.0 <= self.doc.width as f64 && last.distort.center.1 <= self.doc.height as f64 { last.distort } else { f.distort }
            };
            self.filter = Some(FilterDialog { kind: f.kind, distort, preview: last.preview, path: f.path, original: f.original, sel: f.sel, dirty: true, ..last });
        }
    }


    /// Пересчитать предпросмотр (или вернуть исходное, если предпросмотр выключен). Счёт идёт в
    /// фоновом потоке — окно не замирает; пока он идёт, новые настройки ждут (берутся последние).
    fn update_filter(&mut self, ctx: &egui::Context) {
        if self.filter_job.is_some() {
            return;
        }
        let Some(f) = self.filter.as_mut() else { return };
        f.dirty = false;
        self.filter_gen += 1;
        if !f.preview {
            let (path, pixels) = (f.path.clone(), (*f.original).clone());
            let keys = self.doc.set_layer_pixels(&path, pixels);
            self.invalidate(keys);
            self.filter_shown = None;
            return;
        }
        let (job, w, h, color, ctx) = (f.clone(), self.doc.width, self.doc.height, self.color(), ctx.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(filter_result(&job, w, h, color));
            ctx.request_repaint();
        });
        self.filter_job = Some(FilterJob { gen: self.filter_gen, rx });
    }

    /// Готов ли фоновый предпросмотр: показать его, если настройки с тех пор не менялись.
    fn poll_filter(&mut self) {
        let Some(job) = &self.filter_job else { return };
        let pixels = match job.rx.try_recv() {
            Ok(p) => p,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.filter_job = None;
                return;
            }
        };
        let gen = job.gen;
        self.filter_job = None;
        let Some(f) = self.filter.as_ref() else { return };
        if f.preview {
            let path = f.path.clone();
            let keys = self.doc.set_layer_pixels(&path, pixels);
            self.invalidate(keys);
            self.filter_shown = Some(gen);
        }
    }

    /// «Изменить разрешение / размер…»: размер — холста, остальное — как в прошлый раз.
    fn open_canvas_dialog(&mut self, resample: bool) {
        let (keep_ratio, mode, anchor) = self.canvas_last;
        self.canvas_dialog = Some(CanvasDialog { resample, size: [self.doc.width, self.doc.height], keep_ratio, mode, anchor });
    }

    fn finish_filter(&mut self, apply: bool) {
        let Some(f) = self.filter.take() else { return };
        let mut f = f;
        let job = self.filter_job.take();
        let current = !f.dirty && f.preview;
        if apply && current && job.is_none() && self.filter_shown == Some(self.filter_gen) {
            // на слое уже последний предпросмотр — второй раз не считать
        } else {
            let pixels = match job {
                // фоновый счёт последних настроек — дождаться его
                Some(j) if apply && current && j.gen == self.filter_gen => j.rx.recv().unwrap_or_else(|_| filter_result(&f, self.doc.width, self.doc.height, self.color())),
                _ if apply => filter_result(&f, self.doc.width, self.doc.height, self.color()),
                _ => (*f.original).clone(),
            };
            let keys = self.doc.set_layer_pixels(&f.path, pixels);
            self.invalidate(keys);
        }
        self.filter_shown = None;
        let original = Arc::try_unwrap(std::mem::take(&mut f.original)).unwrap_or_else(|a| (*a).clone());
        f.sel = Default::default();
        if apply {
            let sel = self.doc.selection.clone();
            self.doc.move_commit(&f.path, original, None, sel);
            self.dirty = true;
            self.thumbs.clear();
        }
        // настройки — для следующего открытия, и после «Отмены» тоже
        self.filter_last = Some(f);
    }

    /// Градиент с текущими настройками и цветами.
    fn make_gradient(&self, from: [f64; 2], to: [f64; 2]) -> paint_core::filters::Gradient {
        let (f, b) = (self.color(), self.back);
        let o = self.grad_opts;
        paint_core::filters::Gradient {
            from,
            to,
            circle: o.circle,
            fg: [f.r(), f.g(), f.b()],
            bg: (!o.to_transparent).then_some([b.r(), b.g(), b.b()]),
            reverse: o.reverse,
            s_curve: o.s_curve,
        }
    }

    /// Нажатие инструментом «Градиент»: у конца — тянуть его, иначе — новый градиент.
    fn gradient_press(&mut self, c: Vec2) {
        let p = [c.x as f64, c.y as f64];
        let tol = 8.0 / self.zoom as f64;
        if let Some(e) = self.gradient.as_mut() {
            let near = |q: [f64; 2]| (q[0] - p[0]).hypot(q[1] - p[1]) < tol;
            e.drag = Some(if near(e.g.to) {
                1
            } else if near(e.g.from) {
                0
            } else {
                e.g.from = p;
                e.g.to = p;
                e.dirty = true;
                1
            });
            return;
        }
        if !self.can_paint() {
            self.status = String::from(tr("Градиент рисуется на видимом обычном слое без запрета рисования"));
            return;
        }
        let Some(layer) = self.doc.layer(&self.selected) else { return };
        self.gradient = Some(GradientEdit {
            path: self.selected.clone(),
            original: layer.pixels.clone(),
            g: self.make_gradient(p, p),
            drag: Some(1),
            dirty: true,
        });
    }

    /// Слой с градиентом (пока концы совпадают — без градиента).
    fn gradient_result(&self, e: &GradientEdit) -> paint_core::tile::ColorMap {
        if e.g.from == e.g.to {
            return e.original.clone();
        }
        let sel = &self.doc.selection;
        let preserve = self.doc.layer(&e.path).is_some_and(|l| l.preserve_opacity);
        paint_core::filters::gradient(&e.original, self.doc.width, self.doc.height, &e.g, &|x, y| sel.amount(x, y), preserve)
    }

    /// Обновить предпросмотр градиента (настройки и цвета могли поменяться).
    fn update_gradient(&mut self) {
        let Some(e) = self.gradient.as_ref() else { return };
        let g = self.make_gradient(e.g.from, e.g.to);
        let e = self.gradient.as_mut().unwrap();
        if g != e.g {
            e.g = g;
            e.dirty = true;
        }
        if !e.dirty {
            return;
        }
        e.dirty = false;
        let e = self.gradient.as_ref().unwrap();
        let (path, pixels) = (e.path.clone(), self.gradient_result(e));
        let keys = self.doc.set_layer_pixels(&path, pixels);
        self.invalidate(keys);
    }

    fn finish_gradient(&mut self, apply: bool) {
        let Some(e) = self.gradient.take() else { return };
        let apply = apply && e.g.from != e.g.to;
        let pixels = if apply { self.gradient_result(&e) } else { e.original.clone() };
        let keys = self.doc.set_layer_pixels(&e.path, pixels);
        self.invalidate(keys);
        if apply {
            let sel = self.doc.selection.clone();
            self.doc.move_commit(&e.path, e.original, None, sel);
            self.dirty = true;
            self.thumbs.clear();
        }
    }

    /// Закрыть многоугольное лассо и выделить.
    fn close_polygon(&mut self) {
        let Some(d) = self.sel_poly.take() else { return };
        if d.fill {
            self.lasso_fill_points(&d.points);
            return;
        }
        let pts: Vec<[f64; 2]> = d.points.iter().map(|p| [p.x as f64, p.y as f64]).collect();
        let area = paint_core::selection::polygon_aa(&pts, self.lasso_aa);
        self.apply_selection(&area, d.mode);
    }

    /// Сдвиг клавишами, как в SAI 2017 (сверено вживую): Alt+стрелки (Nudge) при выделении
    /// двигают только его рамку, без выделения — слой; Ctrl+стрелки (Cut & Nudge) вырезают
    /// выделенное и двигают вместе с рамкой, без выделения — слой. Shift — шаг 10 px.
    fn nudge(&mut self, dx: i32, dy: i32, cut: bool) {
        if !cut && !self.doc.selection.is_empty() {
            self.doc.nudge_selection(dx, dy);
            self.sel_dirty = true;
            return;
        }
        if !self.doc.layer(&self.selected).is_some_and(|l| l.movable()) {
            return;
        }
        if cut && !self.doc.selection.is_empty() && !self.doc.has_pixels_in_selection(&self.selected) {
            self.status = String::from(tr("В выделении нет рисунка, нечего вырезать"));
            return;
        }
        let keys = self.doc.move_layer_pixels(&self.selected.clone(), dx, dy);
        if !keys.is_empty() {
            self.invalidate(keys);
            self.sel_dirty = true;
            self.thumbs.clear();
            self.dirty = true;
        }
    }

    /// Залить область `area` основным цветом на текущем слое.
    fn fill_area(&mut self, area: &paint_core::tile::MaskMap) {
        if !self.can_paint() {
            self.status = String::from(tr("Заливать можно видимый обычный слой без запрета рисования"));
            return;
        }
        let c = self.color();
        let keys = self.doc.fill(&self.selected.clone(), area, [c.r(), c.g(), c.b()]);
        self.invalidate(keys);
        self.thumbs.clear();
        self.dirty = true;
    }

    /// Заливка «ведром» с точки (x, y).
    /// «Закрыть щели» и «Под линию» у палочки и заливки.
    fn gap_rows(&mut self, ui: &mut egui::Ui, lw: f32, w: f32) {
        for (label, v, max) in [(tr("Закрыть щели"), &mut self.fill_gap, 20u32), (tr("Под линию"), &mut self.fill_expand, 10)] {
            sai_ui::row(ui, label, lw, true, |ui| {
                let mut f = *v as f32;
                let text = if *v == 0 { tr("нет").to_string() } else { format!("{} px", *v) };
                if sai_ui::bar(ui, &mut f, 0.0, max as f32, text, w, true).changed() {
                    *v = f.round().clamp(0.0, max as f32) as u32;
                }
            });
        }
    }

    fn fill_at(&mut self, x: f32, y: f32) {
        let src = self.doc.pick_source(&self.selected);
        let area = paint_core::selection::magic_wand_closing(&src, self.doc.width, self.doc.height, x.floor() as i32, y.floor() as i32, paint_core::selection::WandOptions { tolerance: self.wand_tolerance, gap: self.fill_gap, expand: self.fill_expand });
        self.fill_area(&area);
    }

    /// Alt+Delete: залить выделенное (без выделения — весь слой).
    fn fill_selection(&mut self) {
        let all = paint_core::selection::Selection::all(self.doc.width, self.doc.height);
        self.fill_area(&all);
    }

    /// Выделение по непрозрачности слоя (Ctrl + щелчок по миниатюре).
    fn select_by_opacity(&mut self, path: &[usize], mode: Combine) {
        let Some(layer) = self.doc.layer(path) else { return };
        if layer.children.is_some() {
            return;
        }
        let area = paint_core::selection::from_opacity(&layer.pixels);
        self.apply_selection(&area, mode);
    }

    /// Волшебная палочка по текущему слою.
    fn wand_at(&mut self, x: f32, y: f32, mode: Combine) {
        let src = self.doc.pick_source(&self.selected);
        let area = paint_core::selection::magic_wand_closing(&src, self.doc.width, self.doc.height, x.floor() as i32, y.floor() as i32, paint_core::selection::WandOptions { tolerance: self.wand_tolerance, gap: self.fill_gap, expand: self.fill_expand });
        self.apply_selection(&area, mode);
    }

    fn pick_color(&mut self, x: f32, y: f32) {
        if self.pick_layer {
            // только текущий слой: прозрачное не берётся
            if let Some(l) = self.doc.layer(&self.selected) {
                let [r, g, b, a] = paint_core::pixel::to_rgba8(l.pixels.get(x.floor() as i32, y.floor() as i32));
                if a > 0 {
                    self.hsv = sai_ui::rgb_to_hsv(Color32::from_rgb(r, g, b));
                }
            }
            return;
        }
        let key = ((x as i32).div_euclid(TILE), (y as i32).div_euclid(TILE));
        let rgba = self.doc.render_tile(key);
        let i = (((y as i32).rem_euclid(TILE) * TILE + (x as i32).rem_euclid(TILE)) * 4) as usize;
        self.hsv = sai_ui::rgb_to_hsv(Color32::from_rgb(rgba[i], rgba[i + 1], rgba[i + 2]));
    }

    /// Новый слой контура над текущим: рисовать пером — получаются кривые, как в SAI.
    fn add_linework_layer(&mut self) {
        let mut path = self.selected.clone();
        let index = path.pop().map_or(0, |i| i + 1);
        let n = count_layers(&self.doc.layers) + 1;
        let mut layer = Layer::new(&trf("Контур{0}", &[&n]));
        layer.vector = Some(paint_formats::linework::empty());
        path.push(index);
        let keys = self.doc.insert_layer(&path, layer);
        self.selected = path;
        self.after_structure(keys);
    }

    fn add_layer(&mut self, folder: bool) {
        let mut path = self.selected.clone();
        let index = path.pop().map_or(0, |i| i + 1);
        let n = count_layers(&self.doc.layers) + 1;
        let new = if folder { Layer::folder(&trf("Папка{0}", &[&n]), Vec::new()) } else { Layer::new(&trf("Слой{0}", &[&n])) };
        path.push(index);
        let keys = self.doc.insert_layer(&path, new);
        self.selected = path;
        self.after_structure(keys);
    }

    /// Действия со слоем из кнопок под списком, меню «Слой» и клавиш образца.
    /// Маска выбранного слоя: 0 — новая, 1 — применить, 2 — удалить.
    fn mask_command(&mut self, what: u8) {
        if self.filter.is_some() {
            self.finish_filter(true);
        }
        if self.transform.is_some() {
            self.finish_transform(true);
        }
        let path = self.selected.clone();
        let keys = match what {
            0 => self.doc.new_mask(&path),
            1 => self.doc.apply_mask(&path),
            _ => self.doc.set_mask(&path, None),
        };
        // после новой маски кисть сразу рисует в ней
        self.mask_edit = what == 0 && self.doc.layer(&path).is_some_and(|l| l.mask.is_some());
        self.invalidate(keys);
        self.thumbs.clear();
        self.dirty = true;
    }

    fn layer_action(&mut self, action: LayerAction) {
        if self.filter.is_some() {
            self.finish_filter(true);
        }
        if self.transform.is_some() {
            self.finish_transform(true);
        }
        let path = self.selected.clone();
        let result = match action {
            LayerAction::Duplicate => {
                let keys = self.doc.duplicate_layer(&path);
                let mut p = path.clone();
                *p.last_mut().unwrap_or(&mut 0) += 1;
                Some((p, keys))
            }
            LayerAction::Delete => {
                let keys = self.doc.delete_layer(&path);
                Some((path.clone(), keys))
            }
            LayerAction::Clear => Some((path.clone(), self.doc.clear_layer(&path))),
            LayerAction::Merge => self.doc.merge_down(&path),
            LayerAction::Transfer => self.doc.transfer_down(&path).map(|(_, k)| (path.clone(), k)),
            LayerAction::Up => self.doc.move_layer(&path, 1),
            LayerAction::Down => self.doc.move_layer(&path, -1),
        };
        match result {
            Some((p, keys)) => {
                self.selected = p;
                self.after_structure(keys);
            }
            None => self.status = String::from(tr("С этим слоем так нельзя (нужен обычный слой и обычный слой под ним)")),
        }
    }

    /// То же, но без пометки «изменено» (живая правка, которая потом запишется шагом).
    fn after_structure_quiet(&mut self, keys: Vec<Key>) {
        let dirty = self.dirty;
        self.after_structure(keys);
        self.dirty = dirty;
    }

    /// После изменения стопки: проверить выбор, перерисовать, обновить миниатюры.
    fn after_structure(&mut self, keys: Vec<Key>) {
        self.dirty = true;
        if self.doc.layer(&self.selected).is_none() {
            let mut p = self.selected.clone();
            while let Some(last) = p.last_mut() {
                if *last > 0 {
                    *last -= 1;
                    if self.doc.layer(&p).is_some() {
                        break;
                    }
                } else {
                    p.pop();
                    if self.doc.layer(&p).is_some() {
                        break;
                    }
                }
            }
            self.selected = if self.doc.layer(&p).is_some() { p } else { first_raster(&self.doc.layers).unwrap_or_default() };
        }
        self.thumbs.clear();
        self.invalidate(keys);
    }

    /// Масштаб вокруг центра области просмотра.
    /// Повернуть вид на `step` градусов (0 — сбросить угол). Рисунок не меняется.
    /// «Сбросить положение вида» (Home в SAI): холст посередине окна, масштаб прежний.
    fn reset_view_position(&mut self) {
        let r = self.view;
        if r.width() > 0.0 {
            let (w, h) = (self.doc.width as f32, self.doc.height as f32);
            self.offset = Vec2::new((w - r.width() / self.zoom) / 2.0, (h - r.height() / self.zoom) / 2.0);
        }
    }

    fn rotate_view(&mut self, step: f32) {
        if step == 0.0 {
            self.view_angle = 0.0;
        } else {
            let mut d = self.view_angle.to_degrees() + step;
            while d > 180.0 {
                d -= 360.0;
            }
            while d <= -180.0 {
                d += 360.0;
            }
            self.view_angle = d.to_radians();
        }
    }

    fn set_zoom(&mut self, zoom: f32) {
        let zoom = zoom.clamp(0.02, 32.0);
        if self.view.is_positive() {
            let c = self.view.size() / 2.0;
            let anchor = c / self.zoom + self.offset;
            self.offset = anchor - c / zoom;
        }
        self.zoom = zoom;
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::menu::bar(ui, |ui| {
            ui.menu_button(keys::hint(tr("Файл(F)")), |ui| {
                if ui.button(tr("Создать холст…")).clicked() {
                    self.show_new = true;
                    ui.close_menu();
                }
                if ui.add(menu_item("Открыть…", "Ctrl+O")).clicked() {
                    self.ask_open();
                    ui.close_menu();
                }
                if ui.add(menu_item("Сохранить", "Ctrl+S")).clicked() {
                    self.save();
                    ui.close_menu();
                }
                if ui.add(menu_item("Сохранить как…", "Shift+Ctrl+S")).clicked() {
                    self.ask_save_as();
                    ui.close_menu();
                }
                if ui.add(menu_item("Сохранить с номером", "")).clicked() {
                    self.save_numbered();
                    ui.close_menu();
                }
                if ui.button(tr("Сохранить набор кистей…")).on_hover_text(tr("Все инструменты сетки — одним файлом .gsbrushes")).clicked() {
                    self.ask_save_brush(None);
                    ui.close_menu();
                }
                self.recording_menu(ui);
                if ui.button(tr("Водяной знак…")).clicked() {
                    self.show_watermark = true;
                    self.watermark_preview = None;
                    ui.close_menu();
                }
                ui.separator();
                if ui.checkbox(&mut self.always_viewer, tr("Всегда использовать обозреватель файлов")).changed() {
                    self.save_settings();
                }
                if ui.button(tr("Открыть через обозреватель…")).clicked() {
                    self.open_viewer(file_viewer::Mode::Open);
                    ui.close_menu();
                }
                if ui.button(tr("Сохранить через обозреватель…")).clicked() {
                    self.open_viewer(file_viewer::Mode::Save);
                    ui.close_menu();
                }
                ui.separator();
                icons::submenu(ui, "export", tr("Экспорт"), |ui| {
                    for (label, ext) in EXPORTS {
                        if ext == "psd" {
                            ui.separator();
                        }
                        if ui.button(tr(label)).clicked() {
                            self.export_as(ext);
                            ui.close_menu();
                        }
                    }
                });
                let mut pick = None;
                let mut clear_recent = false;
                ui.add_enabled_ui(!self.recent.is_empty(), |ui| {
                    ui.menu_button(tr("Недавние"), |ui| {
                        for p in &self.recent {
                            let name = p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned());
                            if ui.button(name).on_hover_text(p.display().to_string()).clicked() {
                                pick = Some(p.clone());
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        if icons::item(ui, "clear", tr("Очистить список")).clicked() {
                            clear_recent = true;
                            ui.close_menu();
                        }
                    });
                });
                if clear_recent {
                    self.recent.clear();
                    self.save_settings();
                }
                if let Some(p) = pick {
                    if p.exists() {
                        self.open(p);
                    } else {
                        self.status = trf("Файла больше нет: {0}", &[&p.display()]);
                        self.recent.retain(|r| *r != p);
                    }
                }
                if ui.add(menu_item("Закрыть", "Ctrl+W")).clicked() {
                    self.request_close_view(self.active_view);
                    ui.close_menu();
                }
                ui.separator();
                if ui.add(menu_item("Холст из буфера обмена", "Ctrl+B")).clicked() {
                    self.canvas_from_clipboard();
                    ui.close_menu();
                }
                ui.separator();
                if ui.button(tr("Выход")).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button(keys::hint(tr("Правка(E)")), |ui| {
                if ui
                    .add_enabled(
                        self.doc.can_undo(),
                        menu_item("Отменить", "Ctrl+Z"),
                    )
                    .clicked()
                {
                    self.undo();
                    ui.close_menu();
                }
                if ui
                    .add_enabled(
                        self.doc.can_redo(),
                        menu_item("Повторить", "Ctrl+Y"),
                    )
                    .clicked()
                {
                    self.redo();
                    ui.close_menu();
                }
                ui.separator();
                if ui.add(menu_item("Вырезать", "Ctrl+X")).clicked() {
                    self.copy(true, false);
                    ui.close_menu();
                }
                if ui.add(menu_item("Копировать", "Ctrl+C")).clicked() {
                    self.copy(false, false);
                    ui.close_menu();
                }
                if ui.add(menu_item("Вставить", "Ctrl+V")).clicked() {
                    self.paste();
                    ui.close_menu();
                }
                if ui.add_enabled(!self.doc.selection.is_empty(), egui::Button::new(tr("Копировать по границам выделения"))).clicked() {
                    self.copy(false, true);
                    ui.close_menu();
                }
            });
            ui.menu_button(keys::hint(tr("Холст(C)")), |ui| {
                for (item, resample) in [(tr("Изменить разрешение…"), true), (tr("Изменить размер…"), false)] {
                    if ui.button(item).clicked() {
                        self.open_canvas_dialog(resample);
                        ui.close_menu();
                    }
                }
                if icons::item(ui, "gray", tr("Перевести в оттенки серого…")).clicked() {
                    self.ask_gray = true;
                    ui.close_menu();
                }
                ui.separator();
                for (item, op) in [
                    (tr("Отразить по горизонтали"), 0),
                    (tr("Отразить по вертикали"), 1),
                    (tr("Повернуть на 90° против часовой"), 2),
                    (tr("Повернуть на 90° по часовой"), 3),
                ] {
                    if ui.button(item).clicked() {
                        self.canvas_op(|d| match op {
                            0 => d.flip_canvas(true),
                            1 => d.flip_canvas(false),
                            2 => d.rotate_canvas(false),
                            _ => d.rotate_canvas(true),
                        });
                        ui.close_menu();
                    }
                }
                if ui.add_enabled(!self.doc.selection.is_empty(), egui::Button::new(tr("Обрезать по выделению"))).clicked() {
                    self.canvas_op(|d| d.trim_to_selection());
                    self.sel_dirty = true;
                    ui.close_menu();
                }
                ui.separator();
                ui.menu_button(tr("Фон холста"), |ui| {
                    if ui.button(tr("Цвет фона холста…")).clicked() {
                        self.bg_color_edit = Some(self.doc.bg_color);
                        ui.close_menu();
                    }
                    ui.separator();
                    // как в SAI 2017: код фона — байт 0x11 заголовка .sai2
                    for (code, name) in [
                        (0x40, tr("Прозрачный (цвет)")),
                        (0x30, tr("Прозрачный (чёрный)")),
                        (0x20, tr("Прозрачный (белый)")),
                        (0x10, tr("Прозрачный (тёмная шашка)")),
                        (0x00, tr("Прозрачный (светлая шашка)")),
                        (0x21, tr("Цвет")),
                        (0x11, tr("Чёрный")),
                        (0x01, tr("Белый")),
                    ] {
                        if ui.radio(self.doc.bg_code == code, name).clicked() {
                            let color = self.doc.bg_color;
                            self.doc.set_background(code, color);
                            self.invalidate_all();
                            self.dirty = true;
                            ui.close_menu();
                        }
                    }
                });
                // как в SAI 2017: цвет каймы и бумаги у эффектов слоя (метка в заголовке .sai2)
                ui.menu_button(tr("Цвет эффектов слоя"), |ui| {
                    for (fx, name) in [
                        (4u8, tr("Как в версии 1")),
                        (3, tr("Умножение")),
                        (2, tr("Глубокий")),
                        (1, tr("Яркий")),
                        (0, tr("Обычный")),
                    ] {
                        if ui.radio(self.doc.fx() == fx, name).clicked() {
                            self.doc.effect_color = paint_core::paper::fx_tag(fx);
                            self.invalidate_all();
                            self.dirty = true;
                            ui.close_menu();
                        }
                    }
                });
            });
            ui.menu_button(keys::hint(tr("Слой(L)")), |ui| {
                if ui.button(tr("Новый слой")).clicked() {
                    self.add_layer(false);
                    ui.close_menu();
                }
                if ui.button(tr("Новый слой контура")).clicked() {
                    self.add_linework_layer();
                    ui.close_menu();
                }
                if ui.button(tr("Новый слой фигур")).clicked() {
                    self.add_shape_layer();
                    ui.close_menu();
                }
                if ui.button(tr("Новая папка")).clicked() {
                    self.add_layer(true);
                    ui.close_menu();
                }
                if ui.button(tr("Новый слой 3D…")).on_hover_text(tr("Модель glTF (.glb, .gltf) или OBJ: камера, поза по суставам, вид")).clicked() {
                    let dir = self.file.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default().join("x"));
                    if !self.native_dialog(files::Ask::Model3d(false), &dir) {
                        self.status = String::from(tr("Окно выбора файла недоступно; перетащите файл модели в окно"));
                    }
                    ui.close_menu();
                }
                if ui.add_enabled(self.s3d.is_some(), egui::Button::new(tr("Окно 3D"))).clicked() {
                    self.show_3d = true;
                    ui.close_menu();
                }
                ui.separator();
                for (item, key, action) in [
                    ("Копировать слой", "", LayerAction::Duplicate),
                    ("Удалить слой", "", LayerAction::Delete),
                    ("Очистить слой", "Delete", LayerAction::Clear),
                    ("Перенести на нижний", "F", LayerAction::Transfer),
                    ("Слить с нижним", "Ctrl+E", LayerAction::Merge),
                    ("Поднять слой", "Ctrl+[", LayerAction::Up),
                    ("Опустить слой", "Ctrl+]", LayerAction::Down),
                ] {
                    if ui.add(menu_item(item, key)).clicked() {
                        self.layer_action(action);
                        ui.close_menu();
                    }
                }
                if ui.button(tr("Слой на передний план")).clicked() {
                    self.layer_to_end(true);
                    ui.close_menu();
                }
                if ui.button(tr("Слой на задний план")).clicked() {
                    self.layer_to_end(false);
                    ui.close_menu();
                }
                let mut alone = self.solo.is_some();
                if ui.checkbox(&mut alone, tr("Показать только этот слой")).clicked() {
                    self.show_alone();
                    ui.close_menu();
                }
                ui.separator();
                for (item, cw) in [(tr("Повернуть слой на 90° против часовой"), false), (tr("Повернуть слой на 90° по часовой"), true)] {
                    if ui.button(item).clicked() {
                        self.layer_turn(Some(cw), None);
                        ui.close_menu();
                    }
                }
                for (item, horizontal) in [(tr("Отразить слой по горизонтали"), true), (tr("Отразить слой по вертикали"), false)] {
                    if ui.button(item).clicked() {
                        self.layer_turn(None, Some(horizontal));
                        ui.close_menu();
                    }
                }
                ui.separator();
                let masked = self.doc.layer(&self.selected).is_some_and(|l| l.mask.is_some());
                let raster = self.doc.layer(&self.selected).is_some_and(|l| l.children.is_none());
                if ui.add_enabled(!masked, egui::Button::new(tr("Новая маска слоя"))).clicked() {
                    self.mask_command(0);
                    ui.close_menu();
                }
                if ui.add_enabled(masked && raster, egui::Button::new(tr("Применить маску"))).clicked() {
                    self.mask_command(1);
                    ui.close_menu();
                }
                if ui.add_enabled(masked, egui::Button::new(tr("Удалить маску"))).clicked() {
                    self.mask_command(2);
                    ui.close_menu();
                }
                ui.separator();
                if ui.add_enabled(raster, egui::Button::new(tr("Раскрасить по контуру"))).on_hover_text(tr("Цветные пятна текущего слоя растекаются до линий остальных видимых слоёв; результат — новый слой под текущим")).clicked() {
                    self.colorize_by_lines();
                    ui.close_menu();
                }
                ui.separator();
                let is_vec = self.doc.layer(&self.selected).is_some_and(|l| l.vector.is_some() || l.scene3d.is_some());
                if ui.add_enabled(is_vec, egui::Button::new(tr("Растеризовать слой"))).clicked() {
                    self.s3d_finish();
                    let path = self.selected.clone();
                    let keys = self.doc.rasterize_layer(&path);
                    self.invalidate(keys);
                    self.thumbs.clear();
                    self.dirty = true;
                    ui.close_menu();
                }
                let can_outline = self.outline_dialog.is_none() && self.doc.layer(&self.selected).is_some_and(|l| l.children.is_none() && l.paintable());
                if ui.add_enabled(can_outline, egui::Button::new(tr("Обводка…"))).clicked() {
                    if let Some(l) = self.doc.layer(&self.selected) {
                        let (width, position, canvas_edges) = self.outline_last;
                        self.outline_dialog = Some(OutlineDialog { path: self.selected.clone(), original: l.pixels.clone(), width, position, canvas_edges, preview: true, dirty: true });
                    }
                    ui.close_menu();
                }
                if ui.button(tr("Яркость в прозрачность")).clicked() {
                    let keys = self.doc.luminance_to_alpha(&self.selected.clone());
                    self.invalidate(keys);
                    self.thumbs.clear();
                    self.dirty = true;
                    ui.close_menu();
                }
                if ui.button(tr("Слить видимые")).clicked() {
                    if self.transform.is_some() {
                        self.finish_transform(true);
                    }
                    let name = trf("Слой{0}", &[&(count_layers(&self.doc.layers) + 1)]);
                    if let Some((i, keys)) = self.doc.merge_visible(&name) {
                        self.selected = vec![i];
                        self.after_structure(keys);
                        self.dirty = true;
                    }
                    ui.close_menu();
                }
                if ui.button(tr("Свести всё")).clicked() {
                    self.canvas_op(|d| d.flatten_layers());
                    self.selected = vec![0];
                    ui.close_menu();
                }
                ui.separator();
                if ui.add_enabled(self.doc.layer(&self.selected).is_some(), egui::Button::new(tr("Свойства слоя…"))).clicked() {
                    self.layer_props = self.doc.layer(&self.selected).map(|l| (self.selected.clone(), l.name.clone()));
                    ui.close_menu();
                }
            });
            ui.menu_button(keys::hint(tr("Выделение(S)")), |ui| {
                let has = !self.doc.selection.is_empty();
                if ui.add(menu_item("Выделить всё", "Ctrl+A")).clicked() {
                    self.select_all();
                    ui.close_menu();
                }
                if ui.add_enabled(has, menu_item("Снять выделение", "Ctrl+D")).clicked() {
                    self.deselect();
                    ui.close_menu();
                }
                if ui.button(tr("Инвертировать")).clicked() {
                    self.invert_selection();
                    ui.close_menu();
                }
                if ui.add(menu_item(if self.show_selection { "Скрыть выделение" } else { "Показать выделение" }, "Ctrl+H")).clicked() {
                    self.show_selection = !self.show_selection;
                    ui.close_menu();
                }
                ui.separator();
                for (grow, name) in [(true, tr("Расширить выделение…")), (false, tr("Сузить выделение…"))] {
                    if ui.add_enabled(has && self.grow_dialog.is_none(), egui::Button::new(name)).clicked() {
                        self.grow_dialog = Some(GrowDialog { grow, n: self.grow_last, preview: true, original: self.doc.selection.clone(), dirty: true });
                        ui.close_menu();
                    }
                }
                if ui.add_enabled(has, egui::Button::new(tr("Расширить на 1 px"))).clicked() {
                    self.doc.grow_selection(true);
                    self.sel_dirty = true;
                    ui.close_menu();
                }
                if ui.add_enabled(has, egui::Button::new(tr("Сузить на 1 px"))).clicked() {
                    self.doc.grow_selection(false);
                    self.sel_dirty = true;
                    ui.close_menu();
                }
                ui.separator();
                // узлы слоя контура, как в SAI (Select CPs in Selection и т. п.)
                let liwk = self.doc.layer(&self.selected).is_some_and(|l| l.vector.as_ref().is_some_and(|v| &v.kind == b"liwk"));
                for (cmd, name, need) in [(Some(false), tr("Выделить узлы в выделении"), has), (Some(true), tr("Выделить штрихи по выделению"), has), (None, tr("Снять выделение узлов"), self.cp_sel.is_some())] {
                    if ui.add_enabled(liwk && need, egui::Button::new(name)).clicked() {
                        self.select_cps(cmd);
                        ui.close_menu();
                    }
                }
                ui.separator();
                // «Region Created from Pixel Opacity» в SAI
                ui.menu_button(tr("Область по непрозрачности слоя"), |ui| {
                    for (mode, name) in [(Combine::Replace, tr("Задать")), (Combine::Add, tr("Добавить")), (Combine::Subtract, tr("Вычесть")), (Combine::Intersect, tr("Пересечь"))] {
                        if ui.button(name).clicked() {
                            let path = self.selected.clone();
                            self.select_by_opacity(&path, mode);
                            ui.close_menu();
                        }
                    }
                });
            });
            ui.menu_button(keys::hint(tr("Линейка(R)")), |ui| {
                let mut shown = self.ruler.visible;
                if ui.checkbox(&mut shown, tr("Показать линейку")).on_hover_text("Ctrl+R").clicked() {
                    self.toggle_ruler();
                    ui.close_menu();
                }
                ui.menu_button(tr("Симметрия"), |ui| {
                    for (v, name) in [(0u8, tr("Выключена")), (1, tr("Лево ↔ право")), (2, tr("Верх ↔ низ")), (3, tr("Обе оси")), (4, tr("По кругу"))] {
                        if ui.radio(self.symmetry == v, name).clicked() {
                            self.symmetry = v;
                            self.save_settings();
                            if v != 4 {
                                ui.close_menu();
                            }
                        }
                    }
                    // по кругу (как симметричная линейка SAI 2019+): части, отражение, угол, центр
                    ui.add_enabled_ui(self.symmetry == 4, |ui| {
                        let mut changed = false;
                        ui.horizontal(|ui| {
                            ui.label(tr("Частей"));
                            changed |= ui.add(egui::DragValue::new(&mut self.sym_n).range(1..=20)).changed();
                        });
                        changed |= ui.checkbox(&mut self.sym_mirror, tr("С отражением")).on_hover_text(tr("С отражением — столько осей, сколько частей; без — копии поворотом вокруг центра")).changed();
                        ui.horizontal(|ui| {
                            ui.label(tr("Угол"));
                            changed |= ui.add(egui::DragValue::new(&mut self.sym_angle).speed(0.5).range(-180.0..=180.0).suffix("°")).changed();
                        });
                        let (w, h) = (self.doc.width as f32, self.doc.height as f32);
                        let mut c = self.sym_center.unwrap_or([w / 2.0, h / 2.0]);
                        ui.horizontal(|ui| {
                            ui.label(tr("Центр"));
                            let a = ui.add(egui::DragValue::new(&mut c[0]).speed(1.0).range(0.0..=w));
                            let b = ui.add(egui::DragValue::new(&mut c[1]).speed(1.0).range(0.0..=h));
                            if a.changed() || b.changed() {
                                self.sym_center = Some(c);
                            }
                        });
                        if ui.button(tr("Центр — середина холста")).clicked() {
                            self.sym_center = None;
                        }
                        if changed {
                            self.save_settings();
                        }
                    });
                });
                ui.separator();
                for (kind, label) in [
                    (ruler::Kind::Straight, tr("Прямая")),
                    (ruler::Kind::Ellipse, tr("Эллипс")),
                    (ruler::Kind::Perspective(1), tr("Перспектива: 1 точка")),
                    (ruler::Kind::Perspective(2), tr("Перспектива: 2 точки")),
                    (ruler::Kind::Perspective(3), tr("Перспектива: 3 точки")),
                    (ruler::Kind::Parallel, tr("Параллельные линии")),
                    (ruler::Kind::Concentric, tr("Концентрические эллипсы")),
                    (ruler::Kind::Radial, tr("Точка схода (лучи)")),
                ] {
                    if ui.radio(self.ruler.kind == kind, label).clicked() {
                        let was = self.ruler.kind;
                        self.ruler.kind = kind;
                        // точки схода — заново посреди вида, если сменилось их число
                        if was != kind && matches!(kind, ruler::Kind::Perspective(_) | ruler::Kind::Radial) && self.ruler.placed {
                            let (c, min) = self.view_frame();
                            self.ruler.place_vps(c, min);
                        }
                        if !self.ruler.visible {
                            self.toggle_ruler();
                        }
                        ui.close_menu();
                    }
                }
                ui.separator();
                ui.checkbox(&mut self.ruler.ignore, tr("Не замечать линейку"));
                ui.add_enabled_ui(matches!(self.ruler.kind, ruler::Kind::Straight | ruler::Kind::Parallel), |ui| {
                    ui.checkbox(&mut self.ruler.lock_pivot, tr("Закрепить ось"))
                        .on_hover_text(tr("Ctrl + перетаскивание полосы поворачивает линейку вокруг оси"));
                });
                if ui.add_enabled(self.ruler.visible, egui::Button::new(tr("Сбросить линейку"))).clicked() {
                    self.reset_ruler();
                    ui.close_menu();
                }
            });
            ui.menu_button(keys::hint(tr("Фильтр(T)")), |ui| {
                for (item, key, kind) in [("Тон и насыщенность…", "Ctrl+U", FilterKind::HueSaturation), ("Яркость и контраст…", "", FilterKind::BrightnessContrast), ("Уровни…", "", FilterKind::Levels), ("Кривые…", "", FilterKind::Curves)] {
                    if ui.add(menu_item(item, key)).clicked() {
                        self.open_filter(kind);
                        ui.close_menu();
                    }
                }
                // подменю — как в Clip Studio Paint
                icons::submenu(ui, "f_blur", tr("Размытие"), |ui| {
                    for (item, kind) in [
                        ("Среднее", FilterKind::Average),
                        ("Размытие", FilterKind::Blur),
                        ("Сильное размытие", FilterKind::BlurStrong),
                        ("Размытие по Гауссу…", FilterKind::GaussianBlur),
                        ("Размытие линзы…", FilterKind::Lens),
                        ("Размытие в движении…", FilterKind::Motion),
                        ("Радиальное размытие…", FilterKind::Radial),
                        ("Размытие вращением…", FilterKind::Spin),
                        ("Сглаживание краёв", FilterKind::Smoothing),
                    ] {
                        if ui.button(tr(item)).clicked() {
                            self.open_filter(kind);
                            ui.close_menu();
                        }
                    }
                });
                icons::submenu(ui, "f_effect", tr("Эффекты"), |ui| {
                    for (item, kind) in [
                        ("Хроматическая аберрация…", FilterKind::Aberration),
                        ("Шум…", FilterKind::Noise),
                        ("Мозаика…", FilterKind::Mosaic),
                        ("Кристаллизация…", FilterKind::Crystallize),
                        ("Ретро-плёнка…", FilterKind::RetroFilm),
                        ("Карандашный рисунок…", FilterKind::Pencil),
                        ("Художественный…", FilterKind::Artistic),
                    ] {
                        if ui.button(tr(item)).clicked() {
                            self.open_filter(kind);
                            ui.close_menu();
                        }
                    }
                });
                icons::submenu(ui, "f_distort", tr("Искажение"), |ui| {
                    use paint_core::filters::DistortKind as D;
                    for (item, kind) in [
                        ("Щипок…", D::Pinch),
                        ("Рябь…", D::Ripple),
                        ("Рыбий глаз…", D::Fisheye),
                        ("Полярные координаты…", D::Polar),
                        ("Волна…", D::Wave),
                        ("Скручивание…", D::Twirl),
                    ] {
                        if ui.button(tr(item)).clicked() {
                            self.open_filter(FilterKind::Distort(kind));
                            ui.close_menu();
                        }
                    }
                });
                icons::submenu(ui, "f_render", tr("Отрисовка"), |ui| {
                    if ui.button(tr("Шум Перлина…")).clicked() {
                        self.open_filter(FilterKind::Perlin);
                        ui.close_menu();
                    }
                });
                icons::submenu(ui, "f_correct", tr("Коррекция"), |ui| {
                    for (item, kind) in [("Удаление пыли…", FilterKind::Dust), ("Изменение толщины линий…", FilterKind::LineWidth), ("Медиана…", FilterKind::Median)] {
                        if ui.button(tr(item)).clicked() {
                            self.open_filter(kind);
                            ui.close_menu();
                        }
                    }
                });
                icons::submenu(ui, "f_sharpen", tr("Резкость"), |ui| {
                    for (item, kind) in [("Резкость", FilterKind::Sharpen), ("Сильная резкость", FilterKind::SharpenMore), ("Нерезкая маска…", FilterKind::Unsharp)] {
                        if ui.button(tr(item)).clicked() {
                            self.open_filter(kind);
                            ui.close_menu();
                        }
                    }
                });
            });
            ui.menu_button(tr("Плагины"), |ui| {
                self.plugins_menu(ui);
            });
            ui.menu_button(keys::hint(tr("Вид(V)")), |ui| {
                if ui.add(menu_item("Увеличить", "Ctrl++")).clicked() {
                    self.set_zoom(self.zoom * 1.25);
                    ui.close_menu();
                }
                if ui.add(menu_item("Уменьшить", "Ctrl+−")).clicked() {
                    self.set_zoom(self.zoom / 1.25);
                    ui.close_menu();
                }
                if ui.add(menu_item("Масштаб 100%", "Ctrl+Alt+0")).clicked() {
                    self.set_zoom(1.0 / self.ppp);
                    ui.close_menu();
                }
                if ui.button(tr("Вписать в окно")).clicked() {
                    self.fit_pending = true;
                    ui.close_menu();
                }
                if ui.checkbox(&mut self.pixel_grid, tr("Пиксельная сетка (от 800 %)")).clicked() {
                    ui.close_menu();
                }
                if ui.checkbox(&mut self.wrap_around, tr("Бесшовный режим (холст повторяется)")).clicked() {
                    ui.close_menu();
                }
                ui.separator();
                if ui.add(menu_item("Повернуть вид против часовой", "Shift+PageUp")).clicked() {
                    self.rotate_view(-15.0);
                    ui.close_menu();
                }
                if ui.add(menu_item("Повернуть вид по часовой", "Shift+PageDown")).clicked() {
                    self.rotate_view(15.0);
                    ui.close_menu();
                }
                if ui.add(menu_item("Сбросить положение вида", "Home")).clicked() {
                    self.reset_view_position();
                    ui.close_menu();
                }
                if ui.add(menu_item("Сбросить угол вида", "Shift+Home")).clicked() {
                    self.rotate_view(0.0);
                    ui.close_menu();
                }
                if ui.add(egui::Button::image_and_text(icons::image(ui.ctx(), "view_tab"), tr("Новый вид"))).clicked() {
                    self.new_view();
                    ui.close_menu();
                }
                if ui.add(egui::Button::image_and_text(icons::image(ui.ctx(), "view_float"), tr("Новый плавающий вид"))).clicked() {
                    self.float_view = Some(FloatView { zoom: self.zoom, offset: self.offset, angle: self.view_angle, flip: self.view_flip });
                    ui.close_menu();
                }
                let many = self.views.len() > 1;
                if ui.add_enabled(many, egui::Button::new(tr("Предыдущий вид"))).clicked() {
                    self.switch_view((self.active_view + self.views.len() - 1) % self.views.len());
                    ui.close_menu();
                }
                if ui.add_enabled(many, egui::Button::new(tr("Следующий вид"))).clicked() {
                    self.switch_view((self.active_view + 1) % self.views.len());
                    ui.close_menu();
                }
                if ui.add_enabled(many, egui::Button::new(tr("Закрыть все виды"))).clicked() {
                    self.close_all_views();
                    ui.close_menu();
                }
                ui.separator();
                let mut flip = self.view_flip;
                if ui.checkbox(&mut flip, tr("Зеркало вида")).on_hover_text("H").clicked() {
                    self.view_flip = flip;
                    ui.close_menu();
                }
                // открытые холсты внизу меню, как в SAI: «1 Имя (масштаб)»
                ui.separator();
                let names: Vec<String> = (0..self.tabs.len())
                    .map(|i| match self.tabs.get(i) {
                        Some(Some(t)) => format!("{} {} ({:.0}%)", i + 1, t.title(), t.zoom * self.ppp * 100.0),
                        _ => format!("{} {} ({:.0}%)", i + 1, self.file_name(), self.zoom * self.ppp * 100.0),
                    })
                    .collect();
                for (i, n) in names.iter().enumerate() {
                    if ui.radio(i == self.active_tab, n).clicked() {
                        self.switch_tab(i);
                        ui.close_menu();
                    }
                }
            });
            ui.menu_button(keys::hint(tr("Окно(W)")), |ui| {
                if icons::item(ui, "assets", tr("Менеджер ресурсов…")).clicked() {
                    self.assets = Some(AssetsWindow::default());
                    ui.close_menu();
                }
                ui.menu_button(tr("Показать"), |ui| {
                    for (i, name) in SHOW_NAMES.iter().enumerate() {
                        if ui.checkbox(&mut self.win.show[i], tr(name)).changed() {
                            self.save_settings();
                        }
                    }
                });
                // как Separate User Interface Panel в SAI: панель — в своём окне
                ui.menu_button(tr("Отдельные панели"), |ui| {
                    for i in [3, 2, 1, 0] {
                        if ui.checkbox(&mut self.win.separate[i], tr(SHOW_NAMES[i])).changed() {
                            self.save_settings();
                        }
                    }
                });
                ui.menu_button(tr("Язык (Language)"), |ui| {
                    for (code, native, _) in i18n::LANGS {
                        if ui.radio(i18n::code() == code, native).clicked() {
                            i18n::set(code);
                            i18n::install_fonts(ui.ctx());
                            let mut s = settings::Settings::load();
                            s.set("lang", code);
                            s.save();
                            ui.close_menu();
                        }
                    }
                });
                ui.separator();
                let mut on = keys::hints();
                if ui
                    .checkbox(&mut on, tr("Показывать быстрые клавиши"))
                    .on_hover_text(tr("Клавиши в пунктах меню и во всплывающих подсказках"))
                    .changed()
                {
                    keys::set_hints(on);
                    let mut s = settings::Settings::load();
                    s.set("key_hints", on as u8);
                    s.save();
                }
                ui.separator();
                if ui.checkbox(&mut self.show_history, tr("История")).clicked() {
                    ui.close_menu();
                }
                if ui.button(tr("Опорная картинка…")).on_hover_text(tr("Или перетащите картинку в окно с Ctrl")).clicked() {
                    let dir = self.file.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default().join("x"));
                    if !self.native_dialog(files::Ask::Reference, &dir) {
                        self.status = String::from(tr("Перетащите картинку в окно с Ctrl"));
                    }
                    ui.close_menu();
                }
                if ui.checkbox(&mut self.dark_theme, tr("Тёмная тема")).changed() {
                    sai_ui::set_dark(self.dark_theme);
                    sai_ui::apply_style(ui.ctx());
                    let mut st = settings::Settings::load();
                    st.set("dark_theme", self.dark_theme as u8);
                    st.save();
                }
                // как меню Window в SAI 2017
                let w = &mut self.win;
                ui.menu_button(tr("Масштаб интерфейса"), |ui| {
                    for z in [1.0_f32, 1.25, 1.5, 2.0] {
                        if ui.radio((w.ui_scale - z).abs() < 0.01, format!("{:.0}%", z * 100.0)).clicked() {
                            w.ui_scale = z;
                            ui.close_menu();
                        }
                    }
                });
                if ui.button(tr("Быстрые клавиши…")).clicked() {
                    self.show_keys = true;
                    ui.close_menu();
                }
                ui.menu_button(tr("Режим ползунков HSV"), |ui| {
                    for (m, name) in [(2u8, "HSV"), (1, "HSL"), (0, "V-HSV")] {
                        if ui.radio(w.hsv_mode == m, name).clicked() {
                            w.hsv_mode = m;
                            ui.close_menu();
                        }
                    }
                });
                ui.menu_button(tr("Размер образцов"), |ui| {
                    for (m, name) in [(0u8, tr("Маленький")), (1, tr("Средний")), (2, tr("Большой"))] {
                        if ui.radio(w.swatch_size == m, name).clicked() {
                            w.swatch_size = m;
                            ui.close_menu();
                        }
                    }
                });
                ui.checkbox(&mut w.tools_right, tr("Панель цвета и инструментов справа"));
                ui.checkbox(&mut w.layers_right, tr("Панель навигатора и слоёв справа"));
                ui.separator();
                if ui.add(egui::Checkbox::new(&mut w.fullscreen, tr("Полный экран"))).on_hover_text("F11, Shift+Tab").changed() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Fullscreen(w.fullscreen));
                }
                let mut shown = !w.panels_hidden;
                if ui.checkbox(&mut shown, tr("Показывать все панели")).on_hover_text("Tab").changed() {
                    w.panels_hidden = !shown;
                }
                ui.checkbox(&mut w.dot_cursor, tr("Курсор-точка у кистей"));
                ui.checkbox(&mut w.brush_circle, tr("Кружок размера кисти"));
                ui.checkbox(&mut w.size_numbers, tr("Список размеров — только числа"));
                ui.checkbox(&mut w.size_list_top, tr("Список размеров — над настройками кисти"));
            });
            ui.menu_button(keys::hint(tr("Помощь(H)")), |ui| {
                if ui.button(tr("Клавиши…")).clicked() {
                    self.show_keys = true;
                    ui.close_menu();
                }
                if ui.button(tr("Настройки…")).clicked() {
                    self.opts_edit = Some(self.opts);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button(tr("GrazySAI на GitHub")).clicked() {
                    update::open_url(update::REPO);
                    ui.close_menu();
                }
                if ui.button(tr("Проверить обновления…")).clicked() {
                    update::check(&self.update, ui.ctx());
                    self.show_update = true;
                    ui.close_menu();
                }
                if ui.checkbox(&mut self.check_updates, tr("Проверять обновления при запуске")).changed() {
                    let mut s = settings::Settings::load();
                    s.set("check_updates", self.check_updates as u8);
                    s.save();
                }
                ui.separator();
                if ui.button(tr("О программе")).clicked() {
                    self.show_about = true;
                    ui.close_menu();
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new(egui::RichText::new("GitHub").weak()).frame(false))
                    .on_hover_text(update::REPO)
                    .clicked()
                {
                    update::open_url(update::REPO);
                }
                // ненавязчиво: одна строка справа в меню, по щелчку — окно обновления
                let state = self.update.lock().unwrap().clone();
                let notice = match &state {
                    update::State::Available(r) => Some(trf("Вышла версия {0}", &[&r.version])),
                    update::State::Downloading(_) => Some(tr("Загрузка обновления…").to_string()),
                    update::State::Installed(r) => Some(trf("Версия {0} готова — перезапустите", &[&r.version])),
                    _ => None,
                };
                if let Some(text) = notice {
                    ui.separator();
                    let text = egui::RichText::new(format!("⬆ {text}")).color(sai_ui::header());
                    if ui.add(egui::Button::new(text).frame(false)).clicked() {
                        self.show_update = true;
                    }
                }
            });
        });
    }

    /// Быстрая панель над холстом: отмена, выделение, масштаб, угол, сглаживание.
    fn quick_bar(&mut self, ui: &mut egui::Ui) {
        let b = Vec2::splat(20.0);
        ui.horizontal(|ui| {
            if sai_ui::icon_button(
                ui,
                b,
                false,
                self.doc.can_undo(),
                &keys::hint(tr("Отменить (Ctrl+Z)")),
                |p, r, c| sai_ui::glyph(p, r, c, "undo"),
            )
            .clicked()
            {
                self.undo();
            }
            if sai_ui::icon_button(
                ui,
                b,
                false,
                self.doc.can_redo(),
                &keys::hint(tr("Повторить (Ctrl+Y)")),
                |p, r, c| sai_ui::glyph(p, r, c, "redo"),
            )
            .clicked()
            {
                self.redo();
            }
            ui.separator();
            let has_sel = !self.doc.selection.is_empty();
            if sai_ui::icon_button(ui, b, false, has_sel, &keys::hint(tr("Снять выделение (Ctrl+D)")), |p, r, c| sai_ui::glyph(p, r, c, "deselect")).clicked() {
                self.deselect();
            }
            if sai_ui::icon_button(ui, b, false, true, tr("Инвертировать выделение"), |p, r, c| sai_ui::glyph(p, r, c, "invert")).clicked() {
                self.invert_selection();
            }
            if sai_ui::icon_button(ui, b, self.show_selection, true, &keys::hint(tr("Показать выделение (Ctrl+H)")), |p, r, c| sai_ui::glyph(p, r, c, "eye")).clicked() {
                self.show_selection = !self.show_selection;
            }
            ui.label(tr("Выделение"));
            ui.separator();
            let current = format!("{:.0}%", self.zoom * self.ppp * 100.0);
            let mut pick = None;
            egui::ComboBox::from_id_salt("zoom")
                .selected_text(current)
                .width(64.0)
                .show_ui(ui, |ui| {
                    for z in ZOOMS {
                        if ui.selectable_label(false, format!("{z}%")).clicked() {
                            pick = Some(z / 100.0);
                        }
                    }
                });
            if let Some(z) = pick {
                self.set_zoom(z / self.ppp);
            }
            if sai_ui::icon_button(ui, b, false, true, tr("Уменьшить"), |p, r, c| {
                sai_ui::glyph(p, r, c, "minus")
            })
            .clicked()
            {
                self.set_zoom(self.zoom / 1.25);
            }
            if sai_ui::icon_button(ui, b, false, true, tr("Увеличить"), |p, r, c| {
                sai_ui::glyph(p, r, c, "plus")
            })
            .clicked()
            {
                self.set_zoom(self.zoom * 1.25);
            }
            if sai_ui::icon_button(ui, b, false, true, tr("Масштаб 100%"), |p, r, c| {
                sai_ui::glyph(p, r, c, "reset")
            })
            .clicked()
            {
                self.set_zoom(1.0 / self.ppp);
            }
            ui.separator();
            let deg = self.view_angle.to_degrees();
            let mut pick_angle = None;
            egui::ComboBox::from_id_salt("angle")
                .selected_text(format!("{:+.1}°", deg))
                .width(56.0)
                .show_ui(ui, |ui| {
                    for a in [-180.0f32, -135.0, -90.0, -45.0, -30.0, -15.0, 0.0, 15.0, 30.0, 45.0, 90.0, 135.0, 180.0] {
                        if ui.selectable_label((deg - a).abs() < 0.05, format!("{a:+}°")).clicked() {
                            pick_angle = Some(a);
                        }
                    }
                });
            if let Some(a) = pick_angle {
                self.view_angle = a.to_radians();
            }
            for (g, tip, step) in [
                ("ccw", &keys::hint(tr("Повернуть вид против часовой (Shift+PageUp)")), -15.0f32),
                ("cw", &keys::hint(tr("Повернуть вид по часовой (Shift+PageDown)")), 15.0),
                ("reset", &keys::hint(tr("Сбросить поворот (Shift+Home)")), 0.0),
            ] {
                if sai_ui::icon_button(ui, b, false, true, tip, |p, r, c| sai_ui::glyph(p, r, c, g)).clicked() {
                    self.rotate_view(step);
                }
            }
            if sai_ui::icon_button(ui, b, self.view_flip, true, &keys::hint(tr("Отразить вид (H)")), |p, r, c| {
                sai_ui::glyph(p, r, c, "flip")
            })
            .clicked()
            {
                self.view_flip = !self.view_flip;
            }
            ui.separator();
            ui.label(tr("Сглаживание"));
            egui::ComboBox::from_id_salt("stab")
                .selected_text(self.stabilizer.label())
                .width(44.0)
                .height(420.0)
                .show_ui(ui, |ui| {
                    for v in paint_core::stabilizer::Level::all() {
                        ui.selectable_value(&mut self.stabilizer, v, v.label());
                    }
                });
            if sai_ui::icon_button(ui, b, self.straight, true, tr("Прямая линия: каждый штрих — прямая (Shift + нажатие — прямая от конца прошлого штриха)"), |p, r, c| sai_ui::glyph(p, r, c, "line")).clicked() {
                self.straight = !self.straight;
            }
            if sai_ui::icon_button(ui, b, self.ruler.ignore, self.ruler.visible, tr("Не замечать линейку"), |p, r, c| {
                sai_ui::glyph(p, r, c, "ruler")
            })
            .clicked()
            {
                self.ruler.ignore = !self.ruler.ignore;
            }
        });
    }

    /// Отделённые панели (Separate User Interface Panel в SAI) — каждая в своём окне системы;
    /// закрыть окно — панель возвращается на место.
    fn separate_panels(&mut self, ctx: &egui::Context) {
        for i in [3usize, 2, 1, 0] {
            if !(self.win.separate[i] && self.win.show[i]) {
                continue;
            }
            let size = [[186.0, 250.0], [186.0, 520.0], [236.0, 300.0], [236.0, 600.0]][i];
            let title = format!("{} — GrazySAI", tr(SHOW_NAMES[i]));
            let id = egui::ViewportId::from_hash_of(("panel", i));
            let builder = egui::ViewportBuilder::default().with_title(title.clone()).with_inner_size(size).with_min_inner_size([120.0, 80.0]);
            let full = self.win.show;
            let mut closed = false;
            ctx.show_viewport_immediate(id, builder, |ctx, class| {
                // в окне — только эта панель
                for (k, v) in self.win.show.iter_mut().take(4).enumerate() {
                    *v = k == i;
                }
                let body = |app: &mut Self, ui: &mut egui::Ui| {
                    egui::ScrollArea::vertical().id_salt(("panel_scroll", i)).show(ui, |ui| {
                        if i >= 2 {
                            app.left_panel(ui);
                        } else {
                            app.right_panel(ui);
                        }
                    });
                };
                if class == egui::ViewportClass::Embedded {
                    // система не даёт второго окна — плавающее окно внутри главного
                    let mut open = true;
                    egui::Window::new(&title).id(egui::Id::new(("panel_win", i))).open(&mut open).default_size(size).show(ctx, |ui| body(self, ui));
                    closed = !open;
                } else {
                    egui::CentralPanel::default().show(ctx, |ui| body(self, ui));
                    closed = ctx.input(|inp| inp.viewport().close_requested());
                }
                self.win.show = full;
            });
            if closed {
                self.win.separate[i] = false;
                self.save_settings();
            }
        }
    }

    fn left_panel(&mut self, ui: &mut egui::Ui) {
        if self.win.show[2] {
        // Переключатели панели цвета, как у образца: круг, ползунки RGB, ползунки HSV — каждый
        // показывается или прячется отдельно.
        ui.horizontal(|ui| {
            let b = Vec2::splat(18.0);
            if sai_ui::icon_button(ui, b, self.color_views[0], true, tr("Цветовой круг"), |p, r, _| {
                p.circle_stroke(r.center(), r.width() / 2.0, egui::Stroke::new(2.0_f32, Color32::from_rgb(200, 80, 160)));
            })
            .clicked()
            {
                self.color_views[0] = !self.color_views[0];
            }
            for (i, tip) in [(1, tr("Ползунки RGB")), (2, tr("Ползунки HSV"))] {
                if sai_ui::icon_button(ui, b, self.color_views[i], true, tip, |p, r, c| {
                    for k in 0..3 {
                        let y = r.top() + 2.0 + k as f32 * 4.0;
                        p.line_segment([Pos2::new(r.left(), y), Pos2::new(r.right(), y)], egui::Stroke::new(1.5_f32, c));
                    }
                })
                .clicked()
                {
                    self.color_views[i] = !self.color_views[i];
                }
            }
            if sai_ui::icon_button(ui, b, self.color_views[3], true, tr("Смеситель цвета"), |p, r, c| {
                for k in 0..4 {
                    let y = r.top() + 2.0 + k as f32 * 3.5;
                    p.line_segment([Pos2::new(r.left(), y), Pos2::new(r.right(), y)], egui::Stroke::new(1.5_f32, c));
                }
            })
            .clicked()
            {
                self.color_views[3] = !self.color_views[3];
            }
            if sai_ui::icon_button(ui, b, self.color_views[4], true, tr("Образцы цвета"), |p, r, c| {
                for k in 0..3 {
                    for j in 0..3 {
                        let q = Rect::from_min_size(r.min + Vec2::new(j as f32 * 6.0, k as f32 * 6.0), Vec2::splat(4.5));
                        p.rect_filled(q, 0.0, c);
                    }
                }
            })
            .clicked()
            {
                self.color_views[4] = !self.color_views[4];
            }
            if sai_ui::icon_button(ui, b, self.color_views[5], true, tr("Черновик"), |p, r, c| {
                p.rect_stroke(r.shrink(1.0), 0.0, egui::Stroke::new(1.0_f32, c));
                let pts: Vec<Pos2> = (0..=8).map(|i| Pos2::new(r.left() + 3.0 + i as f32 * (r.width() - 6.0) / 8.0, r.center().y + ((i as f32) * 1.4).sin() * r.height() * 0.22)).collect();
                p.add(egui::Shape::line(pts, egui::Stroke::new(1.5_f32, c)));
            })
            .clicked()
            {
                self.color_views[5] = !self.color_views[5];
            }
        });
        if self.color_views[0] {
            ui.vertical_centered(|ui| {
                sai_ui::color_wheel(ui, &mut self.hsv, 178.0);
            });
        }
        if self.color_views[1] {
            let c = sai_ui::hsv_to_rgb(self.hsv[0], self.hsv[1], self.hsv[2]);
            let mut rgb = [c.r() as f32, c.g() as f32, c.b() as f32];
            let mut moved = false;
            for (i, name) in ["R", "G", "B"].into_iter().enumerate() {
                let base = rgb;
                moved |= sai_ui::channel_slider(ui, name, &mut rgb[i], 255.0, |t| {
                    let mut v = base.map(|x| x as u8);
                    v[i] = (t * 255.0).round() as u8;
                    Color32::from_rgb(v[0], v[1], v[2])
                });
            }
            if moved {
                let hsv = sai_ui::rgb_to_hsv(Color32::from_rgb(rgb[0] as u8, rgb[1] as u8, rgb[2] as u8));
                // у серого тон не определён — прежний тон оставить
                let hue = if hsv[1] == 0.0 { self.hsv[0] } else { hsv[0] };
                self.hsv = [hue, hsv[1], hsv[2]];
            }
        }
        if self.color_views[2] {
            // как в SAI 2017: тон 0–359, насыщенность и яркость (светлота) 0–100 в режиме
            // «Окно → Режим ползунков HSV» (V-HSV, HSL, HSV)
            let hsv = self.hsv;
            let mode = self.win.hsv_mode;
            let (a, b) = sai_ui::sliders_from_hsv(mode, hsv[1], hsv[2]);
            let mut h = (hsv[0] * 360.0).round();
            let (mut sat, mut val) = ((a * 100.0).round(), (b * 100.0).round());
            let rgb_of = |a: f32, b: f32| {
                let (s, v) = sai_ui::hsv_from_sliders(mode, a, b);
                sai_ui::hsv_to_rgb(hsv[0], s, v)
            };
            if sai_ui::channel_slider(ui, "H", &mut h, 359.0, |t| sai_ui::hsv_to_rgb(t * 359.0 / 360.0, 1.0, 1.0)) {
                self.hsv[0] = h / 360.0;
            }
            if sai_ui::channel_slider(ui, "S", &mut sat, 100.0, |t| rgb_of(t, b)) {
                let (s, v) = sai_ui::hsv_from_sliders(mode, sat / 100.0, b);
                self.hsv[1] = s;
                self.hsv[2] = v;
            }
            if sai_ui::channel_slider(ui, if mode == 1 { "L" } else { "V" }, &mut val, 100.0, |t| rgb_of(a, t)) {
                let (s, v) = sai_ui::hsv_from_sliders(mode, a, val / 100.0);
                self.hsv[1] = s;
                self.hsv[2] = v;
            }
        }
        if self.color_views[3] {
            let current = self.color();
            ui.add_space(2.0);
            for r in 0..4 {
                if let Some(c) = sai_ui::mixer_row(ui, &mut self.mixer[r], &mut self.mixer_pos[r], current) {
                    self.set_color(c);
                }
            }
        }
        if self.color_views[4] {
            if self.swatch_sets.len() > 1 {
                let mut pick = None;
                egui::ComboBox::from_id_salt("swatch_set").selected_text(self.swatch_sets[self.swatch_set].0.clone()).width(ui.available_width() - 8.0).show_ui(ui, |ui| {
                    for (k, (name, _)) in self.swatch_sets.iter().enumerate() {
                        if ui.selectable_label(k == self.swatch_set, name).clicked() {
                            pick = Some(k);
                        }
                    }
                });
                if let Some(k) = pick {
                    self.switch_swatch_set(k);
                }
            }
            let labels = [tr("Зарегистрировать цвет"), tr("Удалить цвет")];
            match sai_ui::swatches(ui, &self.swatches, &mut self.swatch_sel, labels, [10.0, 12.0, 14.0][self.win.swatch_size.min(2) as usize]) {
                Some(sai_ui::SwatchAction::Pick(c)) => self.set_color(c),
                Some(sai_ui::SwatchAction::Register(i)) => self.swatches[i] = Some(self.color()),
                Some(sai_ui::SwatchAction::Delete(i)) => self.swatches[i] = None,
                None => {}
            }
        }
        if self.color_views[5] {
            self.scratch_pad(ui);
        }
        ui.add_space(4.0);
        }
        if !self.win.show[3] {
            return;
        }
        // вкладки групп — над общими инструментами, как в SAI 2
        self.tool_group_tabs(ui);
        // Общие инструменты и два цвета.
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                let b = Vec2::splat(24.0);
                ui.horizontal(|ui| {
                    for (g, tip, tool) in [
                        ("select", tr("Выделение (M): Shift — добавить, Alt — вычесть"), Tool::Marquee),
                        ("lasso", tr("Лассо (L): Shift — добавить, Alt — вычесть"), Tool::Lasso),
                        ("wand", tr("Волшебная палочка (W): Shift — добавить, Alt — вычесть"), Tool::Wand),
                    ] {
                        if sai_ui::icon_button(ui, b, self.tool == tool, true, tip, |p, r, c| sai_ui::glyph(p, r, c, g)).clicked() {
                            self.tool = tool;
                        }
                    }
                    if sai_ui::icon_button(ui, b, self.tool == Tool::Fill, true, &keys::hint(tr("Заливка (Alt+Delete — залить выделенное)")), |p, r, c| sai_ui::glyph(p, r, c, "bucket")).clicked() {
                        self.tool = Tool::Fill;
                    }
                    if sai_ui::icon_button(ui, b, self.tool == Tool::Text, true, tr("Текст: щелчок — новая надпись, по текстовому слою — правка"), |p, r, c| sai_ui::glyph(p, r, c, "text")).clicked() {
                        self.tool = Tool::Text;
                    }
                });
                ui.horizontal(|ui| {
                    if sai_ui::icon_button(
                        ui,
                        b,
                        self.tool == Tool::Move,
                        true,
                        &keys::hint(tr("Перемещение слоя или выделенного (Ctrl + перетаскивание, Alt+стрелки)")),
                        |p, r, c| sai_ui::glyph(p, r, c, "move"),
                    )
                    .clicked()
                    {
                        self.tool = Tool::Move;
                    }
                    if sai_ui::icon_button(
                        ui,
                        b,
                        self.tool == Tool::Zoom,
                        true,
                        tr("Масштаб: щелчок — крупнее, Alt+щелчок — мельче"),
                        |p, r, c| sai_ui::glyph(p, r, c, "zoom"),
                    )
                    .clicked()
                    {
                        self.tool = Tool::Zoom;
                    }
                    if sai_ui::icon_button(ui, b, self.tool == Tool::RotateView, true, tr("Поворот вида: тянуть по холсту, двойной щелчок — сброс"), |p, r, c| sai_ui::glyph(p, r, c, "rotate")).clicked() {
                        self.tool = Tool::RotateView;
                    }
                    if sai_ui::icon_button(
                        ui,
                        b,
                        self.tool == Tool::Hand,
                        true,
                        tr("Рука (пробел)"),
                        |p, r, c| sai_ui::glyph(p, r, c, "hand"),
                    )
                    .clicked()
                    {
                        self.tool = Tool::Hand;
                    }
                    if sai_ui::icon_button(
                        ui,
                        b,
                        self.tool == Tool::Picker,
                        true,
                        &keys::hint(tr("Пипетка (Alt)")),
                        |p, r, c| sai_ui::glyph(p, r, c, "picker"),
                    )
                    .clicked()
                    {
                        self.tool = Tool::Picker;
                    }
                });
            });
            // основной и фоновый цвет, обмен
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(52.0, 50.0), egui::Sense::click());
            let p = ui.painter();
            let back = Rect::from_min_size(rect.min + Vec2::new(18.0, 20.0), Vec2::splat(26.0));
            p.rect_filled(back, 0.0, self.back);
            p.rect_stroke(back, 0.0, egui::Stroke::new(1.0_f32, sai_ui::border()));
            let fore = Rect::from_min_size(rect.min + Vec2::new(2.0, 2.0), Vec2::splat(26.0));
            if self.transparent {
                sai_ui::checker(p, fore, 6.5);
            } else {
                p.rect_filled(fore, 0.0, self.color());
            }
            p.rect_stroke(fore, 0.0, egui::Stroke::new(1.0_f32, sai_ui::border()));
            let swap = Rect::from_min_size(rect.min + Vec2::new(34.0, 2.0), Vec2::splat(14.0));
            sai_ui::glyph(p, swap, sai_ui::ink(), "cw");
            let clear = Rect::from_min_size(rect.min + Vec2::new(2.0, 34.0), Vec2::splat(12.0));
            sai_ui::checker(p, clear, 3.0);
            if self.transparent {
                p.rect_stroke(clear.expand(1.0), 0.0, egui::Stroke::new(1.0_f32, sai_ui::select_edge()));
            }
            let at = response.interact_pointer_pos();
            if response.clicked() && at.is_some_and(|q| swap.expand(2.0).contains(q)) {
                self.swap_colors();
            }
            // клетка — «прозрачный цвет» (как в SAI)
            if response.clicked() && at.is_some_and(|q| clear.expand(2.0).contains(q)) {
                self.transparent = !self.transparent;
            }
            response.on_hover_text(tr("Стрелка — поменять цвета; клетка — прозрачный цвет (кисть стирает)"));
        });
        ui.add_space(2.0);
        // Кисти слоя: 4 в ряд, имя сверху, значок снизу справа. Как в SAI: правый щелчок по пустой
        // ячейке — новый инструмент выбранного вида, по инструменту — копия, удаление, свойства.
        let mut grid_action: Option<(usize, GridAction)> = None;
        // в сетке — инструменты открытой группы (номера — общие)
        let shown: Vec<usize> = (0..self.brushes.len()).filter(|&i| self.brushes[i].group == self.tool_group).collect();
        let groups = self.tool_groups.clone();
        egui::Frame::none()
            .stroke(egui::Stroke::new(1.0_f32, sai_ui::border()))
            .fill(sai_ui::gray(255))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let w = (ui.available_width() / 4.0).floor();
                // пустые ячейки — до конца ряда, а если ряд полон — ещё ряд
                let cells = (shown.len() / 4 + 1) * 4;
                egui::ScrollArea::vertical().id_salt("tool_grid").max_height(36.0 * 3.0).auto_shrink([false, true]).show(ui, |ui| {
                    for row in 0..cells / 4 {
                        ui.horizontal(|ui| {
                            for cell in row * 4..row * 4 + 4 {
                                let Some((i, tool)) = shown.get(cell).map(|&i| (i, &self.brushes[i])) else {
                                    let r = sai_ui::icon_button(ui, Vec2::new(w, 36.0), false, true, tr("Правый щелчок — новый инструмент"), |_, _, _| {});
                                    r.context_menu(|ui| {
                                        for k in ToolKind::ALL {
                                            let k_icon = k.new_brush().icon;
                                            if icons::item(ui, k_icon, tr(k.label())).clicked() {
                                                grid_action = Some((0, GridAction::Add(k)));
                                                ui.close_menu();
                                            }
                                        }
                                    });
                                    continue;
                                };
                                let (title, extra, icon, enabled) = (tool.title(), tool.extra.clone(), tool.icon, tool.enabled);
                                let selected = self.tool == Tool::Brush && self.brush == i;
                                let tip = match tool.key.filter(|_| keys::hints()) {
                                    Some(k) => format!("{title} ({})", k.label()),
                                    None => title.clone(),
                                };
                                let r = sai_ui::icon_button(ui, Vec2::new(w, 36.0), selected, enabled, &tip, |p, r, c| {
                                    sai_ui::fit_text(p, r.left_top(), egui::Align2::LEFT_TOP, &title, r.width(), 10.0, c);
                                    if !extra.is_empty() {
                                        sai_ui::fit_text(p, r.left_top() + Vec2::new(0.0, 11.0), egui::Align2::LEFT_TOP, &extra, r.width() - 16.0, 9.0, c);
                                    }
                                    sai_ui::glyph(p, Rect::from_min_size(r.right_bottom() - Vec2::splat(16.0), Vec2::splat(16.0)), c, icon);
                                });
                                if r.clicked() {
                                    self.tool = Tool::Brush;
                                    self.brush = i;
                                }
                                let (has_res, has_undo) = (tool.reserved.is_some(), tool.before_restore.is_some());
                                r.context_menu(|ui| {
                                    for (act, name) in [(GridAction::Duplicate, tr("Копия")), (GridAction::Delete, tr("Удалить")), (GridAction::Property, tr("Свойства…")), (GridAction::Save, tr("Сохранить кисть…"))] {
                                        if ui.button(name).clicked() {
                                            grid_action = Some((i, act));
                                            ui.close_menu();
                                        }
                                    }
                                    if groups.len() > 1 {
                                        ui.menu_button(tr("В группу"), |ui| {
                                            for (g, name) in groups.iter().enumerate() {
                                                if ui.radio(tool.group == g, name).clicked() {
                                                    grid_action = Some((i, GridAction::ToGroup(g)));
                                                    ui.close_menu();
                                                }
                                            }
                                        });
                                    }
                                    ui.separator();
                                    for (act, name, on) in [
                                        (GridAction::Reserve, tr("Запомнить настройки"), true),
                                        (GridAction::Restore, tr("Вернуть запомненные"), has_res),
                                        (GridAction::UndoRestore, tr("Отменить возврат"), has_undo),
                                    ] {
                                        if ui.add_enabled(on, egui::Button::new(name)).clicked() {
                                            grid_action = Some((i, act));
                                            ui.close_menu();
                                        }
                                    }
                                });
                            }
                        });
                    }
                });
            });
        if let Some((i, act)) = grid_action {
            self.grid_action(i, act);
        }
        ui.add_space(4.0);
        let lw = sai_ui::label_w(
            ui,
            64.0,
            &[
                tr("Трансформация"), tr("Допуск"), tr("Режим"), tr("Жесткость"), tr("Размер"), tr("Минимальный"),
                tr("Плотн."), tr("Мин. Плотн."), tr("Смешивание"), tr("Разбавление"), tr("Стойкость"),
                tr("Порог размыт."), tr("   Усил. плотн."), tr("Перспектива"),
            ],
        );
        let w = ui.available_width() - lw - 8.0;
        let mut finish = None;
        if let Some(t) = self.transform.as_mut() {
            sai_ui::row(ui, tr("Трансформация"), lw, true, |ui| {
                use paint_core::resample::Mode;
                let name = |m: Mode| match m {
                    Mode::Nearest => tr("Ближайший"),
                    Mode::Bilinear => tr("Билинейный"),
                    Mode::Bicubic => tr("Бикубический"),
                    Mode::Lanczos2 => tr("Ланцош 2"),
                    Mode::Lanczos3 => tr("Ланцош 3"),
                };
                let before = t.mode;
                egui::ComboBox::from_id_salt("tmode").selected_text(name(t.mode)).width(w - 8.0).show_ui(ui, |ui| {
                    for m in [Mode::Nearest, Mode::Bilinear, Mode::Bicubic, Mode::Lanczos2, Mode::Lanczos3] {
                        ui.selectable_value(&mut t.mode, m, name(m));
                    }
                });
                if t.mode != before {
                    t.dirty = true;
                }
            });
            for (k, name) in [
                (TransformKind::Free, tr("Свободно")),
                (TransformKind::Resize, tr("Изменить размер")),
                (TransformKind::Deform, tr("Деформация")),
                (TransformKind::Rotate, tr("Поворот")),
                (TransformKind::Lattice, tr("Решётка")),
                (TransformKind::Mesh, tr("Сетка")),
            ] {
                ui.radio_value(&mut self.transform_kind, k, name);
            }
            if self.transform_kind.grid().is_some() {
                // деление сетки и складки — как в SAI 2 Alpha 2026
                ui.horizontal(|ui| {
                    ui.label(tr("Деление"));
                    egui::ComboBox::from_id_salt("griddiv").selected_text(format!("{} × {}", self.grid_div[0], self.grid_div[1])).show_ui(ui, |ui| {
                        for n in 2..=5u8 {
                            if ui.selectable_label(self.grid_div == [n, n], format!("{n} × {n}")).clicked() {
                                self.grid_div = [n, n];
                            }
                        }
                    });
                    ui.add(egui::DragValue::new(&mut self.grid_div[0]).range(1..=32));
                    ui.label("×");
                    ui.add(egui::DragValue::new(&mut self.grid_div[1]).range(1..=32));
                });
                ui.horizontal(|ui| {
                    ui.label(tr("Наложение"));
                    ui.radio_value(&mut self.grid_blend, false, tr("Поверх"));
                    ui.radio_value(&mut self.grid_blend, true, tr("Смешать"));
                });
            }
            sai_ui::row(ui, tr("Перспектива"), lw, true, |ui| {
                let mut v = self.transform_perspective as f32;
                if sai_ui::bar(ui, &mut v, 0.0, 100.0, format!("{}", self.transform_perspective), w, true).changed() {
                    self.transform_perspective = v.round().clamp(0.0, 100.0) as u8;
                    t.perspective = self.transform_perspective as f64 / 100.0;
                    t.dirty = true;
                }
            });
            ui.horizontal(|ui| {
                if ui.button(keys::hint(tr("Применить (Enter)"))).clicked() {
                    finish = Some(true);
                }
                if ui.button(keys::hint(tr("Отмена (Esc)"))).clicked() {
                    finish = Some(false);
                }
            });
            ui.add_space(4.0);
        }
        if finish.is_none() && self.transform.as_ref().is_some_and(|t| t.drag.is_none()) {
            self.ensure_grid();
        }
        if let Some(apply) = finish {
            self.finish_transform(apply);
        } else if self.transform.as_ref().is_some_and(|t| t.dirty && t.drag.is_none()) {
            self.update_transform(false);
        }
        if self.doc.layer(&self.selected).is_some_and(|l| l.vector.as_ref().is_some_and(|v| &v.kind == b"shap")) {
            use paint_formats::linework::ShapeKind;
            ui.label(tr("Фигура (протяните на холсте):"));
            ui.horizontal(|ui| {
                for (k, name) in [(ShapeKind::Rectangle, tr("Прямоугольник")), (ShapeKind::Ellipse, tr("Эллипс")), (ShapeKind::Triangle, tr("Треугольник"))] {
                    if ui.selectable_label(self.shape_kind == k, name).clicked() {
                        self.shape_kind = k;
                        self.tool = Tool::Brush;
                    }
                }
            });
            ui.add_space(4.0);
        }
        if self.doc.layer(&self.selected).is_some_and(|l| l.vector.as_ref().is_some_and(|v| &v.kind == b"liwk")) {
            ui.label(tr("Инструменты контура:"));
            ui.horizontal_wrapped(|ui| {
                for (t, name, tip) in [
                    (LineTool::Pen, tr("Перо"), tr("Рисовать кривые")),
                    (LineTool::Eraser, tr("Ластик"), &keys::hint(tr("Удалить путь, которого коснулись (E)"))),
                    (LineTool::Weight, tr("Толщина"), tr("Задать путям размер кисти")),
                    (LineTool::Color, tr("Цвет"), tr("Перекрасить пути в основной цвет")),
                    (LineTool::Edit, tr("Узлы"), &keys::hint(tr("Двигать узлы (I)"))),
                    (LineTool::Pressure, tr("Нажим"), &keys::hint(tr("Нажим узла: тянуть вверх или вниз (P)"))),
                ] {
                    if ui.selectable_label(self.line_tool == t, name).on_hover_text(tip).clicked() {
                        self.line_tool = t;
                        self.tool = Tool::Brush;
                    }
                }
            });
            ui.add_space(4.0);
        }
        if matches!(self.tool, Tool::Wand | Tool::Fill) {
            let tol = &mut self.wand_tolerance;
            sai_ui::row(ui, tr("Допуск"), lw, true, |ui| {
                let mut v = *tol as f32;
                let text = format!("{}", *tol);
                if sai_ui::bar(ui, &mut v, 0.0, 255.0, text, w, true).changed() {
                    *tol = v.round().clamp(0.0, 255.0) as u8;
                }
            });
            self.gap_rows(ui, lw, w);
            ui.add_space(4.0);
        }
        self.tool_buttons(ui);
        if self.tool == Tool::Lasso {
            ui.label(tr("Режим выделения:"));
            ui.radio_value(&mut self.lasso_polygon, false, tr("Свободно"));
            ui.radio_value(&mut self.lasso_polygon, true, tr("Многоугольник"))
                .on_hover_text(tr("Щелчки ставят вершины; Enter или двойной щелчок — закрыть, Esc — отменить"));
            ui.checkbox(&mut self.lasso_aa, tr("Сглаживание края"));
            ui.add_space(4.0);
        }
        if self.tool == Tool::Brush && self.kind() == ToolKind::Gradient {
            self.gradient_panel(ui);
            return;
        }
        if self.tool == Tool::Text {
            self.text_panel(ui, lw, w);
            return;
        }
        self.font_scroll = true;
        if self.tool == Tool::Brush && self.kind() == ToolKind::LassoFill {
            self.lasso_fill_panel(ui, lw, w);
            return;
        }
        if self.tool == Tool::Brush && self.kind() == ToolKind::Bucket {
            let tol = &mut self.wand_tolerance;
            sai_ui::row(ui, tr("Допуск"), lw, true, |ui| {
                let mut v = *tol as f32;
                let text = format!("{}", *tol);
                if sai_ui::bar(ui, &mut v, 0.0, 255.0, text, w, true).changed() {
                    *tol = v.round().clamp(0.0, 255.0) as u8;
                }
            });
            self.gap_rows(ui, lw, w);
            return;
        }
        // настройки кисти — только у кистей (как в SAI: у выделения, перемещения, вида их нет)
        if self.tool != Tool::Brush {
            return;
        }
        if self.grain_names.is_none() {
            use paint_core::paper::names_of;
            self.grain_names = Some((names_of("blotmap"), names_of("brushtex"), names_of("bristle"), paint_core::paper::tip_names()));
        }
        let (blot_names, tex_names, bristle_names, tip_names) = self.grain_names.clone().unwrap_or_default();
        let mut set_dir = false;
        let mut add_tip = false;
        // своя динамика из плагинов: «id/имя» → подпись
        let plugin_dyns: Vec<(String, String)> = self.plugins.iter().flat_map(|p| p.plugin.dynamics.iter().map(move |(n, _)| (format!("{}/{n}", p.plugin.id), format!("{} — {n}", p.plugin.name)))).collect();
        let (size_top, size_numbers) = (self.win.size_list_top, self.win.size_numbers);
        let current_size = self.brushes[self.brush].params.size;
        let mut pick = None;
        if size_top {
            pick = size_grid(ui, current_size, size_numbers, true);
        }
        self.brush_preview_ui(ui, lw + w);
        let mut size_range = self.size_range as usize;
        let tool = &mut self.brushes[self.brush];
        let tool = &mut tool.brush;
        let (brush, grain, dyns, dyn_plugin, anti_hook) = (&mut tool.params, &mut tool.grain, &mut tool.dynamics, &mut tool.dyn_plugin, &mut tool.anti_hook);
        // режим наложения мазка — у пера эффектов и разброса (у остальных в SAI он неактивен)
        let has_mode = brush.effect.is_some();
        sai_ui::row(ui, tr("Режим"), lw, has_mode, |ui| {
            let cur = brush.effect.unwrap_or(BlendMode::Normal);
            let shown = MODES.iter().find(|(m, _)| *m == cur).map_or(tr("Обычный"), |(_, n)| tr(n));
            ui.add_enabled_ui(has_mode, |ui| {
                egui::ComboBox::from_id_salt("paintmode").selected_text(shown).width(w - 8.0).height(500.0).show_ui(ui, |ui| {
                    for (m, n) in MODES {
                        if ui.selectable_label(cur == m, tr(n)).clicked() {
                            brush.effect = Some(m);
                        }
                    }
                });
            });
        });
        sai_ui::row(ui, tr("Жесткость"), lw, true, |ui| {
            ui.spacing_mut().item_spacing.x = 1.0;
            for (i, step) in paint_core::brush::HARDNESS_STEPS.iter().enumerate() {
                let sel = (brush.hardness - step).abs() < 0.01;
                if sai_ui::icon_button(
                    ui,
                    Vec2::new(w / 5.0 - 1.0, 16.0),
                    sel,
                    true,
                    &trf("Ступень {0}", &[&(i + 1)]),
                    |p, r, c| sai_ui::hardness_icon(p, r, i, c),
                )
                .clicked()
                {
                    brush.hardness = *step;
                }
            }
        });
        // размер — в выбранном диапазоне (▼, как в SAI 2: ×0,1 … ×50), полоска линейная
        let ranges = SIZE_RANGES;
        let fits = |r: usize| brush.size >= ranges[r].1 - 1e-3 && brush.size <= ranges[r].2 + 1e-3;
        if !fits(size_range) {
            size_range = (0..ranges.len()).find(|&r| fits(r)).unwrap_or(ranges.len() - 1);
        }
        sai_ui::row(ui, tr("Размер"), lw, true, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            egui::ComboBox::from_id_salt("size_range").selected_text(ranges[size_range].0).width(38.0).show_ui(ui, |ui| {
                for (r, (name, lo, hi)) in ranges.iter().enumerate() {
                    if ui.selectable_label(size_range == r, format!("{name}   ({lo} … {hi})")).clicked() {
                        size_range = r;
                        brush.size = brush.size.clamp(*lo, *hi);
                    }
                }
            })
            .response
            .on_hover_text(tr("Диапазон размера"));
            let (_, lo, hi) = ranges[size_range];
            let text = if brush.size < 10.0 { format!("{:.1}", brush.size) } else { format!("{:.0}", brush.size) };
            let mut v = brush.size;
            if sai_ui::bar(ui, &mut v, lo, hi, text, w - 52.0, true).changed() {
                let step = if hi <= 10.0 { 0.1 } else if hi <= 100.0 { 0.5 } else { 1.0 };
                brush.size = ((v / step).round() * step).clamp(lo, hi);
            }
        });
        self.size_range = size_range as u8;
        // нажим на размер и на плотность — значок пера у строки, как в SAI 2
        let pen_toggle = |ui: &mut egui::Ui, on: &mut bool, tip: &str| {
            if sai_ui::icon_button(ui, Vec2::new(16.0, 16.0), *on, true, tip, |p, r, c| sai_ui::glyph(p, r, c, "pen")).clicked() {
                *on = !*on;
            }
        };
        sai_ui::row(
            ui,
            tr("Минимальный"),
            lw,
            brush.size_by_pressure,
            |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                pen_toggle(ui, &mut brush.size_by_pressure, tr("Нажим меняет размер"));
                let on = brush.size_by_pressure;
                let mut v = brush.min_size * 100.0;
                let text = format!("{:.0}%", v);
                if sai_ui::bar(ui, &mut v, 0.0, 100.0, text, w - 18.0, on).changed() {
                    brush.min_size = (v.round() / 100.0).clamp(0.0, 1.0);
                }
            },
        );
        sai_ui::row(ui, tr("Плотн."), lw, true, |ui| {
            let mut v = brush.density * 100.0;
            let text = format!("{:.0}", v);
            if sai_ui::bar(ui, &mut v, 0.0, 100.0, text, w, true).changed() {
                brush.density = (v.round() / 100.0).clamp(0.0, 1.0);
            }
        });
        sai_ui::row(
            ui,
            tr("Мин. Плотн."),
            lw,
            brush.density_by_pressure,
            |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                pen_toggle(ui, &mut brush.density_by_pressure, tr("Нажим меняет плотность"));
                let on = brush.density_by_pressure;
                let mut v = brush.min_density * 100.0;
                let text = format!("{:.0}%", v);
                if sai_ui::bar(ui, &mut v, 0.0, 100.0, text, w - 18.0, on).changed()
                {
                    brush.min_density = (v.round() / 100.0).clamp(0.0, 1.0);
                }
            },
        );
        if let Some(mix) = brush.mix.as_mut() {
            // Смешивание с холстом (как у Кисти, Акварели и Маркера в SAI).
            let pct = |ui: &mut egui::Ui, label: &str, v: &mut f32| {
                sai_ui::row(ui, label, lw, true, |ui| {
                    let mut t = *v * 100.0;
                    let text = format!("{:.0}", t);
                    if sai_ui::bar(ui, &mut t, 0.0, 100.0, text, w, true).changed() {
                        *v = (t.round() / 100.0).clamp(0.0, 1.0);
                    }
                });
            };
            if mix.smudge {
                // «Окрашивание»: флажок и доля цвета кисти; ниже «Нажима без окраски» — только тянет
                let mut on = mix.smudge_color > 0.0;
                if ui.checkbox(&mut on, tr("Окрашивание")).changed() {
                    mix.smudge_color = if on { 0.2 } else { 0.0 };
                }
                if on {
                    pct(ui, tr("Окрашивание"), &mut mix.smudge_color);
                    pct(ui, tr("Нажим без окраски"), &mut mix.smudge_uncolor);
                }
            } else if mix.blur_only {
                pct(ui, tr("Ширина размытия"), &mut mix.blur_width);
            } else {
                pct(ui, tr("Смешивание"), &mut mix.blending);
                pct(ui, tr("Разбавление"), &mut mix.dilution);
                pct(ui, tr("Стойкость"), &mut mix.persistence);
            }
            if mix.blend_by_pressure && !mix.blur_only && !mix.smudge {
                pct(ui, tr("Порог размыт."), &mut mix.blur_pressure);
            }
        }
        if let Some(sc) = brush.scatter.as_mut() {
            scatter_panel(ui, sc, lw, w);
        }
        // проценты и масштаб 10–500 % — полоской, как в SAI
        let pct_bar = |ui: &mut egui::Ui, label: &str, v: &mut f32, on: bool| {
            sai_ui::row(ui, label, lw, on, |ui| {
                let mut x = *v * 100.0;
                let text = format!("{:.0}", x);
                if sai_ui::bar(ui, &mut x, 0.0, 100.0, text, w, on).changed() {
                    *v = (x.round() / 100.0).clamp(0.0, 1.0);
                }
            });
        };
        let scale_bar = |ui: &mut egui::Ui, v: &mut u16, on: bool| {
            sai_ui::row(ui, tr("   Масштаб"), lw, on, |ui| {
                let mut x = *v as f32;
                let text = format!("{}%", *v);
                if sai_ui::bar(ui, &mut x, 10.0, 500.0, text, w, on).changed() {
                    *v = x.round().clamp(10.0, 500.0) as u16;
                }
            });
        };
        let no_files = |ui: &mut egui::Ui, dir: &mut String, set: &mut bool| {
            ui.label(tr("   Папка SAI (с blotmap и brushtex):"));
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.add(egui::TextEdit::singleline(dir).desired_width(150.0));
                if ui.button("OK").clicked() {
                    *set = true;
                }
            });
        };
        // форма и текстура — главное поле видно всегда, подробности — под треугольником (SAI 2)
        let circle = tr("[ Простой круг ]");
        let combo_w = ((lw + w) * 0.64).max(90.0);
        let form_open = sai_ui::section_row(ui, "sec_form", |ui| {
                let shown = grain.bristle.clone().or_else(|| grain.form.clone()).unwrap_or_else(|| circle.to_string());
                egui::ComboBox::from_id_salt("form").selected_text(shown).width(combo_w).height(400.0).show_ui(ui, |ui| {
                    if ui.selectable_label(grain.form.is_none() && grain.bristle.is_none(), circle).clicked() {
                        grain.form = None;
                        grain.bristle = None;
                    }
                    for n in &blot_names {
                        if ui.selectable_label(grain.form.as_deref() == Some(n), n).clicked() {
                            grain.form = Some(n.clone());
                            grain.bristle = None;
                        }
                    }
                    // как в SAI: после «пятен» — щетины
                    if !bristle_names.is_empty() {
                        ui.separator();
                    }
                    for n in &bristle_names {
                        if ui.selectable_label(grain.bristle.as_deref() == Some(n), n).clicked() {
                            grain.bristle = Some(n.clone());
                            grain.form = None;
                        }
                    }
                });
                // сила формы — поле рядом, как в SAI
                let on = grain.form.is_some();
                let mut v = (grain.form_strength * 100.0).round();
                if ui.add_enabled(on, egui::DragValue::new(&mut v).range(0.0..=100.0).speed(1.0)).on_hover_text(tr("Сила формы")).changed() {
                    grain.form_strength = v / 100.0;
                }
        });
        if form_open {
            sai_ui::row(ui, tr("   Отпечаток"), lw, true, |ui| {
                let round = tr("[ Круг ]");
                egui::ComboBox::from_id_salt("tip").selected_text(grain.tip.clone().unwrap_or_else(|| round.to_string())).width(w - 8.0).height(400.0).show_ui(ui, |ui| {
                    if ui.selectable_label(grain.tip.is_none(), round).clicked() {
                        grain.tip = None;
                    }
                    for n in &tip_names {
                        if ui.selectable_label(grain.tip.as_deref() == Some(n), n).clicked() {
                            grain.tip = Some(n.clone());
                        }
                    }
                    ui.separator();
                    if ui.button(tr("Добавить картинку…")).on_hover_text(tr("PNG, BMP и др.: тёмное на белом или непрозрачное на прозрачном; кисти GIMP .gbr")).clicked() {
                        add_tip = true;
                    }
                });
            });
            pct_bar(ui, tr("   Толщина"), &mut grain.tip_ratio, true);
            grain.tip_ratio = grain.tip_ratio.max(0.01);
            sai_ui::row(ui, tr("   Поворот"), lw, true, |ui| {
                ui.add(egui::DragValue::new(&mut grain.tip_angle).range(-180.0..=180.0).suffix("°").speed(1.0));
                ui.checkbox(&mut grain.tip_follow, tr("по мазку")).on_hover_text(tr("Отпечаток поворачивается по направлению мазка"));
            });
            let cells = grain.tip.as_deref().map_or(0, |n| paint_core::paper::stamps_of(n).len());
            if cells > 1 {
                sai_ui::row(ui, tr("   Набор"), lw, true, |ui| {
                    ui.label(trf("{0} картинок", &[&cells]));
                    ui.checkbox(&mut grain.tip_random, tr("вразнобой")).on_hover_text(tr("Картинки набора сменяются вразнобой, а не по порядку"));
                });
            }
            if grain.bristle.is_some() {
                pct_bar(ui, tr("   Размер волоска"), &mut grain.hair_size, true);
                pct_bar(ui, tr("   Мин. волоска"), &mut grain.hair_min, true);
                pct_bar(ui, tr("   Разброс"), &mut grain.randomize, true);
                sai_ui::row(ui, tr("   Направление"), lw, true, |ui| {
                    let names = [tr("Нет"), tr("Авто"), tr("По перу")];
                    egui::ComboBox::from_id_salt("hairdir").selected_text(names[grain.direction.min(2) as usize]).width(w - 8.0).show_ui(ui, |ui| {
                        for (i, n) in names.iter().enumerate() {
                            ui.selectable_value(&mut grain.direction, i as u8, *n);
                        }
                    });
                });
            } else {
                let on = grain.form.is_some();
                scale_bar(ui, &mut grain.form_scale, on);
            }
            if blot_names.is_empty() {
                no_files(ui, &mut self.paper_dir, &mut set_dir);
            }
        }
        if sai_ui::section(ui, "sec_dyn", tr("Динамика")) {
            use paint_core::dynamics::{DynInput, DynOutput, Dynamic};
            let input_name = |i: DynInput| match i {
                DynInput::Pressure => tr("Нажим"),
                DynInput::Speed => tr("Скорость"),
                DynInput::Direction => tr("Направление"),
                DynInput::Distance => tr("Путь"),
                DynInput::Random => tr("Случайно"),
            };
            let output_name = |o: DynOutput| match o {
                DynOutput::Size => tr("Размер"),
                DynOutput::Density => tr("Плотность"),
                DynOutput::Angle => tr("Поворот"),
                DynOutput::Ratio => tr("Толщина"),
                DynOutput::Scatter => tr("Разброс"),
            };
            let mut remove = None;
            for (n, d) in dyns.iter_mut().enumerate() {
                ui.push_id(("dyn", n), |ui| {
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("in").selected_text(input_name(d.input)).width(w * 0.45).show_ui(ui, |ui| {
                            for i in DynInput::ALL {
                                if ui.selectable_label(d.input == i, input_name(i)).clicked() && d.input != i {
                                    d.input = i;
                                    d.scale = i.default_scale();
                                }
                            }
                        });
                        ui.label("→");
                        egui::ComboBox::from_id_salt("out").selected_text(output_name(d.output)).width(w * 0.45).show_ui(ui, |ui| {
                            for o in DynOutput::ALL {
                                ui.selectable_value(&mut d.output, o, output_name(o));
                            }
                        });
                        if ui.small_button("×").on_hover_text(tr("Удалить связь")).clicked() {
                            remove = Some(n);
                        }
                    });
                    if matches!(d.input, DynInput::Speed | DynInput::Distance) {
                        sai_ui::row(ui, tr("   Масштаб"), lw, true, |ui| {
                            ui.add(egui::DragValue::new(&mut d.scale).range(1.0..=100_000.0).suffix(" px").speed(1.0));
                        });
                    }
                    sai_ui::points_editor(ui, &mut d.curve, w + lw - 8.0, 56.0);
                    ui.add_space(4.0);
                });
            }
            if let Some(n) = remove {
                dyns.remove(n);
            }
            if dyns.len() < 32 && ui.button(tr("+ Связь «вход → кривая → параметр»")).clicked() {
                dyns.push(Dynamic::new(DynInput::Pressure, DynOutput::Size));
            }
            if !plugin_dyns.is_empty() || dyn_plugin.is_some() {
                sai_ui::row(ui, tr("   Из плагина"), lw, true, |ui| {
                    let none = tr("[ Нет ]");
                    let shown = dyn_plugin.as_ref().map(|s| plugin_dyns.iter().find(|(k, _)| k == s).map_or_else(|| s.clone(), |(_, l)| l.clone())).unwrap_or_else(|| none.to_string());
                    egui::ComboBox::from_id_salt("dynplug").selected_text(shown).width(w - 8.0).show_ui(ui, |ui| {
                        if ui.selectable_label(dyn_plugin.is_none(), none).clicked() {
                            *dyn_plugin = None;
                        }
                        for (k, l) in &plugin_dyns {
                            if ui.selectable_label(dyn_plugin.as_deref() == Some(k), l).clicked() {
                                *dyn_plugin = Some(k.clone());
                            }
                        }
                    });
                });
            }
        }
        let (none, paper) = (tr("[ Нет текстуры ]"), tr("[ Бумага слоя ]"));
        let tex_open = sai_ui::section_row(ui, "sec_tex", |ui| {
                let shown = match &grain.tex {
                    TexChoice::None => none.to_string(),
                    TexChoice::Paper => paper.to_string(),
                    TexChoice::File(n) => n.clone(),
                };
                egui::ComboBox::from_id_salt("btex").selected_text(shown).width(combo_w).height(400.0).show_ui(ui, |ui| {
                    if ui.selectable_label(grain.tex == TexChoice::None, none).clicked() {
                        grain.tex = TexChoice::None;
                    }
                    if ui.selectable_label(grain.tex == TexChoice::Paper, paper).clicked() {
                        grain.tex = TexChoice::Paper;
                    }
                    ui.separator();
                    for n in &tex_names {
                        let sel = matches!(&grain.tex, TexChoice::File(f) if f == n);
                        if ui.selectable_label(sel, n).clicked() {
                            grain.tex = TexChoice::File(n.clone());
                        }
                    }
                });
                let on = grain.tex != TexChoice::None;
                let mut v = (grain.tex_strength * 100.0).round();
                if ui.add_enabled(on, egui::DragValue::new(&mut v).range(0.0..=100.0).speed(1.0)).on_hover_text(tr("Сила текстуры")).changed() {
                    grain.tex_strength = v / 100.0;
                }
        });
        if tex_open {
            let on = grain.tex != TexChoice::None;
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.add_enabled(on, egui::Checkbox::new(&mut grain.by_min_density, tr("Зависит от мин. плотн.")))
                    .on_hover_text(tr("Как «Affect by MinDens» в SAI: у тонкой кисти и при слабом нажиме текстура слабее"));
            });
            // у бумаги слоя масштаб свой — из слоя
            scale_bar(ui, &mut grain.tex_scale, matches!(grain.tex, TexChoice::File(_)));
            if tex_names.is_empty() {
                no_files(ui, &mut self.paper_dir, &mut set_dir);
            }
        }
        // «Разное»: справа — вернуть запомненные и запомнить настройки (значки, как в SAI 2)
        let mut misc_act = None;
        let misc_open = sai_ui::section_row(ui, "sec_misc", |ui| {
            ui.label(egui::RichText::new(tr("Разное")).size(12.0).color(sai_ui::header()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if sai_ui::icon_button(ui, Vec2::splat(18.0), false, true, tr("Запомнить настройки"), |p, r, c| sai_ui::glyph(p, r, c, "save")).clicked() {
                    misc_act = Some(GridAction::Reserve);
                }
                if sai_ui::icon_button(ui, Vec2::splat(18.0), false, true, tr("Вернуть запомненные"), |p, r, c| sai_ui::glyph(p, r, c, "undo")).clicked() {
                    misc_act = Some(GridAction::Restore);
                }
            });
        });
        if misc_open {
            sai_ui::row(ui, tr("   Усил. плотн."), lw, true, |ui| {
                let mut v = brush.amplify * 100.0;
                let text = format!("{:.0}", v);
                if sai_ui::bar(ui, &mut v, 0.0, 100.0, text, w, true).changed() {
                    brush.amplify = (v.round() / 100.0).clamp(0.0, 1.0);
                }
            });
            ui.checkbox(&mut brush.ver1_density, tr("Нажим на плотность как в версии 1"));
            sai_ui::row(ui, tr("   Срезать крючки"), lw, true, |ui| {
                let mut v = *anti_hook;
                let text = if v > 0.0 { format!("{v:.0} px") } else { tr("нет").to_string() };
                if sai_ui::bar(ui, &mut v, 0.0, 30.0, text, w, true).on_hover_text(tr("Длина загибов на концах мазка, которые срезаются (точки экрана)")).changed() {
                    *anti_hook = v.round().clamp(0.0, 30.0);
                }
            });
            sai_ui::row(ui, "   Sharpness", lw, true, |ui| {
                let mut v = brush.sharpness * 100.0;
                let text = format!("{:.0}", v);
                if sai_ui::bar(ui, &mut v, 0.0, 100.0, text, w, true).changed() {
                    brush.sharpness = (v.round() / 100.0).clamp(0.0, 1.0);
                }
            });
        }
        if let Some(mix) = brush.mix.as_mut() {
            ui.checkbox(&mut mix.blend_by_pressure, tr("Нажим на смешивание"));
        }
        // Сетка размеров: кружок и число, 5 в ряд (или над настройками — см. выше).
        if !size_top {
            pick = size_grid(ui, current_size, size_numbers, false);
        }
        if let Some(size) = pick {
            self.brushes[self.brush].params.size = size;
        }
        if let Some(a) = misc_act {
            self.grid_action(self.brush, a);
        }
        if set_dir {
            self.set_paper_dir();
        }
        if add_tip {
            let dir = self.file.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default().join("x"));
            if !self.native_dialog(files::Ask::Tip, &dir) {
                self.status = String::from(tr("Перетащите картинку в окно с Shift — она станет отпечатком кисти"));
            }
        }
    }

    /// Кисти Photoshop `.abr` — каждая отдельным отпечатком в библиотеке; первая — у текущей кисти.
    fn add_abr(&mut self, path: &std::path::Path) {
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let list = match std::fs::read(path).map_err(Into::into).and_then(|d| paint_formats::brush_tip::abr(&d)) {
            Ok(l) => l,
            Err(e) => {
                self.status = trf("Кисть Photoshop не открылась: {0}", &[&e]);
                return;
            }
        };
        let mut first = None;
        let mut n = 0;
        for (name, st) in &list {
            if let Ok(saved) = paint_core::paper::save_tip(&format!("{stem} {name}"), st) {
                first.get_or_insert(saved);
                n += 1;
            }
        }
        if let Some(f) = first {
            self.brushes[self.brush].grain.tip = Some(f);
        }
        self.grain_names = None;
        self.status = trf("Отпечатков из кистей Photoshop: {0}; выбрать — «Форма → Отпечаток»", &[&n]);
    }

    /// Картинка или кисть GIMP — новый отпечаток текущего инструмента (копия в своей папке `tips`).
    fn add_tip(&mut self, path: &std::path::Path) {
        if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("abr")) {
            return self.add_abr(path);
        }
        let saved = paint_formats::brush_tip::load_all(path).and_then(|(name, cells, random)| Ok((paint_core::paper::save_tips(&name, &cells)?, random)));
        match saved {
            Ok((name, random)) => {
                self.brushes[self.brush].grain.tip = Some(name.clone());
                self.brushes[self.brush].grain.tip_random = random;
                self.grain_names = None;
                self.status = trf("Отпечаток «{0}» добавлен", &[&name]);
            }
            Err(e) => self.status = trf("Картинка не открылась: {0}", &[&e]),
        }
    }

    /// Настройки градиента, как в SAI 2017: форма, цвета, «Обратный», «S-кривая», OK и «Отмена».
    fn gradient_panel(&mut self, ui: &mut egui::Ui) {
        let o = &mut self.grad_opts;
        ui.label(tr("Форма"));
        ui.radio_value(&mut o.circle, false, tr("Прямой"));
        ui.radio_value(&mut o.circle, true, tr("Круг"));
        ui.add_space(4.0);
        ui.label(tr("Цвет"));
        ui.radio_value(&mut o.to_transparent, false, tr("Основной → фоновый"));
        ui.radio_value(&mut o.to_transparent, true, tr("Основной → прозрачность"));
        ui.add_space(4.0);
        ui.checkbox(&mut o.reverse, tr("Обратный"));
        ui.checkbox(&mut o.s_curve, tr("S-кривая"));
        ui.add_space(4.0);
        let editing = self.gradient.is_some();
        ui.horizontal(|ui| {
            if ui.add_enabled(editing, egui::Button::new(keys::hint(tr("Применить (Enter)")))).clicked() {
                self.finish_gradient(true);
            }
            if ui.add_enabled(editing, egui::Button::new(keys::hint(tr("Отмена (Esc)")))).clicked() {
                self.finish_gradient(false);
            }
        });
        if !editing {
            ui.label(egui::RichText::new(tr("Протяните по холсту; концы можно двигать до «Применить».")).small().weak());
        }
    }

    fn layer_rows(
        &mut self,
        ui: &mut egui::Ui,
        path: &mut Vec<usize>,
        depth: usize,
        changed: &mut bool,
    ) {
        let count = match path.as_slice() {
            [] => self.doc.layers.len(),
            p => self
                .doc
                .layer(p)
                .and_then(|l| l.children.as_ref())
                .map_or(0, Vec::len),
        };
        // В списке верхний слой — первым.
        for i in (0..count).rev() {
            path.push(i);
            let Some(layer) = self.doc.layer(path) else {
                path.pop();
                continue;
            };
            let is_folder = layer.children.is_some();
            let (name, visible, clipping) = (layer.name.clone(), layer.visible, layer.clipping);
            let mode = MODES
                .iter()
                .find(|(m, _)| *m == layer.mode)
                .map_or(tr("Обычный"), |(_, n)| tr(n));
            let mode = if is_folder && layer.pass_through {
                tr("Насквозь")
            } else {
                mode
            };
            let opacity = format!("{}%", (layer.opacity * 100.0).round());
            let selected = self.selected == *path;
            let fid = ui.make_persistent_id(("folder", path.clone()));
            self.layers_ui = Some(ui.id());
            let mut open = ui.data_mut(|d| *d.get_persisted_mut_or(fid, false));
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 42.0), egui::Sense::click_and_drag());
            self.layer_rects.push((path.clone(), rect, is_folder));
            let p = ui.painter();
            p.rect_filled(
                rect,
                0.0,
                if selected {
                    sai_ui::select_bg()
                } else {
                    sai_ui::gray(255)
                },
            );
            p.rect_stroke(
                rect.shrink(0.5),
                0.0,
                egui::Stroke::new(
                    1.0_f32,
                    if selected {
                        sai_ui::select_edge()
                    } else {
                        sai_ui::gray(200)
                    },
                ),
            );
            let x0 = rect.left() + 3.0 + depth as f32 * 10.0;
            let eye = Rect::from_min_size(Pos2::new(x0, rect.top() + 3.0), Vec2::new(18.0, 17.0));
            p.rect_stroke(eye, 0.0, egui::Stroke::new(1.0_f32, sai_ui::border()));
            if visible {
                sai_ui::glyph(p, eye.shrink(2.0), sai_ui::ink(), "eye");
            }
            if is_folder && self.doc.layer(path).is_some_and(|l| l.marked) {
                // отметка папки — уголок в окошке глаза
                let c = eye.right_top();
                p.add(egui::Shape::convex_polygon(vec![c, c + Vec2::new(0.0, 7.0), c + Vec2::new(-7.0, 0.0)], sai_ui::select_edge(), egui::Stroke::NONE));
            }
            let mark = Rect::from_min_size(Pos2::new(x0, rect.top() + 22.0), Vec2::new(18.0, 17.0));
            p.rect_stroke(mark, 0.0, egui::Stroke::new(1.0_f32, sai_ui::border()));
            if selected && !is_folder {
                sai_ui::glyph(p, mark.shrink(3.0), sai_ui::ink(), "pen");
            }
            let thumb =
                Rect::from_min_size(Pos2::new(x0 + 22.0, rect.top() + 3.0), Vec2::splat(36.0));
            if is_folder {
                sai_ui::glyph(p, thumb.shrink(6.0), sai_ui::gray(90), "new_folder");
                let arrow = if open { "▾" } else { "▸" };
                p.text(
                    thumb.left_bottom() + Vec2::new(2.0, -2.0),
                    egui::Align2::LEFT_BOTTOM,
                    arrow,
                    egui::FontId::proportional(12.0),
                    sai_ui::ink(),
                );
            } else {
                p.rect_filled(thumb, 0.0, Color32::WHITE);
                if !self.thumbs.contains_key(path.as_slice()) {
                    if let Some(img) = layer_thumbnail(&self.doc, path, 36) {
                        let t = ui.ctx().load_texture(format!("thumb-{path:?}"), img, egui::TextureOptions::LINEAR);
                        self.thumbs.insert(path.clone(), t);
                    }
                }
                if let Some(t) = self.thumbs.get(path.as_slice()) {
                    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                    ui.painter().image(t.id(), thumb, uv, Color32::WHITE);
                }
                let p = ui.painter();
                p.rect_stroke(
                    thumb,
                    0.0,
                    egui::Stroke::new(1.0_f32, sai_ui::gray(190)),
                );
            }
            // маска — вторая миниатюра справа от первой
            let layer_thumb = thumb;
            let has_mask = layer_has_mask(&self.doc, path);
            let thumb = if has_mask {
                let mt = Rect::from_min_size(Pos2::new(thumb.right() + 3.0, thumb.top()), thumb.size());
                let key = { let mut k = path.clone(); k.push(usize::MAX); k };
                if !self.thumbs.contains_key(&key) {
                    if let Some(img) = mask_thumbnail(&self.doc, path, 36) {
                        let t = ui.ctx().load_texture(format!("mask-{path:?}"), img, egui::TextureOptions::LINEAR);
                        self.thumbs.insert(key.clone(), t);
                    }
                }
                if let Some(t) = self.thumbs.get(&key) {
                    ui.painter().image(t.id(), mt, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                }
                ui.painter().rect_stroke(mt, 0.0, egui::Stroke::new(1.0_f32, sai_ui::gray(190)));
                mt
            } else {
                thumb
            };
            if clipping {
                p.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(layer_thumb.left() - 3.0, rect.top() + 3.0),
                        Vec2::new(2.0, 36.0),
                    ),
                    0.0,
                    Color32::from_rgb(200, 60, 60),
                );
            }
            let tx = thumb.right() + 4.0;
            p.text(
                Pos2::new(tx, rect.top() + 3.0),
                egui::Align2::LEFT_TOP,
                &name,
                egui::FontId::proportional(12.0),
                sai_ui::ink(),
            );
            p.text(
                Pos2::new(tx, rect.top() + 18.0),
                egui::Align2::LEFT_TOP,
                mode,
                egui::FontId::proportional(10.0),
                sai_ui::ink(),
            );
            p.text(
                Pos2::new(tx, rect.top() + 29.0),
                egui::Align2::LEFT_TOP,
                &opacity,
                egui::FontId::proportional(10.0),
                sai_ui::ink(),
            );
            if response.clicked() {
                let at = response.interact_pointer_pos().unwrap_or_default();
                if eye.contains(at) && is_folder && ui.input(|i| i.modifiers.command) {
                    // Ctrl + щелчок по глазу папки — отметка для команд видимости (как в SAI)
                    if let Some(l) = self.doc.layer_mut(path) {
                        l.marked = !l.marked;
                    }
                } else if eye.contains(at) {
                    if let Some(l) = self.doc.layer_mut(path) {
                        l.visible = !l.visible;
                        *changed = true;
                    }
                } else if !is_folder && thumb.contains(at) && ui.input(|i| i.modifiers.command) {
                    let m = ui.input(|i| i.modifiers);
                    self.select_by_opacity(&path.to_vec(), Combine::from_modifiers(m.shift, m.alt));
                } else if is_folder && thumb.contains(at) {
                    open = !open;
                    ui.data_mut(|d| d.insert_persisted(fid, open));
                } else {
                    if self.transform.is_some() {
                        self.finish_transform(true);
                    }
                    if self.filter.is_some() {
                        self.finish_filter(true);
                    }
                    self.selected = path.clone();
                    // щелчок по миниатюре маски — рисовать в маске, по остальному — в слое
                    self.mask_edit = has_mask && thumb.contains(at);
                }
            }
            if selected && has_mask {
                let active = if self.mask_edit { thumb } else { layer_thumb };
                ui.painter().rect_stroke(active.expand(1.0), 0.0, egui::Stroke::new(2.0_f32, sai_ui::select_edge()));
            }
            if response.drag_started() {
                let at = response.interact_pointer_pos().unwrap_or_default();
                if !eye.contains(at) {
                    self.layer_drag = Some(LayerDrag { path: path.clone(), width: rect.width() });
                }
            }
            // Схваченный слой (и всё внутри схваченной папки) бледнеет на старом месте.
            if self.layer_drag.as_ref().is_some_and(|d| path.starts_with(&d.path)) {
                let p = ui.painter();
                p.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(236, 236, 240, 190));
                if self.layer_drag.as_ref().is_some_and(|d| d.path == *path) {
                    sai_ui::dashed_rect(p, rect.shrink(1.0), egui::Stroke::new(1.0_f32, sai_ui::select_edge()));
                }
            }
            if response.double_clicked() && is_folder {
                open = !open;
                ui.data_mut(|d| d.insert_persisted(fid, open));
            } else if response.double_clicked() && self.opts.dbl_props {
                // двойной щелчок по слою — «Свойства слоя» (имя), как в SAI
                self.layer_props = self.doc.layer(path).map(|l| (path.clone(), l.name.clone()));
            }
            if is_folder && open {
                self.layer_rows(ui, path, depth + 1, changed);
            }
            path.pop();
        }
    }

    /// Навигатор: уменьшенный холст из тех же текстур, что и на экране, и рамка видимой части.
    /// Навигатор, как в SAI 2 (сверено на настоящей Windows): весь холст на сером поле, рамка вида
    /// тёмной линией с чёрточкой от середины к верхнему краю (поворачивается с видом); протяжка из
    /// любой точки двигает вид на столько же, щелчок ничего не делает.
    fn navigator(&mut self, ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), 150.0),
            egui::Sense::click_and_drag(),
        );
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, sai_ui::gray(176));
        let (w, h) = (self.doc.width as f32, self.doc.height as f32);
        let k = ((rect.width() - 6.0) / w).min((rect.height() - 6.0) / h);
        let origin = rect.center() - Vec2::new(w, h) * k / 2.0;
        let canvas = Rect::from_min_size(origin, Vec2::new(w, h) * k);
        p.rect_filled(canvas, 0.0, Color32::WHITE);
        self.update_nav(ui.ctx(), k * self.ppp);
        if let Some((_, _, _, tex)) = &self.nav {
            p.image(tex.id(), canvas, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        if self.view.is_positive() {
            // углы вида на холсте (с поворотом и зеркалом) → навигатор
            let xf = ViewXf::new(self.view, self.zoom, self.offset, self.view_angle, self.view_flip);
            let v = self.view;
            let nav = |q: Pos2| origin + xf.to_canvas(q) * k;
            let corners = [v.left_top(), v.right_top(), v.right_bottom(), v.left_bottom()].map(nav);
            let frame = egui::Stroke::new(1.0_f32, sai_ui::gray(90));
            p.add(egui::Shape::closed_line(corners.to_vec(), frame));
            let top = Pos2::new(v.center().x, v.top());
            p.line_segment([nav(v.center()), nav(top)], frame);
        }
        if response.dragged() {
            let d = response.drag_delta() / k;
            if d != Vec2::ZERO {
                self.offset += d;
            }
        }
        p.rect_stroke(rect, 0.0, egui::Stroke::new(1.0_f32, sai_ui::border()));
    }

    /// Пересчитать миниатюру навигатора: целиком (сменились масштаб или холст) или только места
    /// изменённых плиток. Точка миниатюры — среднее точек холста под ней.
    fn update_nav(&mut self, ctx: &egui::Context, kp: f32) {
        let (w, h) = (self.doc.width as usize, self.doc.height as usize);
        let size = [((w as f32 * kp).round() as usize).max(1), ((h as f32 * kp).round() as usize).max(1)];
        let fresh = self.nav.as_ref().is_none_or(|n| (n.0 - kp).abs() > 1e-4 || n.1 != size);
        if !fresh && self.nav_dirty.is_empty() {
            return;
        }
        let (sx, sy) = (w as f32 / size[0] as f32, h as f32 / size[1] as f32);
        let mut img = if fresh { vec![Color32::TRANSPARENT; size[0] * size[1]] } else { self.nav.as_ref().map(|n| n.2.clone()).unwrap_or_default() };
        // какие точки миниатюры пересчитать
        let mut spans: Vec<(usize, usize, usize, usize)> = Vec::new();
        if fresh {
            spans.push((0, 0, size[0], size[1]));
        } else {
            for &(tx, ty) in &self.nav_dirty {
                let (x0, y0) = ((tx * TILE) as f32 / sx, (ty * TILE) as f32 / sy);
                let (x1, y1) = (((tx + 1) * TILE) as f32 / sx, ((ty + 1) * TILE) as f32 / sy);
                spans.push((x0.floor().max(0.0) as usize, y0.floor().max(0.0) as usize, (x1.ceil() as usize).min(size[0]), (y1.ceil() as usize).min(size[1])));
            }
        }
        self.nav_dirty.clear();
        let display = &self.display;
        let px = |x: usize, y: usize| -> [u32; 4] {
            let (dx, dy) = ((x as i32) / DT, (y as i32) / DT);
            display.get(&(dx, dy)).map_or([0; 4], |t| {
                let c = t.pixels[((y as i32 % DT) * DT + x as i32 % DT) as usize];
                [c.r() as u32, c.g() as u32, c.b() as u32, c.a() as u32]
            })
        };
        for (u0, v0, u1, v1) in spans {
            for v in v0..v1 {
                let (ya, yb) = ((v as f32 * sy) as usize, (((v + 1) as f32 * sy).ceil() as usize).min(h).max((v as f32 * sy) as usize + 1));
                for u in u0..u1 {
                    let (xa, xb) = ((u as f32 * sx) as usize, (((u + 1) as f32 * sx).ceil() as usize).min(w).max((u as f32 * sx) as usize + 1));
                    // при сильном уменьшении — не больше 8×8 проб на точку
                    let (stx, sty) = ((xb - xa).div_ceil(8).max(1), (yb - ya).div_ceil(8).max(1));
                    let (mut acc, mut n) = ([0u32; 4], 0u32);
                    for y in (ya..yb).step_by(sty) {
                        for x in (xa..xb).step_by(stx) {
                            let c = px(x, y);
                            for i in 0..4 {
                                acc[i] += c[i];
                            }
                            n += 1;
                        }
                    }
                    let n = n.max(1);
                    img[v * size[0] + u] = Color32::from_rgba_premultiplied((acc[0] / n) as u8, (acc[1] / n) as u8, (acc[2] / n) as u8, (acc[3] / n) as u8);
                }
            }
        }
        let image = egui::ColorImage { size, pixels: img.clone() };
        match self.nav.as_mut() {
            Some(n) if !fresh => {
                n.3.set(image, egui::TextureOptions::LINEAR);
                n.2 = img;
            }
            _ => {
                let tex = ctx.load_texture("navigator", image, egui::TextureOptions::LINEAR);
                self.nav = Some((kp, size, img, tex));
            }
        }
    }

    fn right_panel(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        let show_nav = self.win.show[0];
        if show_nav {
            self.navigator(ui);
        }
        let lw = sai_ui::label_w(ui, 44.0, &[tr("Масштаб"), tr("Угол")]);
        let lw_fx = sai_ui::label_w(
            ui,
            lw + 10.0,
            &[tr("   Эффект"), tr("   Ширина"), tr("   Сила"), tr("   Порог"), tr("   Выбрать"), tr("   Масштаб")],
        ) - 10.0;
        let b = Vec2::splat(18.0);
        if show_nav {
        // как навигатор SAI: подпись и значение над линией с треугольным бегунком
        let wn = ui.available_width() - 3.0 * 20.0 - 4.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let mut z = self.zoom.ln();
            let text = format!("{:.0}%", self.zoom * self.ppp * 100.0);
            if sai_ui::nav_slider(ui, tr("Масштаб"), &mut z, 0.02f32.ln(), 32f32.ln(), text, wn).changed() {
                self.set_zoom(z.exp());
            }
            if sai_ui::icon_button(ui, b, false, true, tr("Уменьшить"), |p, r, c| {
                sai_ui::glyph(p, r, c, "minus")
            })
            .clicked()
            {
                self.set_zoom(self.zoom / 1.25);
            }
            if sai_ui::icon_button(ui, b, false, true, tr("Увеличить"), |p, r, c| {
                sai_ui::glyph(p, r, c, "plus")
            })
            .clicked()
            {
                self.set_zoom(self.zoom * 1.25);
            }
            if sai_ui::icon_button(ui, b, false, true, "100%", |p, r, c| {
                sai_ui::glyph(p, r, c, "reset")
            })
            .clicked()
            {
                self.set_zoom(1.0 / self.ppp);
            }
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let mut a = self.view_angle.to_degrees();
            let text = format!("{a:+.0}°");
            if sai_ui::nav_slider(ui, tr("Угол"), &mut a, -180.0, 180.0, text, wn).changed() {
                self.view_angle = a.round().to_radians();
            }
            for (g, tip, step) in [("ccw", tr("Повернуть вид против часовой"), -15.0), ("cw", tr("Повернуть вид по часовой"), 15.0), ("reset", tr("Сбросить угол вида"), 0.0)] {
                if sai_ui::icon_button(ui, b, false, true, tip, |p, r, c| sai_ui::glyph(p, r, c, g)).clicked() {
                    self.rotate_view(step);
                }
            }
        });
        ui.add_space(3.0);
        }
        if !self.win.show[1] {
            return;
        }
        if sai_ui::section(ui, "sec_effect", tr("Эффекты слоя")) {
            changed |= self.effect_panel(ui, lw_fx);
        }
        if sai_ui::section(ui, "sec_paper", tr("Текстура")) {
            changed |= self.paper_panel(ui, lw_fx);
        }
        let lw2 = sai_ui::label_w(ui, 46.0, &[tr("Режим"), tr("Непроз."), tr("Замок")]);
        let w2 = ui.available_width() - lw2 - 8.0;
        let path = self.selected.clone();
        if let Some(layer) = self.doc.layer_mut(&path) {
            let current = MODES
                .iter()
                .find(|(m, _)| *m == layer.mode)
                .map_or(tr("Обычный"), |(_, n)| tr(n));
            sai_ui::row(ui, tr("Режим"), lw2, true, |ui| {
                egui::ComboBox::from_id_salt("mode")
                    .selected_text(current)
                    .width(w2 - 4.0)
                    .height(420.0)
                    .show_ui(ui, |ui| {
                        for (mode, name) in MODES {
                            changed |= ui.selectable_value(&mut layer.mode, mode, tr(name)).changed();
                        }
                    });
            });
            sai_ui::row(ui, tr("Непроз."), lw2, true, |ui| {
                let mut v = layer.opacity * 100.0;
                let text = format!("{:.0}%", v);
                if sai_ui::bar(ui, &mut v, 0.0, 100.0, text, w2, true).changed() {
                    layer.opacity = v.round() / 100.0;
                    changed = true;
                }
            });
            sai_ui::row(ui, tr("Замок"), lw2, true, |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if sai_ui::icon_button(
                    ui,
                    b,
                    layer.preserve_opacity,
                    true,
                    tr("Сохранять непрозрачность"),
                    |p, r, c| sai_ui::glyph(p, r, c, "lock_alpha"),
                )
                .clicked()
                {
                    layer.preserve_opacity = !layer.preserve_opacity;
                }
                for (g, tip, which) in [
                    ("lock_paint", tr("Запрет рисования"), 0),
                    ("move", tr("Запрет перемещения"), 1),
                    ("lock_all", tr("Запрет всего"), 2),
                ] {
                    let on = [layer.lock_paint, layer.lock_move, layer.lock_all][which];
                    if sai_ui::icon_button(ui, b, on, true, tip, |p, r, c| sai_ui::glyph(p, r, c, g)).clicked() {
                        match which {
                            0 => layer.lock_paint = !on,
                            1 => layer.lock_move = !on,
                            _ => layer.lock_all = !on,
                        }
                        changed = true;
                    }
                }
            });
            changed |= ui.checkbox(&mut layer.clipping, tr("Прикрепление")).changed();
            if layer.children.is_some() {
                changed |= ui
                    .checkbox(&mut layer.pass_through, tr("Насквозь (папка)"))
                    .changed();
            }
            if ui.radio(layer.selection_source, tr("Источник выбора")).on_hover_text(tr("Палочка и заливка берут цвет со всех слоёв-источников")).clicked() {
                layer.selection_source = !layer.selection_source;
                changed = true;
            }
        }
        ui.add_space(3.0);
        let b2 = Vec2::splat(22.0);
        ui.horizontal(|ui| {
            if sai_ui::icon_button(ui, b2, false, true, tr("Новый слой"), |p, r, c| {
                sai_ui::glyph(p, r, c, "new_layer")
            })
            .clicked()
            {
                self.add_layer(false);
            }
            if sai_ui::icon_button(ui, b2, false, true, tr("Новый слой контура"), |p, r, c| sai_ui::glyph(p, r, c, "new_linework")).clicked() {
                self.add_linework_layer();
            }
            if sai_ui::icon_button(ui, b2, false, true, tr("Новая папка"), |p, r, c| {
                sai_ui::glyph(p, r, c, "new_folder")
            })
            .clicked()
            {
                self.add_layer(true);
            }
            // как в SAI: кнопка раскрывает выбор числа точек схода
            let persp = self.ruler.visible && matches!(self.ruler.kind, ruler::Kind::Perspective(_));
            let resp = sai_ui::icon_button(ui, b2, persp, true, tr("Линейка перспективы"), |p, r, c| sai_ui::glyph(p, r, c, "ruler"));
            let popup = ui.make_persistent_id("persp_popup");
            if resp.clicked() {
                ui.memory_mut(|m| m.toggle_popup(popup));
            }
            egui::popup_below_widget(ui, popup, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                ui.set_min_width(170.0);
                for n in 1..=3u8 {
                    let label = match n {
                        1 => tr("Перспектива: 1 точка"),
                        2 => tr("Перспектива: 2 точки"),
                        _ => tr("Перспектива: 3 точки"),
                    };
                    if ui.selectable_label(persp && self.ruler.kind == ruler::Kind::Perspective(n), label).clicked() {
                        self.show_perspective(n);
                    }
                }
                if persp && ui.button(tr("Спрятать линейку")).clicked() {
                    self.ruler.visible = false;
                }
            });
            let masked = self.doc.layer(&self.selected).is_some_and(|l| l.mask.is_some());
            if sai_ui::icon_button(ui, b2, false, !masked, tr("Новая маска слоя"), |p, r, c| sai_ui::glyph(p, r, c, "mask")).clicked() {
                self.mask_command(0);
            }
        });
        ui.horizontal(|ui| {
            for (g, tip, action) in [
                ("down", &keys::hint(tr("Перенести на нижний (F)")), LayerAction::Transfer),
                ("merge", &keys::hint(tr("Слить с нижним (Ctrl+E)")), LayerAction::Merge),
                ("clear", &keys::hint(tr("Очистить слой (Delete)")), LayerAction::Clear),
                ("delete", &tr("Удалить слой").to_string(), LayerAction::Delete),
            ] {
                if sai_ui::icon_button(ui, b2, false, true, tip, |p, r, c| sai_ui::glyph(p, r, c, g)).clicked() {
                    self.layer_action(action);
                }
            }
            if sai_ui::icon_button(ui, b2, false, true, tr("Слой выше"), |p, r, c| sai_ui::glyph(p, r, c, "up")).clicked() {
                self.layer_action(LayerAction::Up);
            }
            if sai_ui::icon_button(ui, b2, false, true, tr("Слой ниже"), |p, r, c| sai_ui::glyph(p, r, c, "lower")).clicked() {
                self.layer_action(LayerAction::Down);
            }
        });
        egui::Frame::none()
            .fill(sai_ui::field())
            .stroke(egui::Stroke::new(1.0_f32, sai_ui::border()))
            .inner_margin(2.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("layer_list")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        let mut path = Vec::new();
                        self.layer_rects.clear();
                        self.layer_rows(ui, &mut path, 0, &mut changed);
                        changed |= self.layer_drop(ui);
                    });
            });
        if changed {
            self.dirty = true;
            self.refresh_all();
        }
    }

    /// Куда упадёт перетаскиваемый слой: стопка и место в ней (как в `move_layer_to`) и вид
    /// подсказки. Верх строки — над слоем, низ — под ним, середина папки — внутрь папки (наверх).
    /// Выше первой строки — на самый верх, ниже последней — в самый низ. `None` — бросить некуда
    /// (в саму себя или на то же место).
    fn layer_drop_target(&self, at: Pos2, list: Rect) -> Option<(Vec<usize>, usize, DropMark)> {
        let from = &self.layer_drag.as_ref()?.path;
        let (first, last) = (self.layer_rects.first()?, self.layer_rects.last()?);
        let found = self.layer_rects.iter().find(|(_, r, _)| r.y_range().contains(at.y));
        let (parent, index, mark) = match found {
            Some((path, rect, folder)) => {
                let (i, parent) = path.split_last()?;
                let t = (at.y - rect.top()) / rect.height();
                if *folder && (0.25..0.75).contains(&t) && !path.starts_with(from) {
                    let n = self.doc.layer(path)?.children.as_ref()?.len();
                    (path.clone(), n, DropMark::Into(*rect))
                } else if t < 0.5 {
                    (parent.to_vec(), i + 1, DropMark::Line(rect.left(), rect.right(), rect.top()))
                } else {
                    (parent.to_vec(), *i, DropMark::Line(rect.left(), rect.right(), rect.bottom()))
                }
            }
            None if at.y < first.1.top() && at.y > list.top() - 30.0 => {
                let r = first.1;
                (Vec::new(), self.doc.layers.len(), DropMark::Line(r.left(), r.right(), r.top()))
            }
            None if at.y > last.1.bottom() && at.y < list.bottom() + 30.0 => {
                let r = last.1;
                (Vec::new(), 0, DropMark::Line(list.left() + 1.0, list.right() - 1.0, r.bottom()))
            }
            None => return None,
        };
        let (fi, fp) = from.split_last()?;
        if parent.starts_with(from) || (parent == fp && (index == *fi || index == fi + 1)) {
            return None;
        }
        Some((parent, index, mark))
    }

    /// Перетаскивание слоя в списке: схваченная строка бледнеет, её копия едет за курсором,
    /// место вставки — полоса с засечками или подсвеченная папка; у краёв список прокручивается;
    /// Esc отменяет. Перенос — при отпускании.
    fn layer_drop(&mut self, ui: &mut egui::Ui) -> bool {
        let Some(drag) = self.layer_drag.as_ref() else { return false };
        let from = drag.path.clone();
        let (pos, released, esc) =
            ui.input(|i| (i.pointer.interact_pos(), !i.pointer.primary_down(), i.key_pressed(egui::Key::Escape)));
        if esc {
            self.layer_drag = None;
            return false;
        }
        let list = ui.clip_rect();
        let target = pos.and_then(|p| self.layer_drop_target(p, list));
        if !released {
            if let Some(p) = pos {
                // прокрутка у краёв списка
                let edge = 24.0;
                let dy = if p.y < list.top() + edge { 6.0 } else if p.y > list.bottom() - edge { -6.0 } else { 0.0 };
                if dy != 0.0 && list.x_range().contains(p.x) {
                    ui.scroll_with_delta(Vec2::new(0.0, dy));
                }
            }
            match &target {
                &Some((_, _, DropMark::Line(x0, x1, y))) => {
                    let p = ui.painter();
                    let ink = DROP_INK;
                    p.rect_filled(Rect::from_min_max(Pos2::new(x0 + 2.0, y - 1.5), Pos2::new(x1 - 2.0, y + 1.5)), 1.0, ink);
                    for (x, dir) in [(x0 + 1.0, 1.0f32), (x1 - 1.0, -1.0)] {
                        p.add(egui::Shape::convex_polygon(
                            vec![Pos2::new(x, y - 5.0), Pos2::new(x + 6.0 * dir, y), Pos2::new(x, y + 5.0)],
                            ink,
                            egui::Stroke::NONE,
                        ));
                    }
                }
                Some((_, _, DropMark::Into(r))) => {
                    let p = ui.painter();
                    p.rect_filled(*r, 0.0, sai_ui::select_bg().gamma_multiply(0.35));
                    p.rect_stroke(r.shrink(1.0), 2.0, egui::Stroke::new(2.0_f32, DROP_INK));
                }
                None => {}
            }
            ui.ctx().set_cursor_icon(if target.is_some() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::NoDrop });
            if let Some(p) = pos {
                self.layer_ghost(ui, p);
            }
            ui.ctx().request_repaint();
            return false;
        }
        self.layer_drag = None;
        let Some((parent, index, mark)) = target else { return false };
        if matches!(mark, DropMark::Into(_)) {
            // папку, в которую положили слой, раскрываем — чтобы было видно, куда он ушёл
            let fid = ui.make_persistent_id(("folder", parent.clone()));
            ui.data_mut(|d| d.insert_persisted(fid, true));
        }
        if self.transform.is_some() {
            self.finish_transform(true);
        }
        if self.filter.is_some() {
            self.finish_filter(true);
        }
        match self.doc.move_layer_to(&from, &parent, index) {
            Some((path, _)) => {
                self.selected = path;
                self.thumbs.clear();
                true
            }
            None => false,
        }
    }

    /// Ярлык схваченного слоя у курсора: миниатюра и имя, поверх всего окна. Сдвинут вправо-
    /// вниз от курсора, чтобы не закрывать строку под ним и подсказку, куда упадёт слой.
    fn layer_ghost(&self, ui: &egui::Ui, at: Pos2) {
        let Some(d) = &self.layer_drag else { return };
        let Some(layer) = self.doc.layer(&d.path) else { return };
        let font = egui::FontId::proportional(12.0);
        let name_w = ui.fonts(|f| f.layout_no_wrap(layer.name.clone(), font.clone(), sai_ui::ink()).size().x);
        let w = (24.0 + 6.0 + name_w + 8.0).min(d.width);
        let rect = Rect::from_min_size(at + Vec2::new(14.0, 12.0), Vec2::new(w, 28.0));
        let p = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("layer_ghost")));
        p.rect_filled(rect.translate(Vec2::new(2.0, 2.0)), 3.0, Color32::from_black_alpha(45));
        p.rect_filled(rect, 3.0, Color32::from_rgba_unmultiplied(232, 232, 252, 235));
        p.rect_stroke(rect.shrink(0.5), 3.0, egui::Stroke::new(1.5_f32, DROP_INK));
        let thumb = Rect::from_min_size(rect.min + Vec2::new(3.0, 2.0), Vec2::splat(24.0));
        if layer.children.is_some() {
            sai_ui::glyph(&p, thumb.shrink(3.0), sai_ui::gray(90), "new_folder");
        } else {
            p.rect_filled(thumb, 0.0, Color32::WHITE);
            if let Some(t) = self.thumbs.get(d.path.as_slice()) {
                p.image(t.id(), thumb, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
            p.rect_stroke(thumb, 0.0, egui::Stroke::new(1.0_f32, sai_ui::gray(170)));
        }
        let clip = Rect::from_min_max(Pos2::new(thumb.right() + 6.0, rect.top()), rect.max - Vec2::new(4.0, 0.0));
        p.with_clip_rect(clip).text(Pos2::new(clip.left(), rect.center().y), egui::Align2::LEFT_CENTER, &layer.name, font, sai_ui::ink());
    }

    /// Эффект выбранного слоя: кайма (ширина, сила) или двухцветный (порог). Как в SAI — только у
    /// обычных слоёв; значения по умолчанию как у SAI.
    fn effect_panel(&mut self, ui: &mut egui::Ui, lw: f32) -> bool {
        use paint_core::effects::Effect;
        let mut changed = false;
        let path = self.selected.clone();
        let Some(layer) = self.doc.layer_mut(&path) else { return false };
        let enabled = layer.children.is_none();
        let current = match &layer.effect {
            None => tr("[ Нет ]"),
            Some(Effect::Fringe { .. }) => tr("Кайма"),
            Some(Effect::Binary { .. }) => tr("Двухцветный"),
        };
        sai_ui::row(ui, tr("   Эффект"), lw + 10.0, enabled, |ui| {
            egui::ComboBox::from_id_salt("effect").selected_text(current).width(100.0).show_ui(ui, |ui| {
                let choices = [
                    (tr("[ Нет ]"), None),
                    (tr("Кайма"), Some(Effect::Fringe { kind: *b"watr", width: 2, strength: 100 })),
                    (tr("Двухцветный"), Some(Effect::Binary { threshold: 50, rest: 0 })),
                ];
                for (name, e) in choices {
                    if ui.selectable_label(current == name, name).clicked() && current != name {
                        layer.effect = e;
                        changed = true;
                    }
                }
            });
        });
        let w = ui.available_width() - lw - 18.0;
        let mut bar = |ui: &mut egui::Ui, label: &str, v: &mut u16, lo: u16, hi: u16| {
            sai_ui::row(ui, label, lw + 10.0, true, |ui| {
                let mut f = *v as f32;
                if sai_ui::bar(ui, &mut f, lo as f32, hi as f32, format!("{v}"), w, true).changed() {
                    *v = (f.round() as u16).clamp(lo, hi);
                    changed = true;
                }
            });
        };
        match &mut layer.effect {
            Some(Effect::Fringe { width, strength, .. }) => {
                bar(ui, tr("   Ширина"), width, 1, 15);
                bar(ui, tr("   Сила"), strength, 0, 100);
            }
            Some(Effect::Binary { threshold, .. }) => bar(ui, tr("   Порог"), threshold, 0, 100),
            None => {}
        }
        changed
    }

    /// Бумага выбранного слоя: текстура, масштаб, сила. Текстуры — из папки `papertex` SAI.
    fn paper_panel(&mut self, ui: &mut egui::Ui, lw: f32) -> bool {
        let mut changed = false;
        if self.paper_names.is_none() {
            self.paper_names = Some(paint_core::paper::names());
        }
        let names = self.paper_names.clone().unwrap_or_default();
        if names.is_empty() {
            ui.label(tr("Папка с текстурами бумаги (papertex из SAI):"));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.paper_dir).desired_width(150.0));
                if ui.button("OK").clicked() {
                    self.set_paper_dir();
                    changed = true;
                }
            });
        }
        let path = self.selected.clone();
        let Some(layer) = self.doc.layer_mut(&path) else { return changed };
        let current = layer.paper.as_ref().map_or(tr("[ Нет ]").to_string(), |p| p.name.clone());
        sai_ui::row(ui, tr("   Выбрать"), lw + 10.0, true, |ui| {
            egui::ComboBox::from_id_salt("paper").selected_text(&current).width(100.0).height(400.0).show_ui(ui, |ui| {
                if ui.selectable_label(layer.paper.is_none(), tr("[ Нет ]")).clicked() {
                    layer.paper = None;
                    changed = true;
                }
                for n in &names {
                    if ui.selectable_label(current == *n, n).clicked() {
                        // по умолчанию как у SAI: сила 15 %, масштаб 100 %
                        let (s, k, f) = layer.paper.as_ref().map_or((15, 100, 0), |p| (p.strength, p.scale, p.flag));
                        layer.paper = Some(paint_core::paper::Paper::new(n, s, k, f));
                        changed = true;
                    }
                }
            });
        });
        if let Some(p) = layer.paper.as_mut() {
            let w = ui.available_width() - lw - 18.0;
            sai_ui::row(ui, tr("   Масштаб"), lw + 10.0, true, |ui| {
                let mut v = p.scale as f32;
                if sai_ui::bar(ui, &mut v, 10.0, 500.0, format!("{}%", p.scale), w, true).changed() {
                    p.scale = v.round().clamp(10.0, 500.0) as u16;
                    changed = true;
                }
            });
            sai_ui::row(ui, tr("   Сила"), lw + 10.0, true, |ui| {
                let mut v = p.strength as f32;
                if sai_ui::bar(ui, &mut v, 0.0, 100.0, format!("{}", p.strength), w, true).changed() {
                    p.strength = v.round().clamp(0.0, 100.0) as u16;
                    changed = true;
                }
            });
            if p.texture.is_none() {
                ui.label(trf("Файл «{0}.bmp» не найден", &[&p.name]));
            }
        }
        changed
    }

    /// Применить папку текстур бумаги и заново найти текстуры у всех слоёв.
    fn set_paper_dir(&mut self) {
        let dir = self.paper_dir.trim();
        paint_core::paper::set_folder((!dir.is_empty()).then(|| std::path::PathBuf::from(dir)));
        paint_core::paper::set_user_dir(settings::dir());
        self.paper_names = None;
        self.grain_names = None;
        fn walk(layers: &mut [paint_core::Layer]) {
            for l in layers {
                if let Some(p) = l.paper.as_mut() {
                    p.texture = paint_core::paper::texture(&p.name);
                }
                if let Some(ch) = l.children.as_mut() {
                    walk(ch);
                }
            }
        }
        walk(&mut self.doc.layers);
        self.invalidate_all();
    }

    /// За что возьмёт линейку Ctrl + нажатие в точке экрана `pointer`.
    fn ruler_grip(&self, pointer: Option<Pos2>, rect: Rect) -> Option<ruler::Grip> {
        let pos = pointer?;
        let c = ViewXf::new(rect, self.zoom, self.offset, self.view_angle, self.view_flip).to_canvas(pos);
        self.ruler.hit([c.x, c.y], self.zoom)
    }

    /// Линейка поверх холста, как у образца: прямая — полупрозрачная бирюзовая полоса по одну
    /// сторону линии с осью-крестиком; эллипс — кольцо внутрь от линии и крестик в центре; с
    /// зажатым Ctrl у эллипса видна рамка с ручками.
    fn draw_ruler(&self, painter: &egui::Painter, xf: ViewXf, ctrl: bool) {
        let r = &self.ruler;
        if !r.visible || !r.placed {
            return;
        }
        // Цвета подобраны по снимку образца: на белом полоса (164, 230, 220), кромка
        // (210, 243, 238). Задаются предумноженными: так их смешивает egui.
        let (band, edge) = if r.ignore {
            (Color32::from_rgba_premultiplied(12, 45, 40, 60), Color32::from_rgba_premultiplied(6, 22, 20, 30))
        } else {
            (Color32::from_rgba_premultiplied(24, 90, 80, 115), Color32::from_rgba_premultiplied(10, 43, 38, 55))
        };
        let ink = Color32::from_rgb(40, 190, 165);
        let s = |p: [f32; 2]| xf.to_screen(Vec2::new(p[0], p[1]));
        let cross = |c: Pos2| {
            for d in [Vec2::new(8.0, 0.0), Vec2::new(0.0, 8.0)] {
                painter.line_segment([c - d, c + d], egui::Stroke::new(1.0_f32, ink));
            }
        };
        // через курсор — линия, по которой пойдёт штрих у линеек «от начала»
        let hover = painter.ctx().input(|i| i.pointer.hover_pos()).filter(|m| painter.clip_rect().contains(*m));
        let guide = Color32::from_rgba_premultiplied(30, 120, 105, 160).gamma_multiply(if r.ignore { 0.4 } else { 1.0 });
        let far = painter.clip_rect().size().length() * 2.0;
        match r.kind {
            ruler::Kind::Parallel | ruler::Kind::Radial => {
                if let Some(m) = hover {
                    let mc = xf.to_canvas(m);
                    let to = if r.kind == ruler::Kind::Parallel { [mc.x + r.angle.cos() * 10.0, mc.y + r.angle.sin() * 10.0] } else { r.vp[0] };
                    let d = (s(to) - m).normalized();
                    painter.line_segment([m - d * far, m + d * far], egui::Stroke::new(1.0_f32, guide));
                }
            }
            ruler::Kind::Concentric => {
                if let Some(m) = hover {
                    let mc = xf.to_canvas(m);
                    let k = r.ellipse_scale([mc.x, mc.y]);
                    let (sn, cs) = r.tilt.sin_cos();
                    let pts: Vec<Pos2> = (0..96).map(|i| {
                        let (st, ct) = (i as f32 / 96.0 * std::f32::consts::TAU).sin_cos();
                        let l = [r.rx * k * ct, r.ry * k * st];
                        s([r.center[0] + cs * l[0] - sn * l[1], r.center[1] + sn * l[0] + cs * l[1]])
                    }).collect();
                    painter.add(egui::Shape::closed_line(pts, egui::Stroke::new(1.0_f32, guide)));
                }
            }
            _ => {}
        }
        match r.kind {
            ruler::Kind::Radial => {
                let c = s(r.vp[0]);
                let faint = Color32::from_rgba_premultiplied(16, 60, 54, 60);
                for k in 0..36 {
                    let t = k as f32 / 36.0 * std::f32::consts::TAU;
                    painter.line_segment([c, c + Vec2::angled(t) * far], egui::Stroke::new(1.0_f32, faint));
                }
                painter.circle_filled(c, 4.0, ink);
                painter.circle_stroke(c, 8.0, egui::Stroke::new(1.5_f32, ink));
            }
            ruler::Kind::Straight | ruler::Kind::Parallel => {
                // линия на экране и нормаль в сторону полосы
                let (sn, cs) = r.angle.sin_cos();
                let a = s(r.pivot);
                let b = s([r.pivot[0] + cs * 100.0, r.pivot[1] + sn * 100.0]);
                let n0 = s([r.pivot[0] - sn * 100.0, r.pivot[1] + cs * 100.0]);
                let dir = (b - a).normalized();
                let nrm = (n0 - a).normalized();
                let far = painter.clip_rect().size().length();
                let quad = |d0: f32, d1: f32, col: Color32| {
                    let pts = vec![
                        a - dir * far + nrm * d0,
                        a + dir * far + nrm * d0,
                        a + dir * far + nrm * d1,
                        a - dir * far + nrm * d1,
                    ];
                    painter.add(egui::Shape::convex_polygon(pts, col, egui::Stroke::NONE));
                };
                quad(0.0, 8.0, edge);
                quad(8.0, ruler::BAND, band);
                cross(a);
                if r.lock_pivot {
                    painter.circle_stroke(a, 5.0, egui::Stroke::new(1.0_f32, ink));
                }
            }
            ruler::Kind::Perspective(_) => {
                let n = r.vps();
                let far = painter.clip_rect().size().length() * 2.0;
                let alpha = if r.ignore { 0.4 } else { 1.0 };
                let faint = Color32::from_rgba_premultiplied(16, 60, 54, 60).gamma_multiply(alpha);
                let through = Color32::from_rgba_premultiplied(30, 120, 105, 160).gamma_multiply(alpha);
                let dir = |at: [f32; 2], d: [f32; 2]| (s([at[0] + d[0] * 10.0, at[1] + d[1] * 10.0]) - s(at)).normalized();
                // горизонт
                let h = r.horizon();
                let c0 = s(r.vp[0]);
                let hd = dir(r.vp[0], h);
                painter.line_segment([c0 - hd * far, c0 + hd * far], egui::Stroke::new(1.0_f32, ink.gamma_multiply(alpha)));
                // веер лучей от каждой точки схода
                for v in &r.vp[..n] {
                    let c = s(*v);
                    for k in 0..24 {
                        let t = k as f32 / 24.0 * std::f32::consts::TAU;
                        painter.line_segment([c, c + Vec2::angled(t) * far], egui::Stroke::new(1.0_f32, faint));
                    }
                }
                // направляющие через курсор — куда пойдёт штрих
                if let Some(m) = painter.ctx().input(|i| i.pointer.hover_pos()).filter(|m| painter.clip_rect().contains(*m)) {
                    let mc = xf.to_canvas(m);
                    for g in r.guides([mc.x, mc.y]) {
                        let gd = dir([mc.x, mc.y], g);
                        painter.line_segment([m - gd * far, m + gd * far], egui::Stroke::new(1.0_f32, through));
                    }
                }
                // точки схода — зелёная, синяя, красная, как в меню SAI
                let cols = [Color32::from_rgb(40, 170, 60), Color32::from_rgb(50, 90, 220), Color32::from_rgb(210, 50, 50)];
                for (v, col) in r.vp[..n].iter().zip(cols) {
                    let c = s(*v);
                    painter.circle_filled(c, 4.0, col);
                    painter.circle_stroke(c, 8.0, egui::Stroke::new(1.5_f32, col));
                }
            }
            ruler::Kind::Ellipse | ruler::Kind::Concentric => {
                let z = self.zoom;
                let inset = ruler::RING / 2.0 / z;
                let n = 96;
                let pts: Vec<Pos2> = (0..n)
                    .map(|i| {
                        let t = i as f32 / n as f32 * std::f32::consts::TAU;
                        let (st, ct) = t.sin_cos();
                        let l = [(r.rx - inset).max(0.0) * ct, (r.ry - inset).max(0.0) * st];
                        let (sn, cs) = r.tilt.sin_cos();
                        s([r.center[0] + cs * l[0] - sn * l[1], r.center[1] + sn * l[0] + cs * l[1]])
                    })
                    .collect();
                painter.add(egui::Shape::closed_line(pts, egui::Stroke::new(ruler::RING, band)));
                let c = s(r.center);
                cross(c);
                if ctrl {
                    let (sn, cs) = r.tilt.sin_cos();
                    let corner = |x: f32, y: f32| s([r.center[0] + cs * x - sn * y, r.center[1] + sn * x + cs * y]);
                    let box_pts = [corner(-r.rx, -r.ry), corner(r.rx, -r.ry), corner(r.rx, r.ry), corner(-r.rx, r.ry)];
                    painter.add(egui::Shape::closed_line(box_pts.to_vec(), egui::Stroke::new(1.0_f32, ink)));
                    for p in box_pts {
                        painter.circle_filled(p, 3.5, ink);
                    }
                    painter.rect_filled(Rect::from_center_size(c, Vec2::splat(7.0)), 0.0, Color32::from_rgb(240, 150, 40));
                }
            }
        }
    }

    fn canvas(&mut self, ui: &mut egui::Ui) {
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        self.view = rect;
        if self.opts.popup_palette && response.secondary_clicked() {
            self.popup_at = response.interact_pointer_pos();
            self.popup_frame = ui.ctx().cumulative_pass_nr();
            ui.ctx().request_repaint();
        }
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, sai_ui::field());
        let (w, h) = (self.doc.width as f32, self.doc.height as f32);
        if self.fit_pending && rect.width() > 10.0 {
            self.zoom = (rect.width() / w).min(rect.height() / h).min(1.0) * 0.95;
            self.offset = Vec2::new(
                (w - rect.width() / self.zoom) / 2.0,
                (h - rect.height() / self.zoom) / 2.0,
            );
            self.fit_pending = false;
        }
        let (vang, vflip) = (self.view_angle, self.view_flip);
        let to_canvas = |p: Pos2, zoom: f32, offset: Vec2| -> Vec2 { ViewXf::new(rect, zoom, offset, vang, vflip).to_canvas(p) };

        // Масштаб колесом — вокруг указателя; перетаскивание средней кнопкой или «Рукой» — сдвиг.
        let (mut scroll, mut zoom_delta, mut middle, space, pointer, mut moves, mut pressed, mut released, touches) = ui
            .input(|i| {
                let moves: Vec<Pos2> = i
                    .events
                    .iter()
                    .filter_map(|e| {
                        if let egui::Event::PointerMoved(p) = e {
                            Some(*p)
                        } else {
                            None
                        }
                    })
                    .collect();
                let touches: Vec<(egui::TouchPhase, Option<f32>)> = i
                    .events
                    .iter()
                    .filter_map(|e| {
                        if let egui::Event::Touch { phase, force, .. } = e {
                            Some((*phase, *force))
                        } else {
                            None
                        }
                    })
                    .collect();
                (
                    i.smooth_scroll_delta.y,
                    i.zoom_delta(),
                    i.pointer.middle_down(),
                    i.key_down(egui::Key::Space),
                    i.pointer.latest_pos(),
                    moves,
                    i.pointer.primary_pressed(),
                    i.pointer.primary_released(),
                    touches,
                )
            });
        // Перо через Windows Ink приходит касанием с силой нажима.
        // несколько видов: ввод принимает тот, в котором нажали; другой в это время только показывает
        if pressed && response.hovered() {
            self.input_view = self.drawing_view;
        }
        if self.input_view != self.drawing_view {
            pressed = false;
            released = false;
            moves.clear();
            middle = false;
            if !response.hovered() {
                scroll = 0.0;
                zoom_delta = 1.0;
            }
        }
        // указатель над окном поверх холста (опорная картинка, «История», диалоги): колесо и
        // нажатие — этому окну, не холсту; начатый мазок отпускается как обычно
        let covered = pointer.is_some_and(|p| ui.ctx().layer_id_at(p).is_some_and(|l| l.order != egui::Order::Background));
        if covered {
            scroll = 0.0;
            zoom_delta = 1.0;
            pressed = false;
        }
        for (phase, force) in touches {
            match phase {
                egui::TouchPhase::Start | egui::TouchPhase::Move => {
                    self.touch_down = true;
                    match force {
                        Some(f) => self.touch_force = Some(f.clamp(0.0, 1.0)),
                        // Перо уже известно, а силы нет — это касание легче первого уровня нажима.
                        None if self.touch_force.is_some() => self.touch_force = Some(0.0),
                        None => {}
                    }
                }
                egui::TouchPhase::End | egui::TouchPhase::Cancel => self.touch_down = false,
            }
        }
        if let Some(pos) = pointer.filter(|p| rect.contains(*p)) {
            let factor = if zoom_delta != 1.0 {
                zoom_delta
            } else if scroll != 0.0 {
                // «Настройки → колесо наоборот»
                (if self.opts.swap_zoom { -scroll } else { scroll } / 240.0).exp()
            } else {
                1.0
            };
            if factor != 1.0 {
                let anchor = to_canvas(pos, self.zoom, self.offset);
                self.zoom = (self.zoom * factor).clamp(0.02, 32.0);
                self.offset += anchor - to_canvas(pos, self.zoom, self.offset);
            }
        }
        let panning = middle || space || self.tool == Tool::Hand;
        let ctrl = ui.input(|i| i.modifiers.command);
        if self.s3d_takes_mouse(pressed && response.hovered() && !panning) {
            // слой 3D: мышь вращает камеру или сустав
            let xf = ViewXf::new(rect, self.zoom, self.offset, vang, vflip);
            let pts: Vec<Vec2> = moves.iter().map(|p| xf.to_canvas(*p)).collect();
            let mods = ui.input(|i| i.modifiers);
            let up = released || !ui.input(|i| i.pointer.primary_down());
            self.s3d_mouse(pointer.map(|p| xf.to_canvas(p)), &pts, pressed, up, mods);
        } else if self.transform.is_some() && !panning {
            let zoom = self.zoom;
            let vx = ViewXf::new(rect, zoom, self.offset, vang, vflip);
            let screen = |c: Vec2| vx.to_screen(c);
            if pressed && response.hovered() {
                if let (Some(pos), Some(t)) = (pointer, self.transform.as_mut()) {
                    let c = to_canvas(pos, zoom, self.offset);
                    let near = t.handle_points().into_iter().find(|&(_, p)| (screen(p) - pos).length() < 8.0).map(|h| h.0);
                    // Ctrl + угол (в режиме «Деформация» — без Ctrl) — свободная деформация: дальше
                    // каждый угол двигается отдельно
                    let kind = self.transform_kind;
                    let near = match near {
                        Some(Handle::Scale(..)) if kind == TransformKind::Rotate => Some(Handle::Rotate),
                        Some(Handle::Scale(hx, hy)) if (ctrl || kind == TransformKind::Deform) && hx != 0 && hy != 0 => {
                            t.quad = Some(t.corners());
                            Some(Handle::Corner(match (hx, hy) {
                                (-1, -1) => 0,
                                (1, -1) => 1,
                                (1, 1) => 2,
                                _ => 3,
                            }))
                        }
                        other => other,
                    };
                    let inside = t.contains(c);
                    let free = t.quad.is_some();
                    let handle = near.unwrap_or(if inside || free { Handle::Move } else { Handle::Rotate });
                    // «Размер» не поворачивает
                    if !(kind == TransformKind::Resize && handle == Handle::Rotate) {
                        t.drag = Some((handle, c));
                    }
                }
            } else if let Some(t) = self.transform.as_mut() {
                if let Some((h, mut last)) = t.drag {
                    for pos in &moves {
                        let c = to_canvas(*pos, zoom, self.offset);
                        t.drag_to(h, last, c);
                        last = c;
                    }
                    t.drag = Some((h, last));
                }
            }
            if released {
                if let Some(t) = self.transform.as_mut() {
                    t.drag = None;
                    t.dirty = true;
                }
                self.update_transform(false);
            } else if self.transform.as_ref().is_some_and(|t| t.dirty) {
                self.update_transform(true);
            }
        } else if self.ruler_drag.is_some()
            || (!panning && self.live.is_none() && pressed && response.hovered() && ctrl && self.ruler_grip(pointer, rect).is_some())
        {
            // Правка линейки Ctrl + перетаскиванием (как в SAI): над полосой или рамкой.
            let xf = ViewXf::new(rect, self.zoom, self.offset, vang, vflip);
            let c = |p: Pos2| { let v = xf.to_canvas(p); [v.x, v.y] };
            if self.ruler_drag.is_none() {
                if let (Some(pos), Some(g)) = (pointer, self.ruler_grip(pointer, rect)) {
                    self.ruler_drag = Some((g, c(pos)));
                }
            } else if let Some((g, mut last)) = self.ruler_drag {
                for pos in &moves {
                    let now = c(*pos);
                    self.ruler.drag(g, last, now);
                    last = now;
                }
                self.ruler_drag = Some((g, last));
            }
            if released || !ui.input(|i| i.pointer.primary_down()) {
                self.ruler_drag = None;
            }
        } else if self.move_drag.is_some() || (!panning && self.live.is_none() && pressed && response.hovered() && (self.tool == Tool::Move || (ctrl && !self.move_guarded()))) {
            // Перемещение: инструмент «Перемещение» или Ctrl + перетаскивание (как в SAI).
            if self.move_drag.is_none() {
                if let Some(pos) = pointer {
                    self.begin_move(to_canvas(pos, self.zoom, self.offset));
                }
            } else {
                for pos in &moves {
                    self.update_move(to_canvas(*pos, self.zoom, self.offset));
                }
            }
            if released {
                self.end_move();
            }
        } else if panning && (response.dragged() || middle) {
            let delta = ui.input(|i| i.pointer.delta());
            self.offset -= ViewXf::new(rect, self.zoom, self.offset, vang, vflip).vec_to_canvas(delta);
        } else if self.tool == Tool::RotateView {
            if response.double_clicked() {
                self.rotate_view(0.0);
            } else if response.dragged() {
                if let Some(pos) = pointer {
                    let c = rect.center();
                    if let Some(prev) = self.rot_last {
                        let (from, to) = (prev - c, pos - c);
                        if from.length() > 4.0 && to.length() > 4.0 {
                            let mut da = to.angle() - from.angle();
                            if da > std::f32::consts::PI {
                                da -= std::f32::consts::TAU;
                            } else if da < -std::f32::consts::PI {
                                da += std::f32::consts::TAU;
                            }
                            // в зеркальном виде поворот идёт в обратную сторону
                            let da = if self.view_flip { -da } else { da };
                            // не через rotate_view: шаг 0 там значит «сбросить»
                            if da != 0.0 {
                                self.rotate_view(da.to_degrees());
                            }
                        }
                    }
                    self.rot_last = Some(pos);
                }
            } else {
                self.rot_last = None;
            }
        } else if self.tool == Tool::Zoom {
            if response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let anchor = to_canvas(pos, self.zoom, self.offset);
                    let out = ui.input(|i| i.modifiers.alt);
                    self.zoom = (if out {
                        self.zoom / 1.5
                    } else {
                        self.zoom * 1.5
                    })
                    .clamp(0.02, 32.0);
                    self.offset += anchor - to_canvas(pos, self.zoom, self.offset);
                }
            }
        } else if self.tool == Tool::Fill {
            if pressed && response.hovered() {
                if let Some(pos) = pointer {
                    let c = to_canvas(pos, self.zoom, self.offset);
                    self.fill_at(c.x, c.y);
                }
            }
        } else if self.tool == Tool::Brush && self.kind() == ToolKind::Bucket && !ui.input(|i| i.modifiers.alt) {
            // «Заливка» из сетки (Bucket в SAI) — как ведро
            if pressed && response.hovered() {
                if let Some(pos) = pointer {
                    let c = to_canvas(pos, self.zoom, self.offset);
                    self.fill_at(c.x, c.y);
                }
            }
        } else if self.tool == Tool::Brush && self.kind() == ToolKind::Gradient && !ui.input(|i| i.modifiers.alt) {
            if pressed && response.hovered() {
                if let Some(pos) = pointer {
                    self.gradient_press(to_canvas(pos, self.zoom, self.offset));
                }
            } else if let (Some(e), Some(pos)) = (self.gradient.as_mut(), moves.last()) {
                if let Some(h) = e.drag {
                    let c = to_canvas(*pos, self.zoom, self.offset);
                    let p = [c.x as f64, c.y as f64];
                    if h == 0 {
                        e.g.from = p;
                    } else {
                        e.g.to = p;
                    }
                    e.dirty = true;
                }
            }
            if released {
                if let Some(e) = self.gradient.as_mut() {
                    e.drag = None;
                }
            }
        } else if self.tool == Tool::Text {
            if pressed && response.hovered() {
                if let Some(pos) = pointer {
                    let c = to_canvas(pos, self.zoom, self.offset);
                    self.begin_text(c.x as f64, c.y as f64);
                }
            }
        } else if self.tool == Tool::Brush && self.kind() == ToolKind::LassoFill && !ui.input(|i| i.modifiers.alt) {
            let xf = ViewXf::new(rect, self.zoom, self.offset, vang, vflip);
            let pts: Vec<Vec2> = moves.iter().map(|p| xf.to_canvas(*p)).collect();
            let at = pointer.filter(|_| response.hovered()).map(|p| xf.to_canvas(p));
            let down = ui.input(|i| i.pointer.primary_down());
            self.lasso_fill_input(at, &pts, pressed && response.hovered(), released, response.double_clicked(), down);
        } else if matches!(self.tool, Tool::Marquee | Tool::Lasso | Tool::Wand) {
            // Выделение: Shift — добавить, Alt — вычесть, Shift+Alt — пересечь.
            let poly = self.tool == Tool::Lasso && self.lasso_polygon;
            // двойной щелчок закрывает многоугольник; его нажатие новую вершину не ставит
            let closed = poly && response.double_clicked();
            if closed {
                self.close_polygon();
            }
            let mods = ui.input(|i| i.modifiers);
            // Shift/Alt — как в SAI, без них — режим кнопок панели
            let mode = if mods.shift || mods.alt { Combine::from_modifiers(mods.shift, mods.alt) } else { self.sel_mode };
            if pressed && !closed && response.hovered() {
                if let Some(pos) = pointer {
                    let c = to_canvas(pos, self.zoom, self.offset);
                    if self.tool == Tool::Wand {
                        self.wand_at(c.x, c.y, mode);
                    } else if poly {
                        match self.sel_poly.as_mut() {
                            Some(d) => {
                                if d.points.last() != Some(&c) {
                                    d.points.push(c);
                                }
                            }
                            None => self.sel_poly = Some(SelDrag { points: vec![c], mode, fill: false }),
                        }
                    } else {
                        // лассо сглаживается, как штрих (SAI 2026-10)
                        self.smoother = Stabilizer::with_level(self.stabilizer);
                        let first = if self.tool == Tool::Lasso { let q = self.smoother.push(InputPoint { x: c.x, y: c.y, pressure: 1.0 }); Vec2::new(q.x, q.y) } else { c };
                        self.sel_drag = Some(SelDrag { points: vec![first, first], mode, fill: false });
                    }
                }
            } else if let Some(d) = self.sel_drag.as_mut() {
                for pos in &moves {
                    let c = to_canvas(*pos, self.zoom, self.offset);
                    if self.tool == Tool::Lasso {
                        let q = self.smoother.push(InputPoint { x: c.x, y: c.y, pressure: 1.0 });
                        d.points.push(Vec2::new(q.x, q.y));
                    } else {
                        d.points[1] = c;
                    }
                }
            }
            if released {
                if let Some(mut d) = self.sel_drag.take() {
                    if self.tool == Tool::Lasso {
                        d.points.extend(self.smoother.finish().into_iter().map(|q| Vec2::new(q.x, q.y)));
                    }
                    let area = if self.tool == Tool::Lasso {
                        let pts: Vec<[f64; 2]> = d.points.iter().map(|p| [p.x as f64, p.y as f64]).collect();
                        paint_core::selection::polygon_aa(&pts, self.lasso_aa)
                    } else {
                        // прямоугольник — по целым пикселям; щелчок без протяжки снимает выделение
                        let (a, b) = (d.points[0], d.points[1]);
                        let (x0, x1) = (a.x.min(b.x).round() as f64, a.x.max(b.x).round() as f64);
                        let (y0, y1) = (a.y.min(b.y).round() as f64, a.y.max(b.y).round() as f64);
                        if x1 - x0 < 1.0 || y1 - y0 < 1.0 {
                            paint_core::tile::MaskMap::new()
                        } else {
                            paint_core::selection::rect(x0, y0, x1, y1)
                        }
                    };
                    self.apply_selection(&area, d.mode);
                }
            }
        } else if self.tool == Tool::Picker
            || (ui.input(|i| i.modifiers.alt) && self.live.is_none())
        {
            if let Some(pos) = response.interact_pointer_pos() {
                let c = to_canvas(pos, self.zoom, self.offset);
                self.pick_color(c.x, c.y);
            }
        } else if !panning {
            let (pressure, eraser_end) = self.pressure();
            // точка пера — середина пикселя экрана под ним, как в SAI (позиция указателя — угол)
            let half = Vec2::splat(0.5 / self.ppp);
            let point = |p: Pos2, zoom: f32, offset: Vec2| {
                let c = to_canvas(p + half, zoom, offset);
                InputPoint {
                    x: c.x,
                    y: c.y,
                    pressure,
                }
            };
            if pressed && response.hovered() && self.below_min {
                // слабее «минимального нажима» — ждать, пока нажмут сильнее
                self.pending_start = Some(eraser_end);
            } else if pressed && response.hovered() {
                if let Some(pos) = pointer {
                    self.raw_last = Some((pos, pressure));
                    self.begin_stroke(point(pos, self.zoom, self.offset), eraser_end);
                }
            } else if let (Some(er), false, Some(pos)) = (self.pending_start, self.below_min, pointer) {
                self.pending_start = None;
                self.raw_last = Some((pos, pressure));
                self.begin_stroke(point(pos, self.zoom, self.offset), er);
            } else if self.live.is_some() || self.sel_stroke.is_some() || self.mask_stroke.is_some() || self.line_stroke.is_some() || self.line_edit.is_some() || self.shape_drag.is_some() {
                // Сглаживание SAI — среднее последних N точек пера, а планшет шлёт их с постоянной
                // частотой. Windows склеивает движения, пока кадр занят (тяжёлые кисти), — точек
                // меньше, и то же N сглаживает сильнее. Поэтому точки кадра дополняются до 240 в
                // секунду вдоль пройденного пути: сила сглаживания одна на любом инструменте.
                let stabilized = self.live.is_some() || self.sel_stroke.is_some() || self.mask_stroke.is_some() || self.line_stroke.is_some();
                let dt = ui.input(|i| i.unstable_dt).clamp(0.0, 0.25);
                let pts = match self.raw_last {
                    Some(last) if stabilized && !moves.is_empty() => resample_moves(last, &moves, pressure, (dt * 240.0).ceil() as usize),
                    _ => moves.iter().map(|&p| (p, pressure)).collect(),
                };
                if let Some(&(p, pr)) = pts.last() {
                    self.raw_last = Some((p, pr));
                }
                for (pos, pr) in pts {
                    let mut q = point(pos, self.zoom, self.offset);
                    q.pressure = pr;
                    self.continue_stroke(q);
                }
            }
            self.drain_points(Some(Duration::from_millis(25)));
            self.render_line_edit();
            if released {
                self.end_stroke();
            }
        }

        // Показ: только плитки, попавшие в окно.
        let xf = ViewXf::new(rect, self.zoom, self.offset, vang, vflip);
        // проценты масштаба — от пикселей экрана, как в SAI; при 100 % без поворота пиксель
        // холста — пиксель экрана, без размытия
        let shown = self.zoom * self.ppp;
        let options = if shown >= 2.0 || ((shown - 1.0).abs() < 1e-3 && vang == 0.0) {
            egui::TextureOptions::NEAREST
        } else if shown < 0.99 && std::env::var_os("PAINT_NO_MIPMAP").is_none() {
            // уменьшение — трилинейно по мипмапам: без мерцания и муара на мелких деталях
            egui::TextureOptions { mipmap_mode: Some(egui::TextureFilter::Linear), ..egui::TextureOptions::LINEAR }
        } else {
            egui::TextureOptions::LINEAR
        };
        let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        // бесшовный режим — холст и восемь его копий вокруг
        let copies: Vec<(f32, f32)> = if self.wrap_around {
            [-1.0, 0.0, 1.0].into_iter().flat_map(|kx| [-1.0, 0.0, 1.0].map(|ky| (kx * w, ky * h))).collect()
        } else {
            vec![(0.0, 0.0)]
        };
        for &(ox, oy) in &copies {
            painter.add(egui::Shape::convex_polygon(
                [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)].map(|(x, y)| xf.to_screen(Vec2::new(x + ox, y + oy))).to_vec(),
                Color32::WHITE,
                egui::Stroke::NONE,
            ));
        }
        for (&(dx, dy), tile) in self.display.iter_mut() {
            let (tx, ty) = ((dx * DT) as f32, (dy * DT) as f32);
            let shown_at: Vec<(f32, f32)> = copies
                .iter()
                .copied()
                .filter(|&(ox, oy)| {
                    let quad = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
                        .map(|(a, b): (f32, f32)| xf.to_screen(Vec2::new(tx + ox + a * DT as f32, ty + oy + b * DT as f32)));
                    Rect::from_points(&quad).intersects(rect)
                })
                .collect();
            if shown_at.is_empty() {
                continue;
            }
            // больше половины кусочков изменилось — одна загрузка целиком дешевле многих мелких
            let full = tile.texture.is_none() || tile.options != Some(options) || tile.dirty.count_ones() > (SUB * SUB / 2) as u32;
            if full || tile.dirty != 0 {
                let t0 = Instant::now();
                match &mut tile.texture {
                    // изменились отдельные кусочки — только они (в 16 раз меньше данных на видеокарту)
                    Some(t) if !full => {
                        let n = TILE as usize;
                        for k in 0..(SUB * SUB) as usize {
                            if tile.dirty & (1 << k) == 0 {
                                continue;
                            }
                            let (sx, sy) = ((k % SUB as usize) * n, (k / SUB as usize) * n);
                            let pixels: Vec<Color32> = (0..n).flat_map(|y| tile.pixels[(sy + y) * DT as usize + sx..][..n].iter().copied()).collect();
                            t.set_partial([sx, sy], egui::ColorImage { size: [n, n], pixels }, options);
                        }
                    }
                    Some(t) => t.set(egui::ColorImage { size: [DT as usize, DT as usize], pixels: tile.pixels.clone() }, options),
                    None => {
                        tile.texture = Some(ui.ctx().load_texture(
                            format!("tile-{dx}-{dy}"),
                            egui::ColorImage { size: [DT as usize, DT as usize], pixels: tile.pixels.clone() },
                            options,
                        ))
                    }
                }
                tile.dirty = 0;
                tile.options = Some(options);
                if let Some(s) = &self.frame_stats {
                    s.borrow_mut().upload += t0.elapsed().as_secs_f32() * 1000.0;
                }
            }
            if let Some(t) = &tile.texture {
                // Край холста внутри плитки: показываем только часть до границы.
                let part = Vec2::new(
                    (w - (dx * DT) as f32).min(DT as f32),
                    (h - (dy * DT) as f32).min(DT as f32),
                );
                let (u, v) = (part.x / DT as f32, part.y / DT as f32);
                for &(x0, y0) in &shown_at {
                    let (x0, y0) = (tx + x0, ty + y0);
                    let mut mesh = egui::Mesh::with_texture(t.id());
                    for (a, b) in [(0.0, 0.0), (u, 0.0), (u, v), (0.0, v)] {
                        mesh.vertices.push(egui::epaint::Vertex {
                            pos: xf.to_screen(Vec2::new(x0 + a * DT as f32, y0 + b * DT as f32)),
                            uv: Pos2::new(uv.min.x + a, uv.min.y + b),
                            color: Color32::WHITE,
                        });
                    }
                    mesh.add_triangle(0, 1, 2);
                    mesh.add_triangle(0, 2, 3);
                    painter.add(egui::Shape::mesh(mesh));
                }
            }
        }
        if self.ruler.visible && !self.ruler.placed {
            // новый или открытый холст — линейка встаёт посреди вида
            self.reset_ruler();
        }
        if self.pixel_grid && shown >= 8.0 {
            draw_pixel_grid(&painter, xf, rect, w, h);
        }
        // оси симметрии — пунктиром
        let axis = egui::Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(60, 140, 230, 200));
        if self.symmetry == 4 {
            // по кругу: оси (с отражением) или границы частей (без) — от центра до края холста
            let [cx, cy] = self.sym_center.unwrap_or([w / 2.0, h / 2.0]);
            let reach = w.max(h) * 1.5;
            let n = self.sym_n.clamp(1, 20) as usize;
            let rays = if self.sym_mirror { 2 * n } else { n };
            for k in 0..rays {
                let t = self.sym_angle.to_radians() + std::f32::consts::TAU * k as f32 / rays as f32;
                let end = Vec2::new(cx + t.sin() * reach, cy - t.cos() * reach);
                painter.extend(egui::Shape::dashed_line(&[xf.to_screen(Vec2::new(cx, cy)), xf.to_screen(end)], axis, 6.0, 4.0));
            }
            painter.circle_stroke(xf.to_screen(Vec2::new(cx, cy)), 4.0, axis);
        }
        if self.symmetry < 4 && self.symmetry & 1 != 0 {
            painter.extend(egui::Shape::dashed_line(&[xf.to_screen(Vec2::new(w / 2.0, 0.0)), xf.to_screen(Vec2::new(w / 2.0, h))], axis, 6.0, 4.0));
        }
        if self.symmetry < 4 && self.symmetry & 2 != 0 {
            painter.extend(egui::Shape::dashed_line(&[xf.to_screen(Vec2::new(0.0, h / 2.0)), xf.to_screen(Vec2::new(w, h / 2.0))], axis, 6.0, 4.0));
        }
        self.draw_ruler(&painter, xf, ctrl);
        self.s3d_overlay(&painter, xf);
        self.draw_text_box(&painter, xf);
        self.draw_line_nodes(&painter, xf);
        // Граница выделения («бегущие муравьи») и выделение, которое сейчас тянут.
        if self.sel_dirty {
            self.sel_outline = self.doc.selection.outline();
            self.sel_dirty = false;
        }
        let to_screen = |x: f32, y: f32| xf.to_screen(Vec2::new(x, y));
        if self.show_selection && !self.sel_outline.is_empty() {
            let phase = (self.started.elapsed().as_millis() / 300).is_multiple_of(2);
            let (a, b) = if phase { (Color32::BLACK, Color32::WHITE) } else { (Color32::WHITE, Color32::BLACK) };
            let mut shapes = Vec::new();
            for seg in &self.sel_outline {
                let p0 = to_screen(seg[0].0 as f32, seg[0].1 as f32);
                let p1 = to_screen(seg[1].0 as f32, seg[1].1 as f32);
                if !rect.intersects(Rect::from_two_pos(p0, p1).expand(1.0)) {
                    continue;
                }
                painter.line_segment([p0, p1], egui::Stroke::new(1.0_f32, a));
                shapes.extend(egui::Shape::dashed_line(&[p0, p1], egui::Stroke::new(1.0_f32, b), 4.0, 4.0));
            }
            painter.extend(shapes);
            ui.ctx().request_repaint_after(Duration::from_millis(300));
        }
        if let Some(d) = &self.sel_drag {
            let pts: Vec<Pos2> = if self.tool == Tool::Lasso || d.fill {
                d.points.iter().map(|p| to_screen(p.x, p.y)).collect()
            } else {
                let (a, b) = (d.points[0], d.points[1]);
                vec![to_screen(a.x, a.y), to_screen(b.x, a.y), to_screen(b.x, b.y), to_screen(a.x, b.y), to_screen(a.x, a.y)]
            };
            painter.add(egui::Shape::line(pts.clone(), egui::Stroke::new(1.0_f32, Color32::WHITE)));
            painter.extend(egui::Shape::dashed_line(&pts, egui::Stroke::new(1.0_f32, Color32::BLACK), 4.0, 4.0));
        }
        // Многоугольное лассо: вершины и линия к курсору.
        if let Some(d) = &self.sel_poly {
            let mut pts: Vec<Pos2> = d.points.iter().map(|p| to_screen(p.x, p.y)).collect();
            if let Some(h) = ui.input(|i| i.pointer.hover_pos()) {
                pts.push(h);
            }
            painter.add(egui::Shape::line(pts.clone(), egui::Stroke::new(1.0_f32, Color32::WHITE)));
            painter.extend(egui::Shape::dashed_line(&pts, egui::Stroke::new(1.0_f32, Color32::BLACK), 4.0, 4.0));
        }
        // Градиент до OK: линия от начала к концу и кружок на конце (как в SAI).
        if let Some(e) = &self.gradient {
            let a = to_screen(e.g.from[0] as f32, e.g.from[1] as f32);
            let b = to_screen(e.g.to[0] as f32, e.g.to[1] as f32);
            for (w, c) in [(3.0_f32, Color32::WHITE), (1.0, Color32::from_gray(90))] {
                painter.line_segment([a, b], egui::Stroke::new(w, c));
                painter.circle_stroke(b, 5.0, egui::Stroke::new(w, c));
                painter.circle_stroke(a, 2.5, egui::Stroke::new(w, c));
            }
        }
        // Мазок пера или ластика выделения — полупрозрачная полоса по пути.
        if let Some((s, path)) = &self.sel_stroke {
            let width = (s.params.size * self.zoom).max(1.0);
            let color = if self.sel_eraser() { Color32::from_rgba_unmultiplied(220, 60, 60, 110) } else { Color32::from_rgba_unmultiplied(60, 110, 230, 110) };
            let pts: Vec<Pos2> = path.iter().map(|p| to_screen(p.x, p.y)).collect();
            if pts.len() == 1 {
                painter.circle_filled(pts[0], width / 2.0, color);
            } else {
                painter.add(egui::Shape::line(pts, egui::Stroke::new(width, color)));
            }
        }
        // Мазок пера на слое контура — тонкая линия по пути, пока перо не поднято.
        if let Some(pts) = &self.line_stroke {
            let c = self.color();
            let width = (self.brushes[self.brush].params.size * self.zoom).max(1.0);
            let line: Vec<Pos2> = pts.iter().map(|p| to_screen(p.at[0] as f32, p.at[1] as f32)).collect();
            if line.len() > 1 {
                painter.add(egui::Shape::line(line, egui::Stroke::new(width, Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 160))));
            }
        }
        // Фигура, которую тянут на слое фигур.
        if let Some((a, b)) = self.shape_drag {
            use paint_formats::linework::ShapeKind;
            let (x0, x1, y0, y1) = (a.x.min(b.x), a.x.max(b.x), a.y.min(b.y), a.y.max(b.y));
            let pts: Vec<Pos2> = match self.shape_kind {
                ShapeKind::Rectangle => vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)],
                ShapeKind::Triangle => vec![((x0 + x1) / 2.0, y0), (x1, y1), (x0, y1), ((x0 + x1) / 2.0, y0)],
                ShapeKind::Ellipse => (0..=48)
                    .map(|i| {
                        let t = i as f32 / 48.0 * std::f32::consts::TAU;
                        ((x0 + x1) / 2.0 + (x1 - x0) / 2.0 * t.cos(), (y0 + y1) / 2.0 + (y1 - y0) / 2.0 * t.sin())
                    })
                    .collect(),
            }
            .into_iter()
            .map(|(x, y)| to_screen(x, y))
            .collect();
            painter.add(egui::Shape::line(pts.clone(), egui::Stroke::new(1.0_f32, Color32::WHITE)));
            painter.extend(egui::Shape::dashed_line(&pts, egui::Stroke::new(1.0_f32, Color32::BLACK), 4.0, 4.0));
        }
        // Рамка трансформации с ручками.
        if let Some(t) = &self.transform {
            let c = t.corners();
            let pts: Vec<Pos2> = [c[0], c[1], c[2], c[3], c[0]].iter().map(|w| to_screen(w.x, w.y)).collect();
            painter.add(egui::Shape::line(pts.clone(), egui::Stroke::new(1.0_f32, Color32::WHITE)));
            painter.extend(egui::Shape::dashed_line(&pts, egui::Stroke::new(1.0_f32, Color32::BLACK), 4.0, 4.0));
            // линии сетки деформации — голубым, как в SAI
            if let Some(g) = &t.grid {
                let blue = egui::Stroke::new(1.0_f32, Color32::from_rgb(90, 170, 250));
                for line in g.guide_lines(8) {
                    let pts: Vec<Pos2> = line.iter().map(|p| to_screen(p[0] as f32, p[1] as f32)).collect();
                    painter.add(egui::Shape::line(pts, blue));
                }
            }
            for (_, w) in t.handle_points() {
                let c = to_screen(w.x, w.y);
                let hr = Rect::from_center_size(c, Vec2::splat(7.0));
                painter.rect_filled(hr, 0.0, Color32::WHITE);
                painter.rect_stroke(hr, 0.0, egui::Stroke::new(1.0_f32, Color32::BLACK));
            }
        }
        // Кружок размера кисти.
        if let Some(pos) = pointer.filter(|p| rect.contains(*p)) {
            if self.tool == Tool::Brush {
                let size = self.brushes[self.brush].params.size;
                let g = &self.brushes[self.brush].grain;
                let tip = g.tip.as_deref().and_then(paint_core::paper::stamp_of).or_else(|| (g.tip_ratio < 0.999).then(|| std::sync::Arc::new(paint_core::Stamp { width: 1, height: 1, alpha: vec![255] })));
                if self.win.brush_circle {
                    match tip {
                        // отпечаток-картинка — её рамка с поворотом (большая сторона — размер кисти)
                        Some(st) => {
                            let vx = ViewXf::new(rect, self.zoom, self.offset, vang, vflip);
                            let c = vx.to_canvas(pos);
                            let k = size / st.width.max(st.height).max(1) as f32;
                            let (hw, hh) = (st.width as f32 * k / 2.0, st.height as f32 * k * g.tip_ratio / 2.0);
                            let (sin, cos) = g.tip_angle.to_radians().sin_cos();
                            // у картинки — её рамка, у сплюснутого круга — эллипс
                            let outline: Vec<(f32, f32)> = if g.tip.is_some() {
                                vec![(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh), (-hw, -hh)]
                            } else {
                                (0..=48).map(|i| (i as f32 / 48.0 * std::f32::consts::TAU).sin_cos()).map(|(sn, cs)| (cs * hw, sn * hh)).collect()
                            };
                            let pts: Vec<Pos2> = outline.iter().map(|&(x, y)| vx.to_screen(c + Vec2::new(x * cos - y * sin, x * sin + y * cos))).collect();
                            painter.add(egui::Shape::line(pts, egui::Stroke::new(1.0_f32, Color32::from_gray(90))));
                        }
                        None => {
                            painter.circle_stroke(pos, (size * self.zoom / 2.0).max(1.0), egui::Stroke::new(1.0_f32, Color32::from_gray(90)));
                            // при «минимальном нажиме» — ещё и кружок наименьшего размера (SAI 2024+)
                            let bp = &self.brushes[self.brush].params;
                            if self.opts.min_pressure > 0.0 && bp.size_by_pressure && bp.min_size < 0.999 {
                                let r = (bp.diameter_at(0.0) * self.zoom / 2.0).max(1.0);
                                painter.circle_stroke(pos, r, egui::Stroke::new(1.0_f32, Color32::from_gray(150)));
                            }
                        }
                    }
                }
                if self.win.dot_cursor {
                    painter.circle_filled(pos, 1.5, Color32::from_gray(40));
                    ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                }
            }
        }
    }
}

impl PaintApp {
    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.show_keys {
            let mut open = true;
            let mut reset = false;
            egui::Window::new(tr("Клавиши")).open(&mut open).resizable(true).default_height(480.0).show(ctx, |ui| {
                ui.label(tr("Нажмите на клавишу действия, затем новое сочетание (Esc — отмена)."));
                ui.horizontal(|ui| {
                    ui.label(tr("Поиск:"));
                    ui.add(egui::TextEdit::singleline(&mut self.keys_filter).desired_width(200.0));
                });
                ui.label(egui::RichText::new(tr("Свою клавишу инструмента сетки задают в его «Свойствах…» (правый щелчок по инструменту).")).small().weak());
                let filter = self.keys_filter.to_lowercase();
                egui::ScrollArea::vertical().id_salt("keys_list").max_height(420.0).show(ui, |ui| {
                    egui::Grid::new("keys").num_columns(2).striped(true).show(ui, |ui| {
                        for (a, _, name, _) in keys::ALL {
                            if !filter.is_empty() && !tr(name).to_lowercase().contains(&filter) {
                                continue;
                            }
                            ui.label(tr(name));
                            let text = if self.key_capture == Some(a) {
                                tr("нажмите сочетание…").to_string()
                            } else {
                                self.keymap.get(&a).copied().flatten().map_or_else(|| "—".to_string(), |b| b.label())
                            };
                            ui.horizontal(|ui| {
                                if ui.button(text).clicked() {
                                    self.key_capture = Some(a);
                                }
                                if ui.small_button("×").on_hover_text(tr("Убрать клавишу")).clicked() {
                                    self.keymap.insert(a, None);
                                }
                            });
                            ui.end_row();
                        }
                    });
                });
                if ui.button(tr("Как в SAI (по умолчанию)")).clicked() {
                    reset = true;
                }
            });
            if reset {
                self.keymap = keys::defaults();
            }
            if !open {
                self.show_keys = false;
                self.key_capture = None;
            }
        }
        let mut answer = None;
        if let Some((_, original)) = &self.recovery_found {
            egui::Window::new(tr("Восстановление работы")).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.label(tr("Прошлый сеанс закончился без сохранения."));
                if !original.is_empty() {
                    ui.label(trf("Файл: {0}", &[&original]));
                }
                ui.label(tr("Есть копия холста, записанная перед сбоем."));
                ui.horizontal(|ui| {
                    if ui.button(tr("Восстановить")).clicked() {
                        answer = Some(true);
                    }
                    if ui.button(tr("Удалить копию")).clicked() {
                        answer = Some(false);
                    }
                });
            });
        }
        if let Some(restore) = answer {
            if let Some((file, original)) = self.recovery_found.take() {
                if restore {
                    match paint_formats::sai2_doc::load(&file) {
                        Ok(loaded) => {
                            self.stop_recording();
                            self.doc = loaded.document;
                            self.file = (!original.is_empty()).then(|| PathBuf::from(&original));
                            self.dirty = true;
                            self.selected = first_raster(&self.doc.layers).unwrap_or_default();
                            self.fit_pending = true;
                            self.invalidate_all();
                            self.status = String::from(&keys::hint(tr("Работа восстановлена — сохраните её (Ctrl+S)")));
                        }
                        Err(e) => self.status = trf("Не удалось восстановить: {0}", &[&e]),
                    }
                }
                if let Some((f, info)) = recovery_files() {
                    let _ = std::fs::remove_file(f);
                    let _ = std::fs::remove_file(info);
                }
            }
        }
        // градиент применяется, если сменили инструмент или слой; лассо без инструмента — отменяется
        if self.gradient.as_ref().is_some_and(|e| self.tool != Tool::Brush || self.kind() != ToolKind::Gradient || e.path != self.selected) {
            self.finish_gradient(true);
        }
        self.update_gradient();
        let fill_poly = self.tool == Tool::Brush && self.kind() == ToolKind::LassoFill && self.lassofill.shape != 0;
        if self.sel_poly.as_ref().is_some_and(|d| !(if d.fill { fill_poly } else { self.tool == Tool::Lasso && self.lasso_polygon })) {
            self.sel_poly = None;
        }
        if let Some(i) = self.close_ask {
            let name = match self.tabs.get(i) {
                Some(Some(t)) => t.title(),
                _ => self.file_name(),
            };
            let mut choice = None;
            egui::Window::new(tr("Закрыть холст")).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.label(trf("В «{0}» есть несохранённые изменения.", &[&name]));
                ui.horizontal(|ui| {
                    if ui.button(tr("Закрыть без сохранения")).clicked() {
                        choice = Some(true);
                    }
                    if ui.button(tr("Отмена")).clicked() {
                        choice = Some(false);
                    }
                });
            });
            match choice {
                Some(true) => {
                    self.close_ask = None;
                    self.close_tab(i);
                    self.close_all_step();
                }
                Some(false) => {
                    self.close_ask = None;
                    self.closing_all = false;
                }
                None => {}
            }
        }
        let mut bg_done = None;
        if let Some(c) = self.bg_color_edit.as_mut() {
            let mut open = true;
            egui::Window::new(tr("Цвет фона холста")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                let mut col = Color32::from_rgb(c[0], c[1], c[2]);
                if egui::color_picker::color_picker_color32(ui, &mut col, egui::color_picker::Alpha::Opaque) {
                    *c = [col.r(), col.g(), col.b()];
                }
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        bg_done = Some(true);
                    }
                    if ui.button(tr("Отмена")).clicked() {
                        bg_done = Some(false);
                    }
                });
            });
            if !open {
                bg_done = Some(false);
            }
        }
        if let Some(ok) = bg_done {
            let c = self.bg_color_edit.take().unwrap();
            if ok && c != self.doc.bg_color {
                let code = self.doc.bg_code;
                self.doc.set_background(code, c);
                self.invalidate_all();
                self.dirty = true;
            }
        }
        let mut lp_done = None;
        if let Some((_, name)) = self.layer_props.as_mut() {
            let mut open = true;
            egui::Window::new(tr("Свойства слоя")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(tr("Имя слоя:"));
                    let r = ui.add(egui::TextEdit::singleline(name).desired_width(200.0));
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        lp_done = Some(true);
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        lp_done = Some(true);
                    }
                    if ui.button(tr("Отмена")).clicked() {
                        lp_done = Some(false);
                    }
                });
            });
            if !open {
                lp_done = Some(false);
            }
        }
        if let Some(ok) = lp_done {
            let (path, name) = self.layer_props.take().unwrap();
            let name = name.trim().to_string();
            if ok && !name.is_empty() {
                if let Some(l) = self.doc.layer_mut(&path) {
                    if l.name != name {
                        l.name = name;
                        self.dirty = true;
                    }
                }
            }
        }
        let mut props_done = None;
        let capturing = self.tool_key_capture;
        let mut start_capture = false;
        if let Some((_, name, extra, key)) = self.tool_props.as_mut() {
            let mut open = true;
            egui::Window::new(tr("Свойства инструмента")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                egui::Grid::new("toolprops").num_columns(2).show(ui, |ui| {
                    ui.label(tr("Имя:"));
                    ui.add(egui::TextEdit::singleline(name).char_limit(16).desired_width(140.0));
                    ui.end_row();
                    ui.label(tr("Вторая строка:"));
                    ui.add(egui::TextEdit::singleline(extra).char_limit(16).desired_width(140.0));
                    ui.end_row();
                    ui.label(tr("Клавиша:"));
                    ui.horizontal(|ui| {
                        let text = if capturing { tr("нажмите сочетание…").to_string() } else { key.map_or_else(|| "—".to_string(), |b| b.label()) };
                        if ui.button(text).clicked() {
                            start_capture = true;
                        }
                        if ui.small_button("×").on_hover_text(tr("Убрать клавишу")).clicked() {
                            *key = None;
                        }
                    });
                    ui.end_row();
                });
                ui.label(egui::RichText::new(tr("Имя — до 8 знаков, как в SAI; вторая строка видна под ним.")).small().weak());
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        props_done = Some(true);
                    }
                    if ui.button(tr("Отмена")).clicked() {
                        props_done = Some(false);
                    }
                });
            });
            if !open {
                props_done = Some(false);
            }
        }
        if start_capture {
            self.tool_key_capture = true;
        }
        if let Some(ok) = props_done {
            self.tool_key_capture = false;
            let (i, name, extra, key) = self.tool_props.take().unwrap();
            if let (true, Some(t)) = (ok, self.brushes.get_mut(i)) {
                let name = name.trim().to_string();
                t.custom = (!name.is_empty() && name != tr(t.name)).then_some(name);
                t.extra = extra.trim().to_string();
                t.key = key;
            }
            if ok {
                // клавиша снимается с других инструментов (как у действий)
                if let Some(k) = key {
                    for (j, b) in self.brushes.iter_mut().enumerate() {
                        if j != i && b.key == Some(k) {
                            b.key = None;
                        }
                    }
                }
            }
        }
        let mut outline_finish = None;
        if let Some(d) = self.outline_dialog.as_mut() {
            let mut open = true;
            egui::Window::new(tr("Обводка")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                let before = (d.width, d.position, d.canvas_edges, d.preview);
                ui.horizontal(|ui| {
                    ui.label(tr("Ширина:"));
                    ui.spacing_mut().slider_width = 220.0;
                    ui.add(egui::Slider::new(&mut d.width, 1..=25).suffix(" px"));
                });
                ui.horizontal(|ui| {
                    ui.label(tr("Положение:"));
                    ui.radio_value(&mut d.position, 0, tr("Внутри"));
                    ui.radio_value(&mut d.position, 1, tr("По центру"));
                    ui.radio_value(&mut d.position, 2, tr("Снаружи"));
                });
                ui.checkbox(&mut d.canvas_edges, tr("Также по краям холста"));
                ui.horizontal(|ui| {
                    ui.checkbox(&mut d.preview, tr("Предпросмотр"));
                    if ui.button("OK").clicked() {
                        outline_finish = Some(true);
                    }
                    if ui.button(tr("Отмена")).clicked() {
                        outline_finish = Some(false);
                    }
                });
                if before != (d.width, d.position, d.canvas_edges, d.preview) {
                    d.dirty = true;
                }
            });
            if !open {
                outline_finish = Some(false);
            }
        }
        if let Some(apply) = outline_finish {
            let d = self.outline_dialog.take().unwrap();
            let keys = self.doc.set_layer_pixels(&d.path, d.original.clone());
            self.invalidate(keys);
            self.outline_last = (d.width, d.position, d.canvas_edges);
            if apply {
                let c = self.color();
                let (o, i) = d.spread();
                let keys = self.doc.outline(&d.path, o, i, d.canvas_edges, [c.r(), c.g(), c.b()]);
                self.invalidate(keys);
                self.dirty = true;
                self.thumbs.clear();
            }
        } else if self.outline_dialog.as_ref().is_some_and(|d| d.dirty) {
            let c = self.color();
            let d = self.outline_dialog.as_mut().unwrap();
            d.dirty = false;
            let d = self.outline_dialog.as_ref().unwrap();
            let mut pixels = d.original.clone();
            if d.preview {
                let (o, i) = d.spread();
                let preserve = self.doc.layer(&d.path).is_some_and(|l| l.preserve_opacity);
                pixels.tiles.extend(self.doc.outline_tiles(&d.original, preserve, o, i, d.canvas_edges, [c.r(), c.g(), c.b()]));
            }
            let path = d.path.clone();
            let keys = self.doc.set_layer_pixels(&path, pixels);
            self.invalidate(keys);
        }
        let mut grow_finish = None;
        if let Some(d) = self.grow_dialog.as_mut() {
            let mut open = true;
            egui::Window::new(if d.grow { tr("Расширить выделение") } else { tr("Сузить выделение") })
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    let before = (d.n, d.preview);
                    ui.horizontal(|ui| {
                        ui.label(tr("Ширина:"));
                        ui.spacing_mut().slider_width = 220.0;
                        ui.add(egui::Slider::new(&mut d.n, 1..=25).suffix(" px"));
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut d.preview, tr("Предпросмотр"));
                        if ui.button("OK").clicked() {
                            grow_finish = Some(true);
                        }
                        if ui.button(tr("Отмена")).clicked() {
                            grow_finish = Some(false);
                        }
                    });
                    if before != (d.n, d.preview) {
                        d.dirty = true;
                    }
                });
            if !open {
                grow_finish = Some(false);
            }
        }
        if let Some(apply) = grow_finish {
            let d = self.grow_dialog.take().unwrap();
            self.grow_last = d.n;
            self.doc.selection = d.original;
            if apply {
                self.doc.grow_selection_by(d.n, d.grow);
            }
            self.sel_dirty = true;
        } else if let Some(d) = self.grow_dialog.as_mut().filter(|d| d.dirty) {
            d.dirty = false;
            let mut s = d.original.clone();
            if d.preview {
                s.grow_by(d.n, d.grow);
            }
            self.doc.selection = s;
            self.sel_dirty = true;
        }
        let mut finish = None;
        let mut reset = false;
        let busy = self.filter_job.is_some();
        if let Some(f) = self.filter.as_mut() {
            let title = match f.kind {
                FilterKind::HueSaturation => tr("Тон и насыщенность"),
                FilterKind::BrightnessContrast => tr("Яркость и контраст"),
                FilterKind::GaussianBlur => tr("Размытие по Гауссу"),
                FilterKind::Sharpen => tr("Резкость"),
                FilterKind::SharpenMore => tr("Сильная резкость"),
                FilterKind::Unsharp => tr("Нерезкая маска"),
                FilterKind::LineWidth => tr("Изменение толщины линий"),
                FilterKind::Dust => tr("Удаление пыли"),
                FilterKind::Levels => tr("Уровни"),
                FilterKind::Curves => tr("Кривые"),
                FilterKind::Median => tr("Медиана"),
                FilterKind::Aberration => tr("Хроматическая аберрация"),
                FilterKind::Noise => tr("Шум"),
                FilterKind::Mosaic => tr("Мозаика"),
                FilterKind::Crystallize => tr("Кристаллизация"),
                FilterKind::RetroFilm => tr("Ретро-плёнка"),
                FilterKind::Perlin => tr("Шум Перлина"),
                FilterKind::Average => tr("Среднее"),
                FilterKind::Blur => tr("Размытие"),
                FilterKind::BlurStrong => tr("Сильное размытие"),
                FilterKind::Motion => tr("Размытие в движении"),
                FilterKind::Radial => tr("Радиальное размытие"),
                FilterKind::Spin => tr("Размытие вращением"),
                FilterKind::Smoothing => tr("Сглаживание краёв"),
                FilterKind::Lens => tr("Размытие линзы"),
                FilterKind::Pencil => tr("Карандашный рисунок"),
                FilterKind::Artistic => tr("Художественный"),
                FilterKind::Distort(k) => {
                    use paint_core::filters::DistortKind as D;
                    tr(match k {
                        D::Pinch => "Щипок",
                        D::Ripple => "Рябь",
                        D::Fisheye => "Рыбий глаз",
                        D::Polar => "Полярные координаты",
                        D::Wave => "Волна",
                        D::Twirl => "Скручивание",
                    })
                }
            };
            let mut open = true;
            egui::Window::new(title).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                let before = (f.hs, f.bc, f.radius, f.preview, f.amount, f.threshold, (f.width_px, f.thicken, f.dust_size, f.dust_mode));
                let mut ch = false;
                egui::Grid::new("filter").num_columns(2).show(ui, |ui| {
                    match f.kind {
                        FilterKind::HueSaturation => {
                            ui.label(tr("Тон:"));
                            ui.add(egui::Slider::new(&mut f.hs.hue, -180..=180));
                            ui.end_row();
                            ui.label(tr("Насыщенность:"));
                            ui.add(egui::Slider::new(&mut f.hs.saturation, -100..=100));
                            ui.end_row();
                            ui.label(tr("Яркость:"));
                            ui.add(egui::Slider::new(&mut f.hs.luminance, -100..=100));
                            ui.end_row();
                            ui.label("");
                            ui.checkbox(&mut f.hs.colorize, tr("Окрасить"));
                            ui.end_row();
                        }
                        FilterKind::BrightnessContrast => {
                            ui.label(tr("Яркость:"));
                            ui.add(egui::Slider::new(&mut f.bc.brightness, -100..=100));
                            ui.end_row();
                            ui.label(tr("Контраст:"));
                            ui.add(egui::Slider::new(&mut f.bc.contrast, -100..=100));
                            ui.end_row();
                            ui.label(tr("Глубина:"));
                            ui.add(egui::Slider::new(&mut f.bc.depth, -100..=100));
                            ui.end_row();
                        }
                        FilterKind::GaussianBlur => {
                            ui.label(tr("Радиус:"));
                            ui.add(egui::Slider::new(&mut f.radius, 0.1..=40.0).step_by(0.1));
                            ui.end_row();
                        }
                        FilterKind::Average | FilterKind::Blur | FilterKind::BlurStrong | FilterKind::Smoothing => {
                            ui.label(tr("Без настроек — OK применяет."));
                            ui.end_row();
                        }
                        FilterKind::Motion | FilterKind::Radial => {
                            let st = if f.kind == FilterKind::Motion { &mut f.motion } else { &mut f.radial };
                            ui.label(tr("Сила:"));
                            if f.kind == FilterKind::Motion {
                                ch |= ui.add(egui::Slider::new(&mut st.strength, 1.0..=300.0).suffix(" px")).changed();
                            } else {
                                ch |= ui.add(egui::Slider::new(&mut st.strength, 0.01..=1.0)).changed();
                            }
                            ui.end_row();
                            if f.kind == FilterKind::Motion {
                                ui.label(tr("Угол:"));
                                ch |= ui.add(egui::Slider::new(&mut st.angle, 0.0..=360.0).suffix("°")).changed();
                                ui.end_row();
                            }
                            ui.label(tr("Направление:"));
                            ui.horizontal(|ui| {
                                let names = if f.kind == FilterKind::Motion { [tr("В обе стороны"), tr("Вперёд"), tr("Назад")] } else { [tr("В обе стороны"), tr("Наружу"), tr("Внутрь")] };
                                for (i, n) in names.iter().enumerate() {
                                    ch |= ui.radio_value(&mut st.dir, i as u8, *n).changed();
                                }
                            });
                            ui.end_row();
                            ui.label("");
                            ch |= ui.checkbox(&mut st.smooth, tr("Плавно")).changed();
                            ui.end_row();
                        }
                        FilterKind::Spin => {
                            ui.label(tr("Угол:"));
                            ch |= ui.add(egui::Slider::new(&mut f.spin_angle, 1.0..=180.0).suffix("°")).changed();
                            ui.end_row();
                            ui.label(tr("Направление:"));
                            ui.horizontal(|ui| {
                                for (i, n) in [tr("В обе стороны"), tr("По часовой"), tr("Против часовой")].iter().enumerate() {
                                    ch |= ui.radio_value(&mut f.spin_dir, i as u8, *n).changed();
                                }
                            });
                            ui.end_row();
                        }
                        FilterKind::Pencil => {
                            let p = &mut f.pencil;
                            ui.label(tr("Толщина линий:"));
                            ch |= ui.add(egui::Slider::new(&mut p.thickness, 1.0..=20.0).suffix(" px")).changed();
                            ui.end_row();
                            ui.label(tr("Темнота:"));
                            ch |= ui.add(egui::Slider::new(&mut p.darkness, 0.0..=1.0)).changed();
                            ui.end_row();
                            ui.label(tr("Штриховка:"));
                            ch |= ui.add(egui::Slider::new(&mut p.hatching, 0.0..=1.0)).changed();
                            ui.end_row();
                            ui.label("");
                            ch |= ui.checkbox(&mut p.color, tr("Оставить цвет")).changed();
                            ui.end_row();
                        }
                        FilterKind::Artistic => {
                            let a = &mut f.artistic;
                            ui.label(tr("Мазок:"));
                            ch |= ui.add(egui::Slider::new(&mut a.radius, 1..=12).suffix(" px")).changed();
                            ui.end_row();
                            ui.label(tr("Уровни яркости:"));
                            ch |= ui.add(egui::Slider::new(&mut a.levels, 1..=16).custom_formatter(|v, _| if v < 2.0 { tr("нет").to_string() } else { format!("{v}") })).changed();
                            ui.end_row();
                            ui.label(tr("Контуры:"));
                            ch |= ui.add(egui::Slider::new(&mut a.edges, 0.0..=1.0)).changed();
                            ui.end_row();
                        }
                        FilterKind::Distort(k) => {
                            use paint_core::filters::DistortKind as D;
                            let (w, h) = (self.doc.width as f64, self.doc.height as f64);
                            let d = &mut f.distort;
                            if k != D::Wave {
                                ui.label(tr("Середина X:"));
                                ch |= ui.add(egui::Slider::new(&mut d.center.0, 0.0..=w).suffix(" px")).changed();
                                ui.end_row();
                                ui.label(tr("Середина Y:"));
                                ch |= ui.add(egui::Slider::new(&mut d.center.1, 0.0..=h).suffix(" px")).changed();
                                ui.end_row();
                            }
                            if !matches!(k, D::Wave | D::Polar) {
                                ui.label(tr("Радиус:"));
                                ch |= ui.add(egui::Slider::new(&mut d.radius, 1.0..=w.max(h)).suffix(" px")).changed();
                                ui.end_row();
                            }
                            match k {
                                D::Pinch | D::Fisheye => {
                                    ui.label(tr("Сила:"));
                                    let lo = if k == D::Pinch { -1.0 } else { 0.0 };
                                    ch |= ui.add(egui::Slider::new(&mut d.amount, lo..=if k == D::Pinch { 1.0 } else { 0.95 })).changed();
                                    ui.end_row();
                                }
                                D::Twirl => {
                                    ui.label(tr("Угол:"));
                                    ch |= ui.add(egui::Slider::new(&mut d.amount, -720.0..=720.0).suffix("°")).changed();
                                    ui.end_row();
                                }
                                D::Ripple | D::Wave => {
                                    ui.label(tr("Размах:"));
                                    ch |= ui.add(egui::Slider::new(&mut d.amplitude, 0.0..=100.0).suffix(" px")).changed();
                                    ui.end_row();
                                    ui.label(tr("Длина волны:"));
                                    ch |= ui.add(egui::Slider::new(&mut d.wavelength, 2.0..=400.0).suffix(" px")).changed();
                                    ui.end_row();
                                    if k == D::Wave {
                                        ui.label(tr("Угол:"));
                                        ch |= ui.add(egui::Slider::new(&mut d.angle, 0.0..=360.0).suffix("°")).changed();
                                        ui.end_row();
                                        ui.label(tr("Форма:"));
                                        ui.horizontal(|ui| {
                                            for (i, n) in [tr("Синус"), tr("Треугольник"), tr("Прямоугольник")].iter().enumerate() {
                                                ch |= ui.radio_value(&mut d.shape, i as u8, *n).changed();
                                            }
                                        });
                                        ui.end_row();
                                    }
                                }
                                D::Polar => {
                                    ui.label(tr("Режим:"));
                                    ui.vertical(|ui| {
                                        ch |= ui.radio_value(&mut d.to_polar, true, tr("Прямоугольные → полярные")).changed();
                                        ch |= ui.radio_value(&mut d.to_polar, false, tr("Полярные → прямоугольные")).changed();
                                    });
                                    ui.end_row();
                                }
                            }
                        }
                        FilterKind::Lens => {
                            ui.label(tr("Радиус:"));
                            ch |= ui.add(egui::Slider::new(&mut f.lens_radius, 1.0..=100.0).suffix(" px")).changed();
                            ui.end_row();
                            ui.label(tr("Форма бликов:"));
                            egui::ComboBox::from_id_salt("lens_shape").selected_text(if f.lens_sides < 3 { tr("Круг").to_string() } else { trf("{0} угла", &[&f.lens_sides]) }).show_ui(ui, |ui| {
                                ch |= ui.selectable_value(&mut f.lens_sides, 0, tr("Круг")).changed();
                                for n in 3..=8u32 {
                                    ch |= ui.selectable_value(&mut f.lens_sides, n, trf("{0} угла", &[&n])).changed();
                                }
                            });
                            ui.end_row();
                            ui.label(tr("Яркость бликов:"));
                            ch |= ui.add(egui::Slider::new(&mut f.lens_highlight, 0.0..=1.0)).changed();
                            ui.end_row();
                        }
                        FilterKind::Sharpen | FilterKind::SharpenMore => {
                            ui.label(tr("Без настроек — OK применяет."));
                            ui.end_row();
                        }
                        FilterKind::Aberration => {
                            ui.label(tr("Режим:"));
                            ui.horizontal(|ui| {
                                ch |= ui.radio_value(&mut f.ca_radial, true, tr("По кругу")).changed();
                                ch |= ui.radio_value(&mut f.ca_radial, false, tr("Сдвигом")).changed();
                            });
                            ui.end_row();
                            ui.label(tr("Сила:"));
                            ch |= ui.add(egui::Slider::new(&mut f.ca_px, 0.0..=50.0).suffix(" px")).changed();
                            ui.end_row();
                            if !f.ca_radial {
                                ui.label(tr("Угол:"));
                                ch |= ui.add(egui::Slider::new(&mut f.ca_angle, 0.0..=360.0).suffix("°")).changed();
                                ui.end_row();
                            }
                        }
                        FilterKind::Noise => {
                            ui.label(tr("Режим:"));
                            ui.horizontal(|ui| {
                                ch |= ui.radio_value(&mut f.noise_color, false, tr("Серый")).changed();
                                ch |= ui.radio_value(&mut f.noise_color, true, tr("Цветной")).changed();
                            });
                            ui.end_row();
                            ui.label(tr("Сила:"));
                            ch |= ui.add(egui::Slider::new(&mut f.noise_amount, 0.0..=1.0)).changed();
                            ui.end_row();
                        }
                        FilterKind::Mosaic => {
                            ui.label(tr("Размер плитки:"));
                            ch |= ui.add(egui::Slider::new(&mut f.tile, 2..=200).suffix(" px")).changed();
                            ui.end_row();
                        }
                        FilterKind::Crystallize => {
                            ui.label(tr("Размер ячейки:"));
                            ch |= ui.add(egui::Slider::new(&mut f.cell, 2..=200).suffix(" px")).changed();
                            ui.end_row();
                            ui.label(tr("Разброс:"));
                            ch |= ui.add(egui::Slider::new(&mut f.randomness, 0.0..=1.0)).changed();
                            ui.end_row();
                            ui.label("");
                            ch |= ui.checkbox(&mut f.tiling, tr("Бесшовно")).changed();
                            ui.end_row();
                        }
                        FilterKind::RetroFilm => {
                            ui.label(tr("Пресет:"));
                            ui.horizontal(|ui| {
                                for (m, n) in [(0u8, tr("Винтаж")), (1, tr("Современный")), (2, tr("Тёплый"))] {
                                    ch |= ui.radio_value(&mut f.retro.preset, m, n).changed();
                                }
                            });
                            ui.end_row();
                            ui.label(tr("Эффект:"));
                            ui.horizontal(|ui| {
                                for (m, n) in [(0u8, tr("Нет")), (1, tr("Сепия")), (2, tr("Засветка")), (3, tr("Сепия и засветка"))] {
                                    ch |= ui.radio_value(&mut f.retro.effect, m, n).changed();
                                }
                            });
                            ui.end_row();
                            ui.label(tr("Сила:"));
                            ch |= ui.add(egui::Slider::new(&mut f.retro.intensity, 0.0..=1.0)).changed();
                            ui.end_row();
                            ui.label(tr("Шум:"));
                            ch |= ui.add(egui::Slider::new(&mut f.retro.noise, 0.0..=1.0)).changed();
                            ui.end_row();
                        }
                        FilterKind::Perlin => {
                            ui.label(tr("Масштаб:"));
                            ch |= ui.add(egui::Slider::new(&mut f.perlin.scale, 2.0..=1000.0).logarithmic(true).suffix(" px")).changed();
                            ui.end_row();
                            ui.label(tr("Амплитуда:"));
                            ch |= ui.add(egui::Slider::new(&mut f.perlin.amplitude, 0.0..=2.0)).changed();
                            ui.end_row();
                            ui.label(tr("Ослабление:"));
                            ch |= ui.add(egui::Slider::new(&mut f.perlin.attenuation, 0.0..=1.0)).changed();
                            ui.end_row();
                            ui.label(tr("Повторения:"));
                            ch |= ui.add(egui::Slider::new(&mut f.perlin.repetition, 1..=10)).changed();
                            ui.end_row();
                            ui.label(tr("Сдвиг:"));
                            ui.horizontal(|ui| {
                                ch |= ui.add(egui::DragValue::new(&mut f.perlin.offset[0]).prefix("X ")).changed();
                                ch |= ui.add(egui::DragValue::new(&mut f.perlin.offset[1]).prefix("Y ")).changed();
                            });
                            ui.end_row();
                            ui.label("");
                            ui.label(egui::RichText::new(tr("Узор заменяет содержимое слоя — лучше на пустом слое.")).small().weak());
                            ui.end_row();
                        }
                        FilterKind::LineWidth => {
                            ui.label(tr("Действие:"));
                            ui.horizontal(|ui| {
                                ui.radio_value(&mut f.thicken, false, tr("Сузить"));
                                ui.radio_value(&mut f.thicken, true, tr("Утолщить"));
                            });
                            ui.end_row();
                            ui.label(tr("На сколько:"));
                            ui.add(egui::Slider::new(&mut f.width_px, 1..=20).suffix(" px"));
                            ui.end_row();
                        }
                        FilterKind::Levels => {
                            let l = &mut f.levels;
                            ui.label(tr("Вход, чёрное:"));
                            ui.add(egui::Slider::new(&mut l.in_black, 0..=254));
                            ui.end_row();
                            ui.label(tr("Вход, белое:"));
                            ui.add(egui::Slider::new(&mut l.in_white, 1..=255));
                            ui.end_row();
                            ui.label(tr("Гамма:"));
                            ui.add(egui::Slider::new(&mut l.gamma, 0.1..=10.0).logarithmic(true));
                            ui.end_row();
                            ui.label(tr("Выход, чёрное:"));
                            ui.add(egui::Slider::new(&mut l.out_black, 0..=255));
                            ui.end_row();
                            ui.label(tr("Выход, белое:"));
                            ui.add(egui::Slider::new(&mut l.out_white, 0..=255));
                            ui.end_row();
                        }
                        FilterKind::Curves => {
                            ui.label(tr("Кривая:"));
                            ui.vertical(|ui| {
                                sai_ui::curve_editor(ui, &mut f.curve, 160.0);
                                if ui.button(tr("Прямая")).clicked() {
                                    f.curve = Default::default();
                                }
                            });
                            ui.end_row();
                        }
                        FilterKind::Median => {
                            ui.label(tr("Радиус:"));
                            ui.add(egui::Slider::new(&mut f.median_r, 1..=5).suffix(" px"));
                            ui.end_row();
                        }
                        FilterKind::Dust => {
                            ui.label(tr("Размер пыли:"));
                            ui.add(egui::Slider::new(&mut f.dust_size, 1..=400).logarithmic(true));
                            ui.end_row();
                            ui.label(tr("Режим:"));
                            ui.vertical(|ui| {
                                for (m, name) in [
                                    (0u8, tr("Убрать пыль с прозрачного")),
                                    (1, tr("Убрать пыль с белого фона")),
                                    (2, tr("Залить дырки цветом вокруг")),
                                    (3, tr("Залить дырки основным цветом")),
                                ] {
                                    ui.radio_value(&mut f.dust_mode, m, name);
                                }
                            });
                            ui.end_row();
                        }
                        FilterKind::Unsharp => {
                            ui.label(tr("Радиус:"));
                            ui.add(egui::Slider::new(&mut f.radius, 0.1..=50.0).step_by(0.1));
                            ui.end_row();
                            ui.label(tr("Сила:"));
                            let mut pct = (f.amount * 100.0).round() as i32;
                            if ui.add(egui::Slider::new(&mut pct, 1..=500).suffix(" %")).changed() {
                                f.amount = pct as f64 / 100.0;
                            }
                            ui.end_row();
                            ui.label(tr("Порог:"));
                            ui.add(egui::Slider::new(&mut f.threshold, 0..=255));
                            ui.end_row();
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut f.preview, tr("Предпросмотр"));
                    if ui.button("OK").clicked() {
                        finish = Some(true);
                    }
                    if ui.button(tr("Отмена")).clicked() {
                        finish = Some(false);
                    }
                    if ui.button(tr("Сбросить")).on_hover_text(tr("Вернуть настройки фильтра по умолчанию")).clicked() {
                        reset = true;
                    }
                    if busy {
                        ui.spinner().on_hover_text(tr("Считается предпросмотр…"));
                    }
                });
                if ch || before != (f.hs, f.bc, f.radius, f.preview, f.amount, f.threshold, (f.width_px, f.thicken, f.dust_size, f.dust_mode)) {
                    f.dirty = true;
                }
            });
            if !open {
                finish = Some(false);
            }
        }
        if let Some(apply) = finish {
            self.finish_filter(apply);
        } else if reset {
            // окно открывается заново без прошлых настроек; слой — исходный
            if let Some(kind) = self.filter.as_ref().map(|f| f.kind) {
                self.finish_filter(false);
                self.filter_last = None;
                self.open_filter(kind);
            }
        } else {
            self.poll_filter();
            if self.filter.as_ref().is_some_and(|f| f.dirty) {
                self.update_filter(ctx);
            }
        }
        let mut apply = None;
        let mut close = false;
        if let Some(d) = self.canvas_dialog.as_mut() {
            let (w, h) = (self.doc.width, self.doc.height);
            let mut open = true;
            egui::Window::new(if d.resample { tr("Разрешение холста") } else { tr("Размер холста") })
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(trf("Сейчас: {0}×{1} px", &[&w, &h]));
                    let before = d.size;
                    egui::Grid::new("canvasdlg").num_columns(2).show(ui, |ui| {
                        ui.label(tr("Ширина:"));
                        ui.add(egui::DragValue::new(&mut d.size[0]).range(1..=16000).suffix(" px"));
                        ui.end_row();
                        ui.label(tr("Высота:"));
                        ui.add(egui::DragValue::new(&mut d.size[1]).range(1..=16000).suffix(" px"));
                        ui.end_row();
                        ui.label("");
                        ui.checkbox(&mut d.keep_ratio, tr("Сохранять пропорции"));
                        ui.end_row();
                    });
                    if d.keep_ratio {
                        if d.size[0] != before[0] {
                            d.size[1] = ((d.size[0] as f64 * h as f64 / w as f64).round() as u32).max(1);
                        } else if d.size[1] != before[1] {
                            d.size[0] = ((d.size[1] as f64 * w as f64 / h as f64).round() as u32).max(1);
                        }
                    }
                    ui.horizontal(|ui| {
                        for p in [50u32, 75, 150, 200] {
                            if ui.small_button(format!("{p}%")).clicked() {
                                d.size = [(w * p / 100).max(1), (h * p / 100).max(1)];
                            }
                        }
                    });
                    if d.resample {
                        use paint_core::resample::Mode;
                        let name = |m: Mode| match m {
                            Mode::Nearest => tr("Ближайший"),
                            Mode::Bilinear => tr("Билинейный"),
                            Mode::Bicubic => tr("Бикубический"),
                            Mode::Lanczos2 => tr("Ланцош 2"),
                            Mode::Lanczos3 => tr("Ланцош 3"),
                        };
                        egui::ComboBox::from_label(tr("Способ")).selected_text(name(d.mode)).show_ui(ui, |ui| {
                            for m in [Mode::Nearest, Mode::Bilinear, Mode::Bicubic, Mode::Lanczos2, Mode::Lanczos3] {
                                ui.selectable_value(&mut d.mode, m, name(m));
                            }
                        });
                    } else {
                        ui.label(tr("Привязка рисунка:"));
                        egui::Grid::new("anchor").spacing([2.0, 2.0]).show(ui, |ui| {
                            for ay in 0..3u8 {
                                for ax in 0..3u8 {
                                    let on = d.anchor == (ax, ay);
                                    if ui.selectable_label(on, if on { "●" } else { "○" }).clicked() {
                                        d.anchor = (ax, ay);
                                    }
                                }
                                ui.end_row();
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            apply = Some((d.resample, d.size, d.mode, d.anchor));
                        }
                        if ui.button(tr("Отмена")).clicked() {
                            close = true;
                        }
                    });
                });
            if !open {
                close = true;
            }
        }
        if let Some(d) = &self.canvas_dialog {
            self.canvas_last = (d.keep_ratio, d.mode, d.anchor);
        }
        if let Some((resample, [nw, nh], mode, anchor)) = apply {
            self.canvas_dialog = None;
            if (nw, nh) != (self.doc.width, self.doc.height) {
                if resample {
                    self.canvas_op(|d| d.resample_canvas(nw, nh, mode));
                } else {
                    self.canvas_op(|d| d.resize_canvas(nw, nh, anchor));
                }
            }
        } else if close {
            self.canvas_dialog = None;
        }
        if self.show_open {
            let mut open = true;
            egui::Window::new(tr("Открыть"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(tr("Путь к файлу .sai2 (или перетащите файл в окно):"));
                    ui.add(egui::TextEdit::singleline(&mut self.open_path).desired_width(420.0));
                    ui.horizontal(|ui| {
                        if ui.button(tr("Открыть")).clicked() && !self.open_path.trim().is_empty()
                        {
                            self.open(PathBuf::from(self.open_path.trim()));
                            self.show_open = false;
                        }
                        if ui.button(tr("Отмена")).clicked() {
                            self.show_open = false;
                        }
                    });
                });
            self.show_open &= open;
        }
        self.assets_window(ctx);
        if self.ask_gray {
            let mut choice = None;
            egui::Window::new(tr("Оттенки серого")).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.label(tr("Все слои холста станут серыми. Цвет вернёт только отмена (Ctrl+Z)."));
                ui.horizontal(|ui| {
                    if ui.button(tr("Перевести")).clicked() {
                        choice = Some(true);
                    }
                    if ui.button(tr("Отмена")).clicked() {
                        choice = Some(false);
                    }
                });
            });
            if let Some(ok) = choice {
                self.ask_gray = false;
                if ok {
                    if self.transform.is_some() {
                        self.finish_transform(true);
                    }
                    let keys = self.doc.to_grayscale();
                    self.invalidate(keys);
                    self.thumbs.clear();
                    self.dirty = true;
                }
            }
        }
        if self.show_new {
            let mut open = true;
            egui::Window::new(tr("Новый холст"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    egui::Grid::new("newcanvas").num_columns(2).show(ui, |ui| {
                        ui.label(tr("Ширина:"));
                        ui.add(
                            egui::DragValue::new(&mut self.new_size[0])
                                .range(1..=16000)
                                .suffix(" px"),
                        );
                        ui.end_row();
                        ui.label(tr("Высота:"));
                        ui.add(
                            egui::DragValue::new(&mut self.new_size[1])
                                .range(1..=16000)
                                .suffix(" px"),
                        );
                        ui.end_row();
                    });
                    // заготовки: выбрать, добавить текущий размер, изменить, удалить, порядок
                    if self.preset_sel.is_none() {
                        self.preset_sel = self.canvas_presets.iter().position(|p| [p.1, p.2] == self.new_size);
                    }
                    ui.horizontal(|ui| {
                        ui.label(tr("Заготовка:"));
                        let shown = self.preset_sel.and_then(|i| self.canvas_presets.get(i)).map_or(String::from(tr("(свой размер)")), |p| format!("{} — {}×{}", p.0, p.1, p.2));
                        egui::ComboBox::from_id_salt("canvas_preset").selected_text(shown).width(220.0).show_ui(ui, |ui| {
                            for (i, (n, w, h)) in self.canvas_presets.iter().enumerate() {
                                if ui.selectable_label(self.preset_sel == Some(i), format!("{n} — {w}×{h}")).clicked() {
                                    self.preset_sel = Some(i);
                                    self.new_size = [*w, *h];
                                }
                            }
                        });
                    });
                    ui.horizontal(|ui| {
                        let mut changed = false;
                        if ui.small_button("+").on_hover_text(tr("Добавить текущий размер в заготовки")).clicked() {
                            let [w, h] = self.new_size;
                            self.canvas_presets.push((format!("{w} × {h}"), w, h));
                            self.preset_sel = Some(self.canvas_presets.len() - 1);
                            changed = true;
                        }
                        if let Some(i) = self.preset_sel.filter(|&i| i < self.canvas_presets.len()) {
                            if ui.small_button(tr("Изменить")).on_hover_text(tr("Поставить заготовке текущий размер")).clicked() {
                                let [w, h] = self.new_size;
                                self.canvas_presets[i].1 = w;
                                self.canvas_presets[i].2 = h;
                                changed = true;
                            }
                            if ui.small_button("↑").clicked() && i > 0 {
                                self.canvas_presets.swap(i, i - 1);
                                self.preset_sel = Some(i - 1);
                                changed = true;
                            }
                            if ui.small_button("↓").clicked() && i + 1 < self.canvas_presets.len() {
                                self.canvas_presets.swap(i, i + 1);
                                self.preset_sel = Some(i + 1);
                                changed = true;
                            }
                            if ui.small_button(tr("Удалить")).clicked() {
                                self.canvas_presets.remove(i);
                                self.preset_sel = None;
                                changed = true;
                            }
                        }
                        if changed {
                            self.save_settings();
                        }
                    });
                    if let Some(i) = self.preset_sel.filter(|&i| i < self.canvas_presets.len()) {
                        ui.horizontal(|ui| {
                            ui.label(tr("Имя заготовки:"));
                            if ui.text_edit_singleline(&mut self.canvas_presets[i].0).lost_focus() {
                                self.save_settings();
                            }
                        });
                        let (_, w, h) = self.canvas_presets[i];
                        if self.new_size != [w, h] {
                            self.preset_sel = None;
                        }
                    }
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            let [w, h] = self.new_size;
                            self.new_canvas(w, h);
                            self.show_new = false;
                        }
                        if ui.button(tr("Отмена")).clicked() {
                            self.show_new = false;
                        }
                    });
                });
            self.show_new &= open;
        }
        if self.show_save_as {
            let mut open = true;
            egui::Window::new(tr("Сохранить как")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.label(tr("Путь к файлу .sai2:"));
                ui.add(egui::TextEdit::singleline(&mut self.save_path).desired_width(460.0));
                if self.had_vectors {
                    ui.colored_label(egui::Color32::from_rgb(170, 60, 20), tr("В открытом файле есть слои контура или фигур: их кривые пока сохраняются\nрастром. Поэтому предложен новый файл — исходный останется нетронутым."));
                }
                let path = PathBuf::from(self.save_path.trim());
                let exists = path.exists();
                if exists {
                    ui.colored_label(egui::Color32::from_rgb(170, 60, 20), tr("Такой файл уже есть — он будет заменён."));
                }
                ui.horizontal(|ui| {
                    if ui.button(if exists { tr("Заменить") } else { tr("Сохранить") }).clicked() && !self.save_path.trim().is_empty() {
                        let ext = layered_ext(&path).unwrap_or("sai2");
                        self.save_to(path.with_extension(ext));
                        self.show_save_as = false;
                    }
                    if ui.button(tr("Отмена")).clicked() {
                        self.show_save_as = false;
                    }
                });
            });
            self.show_save_as &= open;
        }
        if self.show_about {
            let mut open = true;
            egui::Window::new(tr("О программе")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.label(trf("GrazySAI {0} — редактор по образцу PaintTool SAI 2, личный проект.", &[&env!("CARGO_PKG_VERSION")]));
                ui.horizontal(|ui| {
                    ui.label(tr("Исходный код:"));
                    if ui.link("github.com/grazyping/GrazySAI").clicked() {
                        update::open_url(update::REPO);
                    }
                });
                ui.label(tr("Открывает .sai2 (растровые слои, папки, контур, фигуры), рисует кистью образца."));
            });
            self.show_about &= open;
        }
        if self.show_update {
            self.update_window(ctx);
        }
        if self.exit_ask {
            self.exit_window(ctx);
        }
        if self.opts_edit.is_some() {
            self.options_window(ctx);
        }
        if self.show_history {
            self.history_window(ctx);
        }
        if self.popup_at.is_some() {
            self.popup_palette(ctx);
        }
        if !self.refs.is_empty() {
            self.reference_windows(ctx);
        }
        if self.plugin_ask.is_some() || self.plugin_install.is_some() || self.show_plugins {
            self.plugin_windows(ctx);
        }
        self.record_export_window(ctx);
        self.s3d_window(ctx);
        if self.show_watermark {
            self.watermark_window(ctx);
        }
        if self.viewer.is_some() {
            self.viewer_window(ctx);
        }
    }

    /// Черновик: рисование текущей кистью, Alt + щелчок или правая кнопка — взять цвет.
    fn scratch_pad(&mut self, ui: &mut egui::Ui) {
        // масштаб экрана сменился (другой монитор, «Масштаб интерфейса») — черновик заново
        let ppp = ui.ctx().pixels_per_point();
        if (self.scratch.scale - ppp).abs() > 0.01 && self.scratch.live.is_none() {
            self.scratch = ScratchPad::new(ppp);
        }
        let k = self.scratch.scale;
        let size = Vec2::new(ScratchPad::W as f32, ScratchPad::H as f32);
        let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        // точка экрана → пиксель черновика
        let local = |p: Pos2| ((p - rect.min) * k).to_pos2();
        let alt = ui.input(|i| i.modifiers.alt);
        let pick = |pad: &ScratchPad, p: Pos2| -> Option<Color32> {
            let (w, h) = (pad.doc.width, pad.doc.height);
            let (x, y) = (p.x as i32, p.y as i32);
            if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                return None;
            }
            let flat = pad.doc.flatten();
            let i = ((y as u32 * w + x as u32) * 4) as usize;
            Some(Color32::from_rgb(flat[i], flat[i + 1], flat[i + 2]))
        };
        if resp.secondary_clicked() || (alt && resp.clicked()) {
            if let Some(c) = resp.interact_pointer_pos().and_then(|p| pick(&self.scratch, local(p))) {
                self.set_color(c);
            }
        } else if !alt {
            // нажим пера — как на холсте
            let (pressure, _) = self.pressure();
            if resp.drag_started_by(egui::PointerButton::Primary) {
                let mut params = self.brushes[self.brush].params;
                params.size = params.size.min(60.0);
                if self.transparent {
                    params.erase = true;
                    params.mix = None;
                }
                let mut stroke = Stroke::new(params);
                let rgb = self.stroke_color(&mut stroke);
                self.scratch.live = Some(LiveStroke::new(vec![0], stroke, rgb));
            }
            if self.scratch.live.is_some() {
                // все движения пера за кадр, а не одно последнее — иначе мазок становится ломаной
                let mut pts: Vec<Pos2> = ui.input(|i| i.events.iter().filter_map(|e| if let egui::Event::PointerMoved(p) = e { Some(*p) } else { None }).collect());
                if pts.is_empty() {
                    pts.extend(resp.interact_pointer_pos());
                }
                if let Some(live) = self.scratch.live.as_mut() {
                    for p in pts {
                        let q = local(p);
                        live.stroke.add_point(InputPoint { x: q.x, y: q.y, pressure });
                    }
                    self.scratch.doc.live_update(live);
                    self.scratch.dirty = true;
                }
            }
            if resp.drag_stopped() {
                if let Some(live) = self.scratch.live.take() {
                    self.scratch.doc.live_finish(live);
                    self.scratch.dirty = true;
                }
            }
        }
        if self.scratch.dirty || self.scratch.tex.is_none() {
            let (w, h) = (self.scratch.doc.width as usize, self.scratch.doc.height as usize);
            let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &self.scratch.doc.flatten());
            self.scratch.tex = Some(ui.ctx().load_texture("scratch", img, egui::TextureOptions::NEAREST));
            self.scratch.dirty = false;
        }
        if let Some(t) = &self.scratch.tex {
            ui.painter().image(t.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        ui.painter().rect_stroke(rect, 0.0, egui::Stroke::new(1.0_f32, sai_ui::border()));
        resp.on_hover_text(tr("Черновик: рисуйте текущей кистью; Alt + щелчок или правая кнопка — взять цвет"));
        // «×» в углу — очистить (рисуется сам: `ui.put` сдвинул бы следующие панели)
        let clear = Rect::from_min_size(rect.right_top() + Vec2::new(-16.0, 2.0), Vec2::splat(14.0));
        let cr = ui.interact(clear, ui.id().with("scratch_clear"), egui::Sense::click()).on_hover_text(tr("Очистить черновик"));
        ui.painter().rect_filled(clear, 2.0, if cr.hovered() { sai_ui::gray(225) } else { sai_ui::gray(245) });
        ui.painter().rect_stroke(clear, 2.0, egui::Stroke::new(1.0_f32, sai_ui::border()));
        ui.painter().text(clear.center(), egui::Align2::CENTER_CENTER, "×", egui::FontId::proportional(12.0), sai_ui::ink());
        if cr.clicked() {
            self.scratch = ScratchPad::new(ppp);
        }
    }

    /// Только что было сочетание с Ctrl/Alt — Ctrl + перетаскивание слой пока не двигает.
    fn move_guarded(&self) -> bool {
        self.last_mod_key.is_some_and(|t| t.elapsed() < Duration::from_millis(self.opts.move_guard_ms as u64))
    }

    /// Окно «Настройки» (как Options в SAI 2017).
    fn add_reference(&mut self, path: &std::path::Path) {
        match paint_core::io::load_image(path) {
            Ok((w, h, rgba)) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.next_ref += 1;
                let scale = (360.0 / w.max(h).max(1) as f32).min(1.0);
                self.refs.push(RefImage { id: self.next_ref, name, size: [w as usize, h as usize], rgba, tex: None, opacity: 1.0, scale });
            }
            Err(e) => self.status = trf("Картинка не открылась: {0}", &[&e]),
        }
    }

    /// Окна опорных картинок. Непрозрачность действует на всё окно (холст виден сквозь него),
    /// размер картинки — свой масштаб окна (колесо над картинкой или ползунок), а не размер окна.
    fn reference_windows(&mut self, ctx: &egui::Context) {
        let mut close = Vec::new();
        let mut pick = None;
        let picker = self.tool == Tool::Picker;
        for r in self.refs.iter_mut() {
            let tex = r.tex.get_or_insert_with(|| ctx.load_texture(format!("ref{}", r.id), egui::ColorImage::from_rgba_unmultiplied(r.size, &r.rgba), egui::TextureOptions::LINEAR)).id();
            let mut open = true;
            let frame = egui::Frame::window(&ctx.style()).fill(sai_ui::panel().gamma_multiply(r.opacity)).shadow(egui::epaint::Shadow::NONE);
            egui::Window::new(&r.name).id(egui::Id::new(("ref", r.id))).open(&mut open).resizable(false).frame(frame).show(ctx, |ui| {
                ui.spacing_mut().slider_width = 110.0;
                ui.add(egui::Slider::new(&mut r.opacity, 0.1..=1.0).show_value(false).text(tr("Непрозрачность")));
                ui.add(egui::Slider::new(&mut r.scale, 0.05..=4.0).logarithmic(true).show_value(false).text(tr("Масштаб")));
                let size = Vec2::new(r.size[0] as f32, r.size[1] as f32) * r.scale;
                let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
                ui.painter().image(tex, rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::from_white_alpha((r.opacity * 255.0) as u8));
                if resp.hovered() {
                    let wheel = ui.input(|i| i.raw_scroll_delta.y);
                    if wheel != 0.0 {
                        r.scale = (r.scale * if wheel > 0.0 { 1.1 } else { 1.0 / 1.1 }).clamp(0.05, 4.0);
                    }
                }
                if resp.clicked() && (picker || ui.input(|i| i.modifiers.alt)) {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        let x = (((pos.x - rect.left()) / r.scale) as usize).min(r.size[0] - 1);
                        let y = (((pos.y - rect.top()) / r.scale) as usize).min(r.size[1] - 1);
                        let i = (y * r.size[0] + x) * 4;
                        pick = Some(Color32::from_rgb(r.rgba[i], r.rgba[i + 1], r.rgba[i + 2]));
                    }
                }
                resp.on_hover_text(tr("Пипетка или Alt + щелчок — взять цвет, колесо — масштаб"));
            });
            if !open {
                close.push(r.id);
            }
        }
        self.refs.retain(|r| !close.contains(&r.id));
        if let Some(c) = pick {
            self.set_color(c);
        }
    }

    /// Палитра у курсора: недавние цвета, образцы, кисти сетки; выбор или щелчок мимо — закрыть.
    fn popup_palette(&mut self, ctx: &egui::Context) {
        let Some(at) = self.popup_at else { return };
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let mut pick: Option<Color32> = None;
        let mut tool: Option<usize> = None;
        let swatches: Vec<Color32> = self.swatches.iter().flatten().copied().take(32).collect();
        let names: Vec<String> = self.brushes.iter().map(|b| b.custom.clone().unwrap_or_else(|| tr(b.name).to_string())).collect();
        let area = egui::Area::new(egui::Id::new("popup_palette")).fixed_pos(at).order(egui::Order::Foreground).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(16.0 * 12.0);
                let cell = 16.0;
                for row in [self.recent_colors.clone(), swatches] {
                    if row.is_empty() {
                        continue;
                    }
                    for chunk in row.chunks(12) {
                        if let Some(c) = sai_ui::recent_row(ui, chunk, cell) {
                            pick = Some(c);
                        }
                    }
                    ui.add_space(3.0);
                }
                ui.separator();
                egui::Grid::new("popup_tools").num_columns(3).spacing([2.0, 2.0]).show(ui, |ui| {
                    for (i, n) in names.iter().enumerate() {
                        if ui.selectable_label(i == self.brush && self.tool == Tool::Brush, n).clicked() {
                            tool = Some(i);
                        }
                        if i % 3 == 2 {
                            ui.end_row();
                        }
                    }
                });
            });
        });
        // щелчок мимо окошка закрывает его (кроме кадра, где оно открылось: нажатие и отпускание
        // правой кнопки могут прийти в одном кадре)
        if ctx.cumulative_pass_nr() != self.popup_frame && ctx.input(|i| i.pointer.any_pressed()) && !area.response.contains_pointer() {
            close = true;
        }
        if let Some(c) = pick {
            self.set_color(c);
            close = true;
        }
        if let Some(i) = tool {
            self.tool = Tool::Brush;
            self.brush = i;
            close = true;
        }
        if close {
            self.popup_at = None;
        }
    }

    /// Окно истории: шаги по порядку, щелчок — вернуться к состоянию после шага.
    fn history_window(&mut self, ctx: &egui::Context) {
        let (undo, redo) = self.doc.history_steps();
        let label = |i: &paint_core::StepInfo| {
            use paint_core::StepKind as K;
            let what = match i.kind {
                K::Paint => tr("Рисование"),
                K::NewLayer => tr("Новый слой"),
                K::DeleteLayer => tr("Удаление слоя"),
                K::Mask => tr("Маска слоя"),
                K::Vector => tr("Контур"),
                K::Scene3d => tr("Сцена 3D"),
                K::Selection => tr("Выделение"),
                K::Canvas => tr("Холст"),
                K::Other => tr("Изменение"),
            };
            match &i.layer {
                Some(l) => format!("{what} — {l}"),
                None => what.to_string(),
            }
        };
        let mut open = true;
        // сколько шагов отменить (+) или повторить (−)
        let mut go: Option<i64> = None;
        egui::Window::new(tr("История")).id(egui::Id::new("history_window")).open(&mut open).default_width(240.0).default_height(360.0).resizable(true).show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
                let n = undo.len() as i64;
                if ui.selectable_label(undo.is_empty(), tr("Исходное состояние")).clicked() {
                    go = Some(n);
                }
                for (i, step) in undo.iter().enumerate() {
                    if ui.selectable_label(i + 1 == undo.len(), format!("{}. {}", i + 1, label(step))).clicked() {
                        go = Some(n - 1 - i as i64);
                    }
                }
                for (j, step) in redo.iter().enumerate() {
                    let text = egui::RichText::new(format!("{}. {}", undo.len() + j + 1, label(step))).color(sai_ui::dim());
                    if ui.selectable_label(false, text).clicked() {
                        go = Some(-(j as i64 + 1));
                    }
                }
            });
        });
        if !open {
            self.show_history = false;
        }
        match go {
            Some(k) if k > 0 => (0..k).for_each(|_| self.undo()),
            Some(k) if k < 0 => (0..-k).for_each(|_| self.redo()),
            _ => {}
        }
    }

    fn options_window(&mut self, ctx: &egui::Context) {
        let Some(mut o) = self.opts_edit else { return };
        let (mut ok, mut cancel) = (false, false);
        egui::Window::new(tr("Настройки")).collapsible(false).resizable(false).default_width(460.0).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            let head = |ui: &mut egui::Ui, t: &str| {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(t).color(Color32::from_rgb(160, 70, 20)));
                ui.separator();
            };
            head(ui, tr("Общие"));
            ui.checkbox(&mut o.dbl_props, tr("Двойной щелчок по слою открывает свойства"));
            ui.checkbox(&mut o.swap_zoom, tr("Колесо мыши масштабирует наоборот"));
            ui.checkbox(&mut o.size_relative, tr("Клавиши размера кисти меняют его на четверть, а не по списку"));
            head(ui, tr("Клавиши инструментов и функций"));
            ui.checkbox(&mut o.spring_tools, tr("Удержание клавиши инструмента — временный выбор до отпускания"));
            ui.checkbox(&mut o.spring_color, tr("То же для смены основного цвета (X)"));
            ui.checkbox(&mut o.spring_funcs, tr("То же для функций (*)"));
            ui.horizontal(|ui| {
                ui.label(tr("Удержание считается с, мс:"));
                ui.add(egui::DragValue::new(&mut o.spring_ms).range(50..=2000));
            });
            ui.checkbox(&mut o.popup_palette, tr("Правый щелчок по холсту открывает палитру: цвета и кисти"));
            ui.checkbox(&mut o.psd16, tr("PSD и PSB — 16 бит на канал (точнее, файл больше)"));
            ui.checkbox(&mut o.psd_exact, tr("PSD и PSB: сохранить вид точно — слои с режимами SAI пишутся готовым видом"));
            ui.checkbox(&mut o.duplex_revert, tr("Повторное нажатие клавиши инструмента возвращает прежний"));
            ui.weak(tr("(*) Зеркало вида, «Не замечать линейку», «Прямая линия»"));
            head(ui, tr("Защита от случайного сдвига слоя"));
            ui.horizontal(|ui| {
                ui.label(tr("После сочетания с Ctrl или Alt Ctrl + перетаскивание не двигает слой, мс:"));
                ui.add(egui::DragValue::new(&mut o.move_guard_ms).range(0..=2000));
            });
            head(ui, tr("Память и восстановление"));
            ui.horizontal(|ui| {
                ui.label(tr("Память истории до переноса на диск, МБ:"));
                ui.add(egui::DragValue::new(&mut o.history_mb).range(64..=65536).speed(16));
            });
            ui.horizontal(|ui| {
                ui.label(tr("Копия для восстановления после сбоя — раз в, мин:"));
                ui.add(egui::DragValue::new(&mut o.recovery_min).range(1..=60));
            });
            head(ui, tr("Кривая нажима пера"));
            ui.horizontal(|ui| {
                sai_ui::curve_editor(ui, &mut o.pressure_curve, 140.0);
                ui.vertical(|ui| {
                    ui.weak(tr("По горизонтали — нажим пера, по вертикали — нажим для кисти. Тяните точки; выше прямой — мягче, ниже — жёстче."));
                    if ui.button(tr("Прямая")).clicked() {
                        o.pressure_curve = paint_core::curve::PressureCurve::default();
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.label(tr("Минимальный нажим для мазка:"));
                let mut v = o.min_pressure * 100.0;
                if ui.add(egui::DragValue::new(&mut v).range(0.0..=90.0).suffix("%")).changed() {
                    o.min_pressure = (v / 100.0).clamp(0.0, 0.9);
                }
            }).response.on_hover_text(tr("Слабее — перо не рисует (защита от случайных касаний); нажим отсчитывается от этого порога"));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ok = ui.button("OK").clicked();
                cancel = ui.button(tr("Отмена")).clicked();
                if ui.button(tr("По умолчанию")).clicked() {
                    o = Options::default();
                }
            });
        });
        if ok {
            self.opts = o;
            paint_core::history_disk::set_ram_budget(o.history_mb as usize * (1 << 20));
            self.save_settings();
            self.opts_edit = None;
        } else if cancel {
            self.opts_edit = None;
        } else {
            self.opts_edit = Some(o);
        }
    }

    /// Есть ли несохранённое хоть в одном холсте.
    fn any_dirty(&self) -> bool {
        self.dirty || self.tabs.iter().flatten().any(|t| t.dirty)
    }

    /// Закрытие окна программы: при несохранённом — вопрос (как у SAI при выходе).
    fn check_exit(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.exit_ok {
            if self.any_dirty() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.exit_ask = true;
            } else {
                self.exit_ok = true;
            }
        }
        if !self.exit_saving {
            return;
        }
        // «Сохранить»: холсты по очереди; окно имени ещё открыто — ждём
        if self.file_dialog.is_some() || self.show_save_as {
            return;
        }
        if let Some(c) = self.exit_waiting.take() {
            let still = if c == self.active_tab { self.dirty } else { self.tabs.get(c).and_then(|t| t.as_ref()).is_some_and(|t| t.dirty) };
            if still {
                // сохранение отменили — программа остаётся открытой
                self.exit_saving = false;
                self.status = String::from(tr("Выход отменён: холст не сохранён"));
                return;
            }
        }
        if self.dirty {
            self.exit_waiting = Some(self.active_tab);
            self.save();
            return;
        }
        if let Some(c) = (0..self.tabs.len()).find(|&c| self.tabs[c].as_ref().is_some_and(|t| t.dirty)) {
            if let Some(v) = self.views.iter().position(|v| v.canvas == c) {
                self.switch_view(v);
                return;
            }
        }
        self.exit_saving = false;
        self.exit_ok = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    /// Окно «Сохранить изменения?» при выходе.
    fn exit_window(&mut self, ctx: &egui::Context) {
        let mut names = Vec::new();
        for (i, v) in self.views.iter().enumerate() {
            let (name, dirty) = match &self.tabs[v.canvas] {
                Some(t) => (t.title(), t.dirty),
                None => (self.file_name(), self.dirty),
            };
            if dirty && !names.contains(&name) && i < 1000 {
                names.push(name);
            }
        }
        let mut choice = None;
        egui::Window::new(tr("Выход")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            ui.label(tr("Сохранить изменения в рисунках?"));
            for n in &names {
                ui.label(format!("• {n}"));
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button(tr("Сохранить")).clicked() {
                    choice = Some(0);
                }
                if ui.button(tr("Не сохранять")).clicked() {
                    choice = Some(1);
                }
                if ui.button(tr("Отмена")).clicked() {
                    choice = Some(2);
                }
            });
        });
        match choice {
            Some(0) => {
                self.exit_ask = false;
                self.exit_saving = true;
            }
            Some(1) => {
                self.exit_ask = false;
                self.exit_ok = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Some(2) => self.exit_ask = false,
            _ => {}
        }
    }
}

/// Миниатюра слоя поверх белого: ближайшие точки на сетке `size`×`size` с полями по пропорции.
/// Щелчок `(x, y)` по надписи: номер ближайшего места курсора, если щелчок в рамке надписи.
fn text_hit(t: &paint_formats::sai2_text::TextLayer, x: f64, y: f64) -> Option<usize> {
    let carets = paint_formats::sai2_text::caret_positions(t);
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in carets.iter().flatten() {
        x0 = x0.min(p[0]);
        y0 = y0.min(p[1]);
        x1 = x1.max(p[0]);
        y1 = y1.max(p[1]);
    }
    let pad = 4.0;
    if x < x0 - pad || x > x1 + pad || y < y0 - pad || y > y1 + pad {
        return None;
    }
    // ближайший курсор: по середине его отрезка
    (0..carets.len()).min_by(|&a, &b| {
        let d = |i: usize| {
            let c = carets[i];
            let (mx, my) = ((c[0][0] + c[1][0]) / 2.0, (c[0][1] + c[1][1]) / 2.0);
            (mx - x).powi(2) + (my - y).powi(2)
        };
        d(a).total_cmp(&d(b))
    })
}

fn layer_thumbnail(doc: &Document, path: &[usize], size: usize) -> Option<egui::ColorImage> {
    let layer = doc.layer(path)?;
    let (w, h) = (doc.width as f32, doc.height as f32);
    let k = (w / size as f32).max(h / size as f32);
    let (ox, oy) = ((size as f32 - w / k) / 2.0, (size as f32 - h / k) / 2.0);
    let mut img = egui::ColorImage::new([size, size], Color32::WHITE);
    for y in 0..size {
        for x in 0..size {
            let (cx, cy) = (((x as f32 + 0.5 - ox) * k) as i32, ((y as f32 + 0.5 - oy) * k) as i32);
            if cx < 0 || cy < 0 || cx >= doc.width as i32 || cy >= doc.height as i32 {
                img[(x, y)] = Color32::from_gray(225);
                continue;
            }
            let [r, g, b, a] = paint_core::pixel::to_rgba8(layer.pixels.get(cx, cy));
            let a = a as u32;
            let over = |c: u8| ((c as u32 * a + 255 * (255 - a)) / 255) as u8;
            img[(x, y)] = Color32::from_rgb(over(r), over(g), over(b));
        }
    }
    Some(img)
}

/// Миниатюра маски слоя: белое — слой виден, чёрное — скрыт.
fn layer_has_mask(doc: &Document, path: &[usize]) -> bool {
    doc.layer(path).is_some_and(|l| l.mask.is_some())
}

fn mask_thumbnail(doc: &Document, path: &[usize], size: usize) -> Option<egui::ColorImage> {
    let mask = doc.layer(path)?.mask.as_ref()?;
    let (w, h) = (doc.width as f32, doc.height as f32);
    let k = (w / size as f32).max(h / size as f32);
    let (ox, oy) = ((size as f32 - w / k) / 2.0, (size as f32 - h / k) / 2.0);
    let mut img = egui::ColorImage::new([size, size], Color32::from_gray(225));
    for y in 0..size {
        for x in 0..size {
            let (cx, cy) = (((x as f32 + 0.5 - ox) * k) as i32, ((y as f32 + 0.5 - oy) * k) as i32);
            if cx >= 0 && cy >= 0 && cx < doc.width as i32 && cy < doc.height as i32 {
                let m = mask.hide.get(cx, cy).min(0x4000) as u32;
                img[(x, y)] = Color32::from_gray((255 - m * 255 / 0x4000) as u8);
            }
        }
    }
    Some(img)
}

fn count_layers(layers: &[Layer]) -> usize {
    layers
        .iter()
        .map(|l| 1 + l.children.as_deref().map_or(0, count_layers))
        .sum()
}

/// Занято и всего на диске с папкой `dir`, байт (как «Drive Usage» в SAI — диск, куда уходит
/// история). `None` — узнать не удалось.
fn disk_use(dir: &std::path::Path) -> Option<(u64, u64)> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        extern "system" {
            fn GetDiskFreeSpaceExW(dir: *const u16, avail: *mut u64, total: *mut u64, free: *mut u64) -> i32;
        }
        let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(Some(0)).collect();
        let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
        // SAFETY: строка с нулём в конце, указатели — на свои переменные
        let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, &mut total, &mut free) };
        (ok != 0 && total > 0).then(|| (total - free, total))
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::ffi::OsStrExt;
        #[repr(C)]
        struct StatVfs {
            bsize: u64,
            frsize: u64,
            blocks: u64,
            bfree: u64,
            bavail: u64,
            files: u64,
            ffree: u64,
            favail: u64,
            fsid: u64,
            flag: u64,
            namemax: u64,
            spare: [i32; 6],
        }
        extern "C" {
            fn statvfs(path: *const std::ffi::c_char, buf: *mut StatVfs) -> i32;
        }
        let c = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
        let mut st = std::mem::MaybeUninit::<StatVfs>::zeroed();
        // SAFETY: путь с нулём в конце, буфер — размера struct statvfs (glibc, 64 бита)
        let st = unsafe {
            if statvfs(c.as_ptr(), st.as_mut_ptr()) != 0 {
                return None;
            }
            st.assume_init()
        };
        let total = st.blocks * st.frsize;
        (total > 0).then(|| (total - st.bfree * st.frsize, total))
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = dir;
        None
    }
}

/// Занятая память процесса и вся память, байт (Linux; на других системах — нули).
fn memory_use() -> (u64, u64) {
    #[cfg(target_os = "linux")]
    {
        let rss = std::fs::read_to_string("/proc/self/statm")
            .ok()
            .and_then(|s| {
                s.split_whitespace()
                    .nth(1)
                    .and_then(|v| v.parse::<u64>().ok())
            })
            .unwrap_or(0)
            * 4096;
        let total = std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|s| {
                s.lines().find(|l| l.starts_with("MemTotal")).and_then(|l| {
                    l.split_whitespace()
                        .nth(1)
                        .and_then(|v| v.parse::<u64>().ok())
                })
            })
            .unwrap_or(0)
            * 1024;
        (rss, total)
    }
    #[cfg(not(target_os = "linux"))]
    {
        (0, 0)
    }
}

fn first_raster(layers: &[Layer]) -> Option<Vec<usize>> {
    for (i, layer) in layers.iter().enumerate().rev() {
        match &layer.children {
            None => return Some(vec![i]),
            Some(children) => {
                if let Some(mut rest) = first_raster(children) {
                    rest.insert(0, i);
                    return Some(rest);
                }
            }
        }
    }
    None
}

/// Файлы восстановления: копия холста и путь исходного файла.
fn recovery_files() -> Option<(PathBuf, PathBuf)> {
    settings::dir().map(|d| (d.join("recovery.sai2"), d.join("recovery.txt")))
}

impl eframe::App for PaintApp {
    /// Для проверок: `PAINT_EXPORT_ON_EXIT=файл.png` — сохранить холст и вывести сводку по перу.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_settings();
        self.stop_recording();
        for t in self.tabs.iter_mut().flatten() {
            if let Some(r) = t.rec.take() {
                let _ = r.stop(&mut t.doc, false);
            }
        }
        paint_core::history_disk::cleanup();
        // для проверок: PAINT_SAVE_ON_EXIT=файл.sai2 — сохранить документ при выходе
        if let Ok(out) = std::env::var("PAINT_SAVE_ON_EXIT") {
            let r = paint_formats::sai2_write::save(&self.doc, std::path::Path::new(&out));
            eprintln!("save: {} -> {out}", if r.is_ok() { "saved" } else { "failed" });
        }
        // нормальный выход: копия для восстановления не нужна (если её не предлагали восстановить)
        if self.recovery_found.is_none() {
            if let Some((file, info)) = recovery_files() {
                while self.recovery_busy.load(std::sync::atomic::Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(50));
                }
                let _ = std::fs::remove_file(file);
                let _ = std::fs::remove_file(info);
            }
        }
        if let Ok(out) = std::env::var("PAINT_EXPORT_ON_EXIT") {
            let r = paint_core::io::save_png(std::path::Path::new(&out), self.doc.width, self.doc.height, &self.doc.flatten());
            eprintln!("export: {} -> {out}", if r.is_ok() { "saved" } else { "failed" });
            let info = self.pen.lock().map(|s| format!("{} [{}]", s.info, s.device)).unwrap_or_default();
            eprintln!("pen: {info}");
            for (src, (lo, hi, n)) in &self.pen_stats {
                eprintln!("pressure {src}: {n} замеров, от {lo:.3} до {hi:.3}");
            }
        }
    }

    /// Виртуальное перо: события касания с силой нажима и указателя, как от Windows Ink.
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        // Tab и Shift+Tab — панели и полный экран (как в SAI): забрать до egui, иначе он
        // переводит ими фокус по кнопкам
        if !ctx.wants_keyboard_input() {
            raw.events.retain(|e| match e {
                egui::Event::Key { key: egui::Key::Tab, pressed, modifiers, .. } if !modifiers.command => {
                    if *pressed {
                        self.tab_pressed = Some(modifiers.shift);
                    }
                    false
                }
                _ => true,
            });
        }
        let Some(vp) = self.virtual_pen.as_mut() else { return };
        ctx.request_repaint();
        if !self.view.is_positive() {
            return;
        }
        let samples = vp.due();
        let finished = vp.finished();
        for smp in samples {
            let pos = ViewXf::new(self.view, self.zoom, self.offset, self.view_angle, self.view_flip).to_screen(Vec2::new(smp.x, smp.y));
            if let Ok(mut st) = self.pen.lock() {
                // обратный конец пера сообщает источник пера, как драйвер на Linux
                if smp.eraser {
                    st.eraser = true;
                    st.pressure = smp.pressure;
                    st.at = Some(Instant::now());
                } else {
                    st.eraser = false;
                    st.at = None;
                }
            }
            let (phase, button) = match smp.phase {
                virtual_pen::Phase::Down => (egui::TouchPhase::Start, Some(true)),
                virtual_pen::Phase::Move => (egui::TouchPhase::Move, None),
                virtual_pen::Phase::Up => (egui::TouchPhase::End, Some(false)),
            };
            raw.events.push(egui::Event::PointerMoved(pos));
            raw.events.push(egui::Event::Touch { device_id: egui::TouchDeviceId(7), id: egui::TouchId(0), phase, pos, force: Some(smp.pressure) });
            if let Some(pressed) = button {
                raw.events.push(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
            }
        }
        if finished {
            self.virtual_done += 1;
            if self.virtual_done == 20 {
                self.status = String::from(tr("Виртуальное перо: сценарий закончен"));
                if let Ok(out) = std::env::var("PAINT_VIRTUAL_PEN_OUT") {
                    let r = paint_core::io::save_png(std::path::Path::new(&out), self.doc.width, self.doc.height, &self.doc.flatten());
                    eprintln!("virtual pen: {} -> {out}", if r.is_ok() { "saved" } else { "save failed" });
                    self.exit_ok = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ppp = ctx.pixels_per_point();
        let _frame_timer = self.frame_stats.clone().filter(|_| self.live.is_some()).map(|s| FrameTimer(s, Instant::now()));
        self.ctx = Some(ctx.clone());
        self.poll_file_dialog();
        self.check_exit(ctx);
        self.autosave();
        self.check_studio(ctx);
        self.recording_tick(ctx);
        self.s3d_sync();
        self.s3d_tick(ctx);
        if let Some(e) = self.dyn_error.as_ref().and_then(|e| e.borrow_mut().take()) {
            self.status = trf("Ошибка плагина «{0}»: {1}", &[&tr("Динамика"), &e.lines().next().unwrap_or("")]);
            self.plugin_log.push(e);
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone()));
        if let Some(path) = dropped {
            // с Ctrl картинка становится опорной, а не открывается
            if ctx.input(|i| i.modifiers.command) {
                self.add_reference(&path);
            } else if ctx.input(|i| i.modifiers.shift) {
                // с Shift — отпечаток текущей кисти
                self.add_tip(&path);
            } else {
                self.open_path = path.display().to_string();
                self.open(path);
            }
        }
        'shortcuts: {
        // клавиша для инструмента сетки (окно «Свойства»)
        if self.tool_key_capture {
            let got = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Key { key, pressed: true, modifiers, .. } if *key != egui::Key::Escape => {
                        Some(keys::Binding { key: *key, ctrl: modifiers.command, shift: modifiers.shift, alt: modifiers.alt })
                    }
                    _ => None,
                })
            });
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.tool_key_capture = false;
            } else if let Some(b) = got {
                if let Some(p) = self.tool_props.as_mut() {
                    p.3 = Some(b);
                }
                self.tool_key_capture = false;
            }
            break 'shortcuts;
        }
        // назначение клавиши в окне «Клавиши»: следующее нажатие с модификаторами
        if let Some(action) = self.key_capture {
            let got = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Key { key, pressed: true, modifiers, .. } if *key != egui::Key::Escape => {
                        Some(keys::Binding { key: *key, ctrl: modifiers.command, shift: modifiers.shift, alt: modifiers.alt })
                    }
                    _ => None,
                })
            });
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.key_capture = None;
            } else if let Some(b) = got {
                // клавиша снимается с другого действия и с инструментов сетки
                for v in self.keymap.values_mut() {
                    if *v == Some(b) {
                        *v = None;
                    }
                }
                for t in &mut self.brushes {
                    if t.key == Some(b) {
                        t.key = None;
                    }
                }
                self.keymap.insert(action, Some(b));
                self.key_capture = None;
            }
            break 'shortcuts;
        }
        let km = self.keymap.clone();
        let hit = |a: keys::Action| ctx.input(|i| km.get(&a).copied().flatten().is_some_and(|b| b.pressed(i)));
        use keys::Action as A;
        // нажатая в этом кадре клавиша (для временного переключения) и время сочетаний с Ctrl/Alt
        let pressed_key = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Key { key, pressed: true, repeat: false, .. } => Some(*key),
                _ => None,
            })
        });
        if ctx.input(|i| (i.modifiers.command || i.modifiers.alt) && i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, .. }))) {
            self.last_mod_key = Some(Instant::now());
        }
        if let Some((k, at, undo)) = self.spring {
            if !ctx.input(|i| i.key_down(k)) {
                self.spring = None;
                if at.elapsed() >= Duration::from_millis(self.opts.spring_ms as u64) {
                    match undo {
                        SpringUndo::Tool(t, b) => (self.tool, self.brush) = (t, b),
                        SpringUndo::Color => self.swap_colors(),
                        SpringUndo::Flip => self.view_flip = !self.view_flip,
                        SpringUndo::IgnoreRuler => self.ruler.ignore = !self.ruler.ignore,
                        SpringUndo::Straight => self.straight = !self.straight,
                    }
                }
            }
        }
        // выбрали цвет — «прозрачный цвет» снимается, как в SAI
        if self.hsv != self.prev_hsv {
            self.prev_hsv = self.hsv;
            self.transparent = false;
        }
        // сменили инструмент — набор закончен
        if self.text_edit.is_some() && self.tool != Tool::Text {
            self.finish_text(true);
        }
        // набор надписи на холсте забирает клавиатуру, как поле ввода
        if self.text_edit.is_some() && !ctx.wants_keyboard_input() {
            self.text_keys(ctx);
        }
        let typing = ctx.wants_keyboard_input() || self.text_edit.is_some() || self.viewer.is_some();
        // запасные сочетания: Shift+Ctrl+Z — повтор, Ctrl+Plus — приблизить
        let redo_alt = ctx.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::Z));
        let zoom_alt = ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Plus));
        // буфер обмена: Ctrl+C/X приходят событиями egui, Ctrl+V опрашивается (см. clip.rs)
        let (ev_copy, ev_cut, ev_paste) = ctx.input(|i| {
            let has = |f: &dyn Fn(&egui::Event) -> bool| i.events.iter().any(f);
            (has(&|e| matches!(e, egui::Event::Copy)), has(&|e| matches!(e, egui::Event::Cut)), has(&|e| matches!(e, egui::Event::Paste(_))))
        });
        // Ctrl+C/X/V egui забирает себе: если действию оставлено обычное сочетание — по событию
        let is_default = |a: keys::Action, d: &str| km.get(&a).copied().flatten() == keys::Binding::parse(d);
        let ctrl_down = ctx.input(|i| i.modifiers.command);
        let paste = if is_default(A::Paste, "Ctrl+V") { self.clip.paste_pressed(ctrl_down && !typing, ev_paste) } else { hit(A::Paste) };
        let copy = hit(A::Copy) || (ev_copy && is_default(A::Copy, "Ctrl+C"));
        let cut = hit(A::Cut) || (ev_cut && is_default(A::Cut, "Ctrl+X"));
        if !typing && self.live.is_none() {
            if hit(A::Close) {
                self.request_close_view(self.active_view);
            }
            if hit(A::New) {
                self.show_new = true;
            }
            if copy {
                self.copy(false, false);
            }
            if cut {
                self.copy(true, false);
            }
            if paste {
                self.paste();
            }
            if hit(A::FromClipboard) {
                self.canvas_from_clipboard();
            }
            if hit(A::InvertSelection) {
                self.invert_selection();
            }
        }
        if !typing {
            if hit(A::Undo) {
                self.undo();
            }
            if hit(A::Redo) || redo_alt {
                self.redo();
            }
            if hit(A::Open) {
                self.ask_open();
            }
            if hit(A::Save) {
                self.save();
            }
            if hit(A::SaveAs) {
                self.ask_save_as();
            }
        }
        let nudge_cut = ctx.input(|i| i.modifiers.command && !i.modifiers.alt);
        let nudge = ctx.input(|i| {
            // Alt — сдвиг, Ctrl — вырезать и сдвинуть (как в SAI)
            if i.modifiers.alt == i.modifiers.command {
                return (0, 0);
            }
            let step = if i.modifiers.shift { 10 } else { 1 };
            let mut d = (0, 0);
            if i.key_pressed(egui::Key::ArrowLeft) {
                d.0 -= step;
            }
            if i.key_pressed(egui::Key::ArrowRight) {
                d.0 += step;
            }
            if i.key_pressed(egui::Key::ArrowUp) {
                d.1 -= step;
            }
            if i.key_pressed(egui::Key::ArrowDown) {
                d.1 += step;
            }
            d
        });
        if !typing {
            if hit(A::Ruler) {
                self.toggle_ruler();
            }
            let func_spring = |app: &mut Self, undo: SpringUndo| {
                if let (true, Some(k)) = (app.opts.spring_funcs, pressed_key) {
                    app.spring = Some((k, Instant::now(), undo));
                }
            };
            if hit(A::FlipView) {
                self.view_flip = !self.view_flip;
                func_spring(self, SpringUndo::Flip);
            }
            if hit(A::IgnoreRuler) {
                self.ruler.ignore = !self.ruler.ignore;
                func_spring(self, SpringUndo::IgnoreRuler);
            }
            if hit(A::StraightLine) {
                self.straight = !self.straight;
                func_spring(self, SpringUndo::Straight);
            }
            // как в SAI: Tab — спрятать панели, F11 и Shift+Tab — полный экран
            // Tab egui забирает до нас (raw_input_hook): если «панели» оставлены на Tab — по флагу
            let shift_tab = self.tab_pressed.take();
            let tab = hit(A::TogglePanels) || (shift_tab == Some(false) && is_default(A::TogglePanels, "Tab"));
            let full = hit(A::Fullscreen) || shift_tab == Some(true);
            if tab {
                self.win.panels_hidden = !self.win.panels_hidden;
            }
            if full {
                self.win.fullscreen = !self.win.fullscreen;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.win.fullscreen));
            }
            // как в SAI: Home — положение вида, Ctrl+Alt+0 — масштаб 100 %
            if hit(A::ResetView) {
                self.reset_view_position();
            }
            if hit(A::Zoom100) {
                self.set_zoom(1.0 / self.ppp);
            }
            if hit(A::NewView) {
                self.new_view();
            }
            // команды SAI, у которых по умолчанию нет клавиш (назначаются в «Быстрых клавишах»)
            let cursor = ctx.input(|i| i.pointer.hover_pos()).filter(|p| self.view.contains(*p));
            let at_cursor = |app: &Self| cursor.map(|p| ViewXf::new(app.view, app.zoom, app.offset, app.view_angle, app.view_flip).to_canvas(p));
            if hit(A::TransparentColor) {
                self.transparent = !self.transparent;
            }
            if hit(A::PickMerged) || hit(A::PickLayer) {
                if let Some(c) = at_cursor(self) {
                    let only = self.pick_layer;
                    self.pick_layer = hit(A::PickLayer);
                    self.pick_color(c.x, c.y);
                    self.pick_layer = only;
                    self.transparent = false;
                }
            }
            if hit(A::PrevView) && !self.views.is_empty() {
                self.switch_view((self.active_view + self.views.len() - 1) % self.views.len());
            }
            if hit(A::NextView) && !self.views.is_empty() {
                self.switch_view((self.active_view + 1) % self.views.len());
            }
            if hit(A::CloseAllViews) {
                self.close_all_views();
            }
            let step = self.view.size() * 0.125 / self.zoom;
            for (a, d) in [(A::ScrollUp, Vec2::new(0.0, -step.y)), (A::ScrollDown, Vec2::new(0.0, step.y)), (A::ScrollLeft, Vec2::new(-step.x, 0.0)), (A::ScrollRight, Vec2::new(step.x, 0.0))] {
                if hit(a) {
                    self.offset += d;
                }
            }
            for (a, k) in [(A::ZoomInCursor, 1.25f32), (A::ZoomOutCursor, 0.8)] {
                if hit(a) {
                    match (cursor, at_cursor(self)) {
                        (Some(p), Some(anchor)) => {
                            self.zoom = (self.zoom * k).clamp(0.02, 32.0);
                            let now = ViewXf::new(self.view, self.zoom, self.offset, self.view_angle, self.view_flip).to_canvas(p);
                            self.offset += anchor - now;
                        }
                        _ => self.set_zoom(self.zoom * k),
                    }
                }
            }
            if hit(A::LayerAbove) || hit(A::LayerBelow) {
                self.select_neighbor_layer(hit(A::LayerAbove));
            }
            if hit(A::LayerRaise) || hit(A::LayerLower) {
                self.layer_action(if hit(A::LayerRaise) { LayerAction::Up } else { LayerAction::Down });
            }
            if hit(A::ToggleFolder) {
                self.toggle_folder(ctx);
            }
            if hit(A::LayerFront) {
                self.layer_to_end(true);
            }
            if hit(A::LayerBack) {
                self.layer_to_end(false);
            }
            if hit(A::ShowAlone) {
                self.show_alone();
            }
            if hit(A::MarkFolder) {
                self.mark_folder();
            }
            if hit(A::ToggleMarkedFolder) || hit(A::ShowAloneMarkedFolder) {
                self.marked_folder_visibility(hit(A::ShowAloneMarkedFolder));
            }
            if hit(A::ToggleVisible) {
                if let Some(l) = self.doc.layer_mut(&self.selected) {
                    l.visible = !l.visible;
                    self.invalidate_all();
                    self.dirty = true;
                }
            }
            if hit(A::GrowSel1) || hit(A::ShrinkSel1) {
                self.doc.grow_selection(hit(A::GrowSel1));
                self.sel_dirty = true;
            }
            for (i, a) in [A::Density10, A::Density20, A::Density30, A::Density40, A::Density50, A::Density60, A::Density70, A::Density80, A::Density90, A::Density100].into_iter().enumerate() {
                if hit(a) {
                    self.brushes[self.brush].params.density = (i + 1) as f32 / 10.0;
                }
            }
            if hit(A::RotateCcw) {
                self.rotate_view(-15.0);
            }
            if hit(A::RotateCw) {
                self.rotate_view(15.0);
            }
            if hit(A::ResetRotation) {
                self.rotate_view(0.0);
            }
        }
        let (key_enter, key_esc) = ctx.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
        if !typing && self.live.is_none() {
            if hit(A::SwapColors) {
                self.swap_colors();
                if let (true, Some(k)) = (self.opts.spring_color, pressed_key) {
                    self.spring = Some((k, Instant::now(), SpringUndo::Color));
                }
            }
            if hit(A::HueSaturation) {
                self.open_filter(FilterKind::HueSaturation);
            }
            if hit(A::Fill) {
                self.fill_selection();
            }
            if self.gradient.is_some() {
                if key_enter {
                    self.finish_gradient(true);
                } else if key_esc {
                    self.finish_gradient(false);
                }
            }
            if self.sel_poly.is_some() {
                if key_enter {
                    self.close_polygon();
                } else if key_esc {
                    self.sel_poly = None;
                }
            }
            let key_t = hit(A::Transform);
            if self.transform.is_some() {
                if key_enter || key_t {
                    self.finish_transform(true);
                } else if key_esc {
                    self.finish_transform(false);
                }
            } else if key_t && self.move_drag.is_none() {
                self.begin_transform();
            }
            if nudge != (0, 0) && self.move_drag.is_none() {
                self.nudge(nudge.0, nudge.1, nudge_cut);
            }
        }
        if !typing && self.live.is_none() && self.sel_drag.is_none() {
            if hit(A::SelectAll) {
                self.select_all();
            }
            if hit(A::Deselect) {
                self.deselect();
            }
            if hit(A::HideSelection) {
                self.show_selection = !self.show_selection;
            }
            let before = (self.tool, self.brush);
            let mut tool_hit = false;
            for (a, t) in [
                (A::Marquee, Tool::Marquee),
                (A::Lasso, Tool::Lasso),
                (A::Wand, Tool::Wand),
                (A::MoveTool, Tool::Move),
                (A::TextTool, Tool::Text),
                (A::HandTool, Tool::Hand),
                (A::RotateTool, Tool::RotateView),
                (A::Picker, Tool::Picker),
                (A::ZoomTool, Tool::Zoom),
                (A::Bucket, Tool::Fill),
            ] {
                if hit(a) {
                    self.tool = t;
                    tool_hit = true;
                }
            }
            // на слое контура карандаш и ластик — перо и ластик контура; узлы и нажим — свои
            let liwk = self.doc.layer(&self.selected).is_some_and(|l| l.vector.as_ref().is_some_and(|v| &v.kind == b"liwk"));
            if liwk {
                for (a, t) in [(A::Pencil, LineTool::Pen), (A::Eraser, LineTool::Eraser), (A::LineEdit, LineTool::Edit), (A::LinePressure, LineTool::Pressure)] {
                    if hit(a) {
                        self.line_tool = t;
                        self.tool = Tool::Brush;
                    }
                }
            }
            for (a, k) in [
                (A::Pencil, ToolKind::Pencil),
                (A::AirBrush, ToolKind::AirBrush),
                (A::Brush, ToolKind::Brush),
                (A::Water, ToolKind::Water),
                (A::Eraser, ToolKind::Eraser),
                (A::Marker, ToolKind::Marker),
                (A::SelPen, ToolKind::SelPen),
                (A::SelEraser, ToolKind::SelErs),
                (A::Gradient, ToolKind::Gradient),
            ] {
                let i = self.brushes.iter().position(|b| b.kind == k);
                if let Some(i) = i.filter(|_| hit(a) && !(liwk && matches!(k, ToolKind::Pencil | ToolKind::Eraser))) {
                    self.brush = i;
                    self.tool = Tool::Brush;
                    tool_hit = true;
                }
            }
            // своя клавиша инструмента сетки («Свойства…», как Shortcut Key в SAI) — важнее общей
            let own = self.brushes.iter().position(|b| b.key.is_some_and(|k| ctx.input(|i| k.pressed(i))));
            if let Some(i) = own {
                self.brush = i;
                self.tool = Tool::Brush;
                tool_hit = true;
            }
            // как в SAI («Настройки»): повторное нажатие — прежний инструмент; удержание —
            // временное переключение до отпускания
            if tool_hit {
                if (self.tool, self.brush) == before {
                    if self.opts.duplex_revert {
                        (self.tool, self.brush) = self.prev_tool;
                        self.prev_tool = before;
                    }
                } else {
                    self.prev_tool = before;
                    if self.opts.spring_tools {
                        if let Some(k) = pressed_key {
                            self.spring = Some((k, Instant::now(), SpringUndo::Tool(before.0, before.1)));
                        }
                    }
                }
            }
            if hit(A::Clear) {
                self.layer_action(LayerAction::Clear);
            }
            if hit(A::Merge) {
                self.layer_action(LayerAction::Merge);
            }
            if hit(A::Transfer) {
                self.layer_action(LayerAction::Transfer);
            }
        }
        if !typing {
            let (bigger, smaller) = (hit(A::SizeUp), hit(A::SizeDown));
            let size = &mut self.brushes[self.brush].params.size;
            let relative = self.opts.size_relative;
            if bigger {
                *size = if relative { (*size * 1.25 * 10.0).round() / 10.0 } else { SIZES.iter().copied().find(|s| *s > *size + 0.01).unwrap_or(*size) }.min(5000.0);
            }
            if smaller {
                *size = if relative { (*size / 1.25 * 10.0).round() / 10.0 } else { SIZES.iter().rev().copied().find(|s| *s < *size - 0.01).unwrap_or(*size) }.max(0.1);
            }
            if hit(A::ZoomIn) || zoom_alt {
                self.set_zoom(self.zoom * 1.25);
            }
            if hit(A::ZoomOut) {
                self.set_zoom(self.zoom / 1.25);
            }
        }
        }

        if let Some(secs) = std::env::var("PAINT_EXIT_AFTER").ok().and_then(|v| v.parse::<f32>().ok()) {
            if self.started.elapsed().as_secs_f32() > secs {
                self.exit_ok = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        let title = format!(
            "GrazySAI {} — {}{}",
            env!("CARGO_PKG_VERSION"),
            self.file_name(),
            if self.dirty { " (*)" } else { "" }
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        egui::TopBottomPanel::top("menu").show(ctx, |ui| self.menu_bar(ui));
        egui::TopBottomPanel::bottom("status").show_animated(ctx, self.win.show[6], |ui| {
            ui.horizontal(|ui| {
                if let Some(r) = &self.rec {
                    ui.colored_label(Color32::from_rgb(220, 50, 50), format!("● REC {}", recording::size_text(r.bytes()))).on_hover_text(r.path.display().to_string());
                    ui.separator();
                }
                ui.label(&self.status);
                for note in &self.notes {
                    ui.separator();
                    ui.weak(note);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (used, total) = memory_use();
                    let mut v = if total > 0 {
                        used as f32 / total as f32 * 100.0
                    } else {
                        0.0
                    };
                    let text = format!("{:.1}%", v);
                    sai_ui::bar(ui, &mut v, 0.0, 100.0, text, 110.0, false);
                    ui.label(tr("Памяти занято"));
                    ui.separator();
                    // диск истории — раз в 5 секунд, не каждый кадр
                    if self.disk_seen.is_none_or(|(t, _)| t.elapsed() > Duration::from_secs(5)) {
                        self.disk_seen = Some((Instant::now(), disk_use(&std::env::temp_dir())));
                    }
                    if let Some((_, Some((used, total)))) = self.disk_seen {
                        let mut v = used as f32 / total as f32 * 100.0;
                        let text = format!("{:.0}%", v);
                        sai_ui::bar(ui, &mut v, 0.0, 100.0, text, 80.0, false).on_hover_text(tr("Диск, куда уходит история отмены"));
                        ui.label(tr("Диск занят"));
                        ui.separator();
                    }
                    if !self.queue.is_empty() {
                        ui.weak(trf("пересчёт: {0} плиток", &[&self.queue.len()]));
                        ui.separator();
                    }
                    // Сведения о пере — наименее важное: на узком окне или в длинном переводе
                    // обрезаются многоточием, а не наезжают на сообщение слева.
                    let info = self.pen.lock().map(|s| s.info.clone()).unwrap_or_default();
                    let text = format!(
                        "{info} | {}",
                        trf("{0}: нажим {1}", &[&self.last_source, &format!("{:.2}", self.last_pressure)])
                    );
                    ui.add(egui::Label::new(egui::RichText::new(text).weak()).truncate());
                });
            });
        });
        if (ctx.zoom_factor() - self.win.ui_scale).abs() > 0.001 {
            ctx.set_zoom_factor(self.win.ui_scale);
        }
        let side = |right: bool, id: &'static str| if right { egui::SidePanel::right(id) } else { egui::SidePanel::left(id) };
        if !self.win.panels_hidden {
            let full = self.win.show;
            // отделённые панели — в своих окнах, в главном их место освобождается
            self.separate_panels(ctx);
            let mut sh = full;
            for (v, sep) in sh.iter_mut().zip(self.win.separate) {
                *v &= !sep;
            }
            self.win.show = sh;
            // как в SAI: навигатор и слои — с краю окна, цвет и инструменты — ближе к холсту
            if sh[0] || sh[1] {
                side(self.win.layers_right, "layers").exact_width(186.0).resizable(false).show(ctx, |ui| self.right_panel(ui));
            }
            if sh[2] || sh[3] {
                side(self.win.tools_right, "tools").exact_width(236.0).resizable(false).show(ctx, |ui| self.left_panel(ui));
            }
            self.win.show[..4].copy_from_slice(&full[..4]);
            if sh[4] {
                egui::TopBottomPanel::top("quick").show(ctx, |ui| self.quick_bar(ui));
            }
        }
        let mut tab_click = None;
        let mut tab_close = None;
        egui::TopBottomPanel::bottom("tabs").show_animated(ctx, !self.win.panels_hidden && self.win.show[5], |ui| {
            ui.horizontal(|ui| {
                for i in 0..self.views.len() {
                    let active = i == self.active_view;
                    let v = self.views[i];
                    let (name, dirty) = match &self.tabs[v.canvas] {
                        Some(t) => (t.title(), t.dirty),
                        None => (self.file_name(), self.dirty),
                    };
                    let zoom = if active { self.zoom } else { v.view.zoom };
                    // у холста несколько видов — номер вида, как в SAI
                    let same = self.views.iter().filter(|o| o.canvas == v.canvas).count();
                    let name = if same > 1 { format!("{name} ({})", self.views[..=i].iter().filter(|o| o.canvas == v.canvas).count()) } else { name };
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(180.0, 18.0), egui::Sense::click());
                    let p = ui.painter();
                    p.rect_filled(rect, 0.0, if active { sai_ui::select_bg() } else { sai_ui::gray(236) });
                    p.rect_stroke(rect, 0.0, egui::Stroke::new(1.0_f32, if active { sai_ui::select_edge() } else { sai_ui::border() }));
                    let label = if dirty { format!("{name} *") } else { name };
                    p.text(rect.left_center() + Vec2::new(6.0, 0.0), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(12.0), sai_ui::ink());
                    p.text(rect.right_center() - Vec2::new(18.0, 0.0), egui::Align2::RIGHT_CENTER, format!("{:.0}%", zoom * self.ppp * 100.0), egui::FontId::proportional(10.0), sai_ui::ink());
                    // крестик закрытия
                    let x = Rect::from_center_size(rect.right_center() - Vec2::new(8.0, 0.0), Vec2::splat(12.0));
                    let over_x = ui.input(|i| i.pointer.hover_pos()).is_some_and(|h| x.contains(h));
                    p.text(x.center(), egui::Align2::CENTER_CENTER, "×", egui::FontId::proportional(12.0), if over_x { Color32::from_rgb(200, 40, 40) } else { sai_ui::ink() });
                    if resp.clicked() {
                        if over_x {
                            tab_close = Some(i);
                        } else {
                            tab_click = Some(i);
                        }
                    }
                    if resp.middle_clicked() {
                        tab_close = Some(i);
                    }
                }
            });
        });
        if let Some(i) = tab_click {
            self.switch_view(i);
        }
        if let Some(i) = tab_close {
            self.request_close_view(i);
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| self.canvas(ui));
        if let Some(mut fv) = self.float_view {
            // плавающий вид рисуется тем же холстом со своим масштабом, сдвигом и поворотом
            let mut open = true;
            let main = (self.zoom, self.offset, self.view_angle, self.view_flip, self.view);
            egui::Window::new(tr("Плавающий вид")).id(egui::Id::new("float_view")).open(&mut open).default_size([480.0, 360.0]).resizable(true).frame(egui::Frame::window(&ctx.style()).inner_margin(0.0)).show(ctx, |ui| {
                (self.zoom, self.offset, self.view_angle, self.view_flip) = (fv.zoom, fv.offset, fv.angle, fv.flip);
                self.drawing_view = 1;
                ui.set_min_size(Vec2::new(160.0, 120.0));
                self.canvas(ui);
                fv = FloatView { zoom: self.zoom, offset: self.offset, angle: self.view_angle, flip: self.view_flip };
                self.drawing_view = 0;
            });
            (self.zoom, self.offset, self.view_angle, self.view_flip, self.view) = main;
            self.float_view = open.then_some(fv);
            if !open && self.input_view == 1 {
                self.input_view = 0;
            }
        }
        self.dialogs(ctx);

        let t = Instant::now();
        self.render_pending(Duration::from_millis(12));
        if let Some(s) = &self.frame_stats {
            s.borrow_mut().render += t.elapsed().as_secs_f32() * 1000.0;
        }
        if !self.queue.is_empty() {
            ctx.request_repaint();
        }
    }
}

/// Новый холст с первым слоем, названным на языке интерфейса.
fn new_document(w: u32, h: u32) -> Document {
    let mut doc = Document::new(w, h);
    if let Some(first) = doc.layers.first_mut() {
        first.name = trf("Слой{0}", &[&1]);
    }
    doc
}

/// Пункт меню: подпись на выбранном языке и клавиша справа.
fn menu_item(label: &'static str, shortcut: &str) -> egui::Button<'static> {
    let b = egui::Button::new(tr(label));
    if keys::hints() {
        b.shortcut_text(shortcut.to_string())
    } else {
        b
    }
}

fn main() -> eframe::Result {
    // На Wayland перо доходит до окна только через XWayland (см. pen.rs), поэтому открываемся там.
    // PAINT_RS_WAYLAND=1 оставляет родной Wayland — без нажима пера.
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_some() && std::env::var_os("PAINT_RS_WAYLAND").is_none() {
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("WAYLAND_SOCKET");
    }
    // Сбой — в crash.log папки настроек (в Windows консоли нет, иначе причину не узнать).
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let log = settings::dir().unwrap_or_else(std::env::temp_dir).join("crash.log");
        let text = format!(
            "GrazySAI {} ({} {})\n{info}\n{}\n\n",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            std::backtrace::Backtrace::force_capture()
        );
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&log) {
            let _ = f.write_all(text.as_bytes());
        }
        default_hook(info);
    }));
    // самопроверка без окна (для проверки на другой машине): отчёт в файл и код выхода
    if std::env::args().nth(1).as_deref() == Some("--self-test") {
        let ok = self_test::run(std::env::args().nth(2).map(PathBuf::from));
        std::process::exit(if ok { 0 } else { 1 });
    }
    let path = std::env::args().nth(1).map(PathBuf::from);
    // Язык: PAINT_LANG, иначе выбранный в меню, иначе язык системы.
    let lang = std::env::var("PAINT_LANG")
        .ok()
        .or_else(|| settings::Settings::load().str("lang").map(str::to_string))
        .unwrap_or_else(|| i18n::system().to_string());
    i18n::set(&lang);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1360.0, 900.0])
            .with_fullscreen(std::env::var_os("PAINT_FULLSCREEN").is_some())
            .with_title("GrazySAI"),
        ..Default::default()
    };
    eframe::run_native(
        "GrazySAI",
        options,
        Box::new(move |cc| {
            i18n::install_fonts(&cc.egui_ctx);
            let feed = pen::new_feed();
            pen::spawn(feed.clone(), cc.egui_ctx.clone());
            let app = PaintApp::new(path, feed);
            // после настроек: тема берётся из них
            sai_ui::apply_style(&cc.egui_ctx);
            update::cleanup();
            // проверка при запуске — только с настройками (не в прогонах проверок) и если не выключена
            let auto = std::env::var("PAINT_UPDATE_CHECK").map_or(settings::dir().is_some(), |v| v != "off");
            if auto && app.check_updates {
                update::check(&app.update, &cc.egui_ctx);
            }
            Ok(Box::new(app))
        }),
    )
}

/// Сетка размеров кисти, как в SAI: кружок и число, 5 в ряд; `numbers` — только числа. Возвращает
/// выбранный размер.
fn size_grid(ui: &mut egui::Ui, current: f32, numbers: bool, top: bool) -> Option<f32> {
    let mut pick = None;
    egui::Frame::none()
        .stroke(egui::Stroke::new(1.0_f32, sai_ui::border()))
        .fill(sai_ui::gray(255))
        .show(ui, |ui| {
            // как у SAI: своя полоса прокрутки сбоку, ячейки её не перекрывают; не меньше 3 рядов
            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
            let v = ui.visuals_mut();
            v.extreme_bg_color = sai_ui::gray(240);
            v.widgets.inactive.bg_fill = sai_ui::gray(185);
            v.widgets.hovered.bg_fill = sai_ui::gray(160);
            v.widgets.active.bg_fill = sai_ui::gray(140);
            egui::ScrollArea::vertical()
                .id_salt("size_grid")
                .auto_shrink([false, top])
                .min_scrolled_height(if top && numbers { 24.0 * 4.0 } else { 40.0 * 3.0 })
                .max_height(if top { if numbers { 24.0 * 4.0 } else { 40.0 * 3.0 } } else { f32::INFINITY })
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    let cw = (ui.available_width() / 5.0).floor();
                    for row in SIZES.chunks(5) {
                        ui.horizontal(|ui| {
                            for &size in row {
                                let sel = (current - size).abs() < 0.01;
                                let r = sai_ui::icon_button(
                                    ui,
                                    Vec2::new(cw, if numbers { 24.0 } else { 40.0 }),
                                    sel,
                                    true,
                                    "",
                                    |p, r, c| {
                                        if !numbers {
                                            let d = (size.sqrt() * 2.6).clamp(1.0, 22.0);
                                            p.circle_filled(
                                                r.center_top() + Vec2::new(0.0, 13.0),
                                                d / 2.0,
                                                c,
                                            );
                                        }
                                        let text = if size < 10.0 && size.fract() != 0.0 {
                                            format!("{size:.1}")
                                        } else {
                                            format!("{size:.0}")
                                        };
                                        p.text(
                                            if numbers { r.center() } else { r.center_bottom() },
                                            if numbers { egui::Align2::CENTER_CENTER } else { egui::Align2::CENTER_BOTTOM },
                                            text,
                                            egui::FontId::proportional(if numbers { 12.0 } else { 10.0 }),
                                            c,
                                        );
                                    },
                                );
                                if r.clicked() {
                                    pick = Some(size);
                                }
                            }
                        });
                    }
                });
        });
    pick
}

/// Раздел «Россыпь» в настройках кисти-россыпи (как в SAI 2 2018+: угол, размер, шаг, сдвиг,
/// ширина : высота, число форм, цвет).
fn scatter_panel(ui: &mut egui::Ui, sc: &mut paint_core::scatter::ScatterParams, lw: f32, w: f32) {
    use paint_core::scatter::AngleControl;
    if !sai_ui::section(ui, "sec_scatter", tr("Россыпь")) {
        return;
    }
    // подписи раздела длиннее общих — своя ширина колонки (в немецком и французском иначе режутся)
    let total = lw + w;
    let lw = sai_ui::label_w(
        ui,
        lw,
        &[tr("   Угол по"), tr("   Угол"), tr("   Разброс угла"), tr("   Масштаб формы"), tr("   Разброс размера"), tr("   Разлёт"), tr("   Ш : В"), tr("   Разброс Ш : В"), tr("   Число"), tr("   Разброс числа"), tr("   Основной → фоновый"), tr("   Разброс оттенка"), tr("   Разброс насыщенности"), tr("   Разброс яркости")],
    )
    .min(total * 0.62);
    let w = total - lw;
    let pct = |ui: &mut egui::Ui, label: &str, v: &mut f32, max: f32| {
        sai_ui::row(ui, label, lw, true, |ui| {
            let mut x = *v * 100.0;
            let text = format!("{:.0}%", x);
            if sai_ui::bar(ui, &mut x, 0.0, max * 100.0, text, w, true).changed() {
                *v = (x.round() / 100.0).clamp(0.0, max);
            }
        });
    };
    sai_ui::row(ui, tr("   Угол по"), lw, true, |ui| {
        let names = [(AngleControl::None, tr("Нет")), (AngleControl::Auto, tr("Авто")), (AngleControl::Direction, tr("Направление пера"))];
        let shown = names.iter().find(|(a, _)| *a == sc.angle_control).map_or("", |(_, n)| n);
        egui::ComboBox::from_id_salt("scat_angle").selected_text(shown).width(w - 8.0).show_ui(ui, |ui| {
            for (a, n) in names {
                ui.selectable_value(&mut sc.angle_control, a, n);
            }
        });
    });
    sai_ui::row(ui, tr("   Угол"), lw, true, |ui| {
        let mut d = sc.angle.to_degrees();
        let text = format!("{:+.0}°", d);
        if sai_ui::bar(ui, &mut d, -180.0, 180.0, text, w, true).changed() {
            sc.angle = d.round().to_radians();
        }
    });
    pct(ui, tr("   Разброс угла"), &mut sc.angle_jitter, 1.0);
    pct(ui, tr("   Масштаб формы"), &mut sc.scaling, 2.0);
    pct(ui, tr("   Разброс размера"), &mut sc.size_jitter, 1.0);
    pct(ui, tr("   Разлёт"), &mut sc.scattering, 2.0);
    sai_ui::row(ui, tr("   Ш : В"), lw, true, |ui| {
        // ширина : высота как в SAI — 100:100, сжатие по одной из сторон
        let mut t = sc.ratio.ln();
        let (wd, ht) = if sc.ratio <= 1.0 { ((sc.ratio * 100.0).round(), 100.0) } else { (100.0, (100.0 / sc.ratio).round()) };
        if sai_ui::bar(ui, &mut t, 0.01f32.ln(), 100f32.ln(), format!("{wd:.0}:{ht:.0}"), w, true).changed() {
            sc.ratio = t.exp().clamp(0.01, 100.0);
        }
    });
    pct(ui, tr("   Разброс Ш : В"), &mut sc.ratio_jitter, 1.0);
    sai_ui::row(ui, tr("   Число"), lw, true, |ui| {
        let mut v = sc.count as f32;
        if sai_ui::bar(ui, &mut v, 1.0, 20.0, format!("{}", sc.count), w, true).changed() {
            sc.count = v.round().clamp(1.0, 20.0) as u8;
        }
    });
    pct(ui, tr("   Разброс числа"), &mut sc.count_jitter, 1.0);
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut sc.all_directions, tr("Во все стороны"));
        ui.checkbox(&mut sc.gaussian, tr("По Гауссу"));
        ui.checkbox(&mut sc.absolute_size, tr("Размер без нажима")).on_hover_text(tr("Размер форм не зависит от нажима пера"));
        ui.checkbox(&mut sc.int_position, tr("В целых точках"));
    });
    ui.label(tr("   Цвет:"));
    pct(ui, tr("   Основной → фоновый"), &mut sc.fg_bg_jitter, 1.0);
    pct(ui, tr("   Разброс оттенка"), &mut sc.hue_jitter, 1.0);
    pct(ui, tr("   Разброс насыщенности"), &mut sc.sat_jitter, 1.0);
    pct(ui, tr("   Разброс яркости"), &mut sc.bri_jitter, 1.0);
    ui.checkbox(&mut sc.each_shape, tr("К каждой форме")).on_hover_text(tr("Свой цвет у каждой формы; без флажка — один на весь мазок"));
}

#[cfg(test)]
mod input_tests {
    use super::*;

    #[test]
    fn numbered_names_count_up_and_skip_taken() {
        let d = std::env::temp_dir().join(format!("grazysai-num-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(numbered_path(&d.join("рисунок.sai2")), d.join("рисунок_001.sai2"));
        assert_eq!(numbered_path(&d.join("рисунок_009.sai2")), d.join("рисунок_010.sai2"));
        assert_eq!(numbered_path(&d.join("кот_2026_7.psd")), d.join("кот_2026_008.psd"));
        std::fs::write(d.join("лист_002.sai2"), b"x").unwrap();
        assert_eq!(numbered_path(&d.join("лист_001.sai2")), d.join("лист_003.sai2"));
        std::fs::remove_dir_all(d).ok();
    }
    use paint_core::stabilizer::{Level, Stabilizer};

    /// Сглаживание не зависит от того, как часто приходят точки: путь, присланный редкими
    /// склеенными точками (тяжёлая кисть, Windows), после дополнения сглаживается почти так же,
    /// как присланный часто (лёгкий карандаш).
    #[test]
    fn stabilizer_strength_does_not_depend_on_input_rate() {
        // дуга за 1 с; «часто» — 240 точек в секунду, «редко» — 20 кадров по 50 мс
        let at = |t: f32| Pos2::new(100.0 + 400.0 * t, 300.0 + 150.0 * (t * std::f32::consts::PI).sin());
        let run = |frames: &[Vec<Pos2>], dts: &[f32], resample: bool| -> Vec<Pos2> {
            let mut st = Stabilizer::with_level(Level::Plain(15));
            let mut last = (at(0.0), 1.0);
            let mut out = vec![st.push(InputPoint { x: last.0.x, y: last.0.y, pressure: 1.0 })];
            for (moves, dt) in frames.iter().zip(dts) {
                let want = if resample { (dt * 240.0).ceil() as usize } else { 0 };
                let pts = resample_moves(last, moves, 1.0, want);
                last = *pts.last().unwrap();
                out.extend(pts.iter().map(|(p, pr)| st.push(InputPoint { x: p.x, y: p.y, pressure: *pr })));
            }
            out.into_iter().map(|q| Pos2::new(q.x, q.y)).collect()
        };
        let fast: Vec<Vec<Pos2>> = (1..=240).map(|k| vec![at(k as f32 / 240.0)]).collect();
        let slow: Vec<Vec<Pos2>> = (1..=20).map(|k| vec![at(k as f32 / 20.0)]).collect();
        let fast_out = run(&fast, &[1.0 / 240.0; 240], true);
        let slow_out = run(&slow, &[0.05; 20], true);
        assert_eq!(fast_out.len(), slow_out.len());
        // сглаженные линии совпадают с точностью до хорд редкого пути
        let worst = fast_out.iter().zip(&slow_out).map(|(a, b)| a.distance(*b)).fold(0.0, f32::max);
        assert!(worst < 3.0, "расхождение {worst}");
        // без дополнения те же 15 точек сглаживают 0,75 с пути вместо 1/16 с — линия отстаёт
        let raw = run(&slow, &[0.05; 20], false);
        let lag = |out: &[Pos2], k: usize| out[k].distance(at(k as f32 / (out.len() - 1) as f32));
        assert!(lag(&raw, 10) > 50.0, "без дополнения сглаживание сильнее: {}", lag(&raw, 10));
    }
}

