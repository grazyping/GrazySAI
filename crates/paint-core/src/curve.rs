//! Кривая нажима пера: монотонная кубическая кривая (Фритч — Карлсон) через (0, 0), две точки
//! пользователя и (1, 1). Не убывает — сильнее нажим, не меньше отклик; без выбросов за 0…1.

/// Две внутренние точки кривой; по умолчанию — прямая (нажим как есть).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PressureCurve {
    pub points: [[f32; 2]; 2],
}

impl Default for PressureCurve {
    fn default() -> Self {
        Self { points: [[1.0 / 3.0, 1.0 / 3.0], [2.0 / 3.0, 2.0 / 3.0]] }
    }
}

impl PressureCurve {
    pub fn is_linear(&self) -> bool {
        self.points.iter().all(|p| (p[0] - p[1]).abs() < 1e-4)
    }

    /// Узлы по возрастанию x, точки внутри (0, 1) и не ближе 0,02 друг к другу и к концам.
    fn knots(&self) -> [[f32; 2]; 4] {
        let mut a = self.points[0];
        let mut b = self.points[1];
        if a[0] > b[0] {
            std::mem::swap(&mut a, &mut b);
        }
        a[0] = a[0].clamp(0.02, 0.96);
        b[0] = b[0].clamp(a[0] + 0.02, 0.98);
        a[1] = a[1].clamp(0.0, 1.0);
        b[1] = b[1].clamp(0.0, 1.0);
        // монотонность: значения не убывают
        let ya = a[1].min(b[1]);
        let yb = a[1].max(b[1]);
        [[0.0, 0.0], [a[0], ya], [b[0], yb], [1.0, 1.0]]
    }

    pub fn apply(&self, p: f32) -> f32 {
        if self.is_linear() {
            return p.clamp(0.0, 1.0);
        }
        let k = self.knots();
        let x = p.clamp(0.0, 1.0);
        // наклоны отрезков и касательные по Фритчу — Карлсону
        let d: Vec<f32> = (0..3).map(|i| (k[i + 1][1] - k[i][1]) / (k[i + 1][0] - k[i][0])).collect();
        let mut m = [d[0], 0.0, 0.0, d[2]];
        for i in 1..3 {
            m[i] = if d[i - 1] * d[i] <= 0.0 { 0.0 } else { (d[i - 1] + d[i]) / 2.0 };
        }
        for i in 0..3 {
            if d[i] == 0.0 {
                m[i] = 0.0;
                m[i + 1] = 0.0;
            } else {
                let (a, b) = (m[i] / d[i], m[i + 1] / d[i]);
                let s = a * a + b * b;
                if s > 9.0 {
                    let t = 3.0 / s.sqrt();
                    m[i] = t * a * d[i];
                    m[i + 1] = t * b * d[i];
                }
            }
        }
        let i = (0..3).find(|&i| x <= k[i + 1][0]).unwrap_or(2);
        let h = k[i + 1][0] - k[i][0];
        let t = (x - k[i][0]) / h;
        let (t2, t3) = (t * t, t * t * t);
        let y = (2.0 * t3 - 3.0 * t2 + 1.0) * k[i][1] + (t3 - 2.0 * t2 + t) * h * m[i] + (-2.0 * t3 + 3.0 * t2) * k[i + 1][1] + (t3 - t2) * h * m[i + 1];
        y.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_identity() {
        let c = PressureCurve::default();
        for i in 0..=10 {
            let p = i as f32 / 10.0;
            assert!((c.apply(p) - p).abs() < 1e-6);
        }
    }

    #[test]
    fn soft_curve_is_monotone_and_passes_points() {
        let c = PressureCurve { points: [[0.25, 0.5], [0.6, 0.85]] };
        assert!((c.apply(0.25) - 0.5).abs() < 1e-4 && (c.apply(0.6) - 0.85).abs() < 1e-4);
        assert_eq!((c.apply(0.0), c.apply(1.0)), (0.0, 1.0));
        let mut prev = 0.0;
        for i in 0..=1000 {
            let y = c.apply(i as f32 / 1000.0);
            assert!(y + 1e-6 >= prev && (0.0..=1.0).contains(&y), "{i}: {y} < {prev}");
            prev = y;
        }
    }

    #[test]
    fn points_out_of_order_or_falling_stay_monotone() {
        let c = PressureCurve { points: [[0.8, 0.2], [0.2, 0.9]] };
        let mut prev = 0.0;
        for i in 0..=200 {
            let y = c.apply(i as f32 / 200.0);
            assert!(y + 1e-6 >= prev);
            prev = y;
        }
    }
}
