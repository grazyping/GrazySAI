//! Штрих: цепочка отпечатков кисти вдоль траектории пера.
//!
//! Пока перо не поднято, штрих копит «покрытие» в отдельной маске. Отпечатки накладываются
//! друг на друга как полупрозрачные слои; непрозрачность каждого подобрана так, чтобы вдоль оси
//! мазка получалась заданная плотность (так устроено в образце). Сверх этой плотности маска не
//! растёт, поэтому повторный проход тем же штрихом краску не темнит. На слой штрих переносится
//! целиком — это же даёт одну запись отмены.

use crate::brush::BrushParams;
use crate::dab::{step_factor, Dab, DabInput};

/// Симметрия штриха: оси x = `x` (отражение слева направо) и/или y = `y` (сверху вниз).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mirror {
    pub x: Option<f32>,
    pub y: Option<f32>,
    /// Бесшовный режим: размер холста (ширина, высота). Отпечаток переносится внутрь холста, а у
    /// края повторяется с другой стороны.
    pub wrap: Option<(f32, f32)>,
}

impl Mirror {
    /// Точка и её отражения (сама точка — первой); в бесшовном режиме — ещё и копии со сдвигом на
    /// размер холста, которые достают до холста отпечатком радиуса `reach`.
    fn images(&self, p: InputPoint, reach: f32) -> Vec<InputPoint> {
        let mut out = self.reflections(p);
        if let Some((w, h)) = self.wrap.filter(|&(w, h)| w > 0.0 && h > 0.0) {
            let near = |v: f32, size: f32| v > -reach && v < size + reach;
            out = out
                .into_iter()
                .flat_map(|q| {
                    let base = InputPoint { x: q.x.rem_euclid(w), y: q.y.rem_euclid(h), ..q };
                    let shifts = [0.0, -1.0, 1.0];
                    shifts.into_iter().flat_map(move |kx| shifts.into_iter().map(move |ky| (kx, ky))).filter_map(move |(kx, ky)| {
                        let c = InputPoint { x: base.x + kx * w, y: base.y + ky * h, ..base };
                        (near(c.x, w) && near(c.y, h)).then_some(c)
                    })
                })
                .collect();
        }
        out
    }

    fn reflections(&self, p: InputPoint) -> Vec<InputPoint> {
        let mut out = vec![p];
        if let Some(ax) = self.x {
            out.push(InputPoint { x: 2.0 * ax - p.x, ..p });
        }
        if let Some(ay) = self.y {
            let n = out.len();
            for i in 0..n {
                out.push(InputPoint { y: 2.0 * ay - out[i].y, ..out[i] });
            }
        }
        out
    }
}

/// Отпечаток по модели образца.
#[derive(Clone)]
enum Shape {
    Sai(Dab, f32),
    /// Двоичное перо: круг радиуса `r` без сглаживания (точка закрашена, если её центр в круге),
    /// непрозрачность `o`.
    Binary(f32, f32),
}

impl Shape {
    fn opacity(&self) -> f32 {
        match self {
            Shape::Sai(_, o) | Shape::Binary(_, o) => *o,
        }
    }
    fn outer(&self) -> f32 {
        match self {
            Shape::Sai(d, _) => d.outer(),
            Shape::Binary(r, _) => *r,
        }
    }
    /// Форма для маски штриха у самой жёсткой ступени от 20 px.
    fn mask(&self) -> Option<(f32, f32, f32)> {
        match self {
            Shape::Sai(d, _) => d.mask,
            Shape::Binary(..) => None,
        }
    }
    fn coverage(&self, dist: f32) -> f32 {
        match self {
            Shape::Sai(d, _) => d.coverage(dist),
            Shape::Binary(r, _) => (dist <= *r) as u8 as f32,
        }
    }
    /// Квадрат радиуса, внутри которого покрытие постоянно (`coverage(0)`): там не нужны ни
    /// корень, ни степень — у большой кисти это почти весь отпечаток.
    fn flat_r2(&self) -> f32 {
        let r = match self {
            Shape::Sai(d, _) => d.inner_radius(),
            Shape::Binary(r, _) => *r,
        };
        // чуть меньше, чтобы округление квадрата не задело край
        let r = (r - 1e-3).max(0.0);
        r * r
    }
}
use crate::mix::{Mixer, Sample};
use crate::paper::PaperTexture;
use std::sync::Arc;
use crate::pixel::{Px, ONE};
use crate::tile::{key_of, ColorMap, Key, MaskMap, Tile, TILE};
use std::collections::{HashMap, HashSet};

/// Форма «пятно» и текстура кисти, как в SAI (docs/29-brush-form-texture.md). Обе текстуры
/// привязаны к холсту, а не к отпечатку.
#[derive(Clone, Debug)]
pub struct Grain {
    /// Форма «пятно» (`blotmap`): текстура, сила 0..=1, масштаб %.
    pub blot: Option<(Arc<PaperTexture>, f32, u16)>,
    /// Текстура кисти (`brushtex` или бумага слоя): текстура, сила 0..=1, масштаб %.
    pub texture: Option<(Arc<PaperTexture>, f32, u16)>,
    /// Текстура — бумага слоя: значения берутся обратными (как в SAI).
    pub paper: bool,
    /// «Affect by MinDens»: сила текстуры зависит от размера и минимальной плотности.
    pub by_min_density: bool,
    /// Форма «щетина» (`bristle`); вместо «пятна».
    pub bristle: Option<Bristle>,
}

/// Щетина, как форма вида 3 в SAI 2017: каждый
/// волосок — свой ряд отпечатков. Волосок смещён от точки штриха на (−y, x)·радиус кисти,
/// повёрнутое по направлению штриха (сглаживание — среднее по 16 последним поворотам, как в
/// SAI); диаметр волоска — 0,4·Hair Size·размер (при 50 — 0,2 размера, так ширина
/// полосы совпала с живой SAI: 47 px при размере 50).
#[derive(Clone, Debug)]
pub struct Bristle {
    pub hairs: Arc<Vec<(f32, f32)>>,
    /// Hair Size, 0..=1 (в SAI 0–100, по умолчанию 50).
    pub hair_size: f32,
    /// Hair Min — доля размера волоска при нулевом нажиме.
    pub hair_min: f32,
    /// Randomize, 0..=1 — разброс волосков в начале штриха.
    pub randomize: f32,
    /// Direction: 0 — None, 1 — Auto (по ходу штриха), 2 — Pen Direction (наклона пера у нас нет — как None).
    pub direction: u8,
}

/// Разлёт волосков относительно радиуса кисти.
const HAIR_SPREAD: f32 = 1.13;

/// Диаметр волоска: 0,2·Hair Size·размер (0,1 размера при 50 — как у живой SAI), с Hair Min по нажиму.
fn hair_diameter(b: &Bristle, diameter: f32, pressure: f32) -> f32 {
    (diameter * 0.2 * b.hair_size).max(diameter * 0.002).max(0.3) * (b.hair_min + (1.0 - b.hair_min) * pressure.clamp(0.0, 1.0))
}

/// Волоски этого штриха: смещения с разбросом и сглаженное направление.
struct Hairs {
    offsets: Vec<(f32, f32)>,
    angle: Option<f32>,
    turns: std::collections::VecDeque<f32>,
    prev: Option<(f32, f32)>,
}

/// Коэффициенты порога «пятна» по ступени жёсткости 0..=4 (таблицы образца).
const BLOT_B: [f64; 5] = [1.263157894736842, 2.0, 2.5714285714285716, 4.0, 6.0];
const BLOT_C: [f64; 5] = [0.15789473684210525, 0.2727272727272727, 0.42857142857142855, 0.5, 0.5];
const BLOT_D: [f64; 5] = [0.6666666666666666, 0.7272727272727273, 0.8888888888888888, 1.0, 1.3333333333333333];

/// Номер ступени жёсткости (0 — самая мягкая).
fn hardness_index(h: f32) -> usize {
    let mut best = 0;
    for (i, v) in crate::brush::HARDNESS_STEPS.iter().enumerate() {
        if (h - v).abs() < (h - crate::brush::HARDNESS_STEPS[best]).abs() {
            best = i;
        }
    }
    best
}

