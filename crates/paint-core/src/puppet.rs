//! Кукольная анимация как Puppet в After Effects: по силуэту рисунка слоя (с запасом
//! «Expansion») строится сетка треугольников («Density» — их размер), булавки двигают её,
//! картинка тянется за сеткой. Деформация — «как можно жёстче» (As-Rigid-As-Possible: Sorkine и
//! Alexa, 2007 — по очереди лучший поворот у каждой вершины и общий решатель положений).
//!
//! Булавки: Position — точка, Starch — жёсткость рядом (Amount, Extent), Bend — поворот и
//! размер вокруг себя (положение считается само), Advanced — точка, поворот и размер, Overlap —
//! что поверх при наложении частей (In Front, Extent). У булавок — метки по кадрам таймлайна.

use crate::pixel::{from_f32, to_f32};
use crate::tile::{ColorMap, TILE};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinKind {
    Position,
    Starch,
    Bend,
    Advanced,
    Overlap,
}

impl PinKind {
    pub const ALL: [PinKind; 5] = [PinKind::Position, PinKind::Starch, PinKind::Bend, PinKind::Advanced, PinKind::Overlap];

    fn key(self) -> &'static str {
        match self {
            PinKind::Position => "position",
            PinKind::Starch => "starch",
            PinKind::Bend => "bend",
            PinKind::Advanced => "advanced",
            PinKind::Overlap => "overlap",
        }
    }

    /// Булавка держит точку (Position, Advanced).
    pub fn holds_position(self) -> bool {
        matches!(self, PinKind::Position | PinKind::Advanced)
    }

    /// Булавка задаёт поворот и размер вокруг себя (Bend, Advanced).
    pub fn turns(self) -> bool {
        matches!(self, PinKind::Bend | PinKind::Advanced)
    }
}

/// Положение булавки в кадре: точка холста, поворот (радианы), размер.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PinPose {
    pub pos: [f32; 2],
    pub rot: f32,
    pub scale: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pin {
    pub kind: PinKind,
    /// Место на исходной картинке.
    pub at: [f32; 2],
    /// Starch: сила жёсткости (складывается у соседних булавок).
    pub amount: f32,
    /// Starch, Overlap, Bend, Advanced: радиус влияния, точки.
    pub extent: f32,
    /// Overlap: насколько ближе к зрителю.
    pub in_front: f32,
    /// Метки по кадрам; без меток — на месте.
    pub keys: BTreeMap<u32, PinPose>,
}

impl Pin {
    pub fn new(kind: PinKind, at: [f32; 2]) -> Pin {
        Pin { kind, at, amount: 0.5, extent: 40.0, in_front: 50.0, keys: BTreeMap::new() }
    }

    fn rest(&self) -> PinPose {
        PinPose { pos: self.at, rot: 0.0, scale: 1.0 }
    }

