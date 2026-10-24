//! Кисть GrazySAI без интерфейса: вид инструмента, настройки движка, форма и текстура, динамика,
//! запись в текст `ключ=значение` и в файл кисти `.gsbrush`. Общая для рисовалки и мастерской
//! (GrazySAI Studio), чтобы кисть вела себя в обеих одинаково.

pub mod sai2_tool;

use paint_core::mix::MixParams;
use paint_core::BrushParams;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Настройки текстом: по строке `ключ=значение` (строки с `#` в начале — пояснения).
#[derive(Default, Clone, Debug)]
pub struct Kv {
    map: BTreeMap<String, String>,
}

impl Kv {
    pub fn from_text(text: &str) -> Self {
        let mut map = BTreeMap::new();
        for line in text.lines() {
            if line.trim_start().starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                map.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
        Self { map }
    }

    /// Текстом, по ключам по алфавиту.
    pub fn to_text(&self) -> String {
        self.map.iter().map(|(k, v)| format!("{k}={v}\n")).collect()
    }

    pub fn set(&mut self, key: &str, value: impl ToString) {
        // перевод строки внутри значения сломал бы файл
        self.map.insert(key.to_string(), value.to_string().replace(['\n', '\r'], " "));
    }

    pub fn f32(&self, key: &str) -> Option<f32> {
        self.map.get(key).and_then(|v| v.parse().ok()).filter(|v: &f32| v.is_finite())
    }

    pub fn bool(&self, key: &str) -> Option<bool> {
        self.map.get(key).map(|v| v == "1" || v == "true")
    }

    pub fn remove(&mut self, key: &str) {
        self.map.remove(key);
    }

    /// Пары с ключом, начинающимся на `prefix` (ключ — без приставки).
    pub fn prefixed(&self, prefix: &str) -> Vec<(String, String)> {
        self.map.range(prefix.to_string()..).take_while(|(k, _)| k.starts_with(prefix)).map(|(k, v)| (k[prefix.len()..].to_string(), v.clone())).collect()
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(|s| s.as_str())
    }

    /// Прочитать число в поле, если оно есть.
    pub fn read_f32(&self, key: &str, into: &mut f32) {
        if let Some(v) = self.f32(key) {
            *into = v;
        }
    }

    pub fn read_bool(&self, key: &str, into: &mut bool) {
        if let Some(v) = self.bool(key) {
            *into = v;
        }
    }
}

/// Кисть (инструмент сетки) без клавиши и прочего интерфейса.
#[derive(Clone)]
pub struct Brush {
    pub kind: ToolKind,
    /// Короткое имя вида для ячейки сетки (ключ перевода).
    pub name: &'static str,
    pub icon: &'static str,
    pub params: BrushParams,
    /// Пока не сделан — показан неактивным.
    pub enabled: bool,
    pub grain: GrainSettings,
    /// Своё имя («Tool Name» в SAI); `None` — имя вида.
    pub custom: Option<String>,
    /// Вторая строка в ячейке («Additional Name» в SAI).
    pub extra: String,
    /// Динамика кисти: «вход → кривая → выход» поверх нажима.
    pub dynamics: Vec<paint_core::dynamics::Dynamic>,
    /// Своя динамика из плагина: «id плагина/имя динамики».
    pub dyn_plugin: Option<String>,
    /// «Срезать крючки» (Anti-Hook, SAI 2018+): длина крючка на концах мазка, точки экрана;
    /// 0 — выключено.
    pub anti_hook: f32,
}

/// Вид инструмента сетки — как в меню SAI 2017 по правому щелчку на пустой ячейке.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToolKind {
    Pencil,
    AirBrush,
    Brush,
    Water,
    Eraser,
    Marker,
    Binary,
    Bucket,
    Gradient,
    SelPen,
    SelErs,
    /// «Размытие» (SAI 2020+): кисть, которая только размывает под собой.
    Blur,
    /// «Растушёвка» (SAI 2017-11+): тянет краску холста.
    Smudge,
    /// Заливка лассо (SAI 2026): обвести — залить; с «замкнутыми областями» — только то, что
    /// замкнуто линиями внутри обводки (docs/41).
    LassoFill,
    /// «Россыпь» — Scatter в SAI 2018+: несколько форм на шаг со случайными сдвигом, размером, углом и цветом.
    Scatter,
    /// «Перо эффектов» (SAI 2019+): мазок ложится выбранным режимом наложения.
    EffectPen,
    /// «Пиксельное перо» (SAI 2020+, вкладка Binary): точки без сглаживания, без нажима.
    PixelPen,
    /// Двоичный ластик (SAI 2020+): стирает точками без сглаживания.
    BinEraser,
    /// Двоичные перо и ластик выделения (SAI 2020+).
    BinSelPen,
    BinSelErs,
}

impl ToolKind {
    /// В порядке меню SAI.
    pub const ALL: [ToolKind; 20] = [
        ToolKind::Pencil,
        ToolKind::AirBrush,
        ToolKind::Brush,
        ToolKind::Water,
        ToolKind::Eraser,
        ToolKind::Marker,
        ToolKind::Binary,
        ToolKind::Bucket,
        ToolKind::Gradient,
        ToolKind::SelPen,
        ToolKind::SelErs,
        ToolKind::Blur,
        ToolKind::Smudge,
        ToolKind::LassoFill,
        ToolKind::Scatter,
        ToolKind::EffectPen,
        ToolKind::PixelPen,
        ToolKind::BinEraser,
        ToolKind::BinSelPen,
        ToolKind::BinSelErs,
    ];

