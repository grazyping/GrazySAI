//! Пересэмплирование (изменение размера и трансформация) по правилам образца —
//! docs/12-resampling.md.
//!
//! - Ядро задаётся таблицей на 1/64 px (от −4 до +4), между элементами — линейно.
//! - При уменьшении ядро растягивается на коэффициент уменьшения.
//! - У бикубики и Lanczos 2 двумерное окно без «углов»: отсчёт берётся, только если по одной из
//!   осей он ближе одной единицы ядра (из 4×4 остаётся 12 отсчётов). У Lanczos 3 окно полное.
//! - Билинейный режим при уменьшении — усреднение по площади, при увеличении — треугольник.
//!
//! Сверено с живым образцом (сборка 2017-01-19, Canvas → Change Canvas Resolution, 300%, 150%,
//! 37%, все пять режимов): расхождение не больше 1/255.
//! - Веса нормируются по взятым отсчётам; за краем повторяется крайний пиксель.
//! - Результат: альфа 0..=1, цвет 0..=альфа (цвет предумножен).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Nearest,
    Bilinear,
    Bicubic,
    Lanczos2,
    Lanczos3,
}

impl Mode {
    /// По номеру, как в образце (`SmplMode_*`).
    pub fn from_index(i: u32) -> Option<Mode> {
        [Mode::Nearest, Mode::Bilinear, Mode::Bicubic, Mode::Lanczos2, Mode::Lanczos3].get(i as usize).copied()
    }

    /// Опора ядра в единицах ядра.
    pub fn support(self) -> f64 {
        match self {
            Mode::Nearest => 0.0,
            Mode::Bilinear => 1.0,
            Mode::Bicubic | Mode::Lanczos2 => 2.0,
            Mode::Lanczos3 => 3.0,
        }
    }

    /// Точное значение ядра.
    pub fn kernel(self, x: f64) -> f64 {
        let x = x.abs();
        match self {
            Mode::Nearest => f64::from(x < 0.5),
            Mode::Bilinear => (1.0 - x).max(0.0),
            Mode::Bicubic => {
                if x < 1.0 {
                    1.2 * x * x * x - 2.2 * x * x + 1.0
                } else if x < 2.0 {
                    -0.8 * x * x * x + 4.0 * x * x - 6.4 * x + 3.2
                } else {
                    0.0
                }
            }
            Mode::Lanczos2 | Mode::Lanczos3 => {
                let a = self.support();
                if x == 0.0 {
                    1.0
                } else if x >= a {
                    0.0
                } else {
                    let p = std::f64::consts::PI * x;
                    (p.sin() / p) * ((p / a).sin() / (p / a))
                }
            }
        }
    }

    /// Таблица, как у образца: 512 значений с шагом 1/64, `table[i]` — ядро в (i − 256)/64.
    fn table(self) -> Vec<f32> {
        (0..512).map(|i| self.kernel((i as f64 - 256.0) / 64.0) as f32).collect()
    }
}

/// Значение ядра по таблице с линейной интерполяцией (t — в единицах ядра, |t| < 4).
fn lookup(table: &[f32], t: f64) -> f64 {
    let q = (t * 4096.0) as i32; // фиксированная точка 1/4096, как в образце
    let idx = ((q >> 6) + 256).clamp(0, 510) as usize;
    let frac = (q & 63) as f64 / 64.0;
    let (a, b) = (table[idx] as f64, table[idx + 1] as f64);
    a + (b - a) * frac
}

/// Отсчёты по одной оси для выходного пикселя `i`: (индекс в источнике, вес, «близкий» отсчёт).
fn taps(mode: Mode, table: &[f32], i: usize, src: usize, dst: usize) -> Vec<(usize, f64, bool)> {
    let scale = src as f64 / dst as f64;
    let kscale = scale.max(1.0);
    let pos = (i as f64 + 0.5) * scale; // центр выходного пикселя в координатах источника
    if mode == Mode::Nearest {
        let j = (pos.floor() as isize).clamp(0, src as isize - 1) as usize;
        return vec![(j, 1.0, true)];
    }
    if mode == Mode::Bilinear && scale > 1.0 {
        // уменьшение: доля площади исходного пикселя внутри выходного
        let (a, b) = (i as f64 * scale, (i + 1) as f64 * scale);
        return (a.floor() as isize..b.ceil() as isize)
            .filter_map(|j| {
                let w = (b.min(j as f64 + 1.0) - a.max(j as f64)).max(0.0);
                (w > 0.0).then_some(((j.clamp(0, src as isize - 1)) as usize, w, true))
            })
            .collect();
    }
    let reach = mode.support() * kscale;
    let first = (pos - 0.5 - reach).floor() as isize + 1;
    let last = (pos - 0.5 + reach).ceil() as isize;
    (first..last)
        .map(|j| {
            let t = (j as f64 + 0.5 - pos) / kscale;
            let w = lookup(table, t);
            ((j.clamp(0, src as isize - 1)) as usize, w, t.abs() < 1.0)
        })
        .collect()
}

