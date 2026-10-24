//! Свои инструменты SAI 2 (2020+): папка `custool` с подпапками по видам слоя (`nrm` — обычный
//! слой, `ink` — контур, `shp` — фигуры). Файлы — текст «ключ=Тип:значение» по строке, разделы
//! через «.», в конце `--EOF--`:
//! - `N.saitdat` — инструмент: `tidstr` (вид: pencil, brush, water…), `xidstr` (вариант:
//!   `v1brush`, `artoil`…), `name`, затем настройки (`cursize` — размер ×10, `minsize`,
//!   `curdens` — %, `wavno` — ступень жёсткости 0…4, `blend`, `dilution`, `persist`…);
//! - `_N.saitgrp` — группа: `name` и «ячейка=номер инструмента»;
//! - `_0.saitset` — порядок групп; `N.saitlnk` — ссылка на инструмент другой группы.
//!
//! Перевод в инструмент GrazySAI: от наших умолчаний вида меняется то, что есть в файле.

use crate::{Brush, ToolKind, TexChoice};
use paint_core::brush::HARDNESS_STEPS;
use paint_core::mix::MixParams;
use std::collections::BTreeMap;

/// Разобранный файл: значения без типа, по разделам подряд (ключи у SAI не повторяются).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sai2Tool {
    pub tid: String,
    pub xid: String,
    pub name: String,
    pub values: BTreeMap<String, String>,
}

/// Значения файла `.saitdat` / `.saitgrp`: «ключ → значение» (тип «I:», «U:», «S:» снят).
pub fn values(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line == "--EOF--" {
            break;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let v = match v.split_once(':') {
            Some((t, rest)) if t.len() == 1 => rest,
            _ => v,
        };
        out.entry(k.to_string()).or_insert_with(|| v.to_string());
    }
    out
}

/// Инструмент из текста `.saitdat`; `None` — это не инструмент SAI 2.
pub fn parse(text: &str) -> Option<Sai2Tool> {
    let values = values(text);
    let tid = values.get("tidstr")?.clone();
    Some(Sai2Tool { xid: values.get("xidstr").cloned().unwrap_or_default(), name: values.get("name").cloned().unwrap_or_default(), tid, values })
}

/// Группа из текста `.saitgrp`: имя и номера инструментов по ячейкам.
pub fn parse_group(text: &str) -> Option<(String, Vec<(usize, u32)>)> {
    let mut name = None;
    let mut cells = Vec::new();
    let mut part = 0;
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line == "." {
            part += 1;
            continue;
        }
        if line == "--EOF--" {
            break;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        if part == 0 && k == "name" {
            name = Some(v.strip_prefix("U:").unwrap_or(v).to_string());
        } else if part == 1 {
            if let (Ok(c), Ok(id)) = (k.parse(), v.parse()) {
                cells.push((c, id));
            }
        }
    }
    Some((name?, cells))
}

/// Вид инструмента GrazySAI по `tidstr`.
pub fn kind_of(tid: &str) -> Option<ToolKind> {
    Some(match tid {
        "pencil" => ToolKind::Pencil,
        "airbrush" => ToolKind::AirBrush,
        "brush" => ToolKind::Brush,
        "water" => ToolKind::Water,
        "marker" => ToolKind::Marker,
        "eraser" => ToolKind::Eraser,
        "selpen" => ToolKind::SelPen,
        "selers" => ToolKind::SelErs,
        "bucket" => ToolKind::Bucket,
        "gradation" => ToolKind::Gradient,
        "blur" => ToolKind::Blur,
        "effpen" => ToolKind::EffectPen,
        "scatter" => ToolKind::Scatter,
        "smudge" => ToolKind::Smudge,
        "binpen" => ToolKind::Binary,
        "dotpen" => ToolKind::PixelPen,
        "biners" => ToolKind::BinEraser,
        "bselpen" => ToolKind::BinSelPen,
        "bselers" => ToolKind::BinSelErs,
        _ => return None,
    })
}

