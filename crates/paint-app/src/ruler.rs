//! Линейка, как в SAI 2 сборки 2017-01-19 (`docs/27-rulers.md`): прямая или эллипс.
//!
//! Пока линейка показана, каждая точка штриха кисти ложится на неё: у прямой — проекцией на
//! линию, у эллипса — по лучу из центра. Правится Ctrl + перетаскиванием:
//! - прямая: за полосу — сдвиг; с закреплённой осью — поворот вокруг оси;
//! - эллипс: внутри рамки — сдвиг, снаружи — поворот вокруг центра, стороны и углы рамки —
//!   размер (противоположная сторона стоит на месте).
//!
//! Все координаты — точки холста; размеры полосы и ручек заданы в точках экрана и делятся на
//! масштаб вида.

/// Ширина полосы прямой линейки и кольца эллипса на экране (замер по образцу: 50 и 18).
pub const BAND: f32 = 50.0;
pub const RING: f32 = 18.0;
/// Насколько близко к стороне или углу рамки эллипса надо попасть, точек экрана.
const GRIP: f32 = 8.0;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Straight,
    Ellipse,
}

/// За что взялись при правке.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Grip {
    Move,
    Rotate,
    /// Сторона или угол рамки эллипса: −1 / 0 / +1 по местным осям.
    Side(i8, i8),
}

#[derive(Clone, Copy, Debug)]
pub struct Ruler {
    pub kind: Kind,
    pub visible: bool,
    /// «Не замечать линейку»: показана, но штрих к ней не прижимается.
    pub ignore: bool,
    /// «Закрепить ось»: Ctrl + перетаскивание прямой поворачивает её вокруг оси.
    pub lock_pivot: bool,
    /// Ещё не ставилась на этот холст — поставить посреди вида при показе.
    pub placed: bool,
    /// Прямая: ось (точка на линии) и угол линии; полоса лежит по нормали (−sin, cos).
    pub pivot: [f32; 2],
    pub angle: f32,
    /// Эллипс: центр, полуоси (до внешнего края кольца) и наклон.
    pub center: [f32; 2],
    pub rx: f32,
    pub ry: f32,
    pub tilt: f32,
}

impl Default for Ruler {
    fn default() -> Self {
        Self {
            kind: Kind::Straight,
            visible: false,
            ignore: false,
            lock_pivot: false,
            placed: false,
            pivot: [0.0; 2],
            angle: 0.0,
            center: [0.0; 2],
            rx: 100.0,
            ry: 100.0,
            tilt: 0.0,
        }
    }
}

fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

/// Повернуть вектор на угол `a`.
fn rot(v: [f32; 2], a: f32) -> [f32; 2] {
    let (s, c) = a.sin_cos();
    [c * v[0] - s * v[1], s * v[0] + c * v[1]]
}

impl Ruler {
    /// Поставить посреди вида: прямую — по горизонтали через середину, эллипс — кругом
    /// диаметром около половины меньшей стороны вида (`view_min` — в точках холста).
    pub fn reset(&mut self, view_center: [f32; 2], view_min: f32) {
        self.pivot = view_center;
        self.angle = 0.0;
        self.center = view_center;
        self.rx = (view_min * 0.24).max(4.0);
        self.ry = self.rx;
        self.tilt = 0.0;
        self.lock_pivot = false;
        self.placed = true;
    }

    /// Прижимать ли сейчас штрих к линейке.
    pub fn active(&self) -> bool {
        self.visible && !self.ignore && self.placed
    }

    /// Точка штриха на линейке.
    pub fn snap(&self, p: [f32; 2]) -> [f32; 2] {
        match self.kind {
            Kind::Straight => {
                let (s, c) = self.angle.sin_cos();
                let d = sub(p, self.pivot);
                let t = d[0] * c + d[1] * s;
                [self.pivot[0] + t * c, self.pivot[1] + t * s]
            }
            Kind::Ellipse => {
                let l = rot(sub(p, self.center), -self.tilt);
                let u = [l[0] / self.rx, l[1] / self.ry];
                let n = (u[0] * u[0] + u[1] * u[1]).sqrt();
                if n < 1e-6 {
                    return p;
                }
                let e = rot([u[0] / n * self.rx, u[1] / n * self.ry], self.tilt);
                [self.center[0] + e[0], self.center[1] + e[1]]
            }
        }
    }

    /// Что возьмёт Ctrl + нажатие в точке `p` при масштабе вида `zoom` (`None` — мимо линейки:
    /// тогда Ctrl + перетаскивание, как обычно, двигает слой).
    pub fn hit(&self, p: [f32; 2], zoom: f32) -> Option<Grip> {
        if !self.visible || !self.placed {
            return None;
        }
        match self.kind {
            Kind::Straight => {
                let (s, c) = self.angle.sin_cos();
                let d = sub(p, self.pivot);
                let across = (-s * d[0] + c * d[1]) * zoom;
                (-3.0..=BAND).contains(&across).then_some(if self.lock_pivot { Grip::Rotate } else { Grip::Move })
            }
            Kind::Ellipse => {
                let l = rot(sub(p, self.center), -self.tilt);
                let tol = GRIP / zoom;
                let side = |v: f32, r: f32| -> Option<i8> {
                    if (v - r).abs() < tol {
                        Some(1)
                    } else if (v + r).abs() < tol {
                        Some(-1)
                    } else {
                        None
                    }
                };
                let (inx, iny) = (l[0].abs() <= self.rx + tol, l[1].abs() <= self.ry + tol);
                match (side(l[0], self.rx), side(l[1], self.ry)) {
                    (Some(sx), Some(sy)) => Some(Grip::Side(sx, sy)),
                    (Some(sx), None) if iny => Some(Grip::Side(sx, 0)),
                    (None, Some(sy)) if inx => Some(Grip::Side(0, sy)),
                    _ if inx && iny => Some(Grip::Move),
                    _ => Some(Grip::Rotate),
                }
            }
        }
    }