/// Изменить размер изображения (RGBA, предумноженный, 0..=1).
pub fn resize(src: &[[f32; 4]], sw: usize, sh: usize, dw: usize, dh: usize, mode: Mode) -> Vec<[f32; 4]> {
    let table = mode.table();
    let xt: Vec<_> = (0..dw).map(|x| taps(mode, &table, x, sw, dw)).collect();
    let yt: Vec<_> = (0..dh).map(|y| taps(mode, &table, y, sh, dh)).collect();
    let cross = matches!(mode, Mode::Bicubic | Mode::Lanczos2);
    let mut out = vec![[0f32; 4]; dw * dh];
    for (y, ty) in yt.iter().enumerate() {
        for (x, tx) in xt.iter().enumerate() {
            let mut acc = [0f64; 4];
            let mut total = 0f64;
            for &(sy, wy, ny) in ty {
                for &(sx, wx, nx) in tx {
                    if cross && !(nx || ny) {
                        continue; // угол окна образец не берёт
                    }
                    let w = wx * wy;
                    let p = src[sy * sw + sx];
                    for c in 0..4 {
                        acc[c] += p[c] as f64 * w;
                    }
                    total += w;
                }
            }
            let k = if total != 0.0 { 1.0 / total } else { 0.0 };
            let a = (acc[3] * k).clamp(0.0, 1.0) as f32;
            out[y * dw + x] = [
                ((acc[0] * k) as f32).clamp(0.0, a),
                ((acc[1] * k) as f32).clamp(0.0, a),
                ((acc[2] * k) as f32).clamp(0.0, a),
                a,
            ];
        }
    }
    out
}

/// Выборка по произвольному отображению точки результата в точку источника (`map`; `None` —
/// точки нет). Ядро — то же, что у `resize`; при уменьшении растягивается по местному масштабу
/// (по разности соседних точек). За краем источника — прозрачность, веса нормируются по всем
/// отсчётам окна. Результат — `dw`×`dh` с левым верхним углом (`ox`, `oy`).
#[allow(clippy::too_many_arguments, reason = "внутренняя функция горячего цикла: параметры раздельно")]
pub fn sample_map(src: &[[f32; 4]], sw: usize, sh: usize, ox: i32, oy: i32, dw: usize, dh: usize, mode: Mode, map: impl Fn(f64, f64) -> Option<(f64, f64)> + Sync) -> Vec<[f32; 4]> {
    let table = mode.table();
    let cross = matches!(mode, Mode::Bicubic | Mode::Lanczos2);
    let get = |x: isize, y: isize| -> [f32; 4] {
        if x < 0 || y < 0 || x >= sw as isize || y >= sh as isize {
            [0.0; 4]
        } else {
            src[y as usize * sw + x as usize]
        }
    };
    let mut out = vec![[0f32; 4]; dw * dh];
    if dw == 0 {
        return out;
    }
    // точки результата независимы — строками по потокам (результат тот же)
    let rows = dh.div_ceil(crate::threads()).max(1);
    std::thread::scope(|scope| {
        for (n, chunk) in out.chunks_mut(rows * dw).enumerate() {
            let (map, table, get) = (&map, &table, &get);
            scope.spawn(move || {
                for (jj, orow) in chunk.chunks_mut(dw).enumerate() {
                    let j = n * rows + jj;
                    for (i, o) in orow.iter_mut().enumerate() {
                        sample_one(o, ox + i as i32, oy + j as i32, sw, sh, mode, cross, table, map, get);
                    }
                }
            });
        }
    });
    out
}

