//! Параметры кисти и форма одного отпечатка.

#[derive(Clone, Copy, Debug)]
pub struct BrushParams {
    /// Диаметр в пикселях при полном давлении.
    pub size: f32,
    /// Доля диаметра при нулевом давлении (0..=1).
    pub min_size: f32,
    /// Плотность краски при полном давлении (0..=1).
    pub density: f32,
    /// Доля плотности при нулевом давлении (0..=1).
    pub min_density: f32,
    /// Жёсткость края: 1 — резкий край, 0 — самый мягкий. Ступени образца — [`HARDNESS_STEPS`].
    pub hardness: f32,
    /// Шаг между отпечатками в долях диаметра; 0 — по правилу образца (зависит от жёсткости).
    pub spacing: f32,
    pub size_by_pressure: bool,
    pub density_by_pressure: bool,
    /// Ластик: штрих не красит, а стирает.
    pub erase: bool,
    /// Штрих слоя контура: отпечатки строго с шагом образца, без поправки «не реже 0,85 px», и без
    /// отдельной ветки самой жёсткой ступени — её образец включает только у обычных инструментов
    /// (проверено по `пинг5.sai2`: на концах толстых штрихов видны отдельные отпечатки).
    pub linework: bool,
    /// Sharpness из раздела Miscellaneous, 0..=1 (у карандаша по умолчанию 0): от неё зависит
    /// ширина края тонкой кисти.
    pub sharpness: f32,
    /// «Ver1 Prs Spec.»: нажим на плотность по правилу версии 1 (`DabInput::ver1`).
    pub ver1_density: bool,
    /// Смешивание с холстом (Кисть, Акварель, Маркер); `None` — краска не смешивается.
    pub mix: Option<crate::mix::MixParams>,
}

impl Default for BrushParams {
    fn default() -> Self {
        Self {
            size: 12.0,
            min_size: 0.0,
            density: 1.0,
            min_density: 0.0,
            hardness: 1.0,
            spacing: 0.0,
            size_by_pressure: true,
            density_by_pressure: false,
            erase: false,
            linework: false,
            sharpness: 0.0,
            ver1_density: false,
            mix: None,
        }
    }
}

impl BrushParams {
    pub fn diameter_at(&self, pressure: f32) -> f32 {
        if !self.size_by_pressure {
            return self.size;
        }
        let p = pressure.clamp(0.0, 1.0);
        self.size * (self.min_size + (1.0 - self.min_size) * p)
    }

    pub fn density_at(&self, pressure: f32) -> f32 {
        if !self.density_by_pressure {
            return self.density;
        }
        let p = pressure.clamp(0.0, 1.0);
        self.density * (self.min_density + (1.0 - self.min_density) * p)
    }
}

/// Пять ступеней жёсткости образца, от самой мягкой к самой жёсткой: значения `hardness`.
pub const HARDNESS_STEPS: [f32; 5] = [0.0, 0.5, 0.75, 0.9, 1.0];

/// Геометрия круглого отпечатка: внутри `r_in` покрытие полное, дальше на ширине `width` оно
/// спадает до нуля по квадрату косинуса. Правила взяты из разбора образца
/// (docs/13-thin-brushes.md) и подогнаны по отпечаткам, снятым с самого образца
/// (docs/08-brush-verification.md): на размерах от 20 px расхождение меньше 1/255.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DabShape {
    /// Радиус сплошной сердцевины.
    pub r_in: f32,
    /// Ширина спада; внешний радиус равен `r_in + width`.
    pub width: f32,
    /// Радиус кисти без сглаженного края.
    pub core: f32,
    /// Самая жёсткая ступень: край строится по отдельному правилу.
    pub flat: bool,
    /// Ослабление для кистей тоньше сглаженного края.
    pub scale: f32,
}

/// Ширина сглаженного края, px (измерено на образце).
const EDGE: f32 = 1.8;
/// Во сколько раз мягкий спад шире сердцевины, которую он заменяет.
const SOFT_SPREAD: f32 = 1.7;
/// Самая жёсткая ступень: край начинается на столько px внутри номинального радиуса (у кистей
/// от 20 px, которые образец рисует по отдельной ветке, — на 1,3 px)…
const FLAT_INSET: f32 = 1.15;
const FLAT_INSET_LARGE: f32 = 1.3;
/// …и тянется на столько px, спадая как `(1 - t)^FLAT_POWER`. Подобрано по отпечаткам и линиям
/// образца размером 10–100 px: расхождение меньше 1 уровня из 255.
const FLAT_WIDTH: f32 = 2.15;
const FLAT_POWER: f32 = 2.5;