/// Значения текстуры на плитках, которые задевает отпечаток.
struct GrainTiles {
    tx0: i32,
    ty0: i32,
    nx: i32,
    tiles: Vec<Arc<[u8]>>,
}

impl GrainTiles {
    fn at(&self, x: i32, y: i32) -> u32 {
        let (tx, ty) = (x.div_euclid(TILE) - self.tx0, y.div_euclid(TILE) - self.ty0);
        let t = &self.tiles[(ty * self.nx + tx) as usize];
        t[(y.rem_euclid(TILE) * TILE + x.rem_euclid(TILE)) as usize] as u32
    }
}

/// Форма и текстура одного отпечатка.
struct DabGrain {
    /// Пятно: текстура и коэффициенты `покрытие = край·B - t·A - C`.
    blot: Option<(GrainTiles, [f32; 3])>,
    /// Текстура: значения и сила в 1/256 (как `dab_make_node`: `(int)(сила·256)`).
    tex: Option<(GrainTiles, u32)>,
}

impl DabGrain {
    /// Покрытие точки (x, y) по покрытию края `cov` (у «пятна» — до порога).
    fn apply(&self, cov: f32, x: i32, y: i32) -> f32 {
        let mut cov = cov;
        if let Some((t, [a, b, c])) = &self.blot {
            cov = (cov * b - t.at(x, y) as f32 * a - c).clamp(0.0, 1.0);
        }
        if let Some((t, s)) = &self.tex {
            // 0x4000 - ((t·257/2 + 1)/2), умножить на силу, вычесть с насыщением
            let g = t.at(x, y);
            let v = 0x4000 - ((((g * 257) >> 1) + 1) >> 1);
            let sub = ((v * s) >> 8).min(0xffff);
            let q = ((cov * 16384.0) as u32).saturating_sub(sub);
            cov = q as f32 * (1.0 / 16384.0);
        }
        cov
    }
}

/// Отпечаток обычной кисти, ждущий наложения в покрытие штриха (`flush_dabs`).
struct Placed {
    p: InputPoint,
    shape: Shape,
    opacity: f32,
    cap: u32,
    reach: f32,
    x0: i32,
    x1: i32,
    y0: i32,
    y1: i32,
    round: bool,
    flat_cov: f32,
    flat_r2: f32,
    flat_a: u32,
    grain: Option<DabGrain>,
}

impl Placed {
    fn new(p: InputPoint, shape: Shape, density: f32, grain: Option<DabGrain>) -> Self {
        let opacity = shape.opacity();
        let cap = (density * ONE as f32 + 0.5) as u32;
        let reach = shape.outer() + 1.0;
        // За пределами круга `reach` покрытие нулевое — строки обходятся только внутри круга.
        let round = shape.coverage(reach - 1.0) <= 0.0;
        let flat_cov = shape.coverage(0.0);
        // с формой или текстурой покрытие меняется от точки к точке
        let flat_r2 = match (&grain, shape.mask()) {
            (Some(_), _) => -1.0,
            // внутри жёсткой формы маска полная — там покрытие постоянно
            (None, Some((inner, _, _))) => {
                let r = (inner - 1e-3).max(0.0).min(shape.flat_r2().sqrt());
                r * r
            }
            (None, None) => shape.flat_r2(),
        };
        Self {
            grain,
            p,
            opacity,
            cap,
            reach,
            x0: (p.x - reach).floor() as i32,
            x1: (p.x + reach).ceil() as i32,
            y0: (p.y - reach).floor() as i32,
            y1: (p.y + reach).ceil() as i32,
            round,
            flat_cov,
            flat_r2,
            flat_a: (flat_cov * opacity * ONE as f32 + 0.5) as u32,
            shape,
        }
    }

    /// Накладывает отпечаток на одну плитку покрытия (создаёт её при первом попадании).
    /// Возвращает, было ли попадание.
    fn paint_tile(&self, key: Key, tile: &mut Option<Tile<u16>>, mask: Option<&Tile<u16>>) -> bool {
        let masked = self.shape.mask().is_some();
        let p = self.p;
        let (tx, ty) = key;
        let (ya, yb) = ((ty * TILE).max(self.y0), (ty * TILE + TILE - 1).min(self.y1));
        let (xa, xb) = ((tx * TILE).max(self.x0), (tx * TILE + TILE - 1).min(self.x1));
        let mut hit = false;
        for y in ya..=yb {
            let cy = y as f32 + 0.5 - p.y;
            let (mut ra, mut rb) = (xa, xb);
            if self.round {
                let w2 = self.reach * self.reach - cy * cy;
                if w2 < 0.0 {
                    continue;
                }
                let w = w2.sqrt() + 1.0;
                ra = ra.max((p.x - 0.5 - w).floor() as i32);
                rb = rb.min((p.x - 0.5 + w).ceil() as i32);
            }
            let row = (y - ty * TILE) * TILE - tx * TILE;
            for x in ra..=rb {
                let i = (row + x) as usize;
                let old = tile.as_ref().map_or(0, |t| t.0[i] as u32);
                if old >= self.cap {
                    continue; // уже набрано до плотности этого отпечатка — не изменится
                }
                // Центр пикселя — в (x + 0.5, y + 0.5).
                let cx = x as f32 + 0.5 - p.x;
                let d2 = cx * cx + cy * cy;
                let a = if d2 < self.flat_r2 {
                    if self.flat_cov <= 0.0 {
                        continue;
                    }
                    self.flat_a
                } else {
                    let mut cov = self.shape.coverage(d2.sqrt());
                    if masked {
                        // не больше маски штриха (в 1/16384)
                        let m = mask.map_or(0, |t| t.0[(row + x) as usize]) as f32 / 16384.0;
                        cov = cov.min(m);
                    }
                    if let Some(g) = &self.grain {
                        cov = g.apply(cov, x, y);
                    }
                    if cov <= 0.0 {
                        continue;
                    }
                    (cov * self.opacity * ONE as f32 + 0.5) as u32
                };
                hit = true;
                let t = tile.get_or_insert_with(|| Tile::filled(0));
                // Наложение «поверх», но не выше плотности этого отпечатка и не ниже уже набранного.
                let sum = old + ((ONE - old) * a + ONE / 2) / ONE;
                t.0[i] = sum.min(self.cap).max(old) as u16;
            }
        }
        hit
    }
}

/// Отпечаток смешивающей кисти, ждущий своей пачки.
struct Pending {
    x: f32,
    y: f32,
    pressure: f32,
    shape: Shape,
    grain: Option<DabGrain>,
}


#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InputPoint {
    pub x: f32,
    pub y: f32,
    /// Давление пера 0..=1.
    pub pressure: f32,
}

pub struct Stroke {
    pub params: BrushParams,
    pub mask: MaskMap,
    /// Симметрия: отпечатки повторяются отражёнными (по умолчанию выключена).
    pub mirror: Mirror,
    last: Option<InputPoint>,
    /// Двоичное перо: последняя поставленная точка (в координатах «−0,5»), как поле +0x1c0 SAI.
    bin_prev: Option<(f64, f64)>,
    /// Щетина: волоски штриха.
    hairs: Option<Hairs>,
    /// Путь, пройденный после последнего отпечатка.
    carry: f32,
    pub dab_count: usize,
    /// Плитки, изменённые с прошлого вызова [`Stroke::take_touched`].
    touched: HashSet<Key>,
    /// Смешивающая кисть: отпечатки, ещё не нарисованные, и краска на кисти.
    pending: Vec<Pending>,
    /// Отпечатки обычной кисти, ещё не наложенные в покрытие.
    queued: Vec<Placed>,
    mixer: Option<Mixer>,
    /// Форма «пятно» и текстура кисти.
    grain: Option<Grain>,
    /// Значения текстур по плиткам холста: пятно и текстура.
    grain_cache: [HashMap<Key, Arc<[u8]>>; 2],
    /// Акварель: уменьшенная копия слоя по плиткам ((уровень, плитка) → её точки уровня), пока
    /// плитка не изменилась. Уровень — в ключе: размер кисти меняется с нажимом, а с ним и уровень.
    level_cache: HashMap<(u32, Key), Box<[[f64; 4]]>>,
    /// Самая жёсткая ступень от 20 px: максимум «жёстких» форм отпечатков (0..=16384).
    hard_mask: MaskMap,
}