/// Инструмент GrazySAI из инструмента SAI 2. `Err` — вид, которого у нас нет (по `tidstr`).
pub fn to_brush(t: &Sai2Tool) -> Result<Brush, String> {
    let kind = kind_of(&t.tid).ok_or_else(|| t.tid.clone())?;
    let mut b = kind.new_brush();
    let int = |k: &str| t.values.get(k).and_then(|v| v.trim().parse::<i64>().ok());
    let pct = |k: &str| int(k).map(|v| (v as f32 / 100.0).clamp(0.0, 1.0));
    let flag = |k: &str| int(k).map(|v| v != 0);
    let text = |k: &str| t.values.get(k).filter(|v| !v.is_empty()).cloned();
    let p = &mut b.params;
    if let Some(v) = int("cursize") {
        p.size = (v as f32 / 10.0).clamp(0.1, 5000.0);
    }
    if let Some(v) = pct("minsize") {
        p.min_size = v;
    }
    if let Some(v) = pct("curdens") {
        p.density = v.max(0.01);
    }
    if let Some(v) = pct("mindens") {
        p.min_density = v;
    }
    if let Some(i) = int("wavno") {
        p.hardness = HARDNESS_STEPS[i.clamp(0, 4) as usize];
    }
    // у «Россыпи» жёсткость формы — своя, в процентах
    if let Some(h) = pct("scathard").filter(|_| kind == ToolKind::Scatter) {
        p.hardness = HARDNESS_STEPS.iter().copied().min_by(|a, b| (a - h).abs().total_cmp(&(b - h).abs())).unwrap_or(1.0);
    }
    if let Some(v) = pct("hardness") {
        p.sharpness = v;
    }
    if let Some(v) = flag("szprsf") {
        p.size_by_pressure = v;
    }
    if let Some(v) = flag("dnprsf") {
        p.density_by_pressure = v;
    }
    if let Some(v) = flag("v1dnprsf") {
        p.ver1_density = v;
    }
    if let Some(v) = pct("densamp") {
        p.amplify = v;
    }
    if let Some(m) = p.mix.as_mut() {
        if let Some(v) = pct("blend") {
            m.blending = v;
        }
        if let Some(v) = pct("dilution") {
            m.dilution = v;
        }
        if let Some(v) = pct("persist") {
            m.persistence = v;
        }
        if let Some(v) = flag("blprsf") {
            m.blend_by_pressure = v;
        }
        if let Some(v) = pct("blurthres") {
            m.blur_pressure = v;
        }
        if m.smudge {
            m.smudge_color = if flag("encolf") == Some(true) { pct("coloring").unwrap_or(0.2) } else { 0.0 };
            if let Some(v) = pct("uncolthres") {
                m.smudge_uncolor = v;
            }
        }
    } else if kind == ToolKind::Brush && (int("blend").is_some() || int("dilution").is_some()) {
        p.mix = Some(MixParams::new(pct("blend").unwrap_or(0.0), pct("dilution").unwrap_or(0.0), pct("persist").unwrap_or(0.5)));
    }
    if let Some(sc) = p.scatter.as_mut() {
        let deg = |k: &str| int(k).map(|v| (v as f32).to_radians());
        if let Some(v) = int("scatanc") {
            sc.angle_control = match v {
                0 => paint_core::scatter::AngleControl::None,
                2 => paint_core::scatter::AngleControl::Direction,
                _ => paint_core::scatter::AngleControl::Auto,
            };
        }
        if let Some(v) = deg("scatang") {
            sc.angle = v;
        }
        let set = |field: &mut f32, k: &str, max: f32| {
            if let Some(v) = int(k) {
                *field = (v as f32 / 100.0).clamp(0.0, max);
            }
        };
        set(&mut sc.angle_jitter, "scatanj", 1.0);
        set(&mut sc.scaling, "scatscl", 10.0);
        set(&mut sc.size_jitter, "scatszj", 1.0);
        set(&mut sc.scattering, "scatsct", 2.0);
        set(&mut sc.ratio_jitter, "scatwhj", 1.0);
        set(&mut sc.count_jitter, "scatcnj", 1.0);
        set(&mut sc.fg_bg_jitter, "scatcolj", 1.0);
        set(&mut sc.hue_jitter, "scathuej", 1.0);
        set(&mut sc.sat_jitter, "scatsatj", 1.0);
        set(&mut sc.bri_jitter, "scatbrij", 1.0);
        // толщина формы: −100…100, меньше 0 — уже, больше — ниже
        if let Some(v) = int("scatwth") {
            sc.ratio = 2f32.powf(v.clamp(-100, 100) as f32 / 100.0);
        }
        if let Some(v) = int("scatcnt") {
            sc.count = v.clamp(1, 20) as u8;
        }
        for (field, k) in [(&mut sc.all_directions, "scataldf"), (&mut sc.gaussian, "scatgauf"), (&mut sc.absolute_size, "scatabsf"), (&mut sc.int_position, "scatintf"), (&mut sc.each_shape, "scataesf")] {
            if let Some(v) = flag(k) {
                *field = v;
            }
        }
        if let Some(v) = int("scatspc") {
            p.spacing = (v as f32 / 100.0).clamp(0.01, 10.0);
        }
    }
    let g = &mut b.grain;
    // форма: 1 — пятно (`blotmap`), 2 — щетина (`bristle`)
    match (int("fomcat"), text("fomnam")) {
        (Some(1), Some(n)) => {
            g.form = Some(n);
            if let Some(v) = pct("blotval") {
                g.form_strength = v;
            }
            if let Some(v) = int("blotscl") {
                g.form_scale = v.clamp(1, 1000) as u16;
            }
        }
        (Some(2), Some(n)) => {
            g.bristle = Some(n);
            if let Some(v) = pct("brslsiz") {
                g.hair_size = v;
            }
            if let Some(v) = pct("brslmin") {
                g.hair_min = v;
            }
            if let Some(v) = pct("brslrnd") {
                g.randomize = v;
            }
            if let Some(v) = int("brsldir") {
                g.direction = v.clamp(0, 3) as u8;
            }
        }
        _ => {}
    }
    if let (Some(2), Some(n)) = (int("texcat"), text("texnam")) {
        g.tex = TexChoice::File(n);
        if let Some(v) = pct("texval") {
            g.tex_strength = v;
        }
        if let Some(v) = int("texscl") {
            g.tex_scale = v.clamp(1, 1000) as u16;
        }
    }
    if !t.name.is_empty() {
        b.custom = Some(t.name.clone());
    }
    Ok(b)
}