    pub fn key(self) -> &'static str {
        match self {
            ToolKind::Pencil => "pencil",
            ToolKind::AirBrush => "airbrush",
            ToolKind::Brush => "brush",
            ToolKind::Water => "water",
            ToolKind::Eraser => "eraser",
            ToolKind::Marker => "marker",
            ToolKind::Binary => "binary",
            ToolKind::Bucket => "bucket",
            ToolKind::Gradient => "gradient",
            ToolKind::SelPen => "selpen",
            ToolKind::SelErs => "selers",
            ToolKind::Blur => "blur",
            ToolKind::Smudge => "smudge",
            ToolKind::LassoFill => "lassofill",
            ToolKind::Scatter => "scatter",
            ToolKind::EffectPen => "effectpen",
            ToolKind::PixelPen => "pixelpen",
            ToolKind::BinEraser => "binerase",
            ToolKind::BinSelPen => "binselpen",
            ToolKind::BinSelErs => "binselers",
        }
    }

    pub fn from_key(k: &str) -> Option<ToolKind> {
        ToolKind::ALL.into_iter().find(|t| t.key() == k)
    }

    /// Полное имя вида (для меню).
    pub fn label(self) -> &'static str {
        match self {
            ToolKind::Pencil => "Карандаш",
            ToolKind::AirBrush => "Аэрограф",
            ToolKind::Brush => "Кисть",
            ToolKind::Water => "Акварель",
            ToolKind::Eraser => "Ластик",
            ToolKind::Marker => "Маркер",
            ToolKind::Binary => "Двоичное перо",
            ToolKind::Bucket => "Заливка",
            ToolKind::Gradient => "Градиент",
            ToolKind::SelPen => "Перо выделения",
            ToolKind::SelErs => "Ластик выделения",
            ToolKind::Blur => "Размытие",
            ToolKind::Smudge => "Растушёвка",
            ToolKind::LassoFill => "Заливка лассо",
            ToolKind::Scatter => "Россыпь",
            ToolKind::EffectPen => "Перо эффектов",
            ToolKind::PixelPen => "Пиксельное перо",
            ToolKind::BinEraser => "Двоичный ластик",
            ToolKind::BinSelPen => "Двоичное перо выделения",
            ToolKind::BinSelErs => "Двоичный ластик выделения",
        }
    }

    /// Новая кисть этого вида со значениями по умолчанию, как у нового инструмента в SAI.
    pub fn new_brush(self) -> Brush {
        let b = |size: f32, hardness: f32, density: f32| BrushParams {
            size,
            hardness,
            density,
            ..Default::default()
        };
        // Значения по умолчанию — как у нового инструмента в SAI: 50 px, мин. размер 50 %,
        // нажим на размер и на плотность (Аэрограф — самая мягкая ступень, остальные — самая жёсткая).
        let (name, icon, params) = match self {
            ToolKind::Pencil => ("Каранд.", "pen", b(3.0, 1.0, 1.0)),
            ToolKind::AirBrush => ("Аэрогр.", "airbrush", BrushParams { min_size: 0.5, density_by_pressure: true, ..b(50.0, 0.0, 1.0) }),
            ToolKind::Brush => (
                "Кисть",
                "brush",
                BrushParams { min_size: 0.5, density_by_pressure: true, mix: Some(MixParams::new(0.0, 0.0, 0.5)), ..b(50.0, 1.0, 1.0) },
            ),
            ToolKind::Water => (
                "Акварель",
                "water",
                BrushParams {
                    min_size: 0.5,
                    density_by_pressure: true,
                    ver1_density: true,
                    amplify: 0.5,
                    mix: Some(MixParams { blur: true, ..MixParams::new(0.3, 0.3, 0.5) }),
                    ..b(50.0, 1.0, 1.0)
                },
            ),
            // у Маркера в SAI нет порога размытия
            ToolKind::Marker => (
                "Маркер",
                "marker",
                BrushParams {
                    min_size: 0.5,
                    density_by_pressure: true,
                    ver1_density: true,
                    amplify: 0.5,
                    mix: Some(MixParams { blur_pressure: 0.0, marker: true, ..MixParams::new(0.0, 0.0, 0.5) }),
                    ..b(50.0, 1.0, 1.0)
                },
            ),
            ToolKind::Eraser => ("Ластик", "eraser", BrushParams { erase: true, size_by_pressure: false, ..b(30.0, 0.75, 1.0) }),
            // Binary Pen в SAI 2017: 1 px, мин. размер 0 %, «Opacity» 255
            ToolKind::Binary => ("Двоичн.", "binary", BrushParams { binary: true, ..b(1.0, 1.0, 1.0) }),
            ToolKind::Bucket => ("Заливка", "bucket", b(10.0, 1.0, 1.0)),
            // В SAI градиент ставится в свободную ячейку сетки («Gradation»)
            ToolKind::Gradient => ("Градиент", "gradient", b(10.0, 1.0, 1.0)),
            // кисти выделения в SAI по умолчанию не реагируют на нажим
            ToolKind::SelPen => ("Выд.К.", "selpen", BrushParams { size_by_pressure: false, ..b(10.0, 1.0, 1.0) }),
            ToolKind::SelErs => ("Выд.Л.", "selers", BrushParams { size_by_pressure: false, ..b(10.0, 1.0, 1.0) }),
            ToolKind::LassoFill => ("Зал.лассо", "lassofill", b(10.0, 1.0, 1.0)),
            // «Размытие» в SAI 2026: 50 px, самая мягкая ступень, плотность от нажима (мин. 0 %),
            // ширина размытия 50 %, резкость 50
            // «Растушёвка» в SAI 2026: 50 px, самая мягкая ступень, нажим на размер и плотность
            // (мин. 0 %), плотность 90, «Окрашивание» выключено (20), «Нажим без окраски» 90 %
            ToolKind::Smudge => (
                "Растуш.",
                "smudge",
                BrushParams { min_size: 0.0, density_by_pressure: true, mix: Some(MixParams { smudge: true, blend_by_pressure: false, ..MixParams::new(0.0, 0.0, 0.5) }), ..b(50.0, 0.0, 0.9) },
            ),
            // «Разброс» в SAI 2026: 50 px, мин. размер 50 %, жёсткость 100 %, шаг 25 %, форма —
            // простой круг; разбросы выключены, одна форма на шаг
            ToolKind::Scatter => (
                "Россыпь",
                "scatter",
                BrushParams { min_size: 0.5, spacing: 0.25, scatter: Some(paint_core::scatter::ScatterParams::default()), effect: Some(paint_core::blend::BlendMode::Normal), ..b(50.0, 1.0, 1.0) },
            ),
            // «Перо эффектов» в SAI 2026: 50 px, мин. размер 50 %, самая жёсткая ступень, режим «Обычный»
            ToolKind::EffectPen => (
                "Эффекты",
                "effectpen",
                BrushParams { min_size: 0.5, effect: Some(paint_core::blend::BlendMode::Normal), ..b(50.0, 1.0, 1.0) },
            ),
            // вкладка Binary в SAI 2026: пиксельное перо — 1 px, без нажима (алгоритм «Simple»),
            // непрозрачность 255; ластик — 50 px; формы — круг точек с центрами внутри (сверено)
            ToolKind::PixelPen => ("Пикс.перо", "pixelpen", BrushParams { binary: true, size_by_pressure: false, ..b(1.0, 1.0, 1.0) }),
            ToolKind::BinEraser => ("Дв.ластик", "binerase", BrushParams { binary: true, erase: true, size_by_pressure: false, ..b(50.0, 1.0, 1.0) }),
            ToolKind::BinSelPen => ("Дв.Выд.К.", "binselpen", BrushParams { binary: true, size_by_pressure: false, ..b(10.0, 1.0, 1.0) }),
            ToolKind::BinSelErs => ("Дв.Выд.Л.", "binselers", BrushParams { binary: true, size_by_pressure: false, ..b(10.0, 1.0, 1.0) }),
            ToolKind::Blur => (
                "Размытие",
                "blur",
                BrushParams { size_by_pressure: false, density_by_pressure: true, sharpness: 0.5, mix: Some(MixParams { blur: true, blur_only: true, ..MixParams::new(0.0, 0.0, 0.5) }), ..b(50.0, 0.0, 1.0) },
            ),
        };
        Brush { kind: self, name, icon, params, enabled: true, grain: GrainSettings::default(), custom: None, extra: String::new(), dynamics: Vec::new(), dyn_plugin: None, anti_hook: 0.0 }
    }
}