    /// Перетащить взятое из `from` в `to`.
    pub fn drag(&mut self, grip: Grip, from: [f32; 2], to: [f32; 2]) {
        let d = sub(to, from);
        match (self.kind, grip) {
            (Kind::Straight, Grip::Move) => {
                self.pivot = [self.pivot[0] + d[0], self.pivot[1] + d[1]];
            }
            (Kind::Straight, Grip::Rotate) => {
                self.angle += angle_between(self.pivot, from, to);
            }
            (Kind::Ellipse, Grip::Move) => {
                self.center = [self.center[0] + d[0], self.center[1] + d[1]];
            }
            (Kind::Ellipse, Grip::Rotate) => {
                self.tilt += angle_between(self.center, from, to);
            }
            (Kind::Ellipse, Grip::Side(sx, sy)) => {
                // в местных осях: тянется одна сторона, противоположная стоит
                let l = rot(d, -self.tilt);
                let (mut dx, mut dy) = (l[0] * sx as f32, l[1] * sy as f32);
                // не меньше 1 точки в радиусе
                dx = dx.max(2.0 - 2.0 * self.rx);
                dy = dy.max(2.0 - 2.0 * self.ry);
                self.rx += dx / 2.0;
                self.ry += dy / 2.0;
                let shift = rot([dx / 2.0 * sx as f32, dy / 2.0 * sy as f32], self.tilt);
                self.center = [self.center[0] + shift[0], self.center[1] + shift[1]];
            }
            (Kind::Straight, Grip::Side(..)) => {}
        }
    }
}

/// Угол поворота вокруг `c` от направления на `a` к направлению на `b`.
fn angle_between(c: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (va, vb) = (sub(a, c), sub(b, c));
    if va[0].hypot(va[1]) < 1e-3 || vb[0].hypot(vb[1]) < 1e-3 {
        return 0.0;
    }
    let mut t = vb[1].atan2(vb[0]) - va[1].atan2(va[0]);
    while t > std::f32::consts::PI {
        t -= std::f32::consts::TAU;
    }
    while t < -std::f32::consts::PI {
        t += std::f32::consts::TAU;
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    fn shown(kind: Kind) -> Ruler {
        let mut r = Ruler { kind, visible: true, ..Default::default() };
        r.reset([500.0, 400.0], 800.0);
        r
    }

    /// Как в образце: любой штрих, где бы его ни начали, ложится на саму линию.
    #[test]
    fn straight_projects_onto_the_line() {
        let mut r = shown(Kind::Straight);
        assert!(close(r.snap([100.0, 50.0]), [100.0, 400.0]));
        assert!(close(r.snap([700.0, 900.0]), [700.0, 400.0]));
        r.angle = std::f32::consts::FRAC_PI_4;
        let p = r.snap([600.0, 400.0]);
        assert!(close(p, [550.0, 450.0]), "{p:?}");
    }

    #[test]
    fn ellipse_projects_along_the_ray_from_the_centre() {
        let mut r = shown(Kind::Ellipse);
        r.rx = 200.0;
        r.ry = 100.0;
        assert!(close(r.snap([900.0, 400.0]), [700.0, 400.0]));
        assert!(close(r.snap([500.0, 410.0]), [500.0, 500.0]));
        // наклон 90°: большая ось вертикальна
        r.tilt = std::f32::consts::FRAC_PI_2;
        let p = r.snap([500.0, 0.0]);
        assert!(close(p, [500.0, 200.0]), "{p:?}");
    }

    /// Правка как в образце: полоса прямой — сдвиг (с закреплённой осью — поворот), мимо —
    /// ничего; у эллипса сторона рамки тянет одну сторону, внутри — сдвиг, снаружи — поворот.
    #[test]
    fn grips_follow_the_reference() {
        let mut s = shown(Kind::Straight);
        assert_eq!(s.hit([300.0, 420.0], 1.0), Some(Grip::Move));
        assert_eq!(s.hit([300.0, 300.0], 1.0), None);
        s.drag(Grip::Move, [300.0, 420.0], [300.0, 520.0]);
        assert!(close(s.pivot, [500.0, 500.0]));
        s.lock_pivot = true;
        assert_eq!(s.hit([600.0, 510.0], 1.0), Some(Grip::Rotate));
        s.drag(Grip::Rotate, [600.0, 500.0], [500.0, 600.0]);
        assert!((s.angle - std::f32::consts::FRAC_PI_2).abs() < 1e-4);

        let mut e = shown(Kind::Ellipse);
        let r0 = e.rx;
        assert_eq!(e.hit([500.0 + r0, 400.0], 1.0), Some(Grip::Side(1, 0)));
        assert_eq!(e.hit([500.0 + r0, 400.0 + r0], 1.0), Some(Grip::Side(1, 1)));
        assert_eq!(e.hit([520.0, 430.0], 1.0), Some(Grip::Move));
        assert_eq!(e.hit([500.0 + r0 + 50.0, 400.0 - r0 - 50.0], 1.0), Some(Grip::Rotate));
        e.drag(Grip::Side(1, 0), [500.0 + r0, 400.0], [500.0 + r0 + 100.0, 400.0]);
        assert!((e.rx - (r0 + 50.0)).abs() < 1e-3 && (e.ry - r0).abs() < 1e-3);
        assert!(close(e.center, [550.0, 400.0]), "левая сторона стоит: {:?}", e.center);
        // сторону не протащить за противоположную
        e.drag(Grip::Side(-1, 0), [0.0, 0.0], [5000.0, 0.0]);
        assert!(e.rx >= 1.0);
    }
}
