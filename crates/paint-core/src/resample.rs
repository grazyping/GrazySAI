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

/// Выборка по аффинной матрице (для трансформации слоя): `inv` переводит точку результата в
/// точку источника, `x = inv[0]·X + inv[1]·Y + inv[2]`, `y = inv[3]·X + inv[4]·Y + inv[5]`.
/// Результат — `dw`×`dh` с левым верхним углом (`ox`, `oy`) в координатах результата. Ядро —
/// то же, что у `resize`; при уменьшении растягивается. За краем источника — прозрачность (края
/// при повороте не размазываются), веса нормируются по всем отсчётам окна.
#[allow(clippy::too_many_arguments)]
pub fn affine(src: &[[f32; 4]], sw: usize, sh: usize, inv: [f64; 6], ox: i32, oy: i32, dw: usize, dh: usize, mode: Mode) -> Vec<[f32; 4]> {
    let table = mode.table();
    // во сколько раз источник крупнее результата по каждой оси
    let kx = (inv[0] * inv[0] + inv[3] * inv[3]).sqrt().max(1.0);
    let ky = (inv[1] * inv[1] + inv[4] * inv[4]).sqrt().max(1.0);
    let k = kx.max(ky);
    let reach = if mode == Mode::Nearest { 0.5 } else { mode.support() * k };
    let cross = matches!(mode, Mode::Bicubic | Mode::Lanczos2);
    let get = |x: isize, y: isize| -> [f32; 4] {
        if x < 0 || y < 0 || x >= sw as isize || y >= sh as isize {
            [0.0; 4]
        } else {
            src[y as usize * sw + x as usize]
        }
    };
    let mut out = vec![[0f32; 4]; dw * dh];
    for j in 0..dh {
        for i in 0..dw {
            let (cx, cy) = ((ox + i as i32) as f64 + 0.5, (oy + j as i32) as f64 + 0.5);
            let sx = inv[0] * cx + inv[1] * cy + inv[2];
            let sy = inv[3] * cx + inv[4] * cy + inv[5];
            if mode == Mode::Nearest {
                out[j * dw + i] = get(sx.floor() as isize, sy.floor() as isize);
                continue;
            }
            let (x0, x1) = ((sx - 0.5 - reach).floor() as isize + 1, (sx - 0.5 + reach).ceil() as isize);
            let (y0, y1) = ((sy - 0.5 - reach).floor() as isize + 1, (sy - 0.5 + reach).ceil() as isize);
            if x1 < 0 || y1 < 0 || x0 >= sw as isize || y0 >= sh as isize {
                continue;
            }
            let mut acc = [0f64; 4];
            let mut total = 0f64;
            for y in y0..y1 {
                let ty = (y as f64 + 0.5 - sy) / k;
                let wy = lookup(&table, ty);
                for x in x0..x1 {
                    let tx = (x as f64 + 0.5 - sx) / k;
                    if cross && tx.abs() >= 1.0 && ty.abs() >= 1.0 {
                        continue;
                    }
                    let w = lookup(&table, tx) * wy;
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
                out[j * dw + i] = [px[0], px[1], px[2], a as f32];
            }
        }
    }
    out
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