/// Форма россыпи SAI 2 (`settings/scatter/<имя>.ini` рядом с `<имя>.bmp`): «Россыпь» с
/// настройками из `.ini` (`Ключ = значение`, `;` — пояснения). Картинку кладёт вызывающий.
pub fn scatter_brush(ini: &str) -> Option<Brush> {
    let mut v: BTreeMap<String, String> = BTreeMap::new();
    for line in ini.lines() {
        let line = line.trim();
        if line.starts_with(';') || line.starts_with('[') {
            continue;
        }
        if let Some((k, x)) = line.split_once('=') {
            v.insert(k.trim().to_string(), x.trim().to_string());
        }
    }
    // это не форма россыпи
    v.get("Scattering")?;
    let num = |k: &str| v.get(k).and_then(|x| x.trim_start_matches('+').parse::<f32>().ok());
    let pct = |k: &str, max: f32| num(k).map(|x| (x / 100.0).clamp(0.0, max));
    let on = |k: &str| num(k).map(|x| x != 0.0);
    let mut b = ToolKind::Scatter.new_brush();
    let sc = b.params.scatter.as_mut()?;
    if let Some(x) = num("AngleControl") {
        sc.angle_control = match x as i32 {
            0 => paint_core::scatter::AngleControl::None,
            2 => paint_core::scatter::AngleControl::Direction,
            _ => paint_core::scatter::AngleControl::Auto,
        };
    }
    if let Some(x) = num("Angle") {
        sc.angle = x.clamp(-180.0, 180.0).to_radians();
    }
    for (field, k, max) in [
        (&mut sc.angle_jitter, "AngleJitter", 1.0),
        (&mut sc.scaling, "Scaling", 2.0),
        (&mut sc.size_jitter, "SizeJitter", 1.0),
        (&mut sc.scattering, "Scattering", 2.0),
        (&mut sc.ratio_jitter, "WxHJitter", 1.0),
        (&mut sc.count_jitter, "CountJitter", 1.0),
        (&mut sc.fg_bg_jitter, "ColJitter", 1.0),
        (&mut sc.hue_jitter, "HueJitter", 1.0),
        (&mut sc.sat_jitter, "SaturationJitter", 1.0),
        (&mut sc.bri_jitter, "BrightnessJitter", 1.0),
    ] {
        if let Some(x) = pct(k, max) {
            *field = x;
        }
    }
    sc.scaling = sc.scaling.max(0.01);
    // «W:H» — «100:50»: шире, чем выше
    if let Some((w, h)) = v.get("WxHRatio").and_then(|r| r.split_once(':')) {
        if let (Ok(w), Ok(h)) = (w.trim().parse::<f32>(), h.trim().parse::<f32>()) {
            if w > 0.0 && h > 0.0 {
                sc.ratio = (w / h).clamp(0.01, 100.0);
            }
        }
    }
    if let Some(x) = num("Count") {
        sc.count = x.clamp(1.0, 20.0) as u8;
    }
    for (field, k) in [(&mut sc.all_directions, "AllDirScattering"), (&mut sc.gaussian, "GaussianDistribution"), (&mut sc.absolute_size, "AbsoluteSize"), (&mut sc.int_position, "IntegerPosition"), (&mut sc.each_shape, "ApplyToEachShape")] {
        if let Some(x) = on(k) {
            *field = x;
        }
    }
    let absolute = sc.absolute_size;
    if let Some(x) = pct("Spacing", 10.0) {
        b.params.spacing = x.max(0.01);
    }
    if absolute {
        if let Some(x) = num("ShapeSize") {
            b.params.size = x.clamp(1.0, 5000.0);
        }
    }
    Some(b)
}

