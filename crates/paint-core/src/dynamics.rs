//! Динамика кисти: вход (нажим, скорость, направление, пройденный путь, случайность) → кривая →
//! выход (размер, плотность, угол и толщина отпечатка, разброс). Считается один раз на отпечаток.
//! Нажим на размер и плотность, как в SAI (`BrushParams`), остаётся как был; динамика — поверх.

/// Что меняет мазок.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynInput {
    /// Нажим пера 0..1.
    Pressure,
    /// Скорость: путь пера за одно событие ввода, доля от `scale` пикселей.
    Speed,
    /// Направление мазка: 0 — вправо, 0,25 — вниз, 0,5 — влево, 0,75 — вверх.
    Direction,
    /// Путь от начала мазка, доля от `scale` пикселей (дальше — 1).
    Distance,
    /// Случайное число на каждый отпечаток.
    Random,
}

/// Что меняется.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynOutput {
    /// Множитель диаметра (кривая 1 — без изменений).
    Size,
    /// Множитель плотности.
    Density,
    /// Поворот отпечатка: 0,5 — без поворота, 0 и 1 — на −180° и +180°.
    Angle,
    /// Множитель толщины отпечатка.
    Ratio,
    /// Разброс: сдвиг поперёк мазка на долю диаметра (в случайную сторону).
    Scatter,
}

impl DynInput {
    pub const ALL: [DynInput; 5] = [DynInput::Pressure, DynInput::Speed, DynInput::Direction, DynInput::Distance, DynInput::Random];

    pub fn key(self) -> &'static str {
        match self {
            DynInput::Pressure => "pressure",
            DynInput::Speed => "speed",
            DynInput::Direction => "direction",
            DynInput::Distance => "distance",
            DynInput::Random => "random",
        }
    }

    pub fn from_key(k: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|i| i.key() == k)
    }

    /// Масштаб по умолчанию (пиксели) у входов, которым он нужен.
    pub fn default_scale(self) -> f32 {
        match self {
            DynInput::Speed => 30.0,
            DynInput::Distance => 500.0,
            _ => 1.0,
        }
    }
}

impl DynOutput {
    pub const ALL: [DynOutput; 5] = [DynOutput::Size, DynOutput::Density, DynOutput::Angle, DynOutput::Ratio, DynOutput::Scatter];

    pub fn key(self) -> &'static str {
        match self {
            DynOutput::Size => "size",
            DynOutput::Density => "density",
            DynOutput::Angle => "angle",
            DynOutput::Ratio => "ratio",
            DynOutput::Scatter => "scatter",
        }
    }

    pub fn from_key(k: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|o| o.key() == k)
    }

    /// Нужен ли отпечатку поворот или толщина (тогда круглая кисть рисуется отпечатком-кругом).
    pub fn needs_tip(self) -> bool {
        matches!(self, DynOutput::Angle | DynOutput::Ratio)
    }
}

/// Одна связь «вход → кривая → выход». Кривая — 4 точки (x по возрастанию, y 0..1), между
/// ними — прямые; может и убывать (например, «быстрее — тоньше»).
#[derive(Clone, Debug, PartialEq)]
pub struct Dynamic {
    pub input: DynInput,
    pub output: DynOutput,
    pub curve: [[f32; 2]; 4],
    /// Пиксели для скорости и пути.
    pub scale: f32,
}

impl Dynamic {
    /// Связь с прямой кривой от 0 до 1.
    pub fn new(input: DynInput, output: DynOutput) -> Self {
        Self { input, output, curve: [[0.0, 0.0], [1.0 / 3.0, 1.0 / 3.0], [2.0 / 3.0, 2.0 / 3.0], [1.0, 1.0]], scale: input.default_scale() }
    }

    /// Значение кривой при входе `x` (0..1).
    pub fn value(&self, x: f32) -> f32 {
        let x = x.clamp(0.0, 1.0);
        let mut k = self.curve;
        k[0][0] = 0.0;
        k[3][0] = 1.0;
        for i in 1..4 {
            k[i][0] = k[i][0].clamp(k[i - 1][0], 1.0);
        }
        for i in 0..3 {
            let (a, b) = (k[i], k[i + 1]);
            if x <= b[0] {
                let t = if b[0] > a[0] { (x - a[0]) / (b[0] - a[0]) } else { 1.0 };
                return (a[1] + (b[1] - a[1]) * t).clamp(0.0, 1.0);
            }
        }
        k[3][1].clamp(0.0, 1.0)
    }

    /// Текстом для файлов: `вход выход масштаб x0 y0 x1 y1 x2 y2 x3 y3`.
    pub fn to_text(&self) -> String {
        let pts: Vec<String> = self.curve.iter().flat_map(|p| [format!("{:.4}", p[0]), format!("{:.4}", p[1])]).collect();
        format!("{} {} {} {}", self.input.key(), self.output.key(), self.scale, pts.join(" "))
    }