/// Текстура кисти: нет, бумага слоя или файл из `brushtex`.
#[derive(Clone, PartialEq)]
pub enum TexChoice {
    None,
    Paper,
    File(String),
}

/// Разделы «Форма» и «Текстура» инструмента (как в SAI 2017; docs/29-brush-form-texture.md).
#[derive(Clone)]
pub struct GrainSettings {
    /// Форма «пятно» из `blotmap`; `None` — простой круг.
    pub form: Option<String>,
    pub form_strength: f32,
    pub form_scale: u16,
    pub tex: TexChoice,
    pub tex_strength: f32,
    pub tex_scale: u16,
    pub by_min_density: bool,
    /// Щетина из `bristle` (вместо «пятна»), её Hair Size, Hair Min, Randomize, Direction.
    pub bristle: Option<String>,
    pub hair_size: f32,
    pub hair_min: f32,
    pub randomize: f32,
    pub direction: u8,
    /// Отпечаток-картинка из `tips` (вместо круга) и его поворот в градусах.
    pub tip: Option<String>,
    pub tip_angle: f32,
    /// Толщина отпечатка (1 — круглый или как картинка; меньше — сплюснут) и поворот по мазку.
    pub tip_ratio: f32,
    pub tip_follow: bool,
    /// У набора картинок — смена вразнобой (иначе по порядку).
    pub tip_random: bool,
}

impl Default for GrainSettings {
    fn default() -> Self {
        Self {
            form: None,
            form_strength: 1.0,
            form_scale: 100,
            tex: TexChoice::None,
            tex_strength: 1.0,
            tex_scale: 100,
            by_min_density: true,
            bristle: None,
            hair_size: 0.5,
            hair_min: 0.0,
            randomize: 0.0,
            direction: 1,
            tip: None,
            tip_angle: 0.0,
            tip_ratio: 1.0,
            tip_follow: false,
            tip_random: false,
        }
    }
}