impl Stroke {
    pub fn new(params: BrushParams) -> Self {
        Self {
            params,
            mask: MaskMap::new(),
            last: None,
            bin_prev: None,
            hairs: None,
            carry: 0.0,
            dab_count: 0,
            touched: HashSet::new(),
            pending: Vec::new(),
            queued: Vec::new(),
            mixer: None,
            grain: None,
            grain_cache: Default::default(),
            level_cache: HashMap::new(),
            hard_mask: MaskMap::new(),
            mirror: Mirror::default(),
        }
    }

    /// Форма «пятно» и текстура кисти для этого штриха.
    pub fn set_grain(&mut self, grain: Option<Grain>) {
        self.grain = grain.filter(|g| g.blot.is_some() || g.texture.is_some() || g.bristle.is_some());
        self.hairs = None;
        self.grain_cache = Default::default();
    }

    fn blot_on(&self) -> bool {
        self.grain.as_ref().is_some_and(|g| g.blot.is_some())
    }

    /// Плитки текстуры `which` (0 — пятно, 1 — текстура) под прямоугольником отпечатка.
    fn grain_tiles(&mut self, which: usize, x0: i32, y0: i32, x1: i32, y1: i32) -> Option<GrainTiles> {
        let g = self.grain.as_ref()?;
        let (tex, _, scale) = if which == 0 { g.blot.as_ref()? } else { g.texture.as_ref()? };
        let invert = which == 1 && g.paper;
        let (tx0, ty0, tx1, ty1) = (x0.div_euclid(TILE), y0.div_euclid(TILE), x1.div_euclid(TILE), y1.div_euclid(TILE));
        let mut tiles = Vec::new();
        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                let t = self.grain_cache[which].entry((tx, ty)).or_insert_with(|| {
                    let mut v = crate::paper::sample(tex, *scale, tx * TILE, ty * TILE, TILE as usize, TILE as usize);
                    if invert {
                        // бумага слоя: каждые 4 байта как одно 32-битное число с обратным знаком
                        for c in v.as_chunks_mut::<4>().0 {
                            *c = u32::from_le_bytes(*c).wrapping_neg().to_le_bytes();
                        }
                    }
                    v.into()
                });
                tiles.push(t.clone());
            }
        }
        Some(GrainTiles { tx0, ty0, nx: tx1 - tx0 + 1, tiles })
    }

    /// Форма и текстура отпечатка в точке `p` диаметром `diameter`.
    fn dab_grain(&mut self, p: InputPoint, diameter: f32, reach: f32) -> Option<DabGrain> {
        let g = self.grain.clone()?;
        let (x0, x1) = ((p.x - reach).floor() as i32, (p.x + reach).ceil() as i32);
        let (y0, y1) = ((p.y - reach).floor() as i32, (p.y + reach).ceil() as i32);
        let blot = match &g.blot {
            Some((_, strength, _)) => {
                let h = hardness_index(self.params.hardness);
                let (b, c) = (BLOT_B[h], BLOT_C[h]);
                let a = b * BLOT_D[h] * *strength as f64 / 255.0;
                Some((self.grain_tiles(0, x0, y0, x1, y1)?, [a as f32, b as f32, c as f32]))
            }
            None => None,
        };
        let tex = match &g.texture {
            Some((_, strength, _)) => {
                // как в SAI
                let mut s = *strength as f64;
                if g.by_min_density {
                    let p = if self.params.density_by_pressure { p.pressure.clamp(0.0, 1.0) } else { 1.0 };
                    let q = 1.0 - p as f64;
                    s *= (self.params.min_density as f64 * 32.0 * q * q + 1.0) * (diameter as f64).clamp(1.0, 5.0) * 0.2;
                }
                let s = ((s.min(255.0) * 256.0) as i64 as u32) & 0xffff;
                Some((self.grain_tiles(1, x0, y0, x1, y1)?, s))
            }
            None => None,
        };
        Some(DabGrain { blot, tex })
    }

    /// Краска смешивается с холстом: штрих рисуется прямо в слой ([`Stroke::paint_mixed`]).
    pub fn is_mixing(&self) -> bool {
        self.params.mix.is_some() && !self.params.erase
    }

    /// Последняя принятая точка траектории.
    pub fn last_point(&self) -> Option<InputPoint> {
        self.last
    }

    /// Отдаёт и забывает список плиток, затронутых новыми отпечатками.
    pub fn take_touched(&mut self) -> HashSet<Key> {
        std::mem::take(&mut self.touched)
    }

    /// Отпечаток по модели образца (docs/13-thin-brushes.md).
    fn sai_dab(&self, pressure: f32) -> Dab {
        self.sai_dab_sized(pressure, self.params.diameter_at(pressure))
    }

    fn sai_dab_sized(&self, pressure: f32, diameter: f32) -> Dab {
        self.sai_dab_with(pressure, diameter, None)
    }

    /// Отпечаток диаметра `diameter`; `softness` — своя мягкость вместо мягкости кисти.
    fn sai_dab_with(&self, pressure: f32, diameter: f32, softness: Option<f32>) -> Dab {
        let density = self.params.density;
        let factor = if density > 0.0 { self.params.density_at(pressure) / density } else { 0.0 };
        let softness = softness.unwrap_or(1.0 - self.params.hardness.clamp(0.0, 1.0));
        Dab::new(DabInput {
            diameter,
            softness,
            density,
            density_factor: factor,
            density_scale: 1.0 - self.params.sharpness.clamp(0.0, 1.0),
            flat: !self.params.linework && softness <= 0.0,
            ver1: self.params.ver1_density,
            amplify: self.params.amplify,
            amplify_by_pressure: self.params.mix.is_some_and(|m| m.blur),
            blot: self.blot_on(),
            taper: match self.params.mix {
                Some(m) if m.blur && m.blend_by_pressure && m.blur_pressure > 0.0 => (1.0 - pressure / m.blur_pressure).max(0.0),
                _ => 0.0,
            },
        })
    }

    /// Шаг, по которому считается непрозрачность отпечатка.
    fn step_at(&self, pressure: f32) -> f32 {
        let d = self.params.diameter_at(pressure);
        if self.params.binary {
            // точки двоичного пера ставятся на пересечения с сеткой — путь проходится мелко
            return 0.25;
        }
        if let Some(b) = self.grain.as_ref().and_then(|g| g.bristle.as_ref()) {
            // щетина: шаг по толщине волоска
            return (hair_diameter(b, d, pressure) * 0.25).max(0.25);
        }
        if self.params.spacing > 0.0 {
            return (self.params.spacing * d.max(1.0)).max(0.05);
        }
        let dab = self.sai_dab(pressure);
        (step_factor(dab.step_diameter, 1.0 - self.params.hardness.clamp(0.0, 1.0)) * dab.step_size).max(0.05)
    }

    pub fn add_point(&mut self, p: InputPoint) {
        let Some(prev) = self.last else {
            self.dab(p);
            self.flush_dabs();
            self.last = Some(p);
            return;
        };
        let (dx, dy) = (p.x - prev.x, p.y - prev.y);
        let len = (dx * dx + dy * dy).sqrt();
        if len <= 0.0 {
            // перо стоит на месте: отпечатков нет, но нажим новой точки запоминается
            self.last = Some(p);
            return;
        }
        // Двоичное перо, как SAI 2017 (сверено вживую): путь проходится до точки на 1 px раньше
        // текущей, остаток — вместе со следующим отрезком; так последний пиксель штриха (у точки
        // отрыва) не ставится, как у SAI.
        let end = if self.params.binary { len - 1.0 } else { len };
        if end <= 0.0 {
            return;
        }
        let mut pos = 0.0;
        let mut pressure = prev.pressure;
        loop {
            // шаг мог уменьшиться (нажим упал) — тогда следующий отпечаток сразу
            let need = (self.step_at(pressure) - self.carry).max(0.0);
            if pos + need > end {
                self.carry += end - pos;
                break;
            }
            pos += need;
            let t = pos / len;
            pressure = prev.pressure + (p.pressure - prev.pressure) * t;
            self.dab(InputPoint { x: prev.x + dx * t, y: prev.y + dy * t, pressure });
            self.carry = 0.0;
        }
        self.flush_dabs();
        self.last = Some(if end < len {
            let t = end / len;
            InputPoint { x: prev.x + dx * t, y: prev.y + dy * t, pressure: prev.pressure + (p.pressure - prev.pressure) * t }
        } else {
            p
        });
    }

    /// Двоичное перо SAI 2017: от прошлой точки к текущей берутся пересечения
    /// с ближайшими линиями сетки x = round(x) и y = round(y); ставится то, что ближе к текущей
    /// точке (если пересечение одно — только не дальше 1 px). Отпечаток — в целом пикселе:
    /// размер округляется, у нечётного центр в середине пикселя.
    fn binary_point(&mut self, x: f64, y: f64, diameter: f32) -> Option<(f32, f32, f32)> {
        // cvtsd2si: к ближайшему, половины — к чётному
        let rnd = |v: f64| v.round_ties_even();
        let (x, y) = (x - 0.5, y - 0.5);
        let at = match self.bin_prev {
            None => (x, y),
            Some((px, py)) => {
                let (dx, dy) = (x - px, y - py);
                let a = (dx != 0.0).then(|| (rnd(x), dy / dx * (rnd(x) - px) + py));
                let b = (dy != 0.0).then(|| (dx / dy * (rnd(y) - py) + px, rnd(y)));
                let d = |q: (f64, f64)| (q.0 - x).hypot(q.1 - y);
                match (a, b) {
                    (Some(a), Some(b)) => {
                        if d(b) < d(a) {
                            b
                        } else {
                            a
                        }
                    }
                    (Some(q), None) | (None, Some(q)) if d(q) <= 1.000001 => q,
                    _ => return None,
                }
            }
        };
        self.bin_prev = Some(at);
        let size = (diameter as f64).round_ties_even().max(1.0);
        let half = if size as i64 % 2 == 1 { 0.5 } else { 0.0 };
        Some(((rnd(at.0) + half) as f32, (rnd(at.1) + half) as f32, (size / 2.0) as f32))
    }

    /// Отпечатки волосков щетины вместо одного круглого.
    fn bristle_dab(&mut self, p: InputPoint, b: &Bristle, diameter: f32, density: f32) {
        let hairs = self.hairs.get_or_insert_with(|| {
            // разброс — свой генератор (у SAI — rand(), повторить точно нельзя)
            let mut seed = 0x2545_f491u32 ^ (p.x.to_bits() ^ p.y.to_bits().rotate_left(7));
            let mut rnd = move || {
                seed = seed.wrapping_mul(214_013).wrapping_add(2_531_011);
                ((seed >> 16) & 0x7fff) as f32 / 32767.0 - 0.5
            };
            let k = b.randomize;
            let offsets = b
                .hairs
                .iter()
                .map(|&(x, y)| {
                    let (u, v) = (-y, x);
                    let ju = if u < 1.0 { rnd() * (1.0 - u).max(0.0).sqrt() } else { 0.0 };
                    let jv = if v < 1.0 { rnd() * (1.0 - v).max(0.0).sqrt() } else { 0.0 };
                    (u + ju * k, v + jv * k)
                })
                .collect();
            Hairs { offsets, angle: None, turns: Default::default(), prev: None }
        });
        if b.direction == 1 {
            let prev = hairs.prev.replace((p.x, p.y));
            let Some((px, py)) = prev else { return };
            let (dx, dy) = (p.x - px, p.y - py);
            if dx.hypot(dy) > 1e-3 {
                let raw = dy.atan2(dx);
                match hairs.angle {
                    None => hairs.angle = Some(raw),
                    Some(a) => {
                        let mut d = raw - a;
                        while d > std::f32::consts::PI {
                            d -= std::f32::consts::TAU;
                        }
                        while d < -std::f32::consts::PI {
                            d += std::f32::consts::TAU;
                        }
                        let n = hairs.turns.len().min(15);
                        let avg = (d + hairs.turns.iter().rev().take(n).sum::<f32>()) / (n + 1) as f32;
                        hairs.turns.push_back(avg);
                        if hairs.turns.len() > 16 {
                            hairs.turns.pop_front();
                        }
                        hairs.angle = Some(a + avg);
                    }
                }
            }
            if hairs.angle.is_none() {
                return;
            }
        }
        let angle = if b.direction == 1 { hairs.angle.unwrap_or(0.0) } else { 0.0 };
        let (sn, cs) = angle.sin_cos();
        // разлёт волосков: по ширине полосы живой SAI (47 px у размера 50 с Bristle.bmp)
        let radius = diameter / 2.0 * HAIR_SPREAD;
        let hair_d = hair_diameter(b, diameter, p.pressure);
        let offsets = hairs.offsets.clone();
        // волоски у SAI рисуются своим жёстким отпечатком, мягкость кисти на них не влияет
        let hard = |s: &Self| s.sai_dab_with(p.pressure, hair_d, Some(0.0));
        let o = hard(self).opacity(self.step_at(p.pressure));
        for (u, v) in offsets {
            let (ox, oy) = (u * radius, v * radius);
            let hp = InputPoint { x: p.x + ox * cs - oy * sn, y: p.y + ox * sn + oy * cs, ..p };
            let shape = Shape::Sai(hard(self), o);
            let with_grain = self.grain.as_ref().is_some_and(|g| g.texture.is_some());
            self.emit(hp, shape, density, hair_d, with_grain);
        }
    }

    /// Отпечаток — в маску штриха и в очередь; при симметрии — и его отражения (как один штрих,
    /// пересекающий сам себя: у оси покрытие не удваивается).
    fn emit(&mut self, p: InputPoint, shape: Shape, density: f32, diameter: f32, with_grain: bool) {
        for q in self.mirror.images(p, shape.outer() + 1.0) {
            add_to_hard_mask(&mut self.hard_mask, q.x, q.y, &shape);
            self.dab_count += 1;
            let grain = if with_grain { self.dab_grain(q, diameter, shape.outer() + 1.0) } else { None };
            if self.is_mixing() {
                self.pending.push(Pending { x: q.x, y: q.y, pressure: q.pressure, shape: shape.clone(), grain });
            } else {
                self.queued.push(Placed::new(q, shape.clone(), density, grain));
            }
        }
    }

    fn dab(&mut self, p: InputPoint) {
        let diameter = self.params.diameter_at(p.pressure);
        let density = self.params.density_at(p.pressure);
        if density <= 0.0 {
            return;
        }
        if !self.params.binary {
            if let Some(b) = self.grain.as_ref().and_then(|g| g.bristle.clone()) {
                return self.bristle_dab(p, &b, diameter, density);
            }
        }
        let (p, shape) = if self.params.binary {
            let Some((cx, cy, r)) = self.binary_point(p.x as f64, p.y as f64, diameter) else { return };
            (InputPoint { x: cx, y: cy, ..p }, Shape::Binary(r, 1.0))
        } else {
            let d = self.sai_dab(p.pressure);
            let o = d.opacity(self.step_at(p.pressure));
            (p, Shape::Sai(d, o))
        };
        // маска штриха — сразу: у образца её строят по окну отпечатков вокруг пачки, включая
        // следующие, так что все известные отпечатки уже в ней
        let with_grain = self.grain.is_some();
        self.emit(p, shape, density, diameter, with_grain);
    }

    /// Кладёт отложенные отпечатки в покрытие штриха. Каждая плитка получает свои отпечатки в
    /// прежнем порядке, поэтому результат тот же, что при поштучном наложении; плитки
    /// независимы и считаются параллельно, если работы много.
    fn flush_dabs(&mut self) {
        let dabs = std::mem::take(&mut self.queued);
        if dabs.is_empty() {
            return;
        }

        // плитка → номера отпечатков, задевающих её, по порядку
        let mut per_tile: HashMap<Key, Vec<usize>> = HashMap::new();
        let mut work = 0usize;
        for (n, d) in dabs.iter().enumerate() {
            for ty in d.y0.div_euclid(TILE)..=d.y1.div_euclid(TILE) {
                for tx in d.x0.div_euclid(TILE)..=d.x1.div_euclid(TILE) {
                    per_tile.entry((tx, ty)).or_default().push(n);
                }
            }
            work += ((d.x1 - d.x0 + 1) * (d.y1 - d.y0 + 1)) as usize;
        }
        let mut jobs: Vec<(Key, Option<Tile<u16>>, Vec<usize>)> =
            per_tile.into_iter().map(|(k, list)| (k, self.mask.tiles.remove(&k), list)).collect();
        let hard_mask = &self.hard_mask;
        let run = |job: &mut (Key, Option<Tile<u16>>, Vec<usize>)| -> bool {
            let (key, tile, list) = job;
            let mut hit = false;
            for &n in list.iter() {
                hit |= dabs[n].paint_tile(*key, tile, hard_mask.tiles.get(key));
            }
            hit
        };
        let threads = crate::threads();
        let hits: Vec<bool> = if work < 200_000 || threads < 2 || jobs.len() < 2 {
            jobs.iter_mut().map(run).collect()
        } else {
            let chunk = jobs.len().div_ceil(threads);
            std::thread::scope(|scope| {
                let handles: Vec<_> = jobs.chunks_mut(chunk).map(|c| scope.spawn(move || c.iter_mut().map(run).collect::<Vec<bool>>())).collect();
                handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
            })
        };
        for ((key, tile, _), hit) in jobs.into_iter().zip(hits) {
            if let Some(t) = tile {
                self.mask.tiles.insert(key, t);
            }
            if hit {
                self.touched.insert(key);
            }
        }
    }

    /// Рисует накопленные отпечатки смешивающей кисти прямо в пиксели слоя `pixels`, пачками
    /// (`mix.rs`). Пока перо не поднято (`finish == false`), неполная пачка ждёт. Перед первой
    /// правкой плитки её прежний вид кладётся в `originals` (для отмены). Возвращает плитки,
    /// которые изменились.
    pub fn paint_mixed(
        &mut self,
        pixels: &mut ColorMap,
        rgb: [u8; 3],
        preserve_opacity: bool,
        sel: &crate::selection::Selection,
        originals: &mut HashMap<Key, Option<Tile<Px>>>,
        finish: bool,
    ) -> Vec<Key> {
        let Some(mix) = self.params.mix else { return Vec::new() };
        let softness = 1.0 - self.params.hardness.clamp(0.0, 1.0);
        let mixer = self.mixer.get_or_insert_with(|| Mixer::new(mix, rgb));
        let mut changed = HashSet::new();
        loop {
            let want = crate::mix::batch_dabs(softness).min(mixer.batch + 2);
            if self.pending.is_empty() || (!finish && self.pending.len() < want) {
                break;
            }
            let n = mixer.next_batch(softness).min(self.pending.len());
            let batch: Vec<Pending> = self.pending.drain(..n).collect();
            let mask = &self.hard_mask;
            // Средний цвет холста под пачкой — по всем пикселям, которых она касается.
            // Полосы строк — параллельно; сумма целая (точка — доли 2^-15), поэтому порядок
            // сложения не важен и итог тот же, что при обходе по очереди.
            let tiles = &*pixels;
            let (_, parts) = Bands::run(&batch, false, |ya, yb, part: &mut [bool]| -> ([u64; 4], u32) {
                let gw = part.len() / (yb - ya).max(1) as usize;
                let gx0 = batch_bounds(&batch).0;
                let (mut sum, mut count) = ([0u64; 4], 0u32);
                for d in &batch {
                    for_rows(d, mask, ya..yb, i32::MIN..i32::MAX, |y, ra, rb, covs| {
                        let row = (y - ya) as usize * gw;
                        let mut cached: (Key, Option<&Tile<Px>>) = ((i32::MIN, 0), None);
                        for x in ra..=rb {
                            if covs[(x - ra) as usize] <= 0.0 {
                                continue;
                            }
                            let first = &mut part[row + (x - gx0) as usize];
                            if *first {
                                continue;
                            }
                            *first = true;
                            let key = key_of(x, y);
                            if cached.0 != key {
                                cached = (key, tiles.tiles.get(&key));
                            }
                            let px = cached.1.map_or([0; 4], |t| t.0[(y.rem_euclid(TILE) * TILE + x.rem_euclid(TILE)) as usize]);
                            for c in 0..4 {
                                sum[c] += px[c] as u64;
                            }
                            count += 1;
                        }
                    });
                }
                (sum, count)
            });
            let k = 1.0 / ONE as f64;
            let mut sample = Sample::default();
            for (sum, count) in parts {
                for (acc, v) in sample.sum.iter_mut().zip(sum) {
                    *acc += v as f64 * k;
                }
                sample.count += count;
            }
            let paint = mixer.paint(sample, n, batch[0].pressure);
            if mix.blur {
                let size = self.params.diameter_at(batch[0].pressure);
                water_blur(pixels, &batch, mask, size, sel, originals, &mut changed, &mut self.level_cache);
            }
            let marker = (mix.marker && !preserve_opacity).then(|| rgb.map(|c| c as f64 / 255.0));
            // Акварель при нажиме не выше порога размытия только размывает — краску не кладёт
            // (по сверке с живой SAI: мазки 0,5 и 0,3 сошлись с 12,9 и 8,9 до 2,1 и 1,4 уровня).
            let only_blur = mix.blur && mix.blend_by_pressure && batch[0].pressure <= mix.blur_pressure + 1e-6;
            if !only_blur {
                lay_batch(pixels, &batch, mask, paint, marker, preserve_opacity, sel, originals, &mut changed);
            }
            if mix.blur {
                // изменённые плитки пересчитаются в копии при следующей пачке
                for key in &changed {
                    for level in 0..=3 {
                        self.level_cache.remove(&(level, *key));
                    }
                }
            }
        }
        let keys: Vec<Key> = changed.into_iter().collect();
        self.touched.extend(keys.iter().copied());
        keys
    }
}