    /// Положение в кадре `f`: между метками — по прямой, до первой и после последней — как у них.
    pub fn pose(&self, f: u32) -> PinPose {
        let before = self.keys.range(..=f).next_back();
        let after = self.keys.range(f..).next();
        match (before, after) {
            (None, None) => self.rest(),
            (Some((_, p)), None) | (None, Some((_, p))) => *p,
            (Some((&a, pa)), Some((&b, pb))) => {
                if a == b {
                    return *pa;
                }
                let t = (f - a) as f32 / (b - a) as f32;
                let l = |x: f32, y: f32| x + (y - x) * t;
                PinPose { pos: [l(pa.pos[0], pb.pos[0]), l(pa.pos[1], pb.pos[1])], rot: l(pa.rot, pb.rot), scale: l(pa.scale, pb.scale) }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Puppet {
    /// Исходный рисунок слоя (без деформации).
    pub source: ColorMap,
    /// Запас вокруг силуэта, точки.
    pub expansion: f32,
    /// Густота сетки 1–10.
    pub density: u8,
    pub pins: Vec<Pin>,
    /// Готовая сетка, исходник плотным массивом и прошлые положения вершин (не сохраняется).
    pub cache: Cache,
}

/// Исходник плотным массивом: левый верхний угол, ширина, высота, точки.
type Dense = (i32, i32, usize, usize, Vec<[f32; 4]>);
/// Булавка, привязанная к вершинам (номер и доля), и её точка в кадре.
type Hold = (Vec<(u32, f32)>, [f32; 2]);

pub struct CacheData {
    key: (u32, u8),
    mesh: std::sync::Arc<Mesh>,
    /// Исходник: левый верхний угол, ширина, высота, точки (доли единицы, предумноженные).
    src: std::sync::Arc<Dense>,
    last: Option<Vec<[f32; 2]>>,
}

/// Кэш для живой правки; копия начинается пустой, на сравнение не влияет.
#[derive(Default)]
pub struct Cache(std::sync::Mutex<Option<CacheData>>);

impl Clone for Cache {
    fn clone(&self) -> Self {
        Cache::default()
    }
}

impl PartialEq for Cache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl std::fmt::Debug for Cache {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("Cache")
    }
}

/// Сетка: вершины в покое, треугольники, рёбра.
pub struct Mesh {
    pub rest: Vec<[f32; 2]>,
    pub tris: Vec<[u32; 3]>,
    pub edges: Vec<(u32, u32)>,
}

/// Квадрат с рисунком: (x0, y0, x1, y1) включительно.
fn bounds(src: &ColorMap) -> Option<(i32, i32, i32, i32)> {
    let mut b: Option<(i32, i32, i32, i32)> = None;
    for (&(tx, ty), t) in &src.tiles {
        for (i, p) in t.0.iter().enumerate() {
            if p[3] == 0 {
                continue;
            }
            let (x, y) = (tx * TILE + i as i32 % TILE, ty * TILE + i as i32 / TILE);
            b = Some(match b {
                None => (x, y, x, y),
                Some((a, c, d, e)) => (a.min(x), c.min(y), d.max(x), e.max(y)),
            });
        }
    }
    b
}

impl Puppet {
    pub fn new(source: ColorMap) -> Puppet {
        Puppet { source, expansion: 3.0, density: 5, pins: Vec::new(), cache: Cache::default() }
    }

    fn cell(&self) -> i32 {
        [32, 26, 22, 18, 15, 12, 10, 8, 7, 6][(self.density.clamp(1, 10) - 1) as usize]
    }

    /// Сетка по силуэту: клетка входит, если в ней есть точка рисунка (с запасом).
    pub fn mesh(&self) -> Mesh {
        let empty = Mesh { rest: Vec::new(), tris: Vec::new(), edges: Vec::new() };
        let Some((x0, y0, x1, y1)) = bounds(&self.source) else { return empty };
        let e = self.expansion.clamp(0.0, 64.0).ceil() as i32;
        let c = self.cell();
        let (ox, oy) = (x0 - e, y0 - e);
        let (w, h) = ((x1 - x0 + 1 + 2 * e) as usize, (y1 - y0 + 1 + 2 * e) as usize);
        if w * h > 64 << 20 {
            return empty;
        }
        // силуэт с запасом: расширение квадратом (по строкам, затем по столбцам)
        let mut m: Vec<bool> = (0..w * h).map(|i| self.source.get(ox + (i % w) as i32, oy + (i / w) as i32)[3] > 0).collect();
        if e > 0 {
            let mut tmp = vec![false; w * h];
            for y in 0..h {
                let mut last: i64 = -(1 << 40);
                for x in 0..w {
                    if m[y * w + x] {
                        last = x as i64;
                    }
                    tmp[y * w + x] = x as i64 - last <= e as i64;
                }
                let mut next: i64 = 1 << 40;
                for x in (0..w).rev() {
                    if m[y * w + x] {
                        next = x as i64;
                    }
                    tmp[y * w + x] |= next - x as i64 <= e as i64;
                }
            }
            for x in 0..w {
                let mut last: i64 = -(1 << 40);
                for y in 0..h {
                    if tmp[y * w + x] {
                        last = y as i64;
                    }
                    m[y * w + x] = y as i64 - last <= e as i64;
                }
                let mut next: i64 = 1 << 40;
                for y in (0..h).rev() {
                    if tmp[y * w + x] {
                        next = y as i64;
                    }
                    m[y * w + x] |= next - y as i64 <= e as i64;
                }
            }
        }
        let (gw, gh) = ((w as i32 + c - 1) / c, (h as i32 + c - 1) / c);
        let mut active = vec![false; (gw * gh) as usize];
        for y in 0..h {
            for x in 0..w {
                if m[y * w + x] {
                    active[(y as i32 / c * gw + x as i32 / c) as usize] = true;
                }
            }
        }
        let mut vid: HashMap<(i32, i32), u32> = HashMap::new();
        let mut rest = Vec::new();
        let mut v = |gx: i32, gy: i32, rest: &mut Vec<[f32; 2]>| -> u32 {
            *vid.entry((gx, gy)).or_insert_with(|| {
                rest.push([(ox + gx * c) as f32, (oy + gy * c) as f32]);
                rest.len() as u32 - 1
            })
        };
        let mut tris = Vec::new();
        for gy in 0..gh {
            for gx in 0..gw {
                if !active[(gy * gw + gx) as usize] {
                    continue;
                }
                let a = v(gx, gy, &mut rest);
                let b = v(gx + 1, gy, &mut rest);
                let cc = v(gx + 1, gy + 1, &mut rest);
                let d = v(gx, gy + 1, &mut rest);
                tris.push([a, b, cc]);
                tris.push([a, cc, d]);
            }
        }
        let mut edges: Vec<(u32, u32)> = tris.iter().flat_map(|t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])]).map(|(a, b)| (a.min(b), a.max(b))).collect();
        edges.sort_unstable();
        edges.dedup();
        Mesh { rest, tris, edges }
    }