/// Параметры инструмента сетки из настроек (ключи с приставкой `prefix`, например `brush.3.`);
/// то же пишет файл кисти `.gsbrush`.
pub fn read(b: &mut Brush, s: &Kv, prefix: &str) {
    let k = |name: &str| format!("{prefix}{name}");
    if let Some(v) = s.str(&k("dyn_plugin")) {
        b.dyn_plugin = (!v.is_empty()).then(|| v.chars().take(200).collect());
    }
    if let Some(n) = s.f32(&k("dyn.count")) {
        b.dynamics = (0..(n as usize).min(32)).filter_map(|i| s.str(&k(&format!("dyn.{i}"))).and_then(paint_core::dynamics::Dynamic::from_text)).collect();
    }
    s.read_f32(&k("anti_hook"), &mut b.anti_hook);
    b.anti_hook = b.anti_hook.clamp(0.0, 100.0);
    let p = &mut b.params;
    s.read_f32(&k("size"), &mut p.size);
    s.read_f32(&k("min_size"), &mut p.min_size);
    s.read_f32(&k("density"), &mut p.density);
    s.read_f32(&k("min_density"), &mut p.min_density);
    s.read_f32(&k("hardness"), &mut p.hardness);
    s.read_f32(&k("spacing"), &mut p.spacing);
    s.read_f32(&k("sharpness"), &mut p.sharpness);
    s.read_f32(&k("amplify"), &mut p.amplify);
    s.read_bool(&k("size_by_pressure"), &mut p.size_by_pressure);
    s.read_bool(&k("density_by_pressure"), &mut p.density_by_pressure);
    s.read_bool(&k("ver1_density"), &mut p.ver1_density);
    if let Some(m) = p.mix.as_mut() {
        s.read_f32(&k("blending"), &mut m.blending);
        s.read_f32(&k("dilution"), &mut m.dilution);
        s.read_f32(&k("persistence"), &mut m.persistence);
        s.read_f32(&k("blur_pressure"), &mut m.blur_pressure);
        s.read_bool(&k("blend_by_pressure"), &mut m.blend_by_pressure);
        s.read_f32(&k("blur_width"), &mut m.blur_width);
        s.read_f32(&k("smudge_color"), &mut m.smudge_color);
        s.read_f32(&k("smudge_uncolor"), &mut m.smudge_uncolor);
        m.smudge_color = m.smudge_color.clamp(0.0, 1.0);
        m.smudge_uncolor = m.smudge_uncolor.clamp(0.0, 1.0);
        m.blur_width = m.blur_width.clamp(0.0, 1.0);
    }
    if let Some(e) = p.effect.as_mut() {
        if let Some(i) = s.f32(&k("effect")) {
            *e = paint_core::blend::BlendMode::ALL.get(i as usize).copied().unwrap_or(paint_core::blend::BlendMode::Normal);
        }
    }
    if let Some(sc) = p.scatter.as_mut() {
        read_scatter(sc, s, prefix);
    }
    p.size = p.size.clamp(0.1, 5000.0);
    let g = &mut b.grain;
    if let Some(v) = s.str(&k("bristle")) {
        g.bristle = (!v.is_empty()).then(|| v.to_string());
    }
    s.read_f32(&k("hair_size"), &mut g.hair_size);
    s.read_f32(&k("hair_min"), &mut g.hair_min);
    s.read_f32(&k("randomize"), &mut g.randomize);
    if let Some(v) = s.f32(&k("direction")) {
        g.direction = (v as u8).min(2);
    }
    if let Some(v) = s.str(&k("tip")) {
        g.tip = (!v.is_empty()).then(|| v.to_string());
    }
    s.read_f32(&k("tip_angle"), &mut g.tip_angle);
    s.read_f32(&k("tip_ratio"), &mut g.tip_ratio);
    g.tip_ratio = g.tip_ratio.clamp(0.01, 1.0);
    s.read_bool(&k("tip_follow"), &mut g.tip_follow);
    s.read_bool(&k("tip_random"), &mut g.tip_random);
    if let Some(v) = s.str(&k("form")) {
        g.form = (!v.is_empty()).then(|| v.to_string());
    }
    s.read_f32(&k("form_strength"), &mut g.form_strength);
    if let Some(v) = s.f32(&k("form_scale")) {
        g.form_scale = v.clamp(10.0, 500.0) as u16;
    }
    if let Some(v) = s.str(&k("texture")) {
        g.tex = match v {
            "" => TexChoice::None,
            "paper" => TexChoice::Paper,
            v => TexChoice::File(v.trim_start_matches("file:").to_string()),
        };
    }
    s.read_f32(&k("tex_strength"), &mut g.tex_strength);
    if let Some(v) = s.f32(&k("tex_scale")) {
        g.tex_scale = v.clamp(10.0, 500.0) as u16;
    }
    s.read_bool(&k("tex_min_density"), &mut g.by_min_density);
}

