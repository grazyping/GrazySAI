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

/// Приближённая степень образца: x^p через разложение логарифма.
pub(crate) fn approx_pow(x: f64, p: f64) -> f64 {
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
    /// Ветка самой жёсткой ступени (`field_0x5a1`: у обычных инструментов при мягкости 0; у слоя
    /// контура не включается) — по коду образца, docs/13-thin-brushes.md, раздел 4.
    pub flat: bool,
    /// «Ver1 Prs Spec.»: нажим на плотность по правилу версии 1 — непрозрачность отпечатка
    /// подбирается без нажима и умножается на `нажим^1,2`, край считается по-старому.
    /// У Маркера и Акварели включено по умолчанию.
    pub ver1: bool,
    /// Форма «пятно» (blotmap): край — конус от центра, профиль считается формулой, а не таблицей
    /// (`dab_make_node`, ветка формы 1). Порог по текстуре пятна — в `stroke.rs`.
    pub blot: bool,
    /// Акварель ниже «Порога размытия»: `1 − нажим/порог` (иначе 0) — край мягче и шире.
    pub taper: f32,
    /// «Amplify Density», 0..=1: усиление плотности. У Акварели и Маркера по умолчанию 0,5.
    pub amplify: f32,
    /// Кисть с размытием (Акварель): множитель усиления растёт только с нажимом — подобрано по
    /// живой SAI (`docs/16-mixing.md`), в коде SAI отдельной ветки для неё нет.
    pub amplify_by_pressure: bool,
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
    /// Цель подбора непрозрачности (у «Ver1» — без нажима) и множитель после подбора.
    solve_target: f32,
    ver1_scale: f32,
    /// Множитель непрозрачности от усиления плотности (1 — без усиления).
    amplify_gain: f32,
    amplify_mix: f32,
    ver1: bool,
    /// Форма «пятно»: выпуклость края `a` в `((f³ - f)·a + f)`.
    blot: Option<f32>,
    /// Самая жёсткая ступень: край формулой `((f³ - f)·a + f)` от `inner` на ширину `edge`,
    /// а не таблицей.
    hard: Option<f32>,
    /// Самая жёсткая ступень от 20 px: форма для маски штриха (внутр. радиус, край, выпуклость) —
    /// покрытие отпечатка не больше максимума таких форм.
    pub mask: Option<(f32, f32, f32)>,
}