fn cos2(t: f32) -> f32 {
    let c = ((std::f32::consts::PI * t).cos() + 1.0) * 0.5;
    c * c
}

impl DabShape {
    pub fn new(diameter: f32, hardness: f32) -> Self {
        let softness = 1.0 - hardness.clamp(0.0, 1.0);
        let d = diameter - 0.1 * (diameter - 1.0).clamp(0.0, 1.0);
        let core = ((d - EDGE) * 0.5).max(0.0);
        let scale = (d / EDGE).clamp(0.0, 1.0);
        if softness <= 0.0 {
            let inset = if diameter >= 20.0 { FLAT_INSET_LARGE } else { FLAT_INSET };
            Self { r_in: (diameter * 0.5 - inset).max(0.0), width: FLAT_WIDTH, core, flat: true, scale }
        } else {
            Self { r_in: (1.0 - softness) * core, width: softness * core * SOFT_SPREAD + EDGE, core, flat: false, scale }
        }
    }

    pub fn outer(&self) -> f32 {
        self.r_in + self.width
    }

    /// Покрытие на расстоянии `d` от центра: 0..=1.
    pub fn coverage(&self, d: f32) -> f32 {
        let t = ((d - self.r_in) / self.width).clamp(0.0, 1.0);
        if self.flat {
            (1.0 - t).powf(FLAT_POWER) * self.scale
        } else {
            cos2(t) * self.scale
        }
    }
}

/// Шаг между отпечатками в долях диаметра, по правилу образца: у крупной жёсткой кисти 4%,
/// у крупной мягкой до 15%, у кистей тоньше пикселя — 15%.
pub fn step_fraction(diameter: f32, hardness: f32) -> f32 {
    let softness = 1.0 - hardness.clamp(0.0, 1.0);
    let large = 0.04 + 0.12222 * (softness - 0.1).max(0.0);
    if diameter <= 1.0 {
        0.15
    } else {
        large + (0.15 - large) / ((diameter - 1.0) * 0.5 + 1.0)
    }
}

/// Плотность, которую должна набрать цепочка отпечатков вдоль оси мазка. Образец целится чуть
/// ниже заданной плотности (на 0,1% и меньше), а у кистей тоньше 6 px — заметно ниже.
pub fn stroke_target(diameter: f32, density: f32) -> f32 {
    let thin = 1.0 - (diameter / 6.0).min(1.0);
    density.clamp(0.0, 1.0) * (0.999 - 0.019 * thin) + (diameter * 4.5e-5).min(0.0009)
}

/// Непрозрачность одного отпечатка, при которой цепочка отпечатков с шагом `step`, накладываясь
/// друг на друга, даёт вдоль оси мазка плотность `density` (точнее — `stroke_target`).
///
/// Расчёт ведётся по упрощённому профилю, как в образце: спад не уже примерно 2,2 px, а на самой
/// жёсткой ступени сердцевина берётся целиком. Поэтому одиночный отпечаток полной плотности
/// выходит полупрозрачным: от 0,31 у жёсткой кисти до 0,97 у самой мягкой.
pub fn dab_opacity(shape: &DabShape, diameter: f32, step: f32, density: f32) -> f32 {
    if density <= 0.0 {
        return 0.0;
    }
    let target = stroke_target(diameter, density);
    let floor = target * 3.0 - diameter.min(1.0) * 0.8;
    let (r, w) = if shape.flat {
        (shape.core, floor.max((0.18 * shape.core).max(EDGE)))
    } else {
        (shape.r_in, floor.max(shape.width))
    };
    // Покрытие точки на оси от соседних отпечатков: свой — один раз, остальные — с двух сторон.
    let mut samples = Vec::with_capacity(50);
    let mut pos = 0.0;
    while pos < r + w && samples.len() < 50 {
        samples.push(cos2(((pos - r).max(0.0) / w).min(1.0)));
        pos += step.max(1e-3);
    }
    let total = |a: f32| -> f32 {
        let mut rest = 1.0 - a * samples[0];
        for c in &samples[1..] {
            let k = 1.0 - a * c;
            rest *= k * k;
        }
        1.0 - rest
    };
    if total(1.0) <= target {
        return 1.0;
    }
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..28 {
        let mid = (lo + hi) * 0.5;
        if total(mid) < target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    hi
}

/// Покрытие круглого отпечатка диаметром `diameter` на расстоянии `d` от центра: 0..=1.
pub fn round_coverage(d: f32, diameter: f32, hardness: f32) -> f32 {
    DabShape::new(diameter, hardness).coverage(d)
}