/// Параметры инструмента сетки в настройки (обратное к [`read`]).
pub fn write(b: &Brush, s: &mut Kv, prefix: &str) {
    let k = |name: &str| format!("{prefix}{name}");
    s.set(&k("dyn_plugin"), b.dyn_plugin.as_deref().unwrap_or(""));
    s.set(&k("dyn.count"), b.dynamics.len());
    for (n, d) in b.dynamics.iter().enumerate() {
        s.set(&k(&format!("dyn.{n}")), d.to_text());
    }
    s.set(&k("anti_hook"), b.anti_hook);
    let p = &b.params;
    s.set(&k("size"), p.size);
    s.set(&k("min_size"), p.min_size);
    s.set(&k("density"), p.density);
    s.set(&k("min_density"), p.min_density);
    s.set(&k("hardness"), p.hardness);
    s.set(&k("spacing"), p.spacing);
    s.set(&k("sharpness"), p.sharpness);
    s.set(&k("amplify"), p.amplify);
    s.set(&k("size_by_pressure"), p.size_by_pressure as u8);
    s.set(&k("density_by_pressure"), p.density_by_pressure as u8);
    s.set(&k("ver1_density"), p.ver1_density as u8);
    if let Some(m) = &p.mix {
        s.set(&k("blending"), m.blending);
        s.set(&k("dilution"), m.dilution);
        s.set(&k("persistence"), m.persistence);
        s.set(&k("blur_pressure"), m.blur_pressure);
        s.set(&k("blend_by_pressure"), m.blend_by_pressure as u8);
        if m.blur_only {
            s.set(&k("blur_width"), m.blur_width);
        }
        if m.smudge {
            s.set(&k("smudge_color"), m.smudge_color);
            s.set(&k("smudge_uncolor"), m.smudge_uncolor);
        }
    }
    if let Some(e) = p.effect {
        s.set(&k("effect"), paint_core::blend::BlendMode::ALL.iter().position(|m| *m == e).unwrap_or(0));
    }
    if let Some(sc) = &p.scatter {
        write_scatter(sc, s, prefix);
    }
    let g = &b.grain;
    s.set(&k("form"), g.form.as_deref().unwrap_or(""));
    s.set(&k("bristle"), g.bristle.as_deref().unwrap_or(""));
    s.set(&k("hair_size"), g.hair_size);
    s.set(&k("hair_min"), g.hair_min);
    s.set(&k("randomize"), g.randomize);
    s.set(&k("direction"), g.direction);
    s.set(&k("tip"), g.tip.as_deref().unwrap_or(""));
    s.set(&k("tip_angle"), g.tip_angle);
    s.set(&k("tip_ratio"), g.tip_ratio);
    s.set(&k("tip_follow"), g.tip_follow as u8);
    s.set(&k("tip_random"), g.tip_random as u8);
    s.set(&k("form_strength"), g.form_strength);
    s.set(&k("form_scale"), g.form_scale);
    let tex = match &g.tex {
        TexChoice::None => String::new(),
        TexChoice::Paper => "paper".to_string(),
        TexChoice::File(n) => format!("file:{n}"),
    };
    s.set(&k("texture"), tex);
    s.set(&k("tex_strength"), g.tex_strength);
    s.set(&k("tex_scale"), g.tex_scale);
    s.set(&k("tex_min_density"), g.by_min_density as u8);
}

/// Настройки разброса (ключи `sc_…`); значения обрезаются до пределов SAI.
fn read_scatter(sc: &mut paint_core::scatter::ScatterParams, s: &Kv, prefix: &str) {
    use paint_core::scatter::AngleControl;
    let k = |name: &str| format!("{prefix}sc_{name}");
    if let Some(v) = s.f32(&k("angle_control")) {
        sc.angle_control = match v as u8 {
            0 => AngleControl::None,
            2 => AngleControl::Direction,
            _ => AngleControl::Auto,
        };
    }
    let mut deg = sc.angle.to_degrees();
    s.read_f32(&k("angle"), &mut deg);
    sc.angle = deg.clamp(-180.0, 180.0).to_radians();
    for (name, v, max) in [
        ("angle_jitter", &mut sc.angle_jitter, 1.0),
        ("scaling", &mut sc.scaling, 10.0),
        ("size_jitter", &mut sc.size_jitter, 1.0),
        ("scattering", &mut sc.scattering, 2.0),
        ("ratio", &mut sc.ratio, 100.0),
        ("ratio_jitter", &mut sc.ratio_jitter, 1.0),
        ("count_jitter", &mut sc.count_jitter, 1.0),
        ("fg_bg_jitter", &mut sc.fg_bg_jitter, 1.0),
        ("hue_jitter", &mut sc.hue_jitter, 1.0),
        ("sat_jitter", &mut sc.sat_jitter, 1.0),
        ("bri_jitter", &mut sc.bri_jitter, 1.0),
    ] {
        s.read_f32(&k(name), v);
        *v = v.clamp(0.0, max);
    }
    sc.scaling = sc.scaling.max(0.01);
    sc.ratio = sc.ratio.max(0.01);
    if let Some(v) = s.f32(&k("count")) {
        sc.count = v.clamp(1.0, 20.0) as u8;
    }
    s.read_bool(&k("all_directions"), &mut sc.all_directions);
    s.read_bool(&k("gaussian"), &mut sc.gaussian);
    s.read_bool(&k("absolute_size"), &mut sc.absolute_size);
    s.read_bool(&k("int_position"), &mut sc.int_position);
    s.read_bool(&k("each_shape"), &mut sc.each_shape);
}