/// Готовые группы SAI 2 2026 (вкладки «Ver.1» и «Artistic» по умолчанию): ячейка и настройки
/// инструмента в виде `.saitdat` через «;» (только то, что переносится). Формы, щетина и текстуры —
/// по именам из папки SAI 2; без неё инструменты рисуют без них.
pub const PRESET_GROUPS: [(&str, &[(usize, &str)]); 2] = [
    (
        "Ver.1",
        &[
            (0, "tidstr=S:pencil;name=U:Pencil;cursize=I:10;minsize=I:0;curdens=I:100;mindens=I:0;wavno=I:4;hardness=I:50;szprsf=I:1;dnprsf=I:0;v1dnprsf=I:1;densamp=I:100"),
            (1, "tidstr=S:airbrush;name=U:AirBrush;cursize=I:500;minsize=I:50;curdens=I:100;mindens=I:0;wavno=I:0;hardness=I:50;szprsf=I:1;dnprsf=I:1;v1dnprsf=I:1;densamp=I:100"),
            (2, "tidstr=S:brush;name=U:Brush;cursize=I:500;minsize=I:50;curdens=I:100;mindens=I:0;wavno=I:4;hardness=I:50;szprsf=I:1;dnprsf=I:1;v1dnprsf=I:1;densamp=I:100;blend=I:50;dilution=I:0;persist=I:80;blprsf=I:1;blurthres=I:0"),
            (3, "tidstr=S:water;name=U:Water;cursize=I:500;minsize=I:60;curdens=I:100;mindens=I:0;wavno=I:4;hardness=I:50;szprsf=I:1;dnprsf=I:1;v1dnprsf=I:1;densamp=I:100;blend=I:50;dilution=I:50;persist=I:80;blprsf=I:1;blurthres=I:50"),
            (4, "tidstr=S:marker;name=U:Marker;cursize=I:500;minsize=I:50;curdens=I:100;mindens=I:0;wavno=I:4;hardness=I:50;szprsf=I:1;dnprsf=I:1;v1dnprsf=I:1;densamp=I:100;blend=I:50;dilution=I:0;persist=I:80;blprsf=I:1;blurthres=I:0"),
            (5, "tidstr=S:eraser;name=U:Eraser;cursize=I:500;minsize=I:100;curdens=I:100;mindens=I:0;wavno=I:4;hardness=I:50;szprsf=I:1;dnprsf=I:0;v1dnprsf=I:1;densamp=I:100"),
            (6, "tidstr=S:selpen;name=U:SelPen;cursize=I:500;minsize=I:100;curdens=I:100;mindens=I:0;wavno=I:4;hardness=I:50;szprsf=I:1;dnprsf=I:0;v1dnprsf=I:1;densamp=I:100"),
            (7, "tidstr=S:selers;name=U:SelErs;cursize=I:500;minsize=I:100;curdens=I:100;mindens=I:0;wavno=I:4;hardness=I:50;szprsf=I:1;dnprsf=I:0;v1dnprsf=I:1;densamp=I:100"),
            (8, "tidstr=S:bucket;name=U:Bucket"),
            (9, "tidstr=S:dotpen;name=U:BinPen;cursize=I:10;minsize=I:0;szprsf=I:1"),
            (16, "tidstr=S:brush;name=U:Canvas;cursize=I:500;minsize=I:50;curdens=I:100;mindens=I:0;wavno=I:4;hardness=I:50;szprsf=I:1;dnprsf=I:1;v1dnprsf=I:1;densamp=I:100;blend=I:50;dilution=I:0;persist=I:74;blprsf=I:1;blurthres=I:0;fomcat=I:2;fomnam=U:Bristle;blotval=I:50;blotscl=I:100;brslsiz=I:60;brslmin=I:0;brslrnd=I:0;brsldir=I:1;texcat=I:2;texnam=U:Canvas;texval=I:80;texscl=I:100"),
            (17, "tidstr=S:brush;name=U:Paper;cursize=I:500;minsize=I:50;curdens=I:100;mindens=I:0;wavno=I:4;hardness=I:50;szprsf=I:1;dnprsf=I:1;v1dnprsf=I:1;densamp=I:100;blend=I:50;dilution=I:0;persist=I:74;blprsf=I:1;blurthres=I:0;fomcat=I:2;fomnam=U:Bristle;blotval=I:50;blotscl=I:100;brslsiz=I:60;brslmin=I:0;brslrnd=I:0;brsldir=I:1;texcat=I:2;texnam=U:Paper;texval=I:70;texscl=I:100"),
            (18, "tidstr=S:brush;name=U:Crayon;cursize=I:500;minsize=I:70;curdens=I:85;mindens=I:0;wavno=I:1;hardness=I:50;szprsf=I:1;dnprsf=I:1;v1dnprsf=I:1;densamp=I:100;blend=I:30;dilution=I:0;persist=I:0;blprsf=I:0;blurthres=I:0;fomcat=I:1;fomnam=U:Blots&Noise;blotval=I:50;blotscl=I:100;brslsiz=I:50;brslmin=I:0;brslrnd=I:0;brsldir=I:1;texcat=I:2;texnam=U:Paper;texval=I:100;texscl=I:100"),
            (19, "tidstr=S:water;name=U:Blur;cursize=I:500;minsize=I:60;curdens=I:100;mindens=I:0;wavno=I:4;hardness=I:50;szprsf=I:1;dnprsf=I:1;v1dnprsf=I:1;densamp=I:100;blend=I:100;dilution=I:100;persist=I:80;blprsf=I:1;blurthres=I:100"),
        ],
    ),
    (
        "Artistic",
        &[
            (0, "tidstr=S:pencil;name=U:Pencil;cursize=I:40;minsize=I:80;curdens=I:70;mindens=I:0;wavno=I:0;hardness=I:50;szprsf=I:1;dnprsf=I:1;v1dnprsf=I:0;densamp=I:0;fomcat=I:1;fomnam=U:Blots&Noise;blotval=I:70;blotscl=I:100;brslsiz=I:50;brslmin=I:0;brslrnd=I:0;brsldir=I:1;texcat=I:2;texnam=U:Paper;texval=I:50;texscl=I:50"),
            (4, "tidstr=S:water;name=U:Water;cursize=I:500;minsize=I:60;curdens=I:100;mindens=I:0;wavno=I:4;hardness=I:0;szprsf=I:1;dnprsf=I:1;v1dnprsf=I:1;densamp=I:100;blend=I:50;dilution=I:50;persist=I:80;blprsf=I:1;blurthres=I:50;fomcat=I:1;fomnam=U:Blots;blotval=I:40;blotscl=I:100;brslsiz=I:50;brslmin=I:0;brslrnd=I:0;brsldir=I:1"),
            (5, "tidstr=S:brush;name=U:Oil;cursize=I:500;minsize=I:50;curdens=I:100;mindens=I:0;wavno=I:4;hardness=I:50;szprsf=I:1;dnprsf=I:1;v1dnprsf=I:0;densamp=I:0;blend=I:50;dilution=I:25;persist=I:50;blprsf=I:1;blurthres=I:50;fomcat=I:2;fomnam=U:Bristle;blotval=I:50;blotscl=I:100;brslsiz=I:50;brslmin=I:0;brslrnd=I:20;brsldir=I:1"),
            (6, "tidstr=S:brush;name=U:Gouache;cursize=I:500;minsize=I:50;curdens=I:100;mindens=I:0;wavno=I:4;hardness=I:50;szprsf=I:1;dnprsf=I:1;v1dnprsf=I:0;densamp=I:0;blend=I:50;dilution=I:0;persist=I:50;blprsf=I:1;blurthres=I:50;fomcat=I:2;fomnam=U:Bristle;blotval=I:50;blotscl=I:100;brslsiz=I:50;brslmin=I:0;brslrnd=I:20;brsldir=I:1"),
            (7, "tidstr=S:brush;name=U:Chalk;cursize=I:500;minsize=I:50;curdens=I:100;mindens=I:0;wavno=I:4;hardness=I:50;szprsf=I:0;dnprsf=I:1;v1dnprsf=I:0;densamp=I:0;blend=I:30;dilution=I:0;persist=I:30;blprsf=I:1;blurthres=I:0;fomcat=I:2;fomnam=U:Flat Face;blotval=I:50;blotscl=I:100;brslsiz=I:50;brslmin=I:0;brslrnd=I:0;brsldir=I:1;texcat=I:2;texnam=U:Paper;texval=I:30;texscl=I:100"),
        ],
    ),
];

