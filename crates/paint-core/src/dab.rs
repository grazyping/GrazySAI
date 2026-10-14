//! Круглый отпечаток так, как его строит образец: размер и край (`dab_size_and_density`),
//! профиль края из таблиц (`profile_tables_init`, выбор — `dab_make_node`), непрозрачность
//! решателем (`dab_solve_opacity`). Своими словами — docs/13-thin-brushes.md.

use std::sync::OnceLock;

/// Таблицы профиля края: значения 0..=1 (в образце u16, 16384 = 1).
struct Tables {
    /// Квадрат косинуса на 64, 256, 1024 и 4096 значений.
    cos2: [Vec<f32>; 4],
    /// 17 «рамп» × 8 смесей с квадратом косинуса, по 64 значения: `ramp[k][j]`.
    ramp: Vec<Vec<Vec<f32>>>,
}

fn q(v: f64) -> f32 {
    // как в образце: (short)(longlong)(v * 16384), затем ×1/16384
    ((v * 16384.0) as i64 as i16 as u16) as f32 * (1.0 / 16384.0)
}

fn cos2_table(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            if i + 1 == n {
                return 0.0;
            }
            let c = ((i as f64 * std::f64::consts::PI) / (n - 1) as f64).cos() * 0.5 + 0.5;
            q(c * c)
        })
        .collect()
}

fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| {
        let cos2 = [cos2_table(64), cos2_table(256), cos2_table(1024), cos2_table(4096)];
        let raw16 = |v: f32| (v * 16384.0).round() as i32;
        let ramp = (0..17)
            .map(|k| {
                let c = k as f64 * 0.0625 * 0.85;
                let base: Vec<i32> = (0..64)
                    .map(|i| {
                        if i == 63 {
                            return 0;
                        }
                        let t = (63 - i) as f64 / 63.0;
                        (((t * t * t - t) * c + t) * 16384.0) as i64 as i32
                    })
                    .collect();
                (0..8)
                    .map(|j| {
                        let w = j as f64 * 0.125;
                        (0..64)
                            .map(|i| {
                                if i == 63 {
                                    return 0.0;
                                }
                                let b = base[i] as f64;
                                let v = ((raw16(cos2[0][i]) as f64 - b) * w + b) as i64;
                                v as f32 / 16384.0
                            })
                            .collect()
                    })
                    .collect()
            })
            .collect();
        Tables { cos2, ramp }
    })
}

/// Приближённая степень образца (`FUN_1400e7e98`): x^p через разложение логарифма.
fn approx_pow(x: f64, p: f64) -> f64 {
    let d = x - 1.0;
    let d2 = d * d;
    let d4 = d2 * d2;
    let d8 = d4 * d4;
    let l = d - (d2 + d4 + d8 + d8 * d8) * 0.7;
    let a = 1.0 - p;
    x / (a * l + 1.0 + a * a * l * l * 0.5)
}

/// Что нужно для отпечатка.
#[derive(Clone, Copy, Debug)]
pub struct DabInput {
    /// Диаметр с учётом нажима.
    pub diameter: f32,
    /// Мягкость ступени жёсткости: 0 — самая жёсткая, 1 — самая мягкая.
    pub softness: f32,
    /// Плотность кисти 0..=1.
    pub density: f32,
    /// Множитель плотности от нажима (1, если нажим на плотность не влияет).
    pub density_factor: f32,
    /// `1 − Sharpness` (у карандаша по умолчанию Sharpness = 0, значит 1).
    pub density_scale: f32,
    /// Ветка самой жёсткой ступени (у обычных инструментов при мягкости 0; у слоя контура не
    /// включается). До 20 px — обычная геометрия с рампой 16 (измерено), от 20 px штрих рисует
    /// подобранная модель `DabShape` (см. `Stroke`).
    pub flat: bool,
}

/// Готовый отпечаток.
#[derive(Clone, Debug)]
pub struct Dab {
    /// Внутри — полное покрытие.
    pub inner: f32,
    /// Ширина края.
    pub edge: f32,
    profile: &'static [f32],
    /// Множитель непрозрачности у кистей тоньше края.
    pub thin: f32,
    /// Шаг до следующего отпечатка, px (до умножения на долю шага — диаметр для шага).
    pub step_size: f32,
    /// Диаметр, по которому считается доля шага.
    pub step_diameter: f32,
    solve: &'static [f32],
    solve_inner: f32,
    solve_width: f32,
    target: f32,
}