fn write_scatter(sc: &paint_core::scatter::ScatterParams, s: &mut Kv, prefix: &str) {
    use paint_core::scatter::AngleControl;
    let k = |name: &str| format!("{prefix}sc_{name}");
    s.set(&k("angle_control"), match sc.angle_control {
        AngleControl::None => 0,
        AngleControl::Auto => 1,
        AngleControl::Direction => 2,
    });
    s.set(&k("angle"), sc.angle.to_degrees());
    for (name, v) in [
        ("angle_jitter", sc.angle_jitter),
        ("scaling", sc.scaling),
        ("size_jitter", sc.size_jitter),
        ("scattering", sc.scattering),
        ("ratio", sc.ratio),
        ("ratio_jitter", sc.ratio_jitter),
        ("count_jitter", sc.count_jitter),
        ("fg_bg_jitter", sc.fg_bg_jitter),
        ("hue_jitter", sc.hue_jitter),
        ("sat_jitter", sc.sat_jitter),
        ("bri_jitter", sc.bri_jitter),
    ] {
        s.set(&k(name), v);
    }
    s.set(&k("count"), sc.count);
    s.set(&k("all_directions"), sc.all_directions as u8);
    s.set(&k("gaussian"), sc.gaussian as u8);
    s.set(&k("absolute_size"), sc.absolute_size as u8);
    s.set(&k("int_position"), sc.int_position as u8);
    s.set(&k("each_shape"), sc.each_shape as u8);
}

/// «Срезать крючки»: сколько точек с конца `pts` (x, y) — крючок длиной до `range`: короткий
/// хвост, повёрнутый к основному направлению мазка больше чем на 60°. 0 — крючка нет.
pub fn hook_len(pts: &[[f32; 2]], range: f32) -> usize {
    let n = pts.len();
    if n < 3 || range <= 0.0 {
        return 0;
    }
    let dist = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
    let end = pts[n - 1];
    let mut best = 0;
    let mut tail = 0.0;
    for k in (1..n - 1).rev() {
        tail += dist(pts[k + 1], pts[k]);
        if tail > range {
            break;
        }
        // основное направление — от точки на 2·range раньше до вершины k
        let mut back = 0.0;
        let mut j = k;
        while j > 0 && back < 2.0 * range {
            back += dist(pts[j], pts[j - 1]);
            j -= 1;
        }
        if back < range {
            continue;
        }
        let main = [pts[k][0] - pts[j][0], pts[k][1] - pts[j][1]];
        let hook = [end[0] - pts[k][0], end[1] - pts[k][1]];
        let (lm, lh) = (main[0].hypot(main[1]), hook[0].hypot(hook[1]));
        if lm < 1e-3 || lh < 1e-3 {
            continue;
        }
        let cos = (main[0] * hook[0] + main[1] * hook[1]) / (lm * lh);
        if cos < 0.5 {
            best = n - 1 - k;
        }
    }
    best
}

/// Имя файла из названия: буквы, цифры, пробел и `-_().`, остальное — `_`.
pub fn safe_name(s: &str) -> String {
    let n: String = s.chars().take(64).map(|c| if c.is_alphanumeric() || " -_().".contains(c) { c } else { '_' }).collect();
    // точки и пробелы в конце имени Windows отбрасывает — и «имя..gsbrush» выглядит странно
    let n = n.trim().trim_end_matches(['.', ' ']);
    if n.is_empty() { "brush".to_string() } else { n.to_string() }
}


impl Brush {
    /// Форма, текстура, щетина и отпечаток для движка. `paper` — бумага текущего слоя (для
    /// текстуры «бумага слоя»): текстура и масштаб.
    pub fn stroke_grain(&self, paper: Option<(Arc<paint_core::paper::PaperTexture>, u16)>) -> Option<paint_core::stroke::Grain> {
        let g = &self.grain;
        let blot = g.form.as_deref().and_then(|n| paint_core::paper::texture_of("blotmap", n)).map(|t| (t, g.form_strength, g.form_scale));
        let texture = match &g.tex {
            TexChoice::None => None,
            TexChoice::Paper => paper.map(|(t, scale)| (t, g.tex_strength, scale)),
            TexChoice::File(n) => paint_core::paper::texture_of("brushtex", n).map(|t| (t, g.tex_strength, g.tex_scale)),
        };
        let bristle = g.bristle.as_deref().and_then(paint_core::paper::bristle_of).map(|hairs| paint_core::stroke::Bristle {
            hairs,
            hair_size: g.hair_size,
            hair_min: g.hair_min,
            randomize: g.randomize,
            direction: g.direction,
        });
        let blot = if bristle.is_some() { None } else { blot };
        // картинка или (у сплюснутой круглой кисти) круг с краем по жёсткости
        let mut cells = g.tip.as_deref().map(paint_core::paper::stamps_of).unwrap_or_default();
        let dyn_tip = self.dynamics.iter().any(|d| d.output.needs_tip());
        // у разброса формы поворачиваются и сжимаются — тоже кругом-картинкой
        if cells.is_empty() && (g.tip_ratio < 0.999 || g.tip_follow || dyn_tip || self.params.scatter.is_some()) {
            cells.push(Arc::new(paint_core::Stamp::disc(self.params.hardness)));
        }
        let tip = (!cells.is_empty()).then(|| {
            let stamp = cells.remove(0);
            paint_core::Tip { stamp, angle: g.tip_angle.to_radians(), ratio: g.tip_ratio.clamp(0.01, 1.0), follow: g.tip_follow, more: cells, random: g.tip_random }
        });
        (blot.is_some() || texture.is_some() || bristle.is_some() || tip.is_some()).then(|| paint_core::stroke::Grain {
            blot,
            texture,
            paper: g.tex == TexChoice::Paper,
            by_min_density: g.by_min_density,
            bristle,
            tip,
        })
    }

    /// Мазок этой кистью (без цвета): параметры, форма и динамика.
    pub fn stroke(&self, paper: Option<(Arc<paint_core::paper::PaperTexture>, u16)>) -> paint_core::Stroke {
        let mut st = paint_core::Stroke::new(self.params);
        st.set_grain(self.stroke_grain(paper));
        st.dynamics = self.dynamics.clone();
        st
    }