/// Инструменты готовой группы `PRESET_GROUPS[g]`: ячейка и инструмент.
pub fn preset_group(g: usize) -> Vec<(usize, Brush)> {
    PRESET_GROUPS.get(g).map_or(&[][..], |p| p.1).iter().filter_map(|(cell, text)| Some((*cell, to_brush(&parse(&text.replace(';', "\n"))?).ok()?))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_tool() {
        let text = "tidstr=S:brush\nxidstr=S:artoil\nvkc=I:0\nname=U:Oil\naddt=U:\n.\nwavno=I:4\ncursize=I:500\nminsize=I:50\ncurdens=I:100\nfomcat=I:2\nfomnam=U:Bristle\nbrslrnd=I:20\nblend=I:50\ndilution=I:25\npersist=I:50\nszprsf=I:1\ndnprsf=I:1\n.\n.\n--EOF--\n";
        let t = parse(text).unwrap();
        assert_eq!((t.tid.as_str(), t.xid.as_str(), t.name.as_str()), ("brush", "artoil", "Oil"));
        let b = to_brush(&t).unwrap();
        assert_eq!(b.kind, ToolKind::Brush);
        assert_eq!(b.params.size, 50.0);
        assert_eq!(b.params.hardness, 1.0);
        let m = b.params.mix.unwrap();
        assert_eq!((m.blending, m.dilution, m.persistence), (0.5, 0.25, 0.5));
        assert_eq!(b.grain.bristle.as_deref(), Some("Bristle"));
        assert!((b.grain.randomize - 0.2).abs() < 1e-6);
        assert_eq!(b.custom.as_deref(), Some("Oil"));
        assert!(to_brush(&Sai2Tool { tid: "inkpen".into(), ..Default::default() }).is_err());
        assert!(parse("что-то").is_none());
    }

    #[test]
    fn preset_groups_convert() {
        let v1 = preset_group(0);
        assert_eq!(v1.len(), 14);
        let art = preset_group(1);
        assert_eq!(art.len(), 5);
        let chalk = &art.iter().find(|(c, _)| *c == 7).unwrap().1;
        assert_eq!(chalk.custom.as_deref(), Some("Chalk"));
        assert_eq!(chalk.grain.bristle.as_deref(), Some("Flat Face"));
        assert!(!chalk.params.size_by_pressure);
    }

    #[test]
    fn reads_a_scatter_shape() {
        let ini = "; x\n[Parameters]\n\t; [Angle]\n\tAngle = +30\n\tAngleJitter = 12\n\tScaling = 50\n\tSpacing = 50\n\tScattering = 100\n\tWxHRatio = 100:50\n\tCount = 3\n\tAllDirScattering = 1\n\tHueJitter = 100\n";
        let b = scatter_brush(ini).unwrap();
        let sc = b.params.scatter.unwrap();
        assert!((sc.angle - 30f32.to_radians()).abs() < 1e-6);
        assert_eq!((sc.scaling, sc.scattering, sc.ratio, sc.count, sc.all_directions, sc.hue_jitter), (0.5, 1.0, 2.0, 3, true, 1.0));
        assert_eq!(b.params.spacing, 0.5);
        assert!(scatter_brush("[Parameters]\nDensity = 50\n").is_none());
    }

    #[test]
    fn reads_a_group() {
        let g = parse_group("vsbpos=I:0\nname=U:Artistic\nvkc=I:0\n.\n0=35\n4=36\n.\n--EOF--\n").unwrap();
        assert_eq!(g, ("Artistic".to_string(), vec![(0, 35), (4, 36)]));
    }

    /// Инструменты из папки `custool` SAI 2 (`SAI_CUSTOOL=…/custool/nrm`).
    #[test]
    #[ignore]
    fn reads_local_tools() {
        let Some(dir) = std::env::var_os("SAI_CUSTOOL") else { return };
        let mut files: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "saitdat")).collect();
        files.sort();
        for f in files {
            let t = parse(&std::fs::read_to_string(&f).unwrap()).unwrap();
            match to_brush(&t) {
                Ok(b) => println!("{} {} {:?} {:.1}px жёстк {:.2} плотн {:.2} смеш {:?} форма {:?} щетина {:?} текстура {}", f.file_name().unwrap().to_string_lossy(), t.name, b.kind, b.params.size, b.params.hardness, b.params.density, b.params.mix.map(|m| (m.blending, m.dilution, m.persistence)), b.grain.form, b.grain.bristle, matches!(b.grain.tex, TexChoice::File(_))),
                Err(e) => println!("{} нет вида {e}", f.display()),
            }
        }
    }
}