    pub fn from_text(s: &str) -> Option<Self> {
        let mut it = s.split_whitespace();
        let input = DynInput::from_key(it.next()?)?;
        let output = DynOutput::from_key(it.next()?)?;
        let scale = it.next()?.parse::<f32>().ok().filter(|v| v.is_finite())?.clamp(1.0, 100_000.0);
        let v: Vec<f32> = it.filter_map(|t| t.parse::<f32>().ok()).filter(|v| v.is_finite()).collect();
        if v.len() != 8 {
            return None;
        }
        let c = |i: usize| [v[i * 2].clamp(0.0, 1.0), v[i * 2 + 1].clamp(0.0, 1.0)];
        Some(Self { input, output, curve: [c(0), c(1), c(2), c(3)], scale })
    }
}

/// Входы одного отпечатка.
#[derive(Clone, Copy, Debug, Default)]
pub struct DynInputs {
    pub pressure: f32,
    /// Путь за событие ввода, px.
    pub speed: f32,
    /// Направление, радианы (`atan2`).
    pub heading: f32,
    /// Путь от начала мазка, px.
    pub distance: f32,
    /// Два случайных числа 0..1 (второе — сторона разброса).
    pub random: [f32; 2],
}

/// Итог динамики для отпечатка.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DynEffect {
    pub size: f32,
    pub density: f32,
    /// Добавка к повороту, радианы.
    pub angle: f32,
    pub ratio: f32,
    /// Сдвиг поперёк мазка в долях диаметра (со знаком).
    pub scatter: f32,
}

impl Default for DynEffect {
    fn default() -> Self {
        Self { size: 1.0, density: 1.0, angle: 0.0, ratio: 1.0, scatter: 0.0 }
    }
}

/// Своя динамика (из плагина): по входам отпечатка — его итог; складывается со связями.
pub type DynHook = Box<dyn FnMut(&DynInputs) -> DynEffect>;

/// Свести два итога: множители перемножаются, повороты и разброс складываются.
pub fn combine(a: DynEffect, b: DynEffect) -> DynEffect {
    DynEffect { size: a.size * b.size, density: a.density * b.density, angle: a.angle + b.angle, ratio: a.ratio * b.ratio, scatter: a.scatter + b.scatter }
}

/// Сложить все связи: множители перемножаются, повороты и разброс складываются.
pub fn apply(dyns: &[Dynamic], i: &DynInputs) -> DynEffect {
    let mut e = DynEffect::default();
    for d in dyns {
        let x = match d.input {
            DynInput::Pressure => i.pressure,
            DynInput::Speed => i.speed / d.scale.max(1.0),
            DynInput::Direction => (i.heading / std::f32::consts::TAU).rem_euclid(1.0),
            DynInput::Distance => i.distance / d.scale.max(1.0),
            DynInput::Random => i.random[0],
        };
        let y = d.value(x);
        match d.output {
            DynOutput::Size => e.size *= y,
            DynOutput::Density => e.density *= y,
            DynOutput::Ratio => e.ratio *= y,
            DynOutput::Angle => e.angle += (y - 0.5) * std::f32::consts::TAU,
            DynOutput::Scatter => e.scatter += y * if i.random[1] < 0.5 { -1.0 } else { 1.0 },
        }
    }
    e
}

/// Случайное число 0..1 по номеру (splitmix64): мазок повторяется одинаково.
pub fn hash01(n: u64) -> f32 {
    let mut z = n.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_is_piecewise_linear_and_may_fall() {
        let mut d = Dynamic::new(DynInput::Speed, DynOutput::Size);
        assert!((d.value(0.5) - 0.5).abs() < 1e-6);
        d.curve = [[0.0, 1.0], [0.5, 0.5], [0.75, 0.2], [1.0, 0.2]];
        assert!((d.value(0.25) - 0.75).abs() < 1e-6, "убывает");
        assert!((d.value(0.9) - 0.2).abs() < 1e-6);
    }

    #[test]
    fn text_round_trip() {
        let mut d = Dynamic::new(DynInput::Distance, DynOutput::Angle);
        d.curve[1] = [0.2, 0.9];
        d.scale = 250.0;
        let back = Dynamic::from_text(&d.to_text()).unwrap();
        assert_eq!(back.input, d.input);
        assert_eq!(back.output, d.output);
        assert_eq!(back.scale, 250.0);
        assert!((back.curve[1][1] - 0.9).abs() < 1e-4);
        assert!(Dynamic::from_text("pressure size nan 0 0 1 1").is_none());
    }

    #[test]
    fn effects_combine() {
        let size = Dynamic::new(DynInput::Pressure, DynOutput::Size);
        let mut half = Dynamic::new(DynInput::Random, DynOutput::Size);
        half.curve = [[0.0, 0.5], [0.3, 0.5], [0.6, 0.5], [1.0, 0.5]];
        let e = apply(&[size, half], &DynInputs { pressure: 0.5, random: [0.3, 0.9], ..Default::default() });
        assert!((e.size - 0.25).abs() < 1e-6);
        assert!(hash01(7) != hash01(8) && (0.0..1.0).contains(&hash01(123)));
    }
}