impl Dab {
    pub fn new(i: DabInput) -> Dab {
        let t = tables();
        let d = i.diameter as f64;
        let ds = i.density_scale as f64;
        // dab_size_and_density
        let e = (d - 1.0).clamp(0.0, 1.0);
        let sharp = if i.ver1 {
            (i.density as f64 * i.density_factor as f64 * 0.6 + 0.4) * ds * (e * 0.4 + 0.6)
        } else {
            i.density as f64 * ds * i.density_factor as f64 * (e * 0.4 + 0.6)
        };
        let a = i.amplify.clamp(0.0, 1.0) as f64;
        let (mut sharp, mut d, mut gain) = (sharp, d, 1.0);
        if a > 0.0 {
            // Усиление плотности: край резче, тонкие кисти — к размерам из таблицы образца,
            // непрозрачность отпечатка умножается на `gain` (растёт с размером и нажимом).
            let f = i.density_factor as f64;
            let q = ((f - 0.5).clamp(0.0, 0.5) * 2.0).powi(2);
            let big = (d - 10.0).clamp(0.0, 10.0);
            let mid = d.clamp(1.0, 3.0);
            let tiny = ((1.0 - d.min(1.0)) * a * 2.0).min(1.0);
            gain = ((1.0 - q) * big * 0.1 + q) * a + 1.0;
            if i.amplify_by_pressure {
                gain = q * a + 1.0;
            }
            sharp *= (1.0 - (mid - 1.0) * 0.5 * a * 0.2) * (1.0 - tiny);
            if d < 10.0 {
                const SIZES: [f64; 21] =
                    [0.0, 0.6, 1.15, 1.35, 1.6, 2.0, 2.6, 3.4, 3.8, 4.2, 4.6, 5.2, 5.7, 6.2, 6.8, 7.3, 7.8, 8.4, 9.0, 9.5, 10.0];
                let x = (d * 2.0).max(0.0);
                let k = (x as usize).min(19);
                let t = x - k as f64;
                d += (SIZES[k] + (SIZES[k + 1] - SIZES[k]) * t - d) * a;
            }
        }
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
        if ds < 1.0 || a > 0.0 {
            let x = (1.0 - ds).max(a);
            small = (1.0 - x * 0.8) * small + x * 0.8;
        }
        let mix = small * target_p * 0.8 + ds * 0.2;
        let mut blot = None;
        let mut hard = None;
        let mut hard_solve: Option<(f64, f64)> = None;
        let mut mask = None;
        let taper = i.taper.clamp(0.0, 1.0) as f64;
        let (inner, edge, profile, solve): (f64, f64, &'static [f32], &'static [f32]) = if i.blot {
            // Край от самого центра, ширина как у самой мягкой ступени на любой жёсткости: ветка
            // формы в `brush_params_from_tool` мягкость не задаёт, и по сверке с живой SAI она 1
            // (жёсткость меняет только порог пятна). Выпуклость — пока край уже двух минимальных.
            let edge = 1.5 * radius + edge_min;
            let a = if edge < edge_min * 2.0 { mix * (1.0 - (edge - edge_min) / edge_min) } else { 0.0 };
            blot = Some((a * 0.85) as f32);
            (0.0, edge, &t.ramp[0][0], &t.ramp[0][0])
        } else if i.flat {
            // Ветка самой жёсткой ступени (`field_0x5a1`, dab_make_node): полное покрытие до
            // радиуса, дальше край шириной в минимальный; непрозрачность — по линейной таблице с
            // краем `max(0,18·r, мин. край)`. У Акварели ниже порога размытия край мягче, а
            // радиус больше на `taper/2`.
            let r = radius * (1.0 + taper * 0.5);
            let th = taper * 0.5;
            let e = (r * 0.18).max(edge_min);
            let e2 = e + (edge_min - e) * (th * 10.0).min(1.0);
            let cov_edge = th * r + edge_min;
            let solve_edge = r * th + e2;
            hard_solve = Some(((r + edge_min - cov_edge).max(0.0), solve_edge));
            if d >= 20.0 {
                // с 20 px: край — cos² на ширину решателя, сверху ограничен маской штриха
                let ds2 = ds * 0.2;
                mask = Some(((r - ds2).max(0.0) as f32, (edge_min + ds2) as f32, (mix * 0.85) as f32));
                ((r + edge_min - cov_edge).max(0.0), solve_edge, &t.cos2[0], &t.ramp[0][0])
            } else {
                hard = Some((mix * 0.85) as f32);
                ((r + edge_min - cov_edge).max(0.0), cov_edge, &t.ramp[0][0], &t.ramp[0][0])
            }
        } else {
            // у Акварели ниже порога размытия мягкость растёт к (s + 1) / 2
            let s = i.softness as f64;
            let s = s + ((s + 1.0) * 0.5 - s) * taper;
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
                let j = (((edge - edge_min) / edge_min * 8.0) as i32).min(7) as usize;
                // В ветке самой жёсткой ступени обычных инструментов (`flat`) образец по коду
                // берёт другую таблицу, но по измерению отпечатки 2–10 px и край линии 10 px
                // совпадают с самой выпуклой рампой (16) при обычной геометрии. У контура и
                // остальных ступеней — по формуле, точно.
                let k = if i.flat { 16 } else { ((mix * 16.0) as i32).min(16) as usize };
                (inner, edge, &t.ramp[k][j], &t.ramp[0][j])
            }
        };
        let (solve_inner, solve_edge) = hard_solve.unwrap_or((inner, edge));
        let solve_width = (target_p * 3.0 - small * 0.8).max(solve_edge);
        Dab {
            inner: inner as f32,
            edge: edge as f32,
            profile,
            thin: thin as f32,
            step_size: size as f32,
            step_diameter: d5 as f32,
            solve,
            solve_inner: solve_inner as f32,
            solve_width: solve_width as f32,
            target: target_p as f32,
            solve_target: if i.ver1 { target } else { target_p } as f32,
            amplify_gain: gain as f32,
            ver1: i.ver1,
            blot,
            hard,
            mask,
            amplify_mix: if a > 0.0 { target_p as f32 } else { 0.0 },
            ver1_scale: if i.ver1 { (i.density_factor as f64).max(0.0).powf(1.2) } else { 1.0 } as f32,
        }
    }

    pub fn outer(&self) -> f32 {
        self.inner + self.edge
    }

    /// Покрытие на расстоянии `dist` от центра (центр пикселя): значение таблицы без
    /// интерполяции, как в `dab_coverage_tile`.
    /// Радиус, внутри которого покрытие постоянно.
    pub fn inner_radius(&self) -> f32 {
        self.inner
    }

    pub fn coverage(&self, dist: f32) -> f32 {
        if let Some(a) = self.hard {
            // f = 1 - max(0, d - внутр.)/край, затем (f³ - f)·a + f
            let f = (1.0 - (dist - self.inner).max(0.0) / self.edge).max(0.0);
            return (f * f * f - f) * a + f;
        }
        if let Some(a) = self.blot {
            // как в SAI: f = 1 - d/край, затем (f³ - f)·a + f — в float
            let f = (1.0 - dist * (1.0 / self.edge)).max(0.0);
            return (f * f * f - f) * a + f;
        }
        let n = self.profile.len();
        let u = (dist.min(self.outer()) - self.inner).max(0.0);
        let idx = (u * ((n - 1) as f32 + 0.001) / self.edge) as usize;
        self.profile[idx.min(n - 1)]
    }

    /// Непрозрачность одного отпечатка при шаге `step` px (`dab_solve_opacity`), с ослаблением
    /// тонкой кисти.
    pub fn opacity(&self, step: f32) -> f32 {
        let target = self.solve_target as f64;
        if (self.target as f64) < 3.0517578125e-05 || target < 3.0517578125e-05 {
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
        let mut o = a * self.thin as f64;
        let gain = self.amplify_gain as f64;
        if self.ver1 {
            // «Ver1»: усиление прямо умножает непрозрачность
            o = (o * gain).min(1.0);
        } else if self.amplify_mix > 0.0 {
            // обычное правило: к усиленной непрозрачности в доле целевой плотности
            o += ((o * gain).min(1.0) - o) * self.amplify_mix as f64;
        }
        (o.min(1.0) * self.ver1_scale as f64) as f32
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