    /// Положения вершин сетки в кадре `f`; `warm` — прошлые положения (быстрее сходится).
    pub fn deform(&self, mesh: &Mesh, f: u32, warm: Option<&[[f32; 2]]>) -> Vec<[f32; 2]> {
        let n = mesh.rest.len();
        let rest = &mesh.rest;
        // булавка → вершины и доли (треугольник покоя, в котором она стоит, или ближняя вершина)
        let attach = |at: [f32; 2]| -> Vec<(u32, f32)> {
            for t in &mesh.tris {
                if let Some(b) = barycentric(at, rest[t[0] as usize], rest[t[1] as usize], rest[t[2] as usize]) {
                    if b.iter().all(|v| *v >= -1e-4) {
                        return vec![(t[0], b[0]), (t[1], b[1]), (t[2], b[2])];
                    }
                }
            }
            let best = (0..n).min_by(|&a, &b| dist2(rest[a], at).total_cmp(&dist2(rest[b], at)));
            best.map(|i| vec![(i as u32, 1.0)]).unwrap_or_default()
        };
        let holds: Vec<Hold> = self.pins.iter().filter(|p| p.kind.holds_position()).map(|p| (attach(p.at), p.pose(f).pos)).filter(|(a, _)| !a.is_empty()).collect();
        if n == 0 || holds.is_empty() {
            return rest.clone();
        }
        // повороты, заданные булавками Bend и Advanced (у вершин в их радиусе)
        let mut forced: Vec<Option<(f32, f32)>> = vec![None; n];
        for p in self.pins.iter().filter(|p| p.kind.turns()) {
            let pose = p.pose(f);
            let r2 = p.extent.max(1.0).powi(2);
            for (i, v) in rest.iter().enumerate() {
                if dist2(*v, p.at) <= r2 {
                    forced[i] = Some((pose.rot, pose.scale.max(0.01)));
                }
            }
        }
        let turned = forced.iter().any(Option::is_some);
        if holds.len() == 1 && !turned {
            // одна булавка — просто сдвиг
            let (a, target) = &holds[0];
            let now = a.iter().fold([0.0f32; 2], |s, (i, w)| [s[0] + rest[*i as usize][0] * w, s[1] + rest[*i as usize][1] * w]);
            let d = [target[0] - now[0], target[1] - now[1]];
            return rest.iter().map(|v| [v[0] + d[0], v[1] + d[1]]).collect();
        }
        // веса рёбер: жёстче у булавок Starch
        let weights: Vec<f32> = mesh
            .edges
            .iter()
            .map(|&(a, b)| {
                let mid = [(rest[a as usize][0] + rest[b as usize][0]) / 2.0, (rest[a as usize][1] + rest[b as usize][1]) / 2.0];
                let s: f32 = self.pins.iter().filter(|p| p.kind == PinKind::Starch && dist2(mid, p.at) <= p.extent.max(1.0).powi(2)).map(|p| p.amount).sum();
                (1.0 + s * 10.0).max(0.05)
            })
            .collect();
        let mut nb: Vec<Vec<(u32, f32)>> = vec![Vec::new(); n];
        for (k, &(a, b)) in mesh.edges.iter().enumerate() {
            nb[a as usize].push((b, weights[k]));
            nb[b as usize].push((a, weights[k]));
        }
        let lambda = 1e3f32;
        let mut p: Vec<[f32; 2]> = match warm.filter(|w| w.len() == n) {
            Some(w) => w.to_vec(),
            None => {
                // начало — средний сдвиг булавок
                let mut d = [0.0f32; 2];
                for (a, t) in &holds {
                    let now = a.iter().fold([0.0f32; 2], |s, (i, w)| [s[0] + rest[*i as usize][0] * w, s[1] + rest[*i as usize][1] * w]);
                    d[0] += (t[0] - now[0]) / holds.len() as f32;
                    d[1] += (t[1] - now[1]) / holds.len() as f32;
                }
                rest.iter().map(|v| [v[0] + d[0], v[1] + d[1]]).collect()
            }
        };
        // диагональ матрицы (для предобуславливателя)
        let mut diag: Vec<f32> = (0..n).map(|i| nb[i].iter().map(|x| x.1).sum::<f32>()).collect();
        for (a, _) in &holds {
            for (i, w) in a {
                diag[*i as usize] += lambda * w * w;
            }
        }
        let apply = |x: &[f32], out: &mut [f32]| {
            for i in 0..n {
                let mut s = 0.0;
                for &(j, w) in &nb[i] {
                    s += w * (x[i] - x[j as usize]);
                }
                out[i] = s;
            }
            for (a, _) in &holds {
                let r: f32 = a.iter().map(|(i, w)| w * x[*i as usize]).sum();
                for (i, w) in a {
                    out[*i as usize] += lambda * w * r;
                }
            }
        };
        let mut rot = vec![(1.0f32, 0.0f32); n];
        let iters = if warm.is_some_and(|w| w.len() == n) { 12 } else { 40 };
        for _ in 0..iters {
            // местный шаг: лучший поворот у каждой вершины
            for i in 0..n {
                if let Some((a, s)) = forced[i] {
                    rot[i] = (a.cos() * s, a.sin() * s);
                    continue;
                }
                let (mut sc, mut ss) = (0.0f32, 0.0f32);
                for &(j, w) in &nb[i] {
                    let e = [rest[i][0] - rest[j as usize][0], rest[i][1] - rest[j as usize][1]];
                    let q = [p[i][0] - p[j as usize][0], p[i][1] - p[j as usize][1]];
                    sc += w * (e[0] * q[0] + e[1] * q[1]);
                    ss += w * (e[0] * q[1] - e[1] * q[0]);
                }
                let a = ss.atan2(sc);
                rot[i] = (a.cos(), a.sin());
            }
            // общий шаг: L p = b по x и по y
            for axis in 0..2 {
                let mut b = vec![0.0f32; n];
                for i in 0..n {
                    for &(j, w) in &nb[i] {
                        let j = j as usize;
                        let e = [rest[i][0] - rest[j][0], rest[i][1] - rest[j][1]];
                        let (c1, s1) = rot[i];
                        let (c2, s2) = rot[j];
                        let (c, s) = ((c1 + c2) / 2.0, (s1 + s2) / 2.0);
                        let re = if axis == 0 { c * e[0] - s * e[1] } else { s * e[0] + c * e[1] };
                        b[i] += w * re;
                    }
                }
                for (a, t) in &holds {
                    for (i, w) in a {
                        b[*i as usize] += lambda * w * t[axis];
                    }
                }
                let mut x: Vec<f32> = p.iter().map(|v| v[axis]).collect();
                cg(&apply, &diag, &b, &mut x, if iters < 40 { 25 } else { 60 });
                for (v, xv) in p.iter_mut().zip(&x) {
                    v[axis] = *xv;
                }
            }
        }
        p
    }