/// Пачка отпечатков кладёт краску `paint`. Плитки независимы: каждая получает свои отпечатки в
/// прежнем порядке, поэтому результат тот же, что при поштучном счёте; много работы — параллельно.
#[allow(clippy::too_many_arguments)]
fn lay_batch(
    pixels: &mut ColorMap,
    batch: &[Pending],
    mask: &MaskMap,
    paint: [f64; 4],
    marker: Option<[f64; 3]>,
    preserve_opacity: bool,
    sel: &crate::selection::Selection,
    originals: &mut HashMap<Key, Option<Tile<Px>>>,
    changed: &mut HashSet<Key>,
) {
    // плитка → отпечатки, чей прямоугольник её задевает
    let mut per_tile: HashMap<Key, Vec<usize>> = HashMap::new();
    let mut work = 0usize;
    for (n, d) in batch.iter().enumerate() {
        let reach = d.shape.outer() + 1.0;
        let (x0, x1) = ((d.x - reach).floor() as i32, (d.x + reach).ceil() as i32);
        let (y0, y1) = ((d.y - reach).floor() as i32, (d.y + reach).ceil() as i32);
        for ty in y0.div_euclid(TILE)..=y1.div_euclid(TILE) {
            for tx in x0.div_euclid(TILE)..=x1.div_euclid(TILE) {
                per_tile.entry((tx, ty)).or_default().push(n);
            }
        }
        work += ((x1 - x0 + 1) * (y1 - y0 + 1)) as usize;
    }
    let mut jobs: Vec<(Key, Option<Tile<Px>>, Vec<usize>)> =
        per_tile.into_iter().map(|(k, list)| (k, pixels.tiles.remove(&k), list)).collect();
    // одна плитка: отпечатки по очереди, точки — как в `for_rows`, только в пределах плитки
    let run = |job: &mut (Key, Option<Tile<Px>>, Vec<usize>)| -> bool {
        let ((tx, ty), tile, list) = job;
        let (tx0, ty0) = (*tx * TILE, *ty * TILE);
        let mut hit = false;
        // выделение — плиткой (как `sel.amount`, без поиска на каждую точку)
        let sel_all = sel.mask.tiles.is_empty();
        let sel_tile = if sel_all { None } else { sel.mask.tiles.get(&(*tx, *ty)) };
        for &n in list.iter() {
            let d = &batch[n];
            let opacity = d.shape.opacity() * if marker.is_some() { marker_dab_scale() } else { 1.0 };
            for_rows(d, mask, ty0..ty0 + TILE, tx0..tx0 + TILE, |y, ra, rb, covs| {
                if y < ty0 || y >= ty0 + TILE {
                    return;
                }
                for x in ra.max(tx0)..=rb.min(tx0 + TILE - 1) {
                    let cov = covs[(x - ra) as usize];
                    if cov <= 0.0 {
                        continue;
                    }
                    let i = ((y - ty0) * TILE + x - tx0) as usize;
                    let s = if sel_all { ONE } else { sel_tile.map_or(0, |t| t.0[i] as u32) };
                    if s == 0 {
                        continue;
                    }
                    let cov = cov * s as f32 / ONE as f32;
                    hit = true;
                    let t = tile.get_or_insert_with(|| Tile::filled([0; 4]));
                    t.0[i] = match marker {
                        Some(brush) => lay_marker(t.0[i], paint, brush, (cov * opacity) as f64),
                        None => lay(t.0[i], paint, (cov * opacity) as f64, preserve_opacity),
                    };
                }
            });
        }
        hit
    };
    let threads = crate::threads();
    // прежний вид плиток — до правки (для отмены)
    let before: Vec<Option<Tile<Px>>> = jobs.iter().map(|(k, t, _)| if originals.contains_key(k) { None } else { t.clone() }).collect();
    let hits: Vec<bool> = if work < 150_000 || threads < 2 || jobs.len() < 2 {
        jobs.iter_mut().map(run).collect()
    } else {
        let chunk = jobs.len().div_ceil(threads);
        std::thread::scope(|scope| {
            let handles: Vec<_> = jobs.chunks_mut(chunk).map(|c| scope.spawn(move || c.iter_mut().map(run).collect::<Vec<bool>>())).collect();
            handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
        })
    };
    for (((key, tile, _), hit), old) in jobs.into_iter().zip(hits).zip(before) {
        if hit {
            originals.entry(key).or_insert(old);
            changed.insert(key);
        }
        if let Some(t) = tile {
            pixels.tiles.insert(key, t);
        }
    }
}