impl Dab {
    pub fn new(i: DabInput) -> Dab {
        let t = tables();
        let d = i.diameter as f64;
        let ds = i.density_scale as f64;
        // dab_size_and_density
        let e = (d - 1.0).clamp(0.0, 1.0);
        let sharp = i.density as f64 * ds * i.density_factor as f64 * (e * 0.4 + 0.6);
        let mut edge_min = sharp + 1.0;
        let d5 = d - (ds + 1.0) * e * 0.1;
        let mut thin = 1.0;
        if d5 < edge_min {
            thin = d5 / edge_min;
            edge_min = approx_pow(thin, 1.35) * sharp + 1.0;
        }
        let size = d5.max(edge_min);
        let radius = (size - edge_min) * 0.5;
        // dab_make_node
        let target = i.density as f64 * (0.999 - (ds * 0.009 + 0.01) * (1.0 - (d / 6.0).min(1.0))) + (d * 4.5e-5).min(0.0009);
        let target_p = target * i.density_factor as f64;
        let mut small = d.min(1.0);
        if ds < 1.0 {
            let x = 1.0 - ds;
            small = (1.0 - x * 0.8) * small + x * 0.8;
        }
        let (inner, edge, profile, solve): (f64, f64, &'static [f32], &'static [f32]) = {
            let s = i.softness as f64;
            let inner = (1.0 - s) * radius;
            let edge = s * radius * 1.7 + edge_min;
            if edge >= edge_min * 2.0 {
                let tab: &'static [f32] = if d >= 1024.0 {
                    &t.cos2[3]
                } else if d >= 256.0 {
                    &t.cos2[2]
                } else {
                    &t.cos2[1]
                };
                (inner, edge, tab, tab)
            } else {
                let mix = small * target_p * 0.8 + ds * 0.2;
                let j = (((edge - edge_min) / edge_min * 8.0) as i32).min(7) as usize;
                // В ветке самой жёсткой ступени обычных инструментов (`flat`) образец по коду
                // берёт другую таблицу, но по измерению отпечатки 2–10 px и край линии 10 px
                // совпадают с самой выпуклой рампой (16) при обычной геометрии. У контура и
                // остальных ступеней — по формуле, точно.
                let k = if i.flat { 16 } else { ((mix * 16.0) as i32).min(16) as usize };
                (inner, edge, &t.ramp[k][j], &t.ramp[0][j])
            }
        };
        let solve_width = (target_p * 3.0 - small * 0.8).max(edge);
        Dab {
            inner: inner as f32,
            edge: edge as f32,
            profile,
            thin: thin as f32,
            step_size: size as f32,
            step_diameter: d5 as f32,
            solve,
            solve_inner: inner as f32,
            solve_width: solve_width as f32,
            target: target_p as f32,
        }
    }

    pub fn outer(&self) -> f32 {
        self.inner + self.edge
    }

    /// Покрытие на расстоянии `dist` от центра (центр пикселя): значение таблицы без
    /// интерполяции, как в `dab_coverage_tile`.
    pub fn coverage(&self, dist: f32) -> f32 {
        let n = self.profile.len();
        let u = (dist.min(self.outer()) - self.inner).max(0.0);
        let idx = (u * ((n - 1) as f32 + 0.001) / self.edge) as usize;
        self.profile[idx.min(n - 1)]
    }

    /// Непрозрачность одного отпечатка при шаге `step` px (`dab_solve_opacity`), с ослаблением
    /// тонкой кисти.
    pub fn opacity(&self, step: f32) -> f32 {
        let target = self.target as f64;
        if target < 3.0517578125e-05 {
            return 0.0;
        }
        let n = self.solve.len();
        let (inner, width, step) = (self.solve_inner as f64, self.solve_width as f64, step as f64);
        let mut samples = Vec::with_capacity(50);
        let mut pos = 0.0;
        if inner + width > 0.0 {
            while samples.len() < 50 {
                let u = (pos - inner).max(0.0);
                let idx = (u * ((n - 1) as f64 + 0.001) / width) as usize;
                samples.push(self.solve[idx.min(n - 1)] as f64);
                pos += step;
                if pos >= inner + width {
                    break;
                }
            }
        }
        if samples.is_empty() {
            return 0.0;
        }
        let total = |a: f64| {
            let mut rest = 1.0 - a * samples[0];
            for s in &samples[1..] {
                let k = 1.0 - a * s;
                rest = k * rest * k;
            }
            1.0 - rest
        };
        const H: f64 = 9.5367431640625e-07;
        let mut a = H;
        for _ in 0..30 {
            let f = total(a) - target;
            if f >= -H {
                break;
            }
            let f0 = total(a - H) - target;
            a += (H / (f - f0)) * -f;
        }
        (a * self.thin as f64) as f32
    }
}

/// Доля шага образца (`dab_step_factor`): от `small` у кистей до 1 px к `large` у крупных.
pub fn step_factor(diameter: f32, softness: f32) -> f32 {
    let large = 0.04 + 0.12222222 * (softness - 0.1).max(0.0);
    let small = 0.15;
    if diameter > 1.0 {
        (1.0 / ((diameter - 1.0) * 0.5 + 1.0)) * (small - large) + large
    } else {
        small
    }
}