/// Одна точка `sample_map`.
#[allow(clippy::too_many_arguments, reason = "внутренняя функция горячего цикла: параметры раздельно")]
#[inline]
fn sample_one(o: &mut [f32; 4], px: i32, py: i32, sw: usize, sh: usize, mode: Mode, cross: bool, table: &[f32], map: &impl Fn(f64, f64) -> Option<(f64, f64)>, get: &impl Fn(isize, isize) -> [f32; 4]) {
    {
        {
            let (cx, cy) = (px as f64 + 0.5, py as f64 + 0.5);
            let Some((sx, sy)) = map(cx, cy) else { return };
            if mode == Mode::Nearest {
                *o = get(sx.floor() as isize, sy.floor() as isize);
                return;
            }
            // местный масштаб: насколько сдвигается точка источника при шаге в 1 px результата
            let k = match (map(cx + 1.0, cy), map(cx, cy + 1.0)) {
                (Some((ax, ay)), Some((bx, by))) => ((ax - sx).hypot(ay - sy)).max((bx - sx).hypot(by - sy)).max(1.0),
                _ => 1.0,
            };
            let reach = mode.support() * k;
            let (x0, x1) = ((sx - 0.5 - reach).floor() as isize + 1, (sx - 0.5 + reach).ceil() as isize);
            let (y0, y1) = ((sy - 0.5 - reach).floor() as isize + 1, (sy - 0.5 + reach).ceil() as isize);
            if x1 < 0 || y1 < 0 || x0 >= sw as isize || y0 >= sh as isize {
                return;
            }
            let mut acc = [0f64; 4];
            let mut total = 0f64;
            for y in y0..y1 {
                let ty = (y as f64 + 0.5 - sy) / k;
                let wy = lookup(table, ty);
                for x in x0..x1 {
                    let tx = (x as f64 + 0.5 - sx) / k;
                    if cross && tx.abs() >= 1.0 && ty.abs() >= 1.0 {
                        continue;
                    }
                    let w = lookup(table, tx) * wy;
                    if w == 0.0 {
                        continue;
                    }
                    total += w;
                    let p = get(x, y);
                    for c in 0..4 {
                        acc[c] += p[c] as f64 * w;
                    }
                }
            }
            if total.abs() > 1e-9 {
                let a = (acc[3] / total).clamp(0.0, 1.0);
                let px = [0, 1, 2].map(|c| (acc[c] / total).clamp(0.0, a) as f32);
                *o = [px[0], px[1], px[2], a as f32];
            }
        }
    }
}

/// Выборка по аффинной матрице: `inv` переводит точку результата в точку источника,
/// `x = inv[0]·X + inv[1]·Y + inv[2]`, `y = inv[3]·X + inv[4]·Y + inv[5]`.
#[allow(clippy::too_many_arguments, reason = "внутренняя функция горячего цикла: параметры раздельно")]
pub fn affine(src: &[[f32; 4]], sw: usize, sh: usize, inv: [f64; 6], ox: i32, oy: i32, dw: usize, dh: usize, mode: Mode) -> Vec<[f32; 4]> {
    sample_map(src, sw, sh, ox, oy, dw, dh, mode, |x, y| Some((inv[0] * x + inv[1] * y + inv[2], inv[3] * x + inv[4] * y + inv[5])))
}

/// Проективная матрица 3×3 (по строкам), переводящая единичный квадрат (0,0)-(1,0)-(1,1)-(0,1)
/// в четырёхугольник `q` (те же углы по порядку).
pub fn square_to_quad(q: [[f64; 2]; 4]) -> [f64; 9] {
    let [[x0, y0], [x1, y1], [x2, y2], [x3, y3]] = q;
    let (dx1, dx2, dx3) = (x1 - x2, x3 - x2, x0 - x1 + x2 - x3);
    let (dy1, dy2, dy3) = (y1 - y2, y3 - y2, y0 - y1 + y2 - y3);
    let det = dx1 * dy2 - dx2 * dy1;
    let (g, h) = if det.abs() < 1e-12 { (0.0, 0.0) } else { ((dx3 * dy2 - dx2 * dy3) / det, (dx1 * dy3 - dx3 * dy1) / det) };
    [x1 - x0 + g * x1, x3 - x0 + h * x3, x0, y1 - y0 + g * y1, y3 - y0 + h * y3, y0, g, h, 1.0]
}

/// Обратная матрица 3×3 (`None` — вырождена).
pub fn invert3(m: [f64; 9]) -> Option<[f64; 9]> {
    let [a, b, c, d, e, f, g, h, i] = m;
    let (ca, cb, cc) = (e * i - f * h, -(d * i - f * g), d * h - e * g);
    let det = a * ca + b * cb + c * cc;
    if det.abs() < 1e-12 {
        return None;
    }
    let r = [ca, -(b * i - c * h), b * f - c * e, cb, a * i - c * g, -(a * f - c * d), cc, -(a * h - b * g), a * e - b * d];
    Some(r.map(|v| v / det))
}

/// Произведение матриц 3×3.
pub fn mul3(a: [f64; 9], b: [f64; 9]) -> [f64; 9] {
    let mut r = [0.0; 9];
    for i in 0..3 {
        for j in 0..3 {
            r[i * 3 + j] = (0..3).map(|k| a[i * 3 + k] * b[k * 3 + j]).sum();
        }
    }
    r
}

/// Обратная аффинная матрица (`None` — вырождена).
pub fn invert_affine(m: [f64; 6]) -> Option<[f64; 6]> {
    let det = m[0] * m[4] - m[1] * m[3];
    if det.abs() < 1e-12 {
        return None;
    }
    let (a, b, d, e) = (m[4] / det, -m[1] / det, -m[3] / det, m[0] / det);
    Some([a, b, -(a * m[2] + b * m[5]), d, e, -(d * m[2] + e * m[5])])
}