/// Акварель: пиксели под пачкой сдвигаются к размытому слою на долю, набранную отпечатками.
#[allow(clippy::too_many_arguments)]
fn water_blur(
    pixels: &mut ColorMap,
    batch: &[Pending],
    mask: &MaskMap,
    size: f32,
    sel: &crate::selection::Selection,
    originals: &mut HashMap<Key, Option<Tile<Px>>>,
    changed: &mut HashSet<Key>,
    cache: &mut HashMap<(u32, Key), Box<[[f64; 4]]>>,
) {
    let (level, r1, r2) = crate::mix::water_blur_level(size);
    let k = 1i32 << level;
    // Доля размытия в каждом пикселе: отпечатки пачки накладываются «поверх».
    // вес < 0 — точку не задел ни один отпечаток
    let (gx0, _, gw, _) = batch_bounds(batch);
    let threads = crate::threads();
    let (weight, parts) = Bands::run(batch, -1.0f64, |ya, yb, part: &mut [f64]| -> (i32, i32, i32, i32) {
        let mut r = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for d in batch {
            let o = d.shape.opacity();
            for_rows(d, mask, ya..yb, i32::MIN..i32::MAX, |y, ra, rb, covs| {
                let row = (y - ya) as usize * gw;
                for x in ra..=rb {
                    let cov = covs[(x - ra) as usize];
                    if cov <= 0.0 {
                        continue;
                    }
                    let w = &mut part[row + (x - gx0) as usize];
                    if *w < 0.0 {
                        *w = 0.0;
                    }
                    *w += (1.0 - *w) * (cov * o) as f64;
                    r = (r.0.min(x), r.1.min(y), r.2.max(x), r.3.max(y));
                }
            });
        }
        r
    });
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for r in parts {
        (x0, y0, x1, y1) = (x0.min(r.0), y0.min(r.1), x1.max(r.2), y1.max(r.3));
    }
    if x0 > x1 {
        return;
    }
    // Копия в точках уровня (точка копии — квадрат k×k слоя, сетка от начала холста) с запасом
    // на радиус размытия и на двулинейный возврат.
    let m = (r1 + r2) as i32 + 2;
    let (lx0, ly0) = (x0.div_euclid(k) - m, y0.div_euclid(k) - m);
    let (lx1, ly1) = (x1.div_euclid(k) + m, y1.div_euclid(k) + m);
    let (w, h) = ((lx1 - lx0 + 1) as usize, (ly1 - ly0 + 1) as usize);
    let scale = 1.0 / (ONE as f64 * (k * k) as f64);
    let side = (TILE / k) as usize; // точек уровня на сторону плитки
    // недостающие плитки копии: среднее квадратов k×k, плитки — параллельно
    let (fx0, fy0, fx1, fy1) = (lx0 * k, ly0 * k, (lx1 + 1) * k - 1, (ly1 + 1) * k - 1);
    let mut missing = Vec::new();
    for ty in fy0.div_euclid(TILE)..=fy1.div_euclid(TILE) {
        for tx in fx0.div_euclid(TILE)..=fx1.div_euclid(TILE) {
            if !cache.contains_key(&(level, (tx, ty))) {
                missing.push((tx, ty));
            }
        }
    }
    let shrink = |key: &Key| -> Box<[[f64; 4]]> {
        let mut out = vec![[0.0f64; 4]; side * side];
        if let Some(t) = pixels.tiles.get(key) {
            for (i, px) in t.0.iter().enumerate() {
                let (x, y) = (i % TILE as usize, i / TILE as usize);
                let v = &mut out[(y / k as usize) * side + x / k as usize];
                for c in 0..4 {
                    v[c] += px[c] as f64;
                }
            }
            for v in out.iter_mut() {
                *v = v.map(|c| c * scale);
            }
        }
        out.into_boxed_slice()
    };
    let blocks: Vec<Box<[[f64; 4]]>> = if missing.len() < 4 || threads < 2 {
        missing.iter().map(shrink).collect()
    } else {
        let per = missing.len().div_ceil(threads);
        std::thread::scope(|scope| {
            let hs: Vec<_> = missing.chunks(per).map(|c| scope.spawn(move || c.iter().map(shrink).collect::<Vec<_>>())).collect();
            hs.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
        })
    };
    cache.extend(missing.into_iter().map(|key| (level, key)).zip(blocks));
    // буфер копии переиспользуется между пачками: выделять и обнулять десятки мегабайт на каждую
    // пачку дороже самого счёта
    thread_local!(static BUF: std::cell::RefCell<Vec<[f64; 4]>> = const { std::cell::RefCell::new(Vec::new()) });
    let mut buf = BUF.with(|b| std::mem::take(&mut *b.borrow_mut()));
    buf.resize(w * h, [0.0; 4]);
    buf.truncate(w * h);
    // строки копии — отрезками по плиткам (плитка из таблицы — раз на отрезок), строки параллельно
    let fill = |j0: usize, lines: &mut [[f64; 4]]| {
        for (dj, line) in lines.chunks_mut(w).enumerate() {
            let fy = (ly0 + (j0 + dj) as i32) * k;
            let row = (fy.rem_euclid(TILE) / k) as usize * side;
            let mut i = 0;
            while i < w {
                let fx = (lx0 + i as i32) * k;
                let cx = (fx.rem_euclid(TILE) / k) as usize;
                let n = (side - cx).min(w - i);
                let block = &cache[&(level, key_of(fx, fy))];
                line[i..i + n].copy_from_slice(&block[row + cx..row + cx + n]);
                i += n;
            }
        }
    };
    if w * h < 40_000 || threads < 2 {
        fill(0, &mut buf);
    } else {
        let per = h.div_ceil(threads);
        std::thread::scope(|scope| {
            for (n, part) in buf.chunks_mut(per * w).enumerate() {
                let fill = &fill;
                scope.spawn(move || fill(n * per, part));
            }
        });
    }
    for r in [r1, r2] {
        crate::mix::box_blur(&mut buf, w, h, r, false);
        crate::mix::box_blur(&mut buf, w, h, r, true);
    }
    // Размытое в точке слоя: на уровне 0 — сама точка копии, выше — двулинейно между центрами.
    let at = |x: i32, y: i32| -> [f64; 4] {
        if k == 1 {
            return buf[(y - ly0) as usize * w + (x - lx0) as usize];
        }
        let u = (x as f64 + 0.5) / k as f64 - 0.5 - lx0 as f64;
        let v = (y as f64 + 0.5) / k as f64 - 0.5 - ly0 as f64;
        let (i, j) = (u.floor() as usize, v.floor() as usize);
        let (fu, fv) = (u - u.floor(), v - v.floor());
        let p = |i: usize, j: usize| buf[j * w + i];
        let (a, b, c, d) = (p(i, j), p(i + 1, j), p(i, j + 1), p(i + 1, j + 1));
        std::array::from_fn(|n| {
            let top = a[n] + (b[n] - a[n]) * fu;
            let bot = c[n] + (d[n] - c[n]) * fu;
            top + (bot - top) * fv
        })
    };

    // Пиксели сдвигаются к размытому на свою долю — по плиткам, плитки параллельно.
    let mut jobs: Vec<(Key, Option<Tile<Px>>)> = Vec::new();
    for ty in y0.div_euclid(TILE)..=y1.div_euclid(TILE) {
        for tx in x0.div_euclid(TILE)..=x1.div_euclid(TILE) {
            jobs.push(((tx, ty), pixels.tiles.remove(&(tx, ty))));
        }
    }
    let (weight, at) = (&weight, &at);
    let run = |job: &mut (Key, Option<Tile<Px>>)| -> bool {
        let ((tx, ty), tile) = job;
        let (tx0, ty0) = (*tx * TILE, *ty * TILE);
        let mut hit = false;
        // выделение — плиткой (как `sel.amount`, без поиска на каждую точку)
        let sel_all = sel.mask.tiles.is_empty();
        let sel_tile = if sel_all { None } else { sel.mask.tiles.get(&(*tx, *ty)) };
        for y in y0.max(ty0)..=y1.min(ty0 + TILE - 1) {
            for x in x0.max(tx0)..=x1.min(tx0 + TILE - 1) {
                let a = weight.get(x, y);
                if a < 0.0 {
                    continue;
                }
                let i = ((y - ty0) * TILE + x - tx0) as usize;
                let s = if sel_all { ONE } else { sel_tile.map_or(0, |t| t.0[i] as u32) };
                let a = a * s as f64 / ONE as f64;
                if a <= 0.0 {
                    continue;
                }
                hit = true;
                let t = tile.get_or_insert_with(|| Tile::filled([0; 4]));
                t.0[i] = lay(t.0[i], at(x, y), a, false);
            }
        }
        hit
    };
    let before: Vec<Option<Tile<Px>>> = jobs.iter().map(|(k, t)| if originals.contains_key(k) { None } else { t.clone() }).collect();
    let area = ((x1 - x0 + 1) * (y1 - y0 + 1)) as usize;
    let hits: Vec<bool> = if area < 40_000 || threads < 2 || jobs.len() < 2 {
        jobs.iter_mut().map(run).collect()
    } else {
        let chunk = jobs.len().div_ceil(threads);
        std::thread::scope(|scope| {
            let hs: Vec<_> = jobs.chunks_mut(chunk).map(|c| scope.spawn(move || c.iter_mut().map(run).collect::<Vec<bool>>())).collect();
            hs.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
        })
    };
    for (((key, tile), hit), old) in jobs.into_iter().zip(hits).zip(before) {
        if hit {
            originals.entry(key).or_insert(old);
            changed.insert(key);
        }
        if let Some(t) = tile {
            pixels.tiles.insert(key, t);
        }
    }
    BUF.with(|b| *b.borrow_mut() = buf);
}