    /// Образец мазка `w`×`h` на белом: волна с нажимом от слабого к сильному и обратно.
    pub fn preview_rgba(&self, w: u32, h: u32) -> Vec<u8> {
        let mut doc = paint_core::Document::new(w, h);
        let mut b = self.clone();
        b.params.size = b.params.size.min(h as f32 * 0.56);
        b.params.erase = false;
        b.params.mix = None;
        let mut st = b.stroke(None);
        let (fw, fh) = (w as f32, h as f32);
        for k in 0..=60 {
            let t = k as f32 / 60.0;
            let pressure = (t * std::f32::consts::PI).sin().max(0.05);
            st.add_point(paint_core::InputPoint { x: fw * 0.09 + t * fw * 0.82, y: fh / 2.0 + (t * std::f32::consts::TAU).sin() * fh * 0.22, pressure });
        }
        doc.commit_stroke(0, &st, [20, 20, 30]);
        doc.flatten()
    }

    /// Тот же образец без фона: RGBA слоя (без предумножения) — для показа на шашке.
    pub fn preview_layer_rgba(&self, w: u32, h: u32) -> Vec<u8> {
        let mut doc = paint_core::Document::new(w, h);
        let mut b = self.clone();
        b.params.size = b.params.size.min(h as f32 * 0.56);
        b.params.erase = false;
        b.params.mix = None;
        let mut st = b.stroke(None);
        let (fw, fh) = (w as f32, h as f32);
        for k in 0..=60 {
            let t = k as f32 / 60.0;
            let pressure = (t * std::f32::consts::PI).sin().max(0.05);
            st.add_point(paint_core::InputPoint { x: fw * 0.09 + t * fw * 0.82, y: fh / 2.0 + (t * std::f32::consts::TAU).sin() * fh * 0.22, pressure });
        }
        doc.commit_stroke(0, &st, [0, 0, 0]);
        let px = &doc.layers[0].pixels;
        (0..h as i32).flat_map(|y| (0..w as i32).map(move |x| (x, y))).flat_map(|(x, y)| paint_core::pixel::to_rgba8(px.get(x, y))).collect()
    }

    /// Образец мазка 256×64 в PNG.
    pub fn preview_png(&self) -> Option<Vec<u8>> {
        paint_core::io::encode_png(256, 64, &self.preview_rgba(256, 64)).ok()
    }
}

/// Файлы одной кисти в архиве `.gsbrush` (с приставкой папки `prefix` у набора): `brush.txt`,
/// картинки отпечатка, текстура из своей папки программы, образец мазка `preview.png`.
/// `name` — имя кисти для файла (своё или вида на языке интерфейса).
pub fn to_files(b: &Brush, name: &str, prefix: &str) -> Vec<(String, Vec<u8>)> {
    let mut s = Kv::default();
    s.set("format", 1);
    s.set("kind", b.kind.key());
    s.set("name", name);
    s.set("extra", &b.extra);
    write(b, &mut s, "");
    let mut out = Vec::new();
    if let Some(n) = &b.grain.tip {
        for (k, st) in paint_core::paper::stamps_of(n).iter().enumerate() {
            let rgba: Vec<u8> = st.alpha.iter().flat_map(|&a| [0, 0, 0, a]).collect();
            if let Ok(png) = paint_core::io::encode_png(st.width as u32, st.height as u32, &rgba) {
                out.push((format!("{prefix}{}", if k == 0 { "tip.png".to_string() } else { format!("tip.{}.png", k + 1) }), png));
            }
        }
    }
    // текстура из своей папки (взятая из чужой кисти) — внутрь файла; узоры SAI — только по имени
    if let TexChoice::File(n) = &b.grain.tex {
        if let Some(bmp) = paint_core::paper::user_dir("brushtex").and_then(|d| std::fs::read(d.join(format!("{n}.bmp"))).ok()) {
            out.push((format!("{prefix}texture.bmp"), bmp));
        }
    }
    if let Some(png) = b.preview_png() {
        out.push((format!("{prefix}preview.png"), png));
    }
    let text = format!("# GrazySAI {} — кисть / brush\n{}", env!("CARGO_PKG_VERSION"), s.to_text()); // не перевод
    out.insert(0, (format!("{prefix}brush.txt"), text.into_bytes()));
    out
}