    /// Картинка кадра `f`: треугольники источника по положениям `pos`, порядок — по Overlap.
    pub fn render(&self, mesh: &Mesh, pos: &[[f32; 2]]) -> ColorMap {
        let mut out = ColorMap::new();
        let depth = |t: &[u32; 3]| -> f32 {
            let c = [(mesh.rest[t[0] as usize][0] + mesh.rest[t[1] as usize][0] + mesh.rest[t[2] as usize][0]) / 3.0, (mesh.rest[t[0] as usize][1] + mesh.rest[t[1] as usize][1] + mesh.rest[t[2] as usize][1]) / 3.0];
            self.pins.iter().filter(|p| p.kind == PinKind::Overlap && dist2(c, p.at) <= p.extent.max(1.0).powi(2)).map(|p| p.in_front).sum()
        };
        let mut order: Vec<(f32, usize)> = mesh.tris.iter().enumerate().map(|(k, t)| (depth(t), k)).collect();
        order.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, k) in order {
            let t = mesh.tris[k];
            let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
            let (ra, rb, rc) = (mesh.rest[t[0] as usize], mesh.rest[t[1] as usize], mesh.rest[t[2] as usize]);
            let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            if area.abs() < 1e-6 || !area.is_finite() {
                continue;
            }
            let x0 = a[0].min(b[0]).min(c[0]).floor().max(-1e6) as i32;
            let x1 = a[0].max(b[0]).max(c[0]).ceil().min(1e6) as i32;
            let y0 = a[1].min(b[1]).min(c[1]).floor().max(-1e6) as i32;
            let y1 = a[1].max(b[1]).max(c[1]).ceil().min(1e6) as i32;
            if (x1 - x0) as i64 * (y1 - y0) as i64 > 1 << 26 {
                continue;
            }
            // правило «верх-лево»: точка на общем ребре достаётся одному треугольнику
            let edge = |p: [f32; 2], q: [f32; 2], x: f32, y: f32| (q[0] - p[0]) * (y - p[1]) - (q[1] - p[1]) * (x - p[0]);
            let top_left = |p: [f32; 2], q: [f32; 2]| {
                let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
                let (dx, dy) = if area > 0.0 { (dx, dy) } else { (-dx, -dy) };
                (dy == 0.0 && dx < 0.0) || dy > 0.0
            };
            let tl = [top_left(b, c), top_left(c, a), top_left(a, b)];
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    let w = [edge(b, c, px, py) / area, edge(c, a, px, py) / area, edge(a, b, px, py) / area];
                    if (0..3).any(|k| w[k] < 0.0 || (w[k] == 0.0 && !tl[k])) {
                        continue;
                    }
                    let sx = ra[0] * w[0] + rb[0] * w[1] + rc[0] * w[2];
                    let sy = ra[1] * w[0] + rb[1] * w[1] + rc[1] * w[2];
                    let s = sample(&self.source, sx, sy);
                    if s[3] <= 0.0 {
                        continue;
                    }
                    let d = to_f32(out.get(x, y));
                    let o = [s[0] + d[0] * (1.0 - s[3]), s[1] + d[1] * (1.0 - s[3]), s[2] + d[2] * (1.0 - s[3]), s[3] + d[3] * (1.0 - s[3])];
                    out.set(x, y, from_f32(o));
                }
            }
        }
        out.tiles.retain(|_, t| t.0.iter().any(|p| p[3] != 0));
        out
    }

    /// Пиксели кадра `f`.
    pub fn frame(&self, f: u32) -> ColorMap {
        let key = (self.expansion.to_bits(), self.density);
        let mut g = self.cache.0.lock().unwrap_or_else(|e| e.into_inner());
        if g.as_ref().is_none_or(|c| c.key != key) {
            let src = match bounds(&self.source) {
                Some((x0, y0, x1, y1)) => {
                    let (w, h) = ((x1 - x0 + 1) as usize, (y1 - y0 + 1) as usize);
                    let mut v = vec![[0.0f32; 4]; w * h];
                    for (i, p) in v.iter_mut().enumerate() {
                        *p = to_f32(self.source.get(x0 + (i % w) as i32, y0 + (i / w) as i32));
                    }
                    (x0, y0, w, h, v)
                }
                None => (0, 0, 0, 0, Vec::new()),
            };
            *g = Some(CacheData { key, mesh: std::sync::Arc::new(self.mesh()), src: std::sync::Arc::new(src), last: None });
        }
        let c = g.as_mut().unwrap();
        let pos = self.deform(&c.mesh, f, c.last.as_deref());
        c.last = Some(pos.clone());
        let (mesh, src) = (c.mesh.clone(), c.src.clone());
        drop(g);
        self.render_dense(&mesh, &pos, &src)
    }

    /// Сетка и положения вершин последнего посчитанного кадра (для показа сетки).
    pub fn last_mesh(&self) -> Option<(std::sync::Arc<Mesh>, Vec<[f32; 2]>)> {
        let g = self.cache.0.lock().unwrap_or_else(|e| e.into_inner());
        let c = g.as_ref()?;
        Some((c.mesh.clone(), c.last.clone()?))
    }

    /// Как `render`, но через плотные массивы (быстрее): исходник `src` — из кэша.
    fn render_dense(&self, mesh: &Mesh, pos: &[[f32; 2]], src: &Dense) -> ColorMap {
        let (sx0, sy0, sw, sh, sv) = src;
        let mut out = ColorMap::new();
        if pos.is_empty() || sv.is_empty() {
            return out;
        }
        let fin = |v: f32| v.is_finite();
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in pos.iter().filter(|p| fin(p[0]) && fin(p[1])) {
            x0 = x0.min(p[0]);
            y0 = y0.min(p[1]);
            x1 = x1.max(p[0]);
            y1 = y1.max(p[1]);
        }
        let (bx0, by0) = (x0.floor().max(-1e6) as i32, y0.floor().max(-1e6) as i32);
        let (bw, bh) = ((x1.ceil().min(1e6) as i32 - bx0 + 1).max(1) as usize, (y1.ceil().min(1e6) as i32 - by0 + 1).max(1) as usize);
        if bw * bh > 256 << 20 {
            return out;
        }
        let mut buf = vec![[0.0f32; 4]; bw * bh];
        let sample = |x: f32, y: f32| -> [f32; 4] {
            let (x, y) = (x - 0.5 - *sx0 as f32, y - 0.5 - *sy0 as f32);
            let (xf, yf) = (x.floor(), y.floor());
            let (fx, fy) = (x - xf, y - yf);
            let at = |dx: i32, dy: i32| -> [f32; 4] {
                let (xi, yi) = (xf as i32 + dx, yf as i32 + dy);
                if xi < 0 || yi < 0 || xi >= *sw as i32 || yi >= *sh as i32 {
                    [0.0; 4]
                } else {
                    sv[yi as usize * sw + xi as usize]
                }
            };
            let (a, b, c, d) = (at(0, 0), at(1, 0), at(0, 1), at(1, 1));
            let mut o = [0.0f32; 4];
            for k in 0..4 {
                o[k] = (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy;
            }
            o
        };
        let depth = |t: &[u32; 3]| -> f32 {
            let c = [(mesh.rest[t[0] as usize][0] + mesh.rest[t[1] as usize][0] + mesh.rest[t[2] as usize][0]) / 3.0, (mesh.rest[t[0] as usize][1] + mesh.rest[t[1] as usize][1] + mesh.rest[t[2] as usize][1]) / 3.0];
            self.pins.iter().filter(|p| p.kind == PinKind::Overlap && dist2(c, p.at) <= p.extent.max(1.0).powi(2)).map(|p| p.in_front).sum()
        };
        let mut order: Vec<(f32, usize)> = mesh.tris.iter().enumerate().map(|(k, t)| (depth(t), k)).collect();
        order.sort_by(|a, b| a.0.total_cmp(&b.0));
        // полосы строк — каждая в своём потоке (порядок треугольников в полосе тот же)
        let threads = crate::threads().max(1);
        let band = bh.div_ceil(threads).max(1);
        let order = &order;
        std::thread::scope(|scope| {
            for (bi, part) in buf.chunks_mut(band * bw).enumerate() {
                let (sample, edge_y0) = (&sample, by0 + (bi * band) as i32);
                let rows = (part.len() / bw) as i32;
                scope.spawn(move || {
        for &(_, k) in order.iter() {
            let t = mesh.tris[k];
            let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
            let (ra, rb, rc) = (mesh.rest[t[0] as usize], mesh.rest[t[1] as usize], mesh.rest[t[2] as usize]);
            let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            if area.abs() < 1e-6 || !area.is_finite() {
                continue;
            }
            let tx0 = (a[0].min(b[0]).min(c[0]).floor() as i32).max(bx0);
            let tx1 = (a[0].max(b[0]).max(c[0]).ceil() as i32).min(bx0 + bw as i32 - 1);
            let ty0 = (a[1].min(b[1]).min(c[1]).floor() as i32).max(edge_y0);
            let ty1 = (a[1].max(b[1]).max(c[1]).ceil() as i32).min(edge_y0 + rows - 1);
            if ty0 > ty1 {
                continue;
            }
            let edge = |p: [f32; 2], q: [f32; 2], x: f32, y: f32| (q[0] - p[0]) * (y - p[1]) - (q[1] - p[1]) * (x - p[0]);
            let top_left = |p: [f32; 2], q: [f32; 2]| {
                let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
                let (dx, dy) = if area > 0.0 { (dx, dy) } else { (-dx, -dy) };
                (dy == 0.0 && dx < 0.0) || dy > 0.0
            };
            let tl = [top_left(b, c), top_left(c, a), top_left(a, b)];
            for y in ty0..=ty1 {
                for x in tx0..=tx1 {
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    let w = [edge(b, c, px, py) / area, edge(c, a, px, py) / area, edge(a, b, px, py) / area];
                    if (0..3).any(|k| w[k] < 0.0 || (w[k] == 0.0 && !tl[k])) {
                        continue;
                    }
                    let s = sample(ra[0] * w[0] + rb[0] * w[1] + rc[0] * w[2], ra[1] * w[0] + rb[1] * w[1] + rc[1] * w[2]);
                    if s[3] <= 0.0 {
                        continue;
                    }
                    let d = &mut part[(y - edge_y0) as usize * bw + (x - bx0) as usize];
                    for ch in 0..4 {
                        d[ch] = s[ch] + d[ch] * (1.0 - s[3]);
                    }
                }
            }
        }
                });
            }
        });
        // в плитки — целыми плитками
        let (bx1, by1) = (bx0 + bw as i32 - 1, by0 + bh as i32 - 1);
        for ty in by0.div_euclid(TILE)..=by1.div_euclid(TILE) {
            for tx in bx0.div_euclid(TILE)..=bx1.div_euclid(TILE) {
                let mut tile: Option<crate::tile::Tile<crate::pixel::Px>> = None;
                for yy in 0..TILE {
                    let y = ty * TILE + yy;
                    if y < by0 || y > by1 {
                        continue;
                    }
                    let row = (y - by0) as usize * bw;
                    for xx in 0..TILE {
                        let x = tx * TILE + xx;
                        if x < bx0 || x > bx1 {
                            continue;
                        }
                        let p = buf[row + (x - bx0) as usize];
                        if p[3] > 0.0 {
                            let t = tile.get_or_insert_with(|| crate::tile::Tile::filled([0; 4]));
                            t.0[(yy * TILE + xx) as usize] = from_f32(p);
                        }
                    }
                }
                if let Some(t) = tile {
                    out.tiles.insert((tx, ty), t);
                }
            }
        }
        out
    }

    /// Настройки и булавки текстом (исходник пишется отдельно).
    pub fn to_text(&self) -> String {
        let mut s = format!("puppet 1\nexpansion {}\ndensity {}\n", self.expansion, self.density);
        for p in &self.pins {
            s += &format!("pin {} {} {} {} {} {}\n", p.kind.key(), p.at[0], p.at[1], p.amount, p.extent, p.in_front);
            for (f, k) in &p.keys {
                s += &format!("key {f} {} {} {} {}\n", k.pos[0], k.pos[1], k.rot, k.scale);
            }
        }
        s
    }

    pub fn read_text(&mut self, s: &str) {
        self.pins.clear();
        for line in s.lines() {
            let mut it = line.split_ascii_whitespace();
            let head = it.next().unwrap_or("");
            let kind = if head == "pin" { it.next().unwrap_or("") } else { "" };
            let n: Vec<f32> = it.filter_map(|v| v.parse().ok()).filter(|v: &f32| v.is_finite()).collect();
            match head {
                "expansion" => self.expansion = n.first().copied().unwrap_or(3.0).clamp(0.0, 64.0),
                "density" => self.density = n.first().copied().unwrap_or(5.0).clamp(1.0, 10.0) as u8,
                "pin" if n.len() >= 5 && self.pins.len() < 10_000 => {
                    let Some(k) = PinKind::ALL.into_iter().find(|k| k.key() == kind) else { continue };
                    self.pins.push(Pin { kind: k, at: [n[0], n[1]], amount: n[2], extent: n[3].max(0.0), in_front: n[4], keys: BTreeMap::new() });
                }
                "key" if n.len() >= 5 => {
                    if let Some(p) = self.pins.last_mut() {
                        if p.keys.len() < 100_000 {
                            p.keys.insert(n[0].max(0.0) as u32, PinPose { pos: [n[1], n[2]], rot: n[3], scale: n[4] });
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn dist2(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)
}

fn barycentric(p: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> Option<[f32; 3]> {
    let d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
    if d.abs() < 1e-9 {
        return None;
    }
    let u = ((b[1] - c[1]) * (p[0] - c[0]) + (c[0] - b[0]) * (p[1] - c[1])) / d;
    let v = ((c[1] - a[1]) * (p[0] - c[0]) + (a[0] - c[0]) * (p[1] - c[1])) / d;
    Some([u, v, 1.0 - u - v])
}

/// Точка источника билинейно (предумноженная, доли единицы).
fn sample(src: &ColorMap, x: f32, y: f32) -> [f32; 4] {
    let (x, y) = (x - 0.5, y - 0.5);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let at = |dx: i32, dy: i32| -> [f32; 4] { to_f32(src.get(x0 as i32 + dx, y0 as i32 + dy)) };
    let (a, b, c, d) = (at(0, 0), at(1, 0), at(0, 1), at(1, 1));
    let mut o = [0.0f32; 4];
    for k in 0..4 {
        o[k] = (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy;
    }
    o
}

/// Сопряжённые градиенты с диагональным предобуславливателем.
fn cg(apply: &dyn Fn(&[f32], &mut [f32]), diag: &[f32], b: &[f32], x: &mut [f32], iters: usize) {
    let n = b.len();
    let mut ax = vec![0.0f32; n];
    apply(x, &mut ax);
    let mut r: Vec<f32> = (0..n).map(|i| b[i] - ax[i]).collect();
    let mut z: Vec<f32> = (0..n).map(|i| r[i] / diag[i].max(1e-6)).collect();
    let mut p = z.clone();
    let mut rz: f32 = r.iter().zip(&z).map(|(a, b)| a * b).sum();
    let mut ap = vec![0.0f32; n];
    for _ in 0..iters {
        if rz.abs() < 1e-10 {
            break;
        }
        apply(&p, &mut ap);
        let pap: f32 = p.iter().zip(&ap).map(|(a, b)| a * b).sum();
        if pap.abs() < 1e-12 {
            break;
        }
        let a = rz / pap;
        for i in 0..n {
            x[i] += a * p[i];
            r[i] -= a * ap[i];
        }
        for i in 0..n {
            z[i] = r[i] / diag[i].max(1e-6);
        }
        let rz2: f32 = r.iter().zip(&z).map(|(a, b)| a * b).sum();
        let beta = rz2 / rz;
        rz = rz2;
        for i in 0..n {
            p[i] = z[i] + beta * p[i];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixel::from_rgba8;

    /// Полоса 120×20: от 40 до 160 по x, от 90 до 110 по y.
    fn bar() -> Puppet {
        let mut src = ColorMap::new();
        for y in 90..110 {
            for x in 40..160 {
                src.set(x, y, from_rgba8([200, 50, 50, 255]));
            }
        }
        Puppet::new(src)
    }

    fn key(p: &mut Pin, f: u32, x: f32, y: f32) {
        p.keys.insert(f, PinPose { pos: [x, y], rot: 0.0, scale: 1.0 });
    }

    #[test]
    fn same_offset_on_all_pins_is_a_translation() {
        let mut pp = bar();
        let mut a = Pin::new(PinKind::Position, [45.0, 100.0]);
        let mut b = Pin::new(PinKind::Position, [155.0, 100.0]);
        key(&mut a, 0, 55.0, 130.0);
        key(&mut b, 0, 165.0, 130.0);
        pp.pins = vec![a, b];
        let mesh = pp.mesh();
        let pos = pp.deform(&mesh, 0, None);
        for (r, p) in mesh.rest.iter().zip(&pos) {
            assert!((p[0] - r[0] - 10.0).abs() < 0.05 && (p[1] - r[1] - 30.0).abs() < 0.05, "{r:?} → {p:?}");
        }
        let img = pp.render(&mesh, &pos);
        assert_eq!(img.get(100, 130)[3], from_rgba8([0, 0, 0, 255])[3]);
        assert_eq!(img.get(100, 100)[3], 0);
    }

    #[test]
    fn rotating_two_pins_keeps_shape() {
        let mut pp = bar();
        let mut a = Pin::new(PinKind::Position, [45.0, 100.0]);
        let mut b = Pin::new(PinKind::Position, [155.0, 100.0]);
        // поворот на 90° вокруг середины (100, 100)
        key(&mut a, 0, 100.0, 45.0);
        key(&mut b, 0, 100.0, 155.0);
        pp.pins = vec![a, b];
        let mesh = pp.mesh();
        let pos = pp.deform(&mesh, 0, None);
        let mut worst = 0.0f32;
        for &(i, j) in &mesh.edges {
            let l0 = dist2(mesh.rest[i as usize], mesh.rest[j as usize]).sqrt();
            let l1 = dist2(pos[i as usize], pos[j as usize]).sqrt();
            worst = worst.max((l1 / l0 - 1.0).abs());
        }
        assert!(worst < 0.05, "рёбра растянуты на {worst}");
    }

    #[test]
    fn starch_keeps_a_region_rigid() {
        // тянем правый конец вниз, левый держим; середина без крахмала гнётся сильнее
        let bend = |starch: bool| {
            let mut pp = bar();
            let a = Pin::new(PinKind::Position, [45.0, 100.0]);
            let mut b = Pin::new(PinKind::Position, [155.0, 100.0]);
            key(&mut b, 0, 140.0, 160.0);
            let mut s = Pin::new(PinKind::Starch, [100.0, 100.0]);
            s.amount = 3.0;
            s.extent = 30.0;
            pp.pins = if starch { vec![a, b, s] } else { vec![a, b] };
            let mesh = pp.mesh();
            let pos = pp.deform(&mesh, 0, None);
            // искажение рёбер в середине
            let mut d = 0.0f32;
            for &(i, j) in &mesh.edges {
                let m = [(mesh.rest[i as usize][0] + mesh.rest[j as usize][0]) / 2.0, 0.0];
                if (m[0] - 100.0).abs() < 20.0 {
                    let l0 = dist2(mesh.rest[i as usize], mesh.rest[j as usize]).sqrt();
                    let l1 = dist2(pos[i as usize], pos[j as usize]).sqrt();
                    d = d.max((l1 / l0 - 1.0).abs());
                }
            }
            d
        };
        let (plain, stiff) = (bend(false), bend(true));
        assert!(stiff < plain, "с крахмалом {stiff}, без {plain}");
    }

    #[test]
    fn text_round_trip_and_broken_text() {
        let mut pp = bar();
        let mut a = Pin::new(PinKind::Advanced, [45.0, 100.0]);
        a.keys.insert(3, PinPose { pos: [1.0, 2.0], rot: 0.5, scale: 1.2 });
        pp.pins = vec![a, Pin::new(PinKind::Overlap, [60.0, 100.0])];
        pp.density = 7;
        let mut back = Puppet::new(pp.source.clone());
        back.read_text(&pp.to_text());
        assert_eq!(back, pp);
        let t = pp.to_text();
        for n in 0..t.len() {
            if t.is_char_boundary(n) {
                back.read_text(&t[..n]);
            }
        }
        back.read_text("pin что-то 1 2\nkey x\n");
        // ни одной булавки — картинка как была
        let empty = Puppet::new(pp.source.clone());
        assert_eq!(empty.frame(0).get(50, 100), pp.source.get(50, 100));
    }
}