/// Самая жёсткая ступень от 20 px: форма отпечатка в маску штриха — максимумом.
fn add_to_hard_mask(mask: &mut MaskMap, px: f32, py: f32, shape: &Shape) {
    let Some((inner, edge, a)) = shape.mask() else { return };
    let reach = inner + edge + 1.0;
    let (x0, x1) = ((px - reach).floor() as i32, (px + reach).ceil() as i32);
    let (y0, y1) = ((py - reach).floor() as i32, (py + reach).ceil() as i32);
    // по плиткам: плитка берётся из таблицы один раз, строка — только в пределах круга (с
    // запасом); значения те же, что при обходе по точкам
    let paint = |key: Key, t: &mut Tile<u16>| -> bool {
        let (bx, by) = (key.0 * TILE, key.1 * TILE);
        let mut hit = false;
        for y in y0.max(by)..=y1.min(by + TILE - 1) {
            let cy = y as f32 + 0.5 - py;
            let w2 = (reach + 1.0) * (reach + 1.0) - cy * cy;
            if w2 < 0.0 {
                continue;
            }
            let w = w2.sqrt() + 1.0;
            let (ra, rb) = (x0.max(bx).max((px - w).floor() as i32), x1.min(bx + TILE - 1).min((px + w).ceil() as i32));
            let row = ((y - by) * TILE) as usize;
            for x in ra..=rb {
                let cx = x as f32 + 0.5 - px;
                let e = ((cx * cx + cy * cy).sqrt() - inner).max(0.0);
                let f = (1.0 - e / edge).max(0.0);
                let v = (((f * f * f - f) * a + f) * 16384.0) as i32;
                if v > 0 {
                    let i = row + (x - bx) as usize;
                    t.0[i] = t.0[i].max(v as u16);
                    hit = true;
                }
            }
        }
        hit
    };
    let keys: Vec<Key> = (y0.div_euclid(TILE)..=y1.div_euclid(TILE)).flat_map(|ty| (x0.div_euclid(TILE)..=x1.div_euclid(TILE)).map(move |tx| (tx, ty))).collect();
    let threads = crate::threads();
    if keys.len() < 64 || threads < 2 {
        for key in keys {
            match mask.tiles.get_mut(&key) {
                Some(t) => {
                    paint(key, t);
                }
                None => {
                    let mut t = Tile::filled(0);
                    if paint(key, &mut t) {
                        mask.tiles.insert(key, t);
                    }
                }
            }
        }
        return;
    }
    // большой отпечаток: плитки — параллельно
    let mut jobs: Vec<(Key, Option<Tile<u16>>, bool)> = keys.into_iter().map(|k| { let t = mask.tiles.remove(&k); let had = t.is_some(); (k, t, had) }).collect();
    let chunk = jobs.len().div_ceil(threads);
    std::thread::scope(|scope| {
        for part in jobs.chunks_mut(chunk) {
            let paint = &paint;
            scope.spawn(move || {
                for (key, tile, keep) in part.iter_mut() {
                    let t = tile.get_or_insert_with(|| Tile::filled(0));
                    // оставить плитку: была раньше или отпечаток её задел
                    *keep |= paint(*key, t);
                }
            });
        }
    });
    for (key, tile, keep) in jobs {
        // новая плитка, которую отпечаток не задел, не нужна (как раньше)
        if let Some(t) = tile.filter(|_| keep) {
            mask.tiles.insert(key, t);
        }
    }
}