/// Кисти из файлов архива `.gsbrush` / `.gsbrushes`. Картинки отпечатков и текстуры кладутся в
/// свою папку программы под очищенными именами; `texture_prefix` — начало имени текстуры
/// («Кисть» на языке интерфейса). Не больше 256 кистей; имена ресурсов с путями отбрасываются.
pub fn from_files(files: &[(String, Vec<u8>)], texture_prefix: &str) -> Vec<Brush> {
    // папки набора: «001/brush.txt» …; у одной кисти — «brush.txt» в корне
    let mut prefixes: Vec<String> = files.iter().filter_map(|(n, _)| n.strip_suffix("brush.txt").map(str::to_string)).filter(|p| p.is_empty() || (p.ends_with('/') && !p[..p.len() - 1].contains('/'))).collect();
    prefixes.sort();
    prefixes.truncate(256);
    let get = |name: &str| files.iter().find(|(n, _)| n == name).map(|(_, d)| d.as_slice());
    let mut out = Vec::new();
    for pre in &prefixes {
        let Some(text) = get(&format!("{pre}brush.txt")).map(String::from_utf8_lossy) else { continue };
        let s = Kv::from_text(&text);
        let kind = s.str("kind").and_then(ToolKind::from_key).unwrap_or(ToolKind::Pencil);
        let mut t = kind.new_brush();
        read(&mut t, &s, "");
        let name: String = s.str("name").unwrap_or("").chars().take(64).collect();
        t.custom = (!name.trim().is_empty()).then(|| name.clone());
        t.extra = s.str("extra").unwrap_or("").chars().take(32).collect();
        // имена файлов ресурсов из чужого файла — без путей
        let g = &mut t.grain;
        for field in [&mut g.form, &mut g.bristle, &mut g.tip] {
            if field.as_deref().is_some_and(|n| n.contains(['/', '\\']) || n.contains("..")) {
                *field = None;
            }
        }
        if matches!(&g.tex, TexChoice::File(n) if n.contains(['/', '\\']) || n.contains("..")) {
            g.tex = TexChoice::None;
        }
        // картинки отпечатка из файла
        let mut cells = Vec::new();
        for k in 1..=64 {
            let file = if k == 1 { format!("{pre}tip.png") } else { format!("{pre}tip.{k}.png") };
            let Some(png) = get(&file) else { break };
            match paint_core::io::decode_png(png) {
                Ok((w, h, rgba)) => cells.push(paint_core::Stamp::from_rgba(w as usize, h as usize, &rgba)),
                Err(_) => break,
            }
        }
        if !cells.is_empty() {
            let base = if name.trim().is_empty() { "brush".to_string() } else { name.clone() };
            if let Ok(saved) = paint_core::paper::save_tips(&base, &cells) {
                t.grain.tip = Some(saved);
            }
        }
        if let (Some(bmp), Some(dir)) = (get(&format!("{pre}texture.bmp")), paint_core::paper::user_dir("brushtex")) {
            if paint_core::paper::PaperTexture::from_bmp(bmp).is_some() {
                let n = safe_name(&format!("{texture_prefix} {}", if name.trim().is_empty() { "brush" } else { &name }));
                if std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(dir.join(format!("{n}.bmp")), bmp)).is_ok() {
                    t.grain.tex = TexChoice::File(n);
                }
            }
        }
        out.push(t);
    }
    out
}

/// Сохранить кисти в `.gsbrush` (одна) или `.gsbrushes` (несколько, по папкам `001/`…).
/// `names` — имя каждой кисти для файла.
pub fn save(path: &std::path::Path, brushes: &[(&Brush, String)]) -> Result<(), Box<dyn std::error::Error>> {
    let entries: Vec<(String, Vec<u8>)> = if let [(b, n)] = brushes {
        to_files(b, n, "")
    } else {
        brushes.iter().enumerate().flat_map(|(k, (b, n))| to_files(b, n, &format!("{:03}/", k + 1))).collect()
    };
    paint_formats::pack::write(path, &entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Параметры кисти → текст → обратно: всё на месте.
    #[test]
    fn settings_round_trip() {
        let mut t = ToolKind::Brush.new_brush();
        t.params.size = 37.5;
        t.params.min_size = 0.25;
        t.params.spacing = 1.5;
        t.params.hardness = 0.75;
        if let Some(m) = t.params.mix.as_mut() {
            m.blending = 0.3;
            m.persistence = 0.7;
        }
        t.grain.tip = Some("Звезда".into());
        t.grain.tip_angle = 20.0;
        t.grain.tip_ratio = 0.4;
        t.grain.tip_follow = true;
        t.grain.tex = TexChoice::File("Бумага".into());
        t.grain.tex_scale = 150;
        t.dynamics.push(paint_core::dynamics::Dynamic::new(paint_core::dynamics::DynInput::Speed, paint_core::dynamics::DynOutput::Density));
        let mut s = Kv::default();
        write(&t, &mut s, "");
        let s = Kv::from_text(&s.to_text());
        let mut back = ToolKind::Brush.new_brush();
        read(&mut back, &s, "");
        assert_eq!((back.params.size, back.params.min_size, back.params.spacing, back.params.hardness), (37.5, 0.25, 1.5, 0.75));
        let m = back.params.mix.unwrap();
        assert_eq!((m.blending, m.persistence), (0.3, 0.7));
        let g = &back.grain;
        assert_eq!((g.tip.as_deref(), g.tip_angle, g.tip_ratio, g.tip_follow, g.tex_scale), (Some("Звезда"), 20.0, 0.4, true, 150));
        assert!(g.tex == TexChoice::File("Бумага".into()));
        assert_eq!(back.dynamics.len(), 1);
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(safe_name("Каранд."), "Каранд");
        assert_eq!(safe_name("../../etc/passwd"), ".._.._etc_passwd");
        assert_eq!(safe_name("  "), "brush");
    }

    #[test]
    fn values_with_line_breaks_stay_on_one_line() {
        let mut s = Kv::default();
        s.set("name", "a\nkind=eraser");
        assert_eq!(Kv::from_text(&s.to_text()).str("kind"), None);
    }

    #[test]
    fn preview_has_paint() {
        let px = ToolKind::Pencil.new_brush();
        let mut b = px;
        b.params.size = 8.0;
        assert!(b.preview_rgba(128, 32).as_chunks::<4>().0.iter().any(|p| p[0] < 100));
    }
}