/// Строки отпечатка в пределах `rows` и столбцов `cols`: `f(y, x_from, x_to, покрытия)`,
/// покрытие точки x — `покрытия[x - x_from]`. `mask` — маска штриха самой жёсткой
/// ступени от 20 px: покрытие не больше её.
fn for_rows(d: &Pending, mask: &MaskMap, rows: std::ops::Range<i32>, cols: std::ops::Range<i32>, mut f: impl FnMut(i32, i32, i32, &[f32])) {
    let masked = d.shape.mask().is_some();
    let reach = d.shape.outer() + 1.0;
    let (x0, x1) = ((d.x - reach).floor() as i32, (d.x + reach).ceil() as i32);
    let (y0, y1) = ((d.y - reach).floor() as i32, (d.y + reach).ceil() as i32);
    // только строки из `rows`: полосы потоков и плитки не перебирают чужие строки
    let (y0, y1) = (y0.max(rows.start), y1.min(rows.end - 1));
    let round = d.shape.coverage(reach - 1.0) <= 0.0;
    let flat_cov = d.shape.coverage(0.0);
    // внутри этого круга покрытие постоянно — без корня; маска и текстура — всё равно после
    let flat_r2 = d.shape.flat_r2();
    // покрытие строки — одним плотным циклом в буфер (без вызова на каждую точку)
    let mut covs: Vec<f32> = Vec::new();
    let mut mrow: Vec<u16> = Vec::new();
    for y in y0..=y1 {
        let cy = y as f32 + 0.5 - d.y;
        let (mut ra, mut rb) = (x0, x1);
        if round {
            let w2 = reach * reach - cy * cy;
            if w2 < 0.0 {
                continue;
            }
            let w = w2.sqrt() + 1.0;
            ra = ra.max((d.x - 0.5 - w).floor() as i32);
            rb = rb.min((d.x - 0.5 + w).ceil() as i32);
        }
        let (ra, rb) = (ra.max(cols.start), rb.min(cols.end - 1));
        if ra > rb {
            continue;
        }
        if masked {
            // строка маски: плитка из таблицы — раз на TILE точек, а не на каждую
            mrow.clear();
            let mut x = ra;
            while x <= rb {
                let end = rb.min((x.div_euclid(TILE) + 1) * TILE - 1);
                match mask.tiles.get(&key_of(x, y)) {
                    Some(t) => {
                        let i = crate::tile::index_of(x, y);
                        mrow.extend_from_slice(&t.0[i..i + (end - x + 1) as usize]);
                    }
                    None => mrow.resize(mrow.len() + (end - x + 1) as usize, 0),
                }
                x = end + 1;
            }
        }
        covs.clear();
        covs.extend((ra..=rb).map(|x| {
            let cx = x as f32 + 0.5 - d.x;
            let d2 = cx * cx + cy * cy;
            let mut cov = if d2 < flat_r2 { flat_cov } else { d.shape.coverage(d2.sqrt()) };
            if masked {
                cov = cov.min(mrow[(x - ra) as usize] as f32 / 16384.0);
            }
            match &d.grain {
                Some(g) => g.apply(cov, x, y),
                None => cov,
            }
        }));
        f(y, ra, rb, &covs);
    }
}

/// Сетка над пачкой по полосам строк: каждая полоса выделяется, заполняется и считается в своём
/// потоке (у огромных кистей сетка — сотни мегабайт, выделять её одним куском в одном потоке
/// дороже самого счёта). Полосы — по `per` строк, последняя короче.
struct Bands<T> {
    x0: i32,
    y0: i32,
    w: usize,
    per: usize,
    parts: Vec<Vec<T>>,
}

impl<T: Copy + Send + Sync> Bands<T> {
    /// `f(ya, yb, полоса)` для строк `ya..yb`; полоса — `(yb - ya) × w`, начало — `x0`.
    fn run<R: Send>(batch: &[Pending], fill: T, f: impl Fn(i32, i32, &mut [T]) -> R + Sync) -> (Self, Vec<R>) {
        let (x0, y0, w, h) = batch_bounds(batch);
        let threads = crate::threads();
        let bands = if w * h < 40_000 || threads < 2 { 1 } else { threads };
        let per = h.div_ceil(bands).max(1);
        let count = if w == 0 { 0 } else { h.div_ceil(per) };
        let job = |n: usize| {
            let rows = per.min(h - n * per);
            let ya = y0 + (n * per) as i32;
            let mut v = vec![fill; rows * w];
            let r = f(ya, ya + rows as i32, &mut v);
            (v, r)
        };
        let out: Vec<(Vec<T>, R)> = if count <= 1 {
            (0..count).map(job).collect()
        } else {
            std::thread::scope(|scope| {
                let job = &job;
                let hs: Vec<_> = (0..count).map(|n| scope.spawn(move || job(n))).collect();
                hs.into_iter().map(|h| h.join().expect("полоса сетки")).collect()
            })
        };
        let (parts, res) = out.into_iter().unzip();
        (Bands { x0, y0, w, per, parts }, res)
    }
    fn get(&self, x: i32, y: i32) -> T {
        let row = (y - self.y0) as usize;
        let b = row / self.per;
        self.parts[b][(row - b * self.per) * self.w + (x - self.x0) as usize]
    }
}

/// Прямоугольник пачки отпечатков: x0, y0, ширина, высота.
fn batch_bounds(batch: &[Pending]) -> (i32, i32, usize, usize) {
    let mut b = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for d in batch {
        let reach = d.shape.outer() + 1.0;
        b.0 = b.0.min((d.x - reach).floor() as i32);
        b.1 = b.1.min((d.y - reach).floor() as i32);
        b.2 = b.2.max((d.x + reach).ceil() as i32);
        b.3 = b.3.max((d.y + reach).ceil() as i32);
    }
    (b.0, b.1, (b.2 - b.0 + 1).max(0) as usize, (b.3 - b.1 + 1).max(0) as usize)
}

/// Пиксель сдвигается к краске `paint` (предумноженной, 0..=1) на долю `a`.
/// Маркер кладёт отпечатки слабее остальных кистей «Ver1»: множитель подобран по живой SAI 2017
/// (перекраска непрозрачного при нажиме 0,3 / 0,5 / 0,75 — 0,66 / 0,84 / 0,954;
/// docs/16-mixing.md); 0,41 — после перехода жёсткой ступени на код образца (0.8.11). По коду образца не объяснён.
const MARKER_DAB_SCALE: f32 = 0.41;

fn marker_dab_scale() -> f32 {
    MARKER_DAB_SCALE
}

/// Маркер, как в образце: из двух результатов — «перекрасить цветом кисти, сохранив
/// непрозрачность пикселя» и обычного «сдвинуть к краске пачки» — берётся тот, что
/// непрозрачнее; при равенстве — перекраска. Поэтому на непрозрачном Маркер кладёт чистый цвет и
/// не тянет подхваченное дальше, а краска с примесью ложится только туда, где было прозрачнее.
fn lay_marker(px: Px, paint: [f64; 4], brush: [f64; 3], a: f64) -> Px {
    let k = ONE as f64;
    let v = px.map(|c| c as f64 / k);
    let mut normal = [0.0f64; 4];
    let mut recolour = [0.0f64; 4];
    for i in 0..4 {
        normal[i] = v[i] + (paint[i] - v[i]) * a;
        let target = if i < 3 { brush[i] * v[3] } else { v[3] };
        recolour[i] = v[i] + (target - v[i]) * a;
    }
    // сравнение — в шкале образца (0x4000 = 1): разница меньше его шага не считается
    let q14 = |v: f64| (v * 16384.0).round() as i32;
    let out = if q14(recolour[3]) < q14(normal[3]) { normal } else { recolour };
    let q = |v: f64| (v.clamp(0.0, 1.0) * k + 0.5) as u16;
    let al = q(out[3]);
    [q(out[0]).min(al), q(out[1]).min(al), q(out[2]).min(al), al]
}

fn lay(px: Px, paint: [f64; 4], a: f64, preserve_opacity: bool) -> Px {
    let k = ONE as f64;
    let mut out = [0.0f64; 4];
    for i in 0..4 {
        let v = px[i] as f64 / k;
        out[i] = v + (paint[i] - v) * a;
    }
    if preserve_opacity {
        // непрозрачность остаётся прежней, меняется только цвет
        let old = px[3] as f64 / k;
        let s = if out[3] > 1e-9 { old / out[3] } else { 0.0 };
        for v in out.iter_mut().take(3) {
            *v *= s;
        }
        out[3] = old;
    }
    let q = |v: f64| (v.clamp(0.0, 1.0) * k + 0.5) as u16;
    let a = q(out[3]);
    [q(out[0]).min(a), q(out[1]).min(a), q(out[2]).min(a), a]
}
