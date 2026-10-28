//! Выделение: маска 0..=ONE поверх холста. Пустая маска — выделения нет, рисовать можно везде.
//!
//! Как в SAI 2: прямоугольник, лассо, волшебная палочка; новая область заменяет выделение,
//! с Shift добавляется, с Alt вычитается, с Shift+Alt остаётся пересечение.

use crate::pixel::{to_rgba8, Px, CLEAR, ONE};
use crate::tile::{ColorMap, Key, MaskMap, Tile, TILE};
use crate::vector::{coverage, Pt};

/// Как новая область соединяется с прежним выделением.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Combine {
    Replace,
    Add,
    Subtract,
    Intersect,
}

impl Combine {
    /// По клавишам-модификаторам, как в SAI.
    pub fn from_modifiers(shift: bool, alt: bool) -> Combine {
        match (shift, alt) {
            (true, true) => Combine::Intersect,
            (true, false) => Combine::Add,
            (false, true) => Combine::Subtract,
            (false, false) => Combine::Replace,
        }
    }
}

#[derive(Clone, Default, Debug, PartialEq)]
pub struct Selection {
    pub mask: MaskMap,
}

impl Selection {
    /// Выделения нет (вся маска пуста).
    pub fn is_empty(&self) -> bool {
        self.mask.tiles.values().all(|t| t.0.iter().all(|&v| v == 0))
    }

    /// Доля выделения в точке 0..=ONE; без выделения — ONE везде.
    pub fn amount(&self, x: i32, y: i32) -> u32 {
        if self.mask.tiles.is_empty() {
            return ONE;
        }
        self.mask.get(x, y) as u32
    }

    /// Плитка выделения для ограничения правки; `None` — ограничения нет (выделения нет).
    /// Пустая плитка при непустом выделении — правка в ней запрещена.
    pub fn tile(&self, key: Key) -> Option<Tile<u16>> {
        if self.mask.tiles.is_empty() {
            return None;
        }
        Some(self.mask.tiles.get(&key).cloned().unwrap_or_else(|| Tile::filled(0)))
    }

    /// Соединяет область `area` с выделением.
    pub fn combine(&mut self, area: &MaskMap, mode: Combine) {
        match mode {
            Combine::Replace => self.mask = area.clone(),
            Combine::Add => {
                for (k, t) in &area.tiles {
                    let dst = self.mask.tile_mut(*k);
                    for (d, &s) in dst.0.iter_mut().zip(t.0.iter()) {
                        *d = (*d).max(s);
                    }
                }
            }
            Combine::Subtract => {
                for (k, t) in &area.tiles {
                    if let Some(dst) = self.mask.tiles.get_mut(k) {
                        for (d, &s) in dst.0.iter_mut().zip(t.0.iter()) {
                            *d = ((*d as u32 * (ONE - s as u32)) / ONE) as u16;
                        }
                    }
                }
            }
            Combine::Intersect => {
                let keys: Vec<Key> = self.mask.tiles.keys().copied().collect();
                for k in keys {
                    match area.tiles.get(&k) {
                        Some(t) => {
                            let dst = self.mask.tile_mut(k);
                            for (d, &s) in dst.0.iter_mut().zip(t.0.iter()) {
                                *d = (*d).min(s);
                            }
                        }
                        None => {
                            self.mask.tiles.remove(&k);
                        }
                    }
                }
            }
        }
        self.prune();
    }

    /// Убирает пустые плитки, чтобы `is_empty` и показ работали быстро.
    fn prune(&mut self) {
        self.mask.tiles.retain(|_, t| t.0.iter().any(|&v| v != 0));
    }

    /// Выделить весь холст.
    pub fn all(width: u32, height: u32) -> MaskMap {
        rect(0.0, 0.0, width as f64, height as f64)
    }

    /// Обратить выделение в пределах холста.
    pub fn invert(&mut self, width: u32, height: u32) {
        let mut out = MaskMap::new();
        for y in 0..height as i32 {
            for x in 0..width as i32 {
                let v = ONE - self.mask.get(x, y) as u32;
                if v > 0 {
                    out.set(x, y, v as u16);
                }
            }
        }
        self.mask = out;
        self.prune();
    }

    /// Расширить (`grow`) или сузить выделение на 1 px: наибольшее или наименьшее значение
    /// среди точки и её восьми соседей.
    pub fn grow(&mut self, grow: bool) {
        self.grow_by(1, grow);
    }

    /// Расширить или сузить на `n` px, как «Dilate/Erode Selection» SAI 2017 (1–25 px): шаги по
    /// очереди квадратом 3×3 и крестом (первый — квадрат), поэтому углы срезаются под 45°
    /// (сверено с живой SAI: прямоугольник +25 px — срез по 12 px).
    pub fn grow_by(&mut self, n: u32, grow: bool) {
        let Some((x0, y0, x1, y1)) = self.bounds() else { return };
        let pad = if grow { n as i32 } else { 0 };
        let (ox, oy) = (x0 - pad, y0 - pad);
        let (bw, bh) = ((x1 - x0 + 2 * pad) as usize, (y1 - y0 + 2 * pad) as usize);
        let mut buf = vec![0u16; bw * bh];
        for (i, v) in buf.iter_mut().enumerate() {
            *v = self.mask.get(ox + (i % bw) as i32, oy + (i / bw) as i32);
        }
        let buf = morph(buf, bw, bh, n, grow, 0);
        let mut out = MaskMap::new();
        for (i, &v) in buf.iter().enumerate() {
            if v > 0 {
                out.set(ox + (i % bw) as i32, oy + (i / bw) as i32, v);
            }
        }
        self.mask = out;
        self.prune();
    }

    /// Границы выделения: (x0, y0, x1, y1), x1 и y1 не включительно.
    pub fn bounds(&self) -> Option<(i32, i32, i32, i32)> {
        let mut b: Option<(i32, i32, i32, i32)> = None;
        for (&(tx, ty), t) in &self.mask.tiles {
            for (i, &v) in t.0.iter().enumerate() {
                if v == 0 {
                    continue;
                }
                let (x, y) = (tx * TILE + i as i32 % TILE, ty * TILE + i as i32 / TILE);
                b = Some(match b {
                    None => (x, y, x + 1, y + 1),
                    Some((a, c, d, e)) => (a.min(x), c.min(y), d.max(x + 1), e.max(y + 1)),
                });
            }
        }
        b
    }

    /// Отрезки границы выделения (по порогу половины) в координатах холста — для показа
    /// «бегущих муравьёв». Соседние отрезки одной линии склеены.
    pub fn outline(&self) -> Vec<[(i32, i32); 2]> {
        let half = (ONE / 2) as u16;
        let inside = |x: i32, y: i32| self.mask.get(x, y) >= half;
        let Some((x0, y0, x1, y1)) = self.bounds() else { return Vec::new() };
        let mut out = Vec::new();
        // горизонтальные границы: между строками y-1 и y
        for y in y0..=y1 {
            let mut run: Option<i32> = None;
            for x in x0..=x1 {
                let edge = x < x1 && inside(x, y - 1) != inside(x, y);
                match (edge, run) {
                    (true, None) => run = Some(x),
                    (false, Some(s)) => {
                        out.push([(s, y), (x, y)]);
                        run = None;
                    }
                    _ => {}
                }
            }
        }
        // вертикальные: между столбцами x-1 и x
        for x in x0..=x1 {
            let mut run: Option<i32> = None;
            for y in y0..=y1 {
                let edge = y < y1 && inside(x - 1, y) != inside(x, y);
                match (edge, run) {
                    (true, None) => run = Some(y),
                    (false, Some(s)) => {
                        out.push([(x, s), (x, y)]);
                        run = None;
                    }
                    _ => {}
                }
            }
        }
        out
    }
}

/// Расширить (`grow`, наибольшее) или сузить (наименьшее) значения плотной карты `bw`×`bh` на
/// `n` px, как SAI 2017 (выделение и «Обводка»): шаги по очереди квадратом 3×3 и крестом, первый —
/// квадрат, поэтому углы срезаются под 45°. За краем карты — `outside`. Строки — в потоках.
pub fn morph(mut buf: Vec<u16>, bw: usize, bh: usize, n: u32, grow: bool, outside: u16) -> Vec<u16> {
    let pick = |a: u16, b: u16| if grow { a.max(b) } else { a.min(b) };
    let at = |buf: &[u16], x: isize, y: isize| -> u16 {
        if x < 0 || y < 0 || x >= bw as isize || y >= bh as isize {
            outside
        } else {
            buf[y as usize * bw + x as usize]
        }
    };
    let threads = crate::threads();
    let rows = bh.div_ceil(threads).max(1);
    // один проход: каждая строка результата по `f(x, y)`
    let pass = |f: &(dyn Fn(isize, isize) -> u16 + Sync)| -> Vec<u16> {
        let mut out = vec![0u16; bw * bh];
        std::thread::scope(|scope| {
            for (k, chunk) in out.chunks_mut(rows * bw).enumerate() {
                scope.spawn(move || {
                    for (i, o) in chunk.iter_mut().enumerate() {
                        let idx = k * rows * bw + i;
                        *o = f((idx % bw) as isize, (idx / bw) as isize);
                    }
                });
            }
        });
        out
    };
    for step in 0..n {
        buf = if step % 2 == 0 {
            // квадрат: по строке, затем по столбцу
            let row = pass(&|x, y| pick(pick(at(&buf, x - 1, y), at(&buf, x, y)), at(&buf, x + 1, y)));
            pass(&|x, y| pick(pick(at(&row, x, y - 1), at(&row, x, y)), at(&row, x, y + 1)))
        } else {
            pass(&|x, y| [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().fold(at(&buf, x, y), |v, &(dx, dy)| pick(v, at(&buf, x + dx, y + dy))))
        };
    }
    buf
}

/// Прямоугольник (координаты холста, края могут быть дробными — частичное покрытие).
pub fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> MaskMap {
    polygon(&[[x0, y0], [x1, y0], [x1, y1], [x0, y1]])
}

/// Многоугольник (лассо) со сглаженным краем.
pub fn polygon(points: &[Pt]) -> MaskMap {
    let mut out = MaskMap::new();
    if points.len() < 3 {
        return out;
    }
    let (left, top, w, _h, cov) = coverage(&[points.to_vec()]);
    for (i, &c) in cov.iter().enumerate() {
        let v = (c.clamp(0.0, 1.0) * ONE as f32 + 0.5) as u32;
        if v > 0 {
            out.set(left + (i % w) as i32, top + (i / w) as i32, v.min(ONE) as u16);
        }
    }
    out
}

/// Многоугольник лассо: со сглаженным краем или без (тогда точка выделена, если покрыта больше
/// чем наполовину) — галочка «Сглаживание» инструмента, как в SAI.
pub fn polygon_aa(points: &[Pt], aa: bool) -> MaskMap {
    let mut m = polygon(points);
    if !aa {
        for t in m.tiles.values_mut() {
            for v in t.0.iter_mut() {
                *v = if *v as u32 * 2 > ONE { ONE as u16 } else { 0 };
            }
        }
    }
    m
}

/// Волшебная палочка: связная (по 4 соседям) область, где цвет отличается от цвета в точке
/// щелчка не больше чем на `tolerance` (0..=255, по наибольшей разнице каналов RGBA).
pub fn magic_wand(pixels: &ColorMap, width: u32, height: u32, x: i32, y: i32, tolerance: u8) -> MaskMap {
    let mut out = MaskMap::new();
    let (w, h) = (width as i32, height as i32);
    if x < 0 || y < 0 || x >= w || y >= h {
        return out;
    }
    let rgba = |p: Px| to_rgba8(p);
    let seed = rgba(pixels.get(x, y));
    let near = |c: [u8; 4]| (0..4).all(|i| (c[i] as i32 - seed[i] as i32).abs() <= tolerance as i32);
    // Плотные буферы и заливка строками — только в рамке плиток слоя с запасом в точку: за рамкой
    // всё пусто и связано между собой, так что эта часть или залита вся (целыми плитками), или нет.
    // Так холст 100000×100000 с небольшим рисунком не требует буфера на весь холст.
    let full = |v: u16| v != 0;
    let tile_box = pixels.tiles.keys().fold(None, |b: Option<(i32, i32, i32, i32)>, &(kx, ky)| {
        let (x0, y0) = (kx * TILE, ky * TILE);
        Some(b.map_or((x0, y0, x0 + TILE, y0 + TILE), |(a, c, d, e)| (a.min(x0), c.min(y0), d.max(x0 + TILE), e.max(y0 + TILE))))
    });
    let (rx0, ry0, rx1, ry1) = match tile_box {
        Some((a, c, d, e)) => ((a - 1).max(0), (c - 1).max(0), (d + 1).min(w), (e + 1).min(h)),
        None => (0, 0, 0, 0),
    };
    let inside = |px: i32, py: i32| px >= rx0 && py >= ry0 && px < rx1 && py < ry1;
    // точка рамки, лежащая вне плиток (запас), — часть пустоты снаружи
    let outer = |px: i32, py: i32| tile_box.is_none_or(|(a, c, d, e)| px < a || py < c || px >= d || py >= e);
    let (wu, hu) = ((rx1 - rx0) as usize, (ry1 - ry0) as usize);
    let ok: Vec<bool> = pixels.read_rect(rx0, ry0, wu, hu).into_iter().map(|p| near(rgba(p))).collect();
    let mut fill = vec![0u16; wu * hu];
    let mut stack = Vec::new();
    let empty_near = near(rgba(CLEAR));
    let mut outside_filled = false;
    if inside(x, y) {
        stack.push(((x - rx0) as usize, (y - ry0) as usize));
    } else {
        // семя в пустоте снаружи: залита вся пустота и всё, что с ней сходится через запас рамки
        outside_filled = true;
    }
    if outside_filled {
        for yy in 0..hu {
            for xx in 0..wu {
                let (gx, gy) = (rx0 + xx as i32, ry0 + yy as i32);
                if outer(gx, gy) && (xx == 0 || yy == 0 || xx + 1 == wu || yy + 1 == hu) {
                    stack.push((xx, yy));
                }
            }
        }
    }
    while let Some((sx, sy)) = stack.pop() {
        let row = sy * wu;
        if full(fill[row + sx]) || !ok[row + sx] {
            continue;
        }
        let (mut l, mut r) = (sx, sx);
        while l > 0 && !full(fill[row + l - 1]) && ok[row + l - 1] {
            l -= 1;
        }
        while r + 1 < wu && !full(fill[row + r + 1]) && ok[row + r + 1] {
            r += 1;
        }
        fill[row + l..=row + r].fill(ONE as u16);
        if !outside_filled && empty_near && (l..=r).any(|xx| outer(rx0 + xx as i32, ry0 + sy as i32)) {
            // дошли до пустоты снаружи рамки — она вся залита
            outside_filled = true;
        }
        for ny in [sy.wrapping_sub(1), sy + 1] {
            if ny >= hu {
                continue;
            }
            let nrow = ny * wu;
            let mut xx = l;
            while xx <= r {
                if !full(fill[nrow + xx]) && ok[nrow + xx] {
                    stack.push((xx, ny));
                    while xx <= r && ok[nrow + xx] {
                        xx += 1;
                    }
                } else {
                    xx += 1;
                }
            }
        }
    }
    // пустота снаружи залита, а внутри рамки до неё ещё не дошли (семя было снаружи) — от края
    if outside_filled {
        let mut again = Vec::new();
        for yy in 0..hu {
            for xx in 0..wu {
                if outer(rx0 + xx as i32, ry0 + yy as i32) && ok[yy * wu + xx] && !full(fill[yy * wu + xx]) {
                    again.push((xx, yy));
                }
            }
        }
        while let Some((sx, sy)) = again.pop() {
            let i = sy * wu + sx;
            if full(fill[i]) || !ok[i] {
                continue;
            }
            fill[i] = ONE as u16;
            if sx > 0 {
                again.push((sx - 1, sy));
            }
            if sx + 1 < wu {
                again.push((sx + 1, sy));
            }
            if sy > 0 {
                again.push((sx, sy - 1));
            }
            if sy + 1 < hu {
                again.push((sx, sy + 1));
            }
        }
    }
    out.write_rect(rx0, ry0, wu, hu, &fill);
    if outside_filled {
        fill_outside(&mut out, w, h, (rx0, ry0, rx1, ry1));
    }
    out.tiles.retain(|_, t| t.0.iter().any(|&v| v != 0));
    out
}

/// Залить всю пустоту холста `w`×`h` за рамкой `r` (x0, y0, x1, y1): целыми плитками, а у рамки и
/// у края холста — по точкам.
fn fill_outside(out: &mut MaskMap, w: i32, h: i32, r: (i32, i32, i32, i32)) {
    let (rx0, ry0, rx1, ry1) = r;
    let inside = |px: i32, py: i32| px >= rx0 && py >= ry0 && px < rx1 && py < ry1;
    let full_tile = Tile::filled(ONE as u16);
    for ky in 0..(h + TILE - 1) / TILE {
        for kx in 0..(w + TILE - 1) / TILE {
            let (tx, ty) = (kx * TILE, ky * TILE);
            let clipped = tx + TILE > w || ty + TILE > h;
            let overlaps = tx < rx1 && ty < ry1 && tx + TILE > rx0 && ty + TILE > ry0;
            if !overlaps && !clipped {
                out.tiles.insert((kx, ky), full_tile.clone());
                continue;
            }
            let t = out.tile_mut((kx, ky));
            for yy in ty..(ty + TILE).min(h) {
                for xx in tx..(tx + TILE).min(w) {
                    if !inside(xx, yy) {
                        t.0[((yy - ty) * TILE + xx - tx) as usize] = ONE as u16;
                    }
                }
            }
        }
    }
}

/// Рамка плиток слоя (x0, y0, x1, y1), расширенная на `pad` и обрезанная холстом.
fn tile_box(pixels: &ColorMap, w: i32, h: i32, pad: i32) -> Option<(i32, i32, i32, i32)> {
    let b = pixels.tiles.keys().fold(None, |b: Option<(i32, i32, i32, i32)>, &(kx, ky)| {
        let (x0, y0) = (kx * TILE, ky * TILE);
        Some(b.map_or((x0, y0, x0 + TILE, y0 + TILE), |(a, c, d, e)| (a.min(x0), c.min(y0), d.max(x0 + TILE), e.max(y0 + TILE))))
    })?;
    Some(((b.0 - pad).max(0), (b.1 - pad).max(0), (b.2 + pad).min(w), (b.3 + pad).min(h)))
}

/// Допуск палочки и заливки, закрытие щелей (px) и запас под линию (px).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WandOptions {
    pub tolerance: u8,
    pub gap: u32,
    pub expand: u32,
    /// «Уменьшить мелкие пропуски» (SAI 2 2026-08-31, Reduce Small Omissions): у кончиков
    /// сужающихся линий заливка не оставляет светлых точек — заходит на 2 px в полупрозрачное
    /// (непрозрачность до 80 %) рядом с залитым. Сверено с живой SAI Alpha 2026-10-11: в режимах
    /// по прозрачности заливаются точки у острия и у хвоста мазка, сплошная линия — нет.
    pub omissions: bool,
}

/// Заливка с закрытием щелей: как волшебная палочка, но щели в контуре до `2·gap` px не дают
/// заливке вытечь (контур мысленно утолщается на `gap`, область находится внутри, затем
/// возвращается к краю линии на те же `gap` px), а `expand` px — запас под линию, чтобы у края
/// не оставалось каймы. `gap = 0` и `expand = 0` — обычная палочка.
pub fn magic_wand_closing(pixels: &ColorMap, width: u32, height: u32, x: i32, y: i32, o: WandOptions) -> MaskMap {
    let WandOptions { tolerance, gap, expand, omissions } = o;
    if gap == 0 && expand == 0 && !omissions {
        return magic_wand(pixels, width, height, x, y, tolerance);
    }
    let (cw, ch) = (width as i32, height as i32);
    if x < 0 || y < 0 || x >= cw || y >= ch {
        return MaskMap::new();
    }
    // Холст до 2^29 точек считается целиком, как всегда. Больше (до 100000×100000) — в рамке
    // плиток слоя с запасом больше всех шагов роста: за ней всё пусто, связано и не задето
    // утолщённым контуром, так что там заливка либо везде, либо нигде.
    let huge = u64::from(width) * u64::from(height) > crate::document::DENSE_LIMIT;
    let margin = (gap + expand) as i32 + 4;
    let (ox, oy, w, h) = if huge {
        let Some((a, b, c, d)) = tile_box(pixels, cw, ch, margin) else { return magic_wand(pixels, width, height, x, y, tolerance) };
        if u64::from((c - a) as u32) * u64::from((d - b) as u32) > crate::document::DENSE_LIMIT {
            return MaskMap::new();
        }
        (a, b, c - a, d - b)
    } else {
        (0, 0, cw, ch)
    };
    let has_outside = huge && (ox > 0 || oy > 0 || ox + w < cw || oy + h < ch);
    let seed = to_rgba8(pixels.get(x, y));
    // семя в пустоте за рамкой — начинаем с точки запаса у края рамки (она той же пустоты)
    let (x, y) = if x >= ox && y >= oy && x < ox + w && y < oy + h {
        (x - ox, y - oy)
    } else if !has_outside {
        return MaskMap::new();
    } else if ox > 0 {
        (0, (y - oy).clamp(0, h - 1))
    } else if oy > 0 {
        ((x - ox).clamp(0, w - 1), 0)
    } else if ox + w < cw {
        (w - 1, (y - oy).clamp(0, h - 1))
    } else {
        ((x - ox).clamp(0, w - 1), h - 1)
    };
    let n = (w * h) as usize;
    let idx = |x: i32, y: i32| (y * w + x) as usize;
    let mut fillable = vec![false; n];
    for yy in 0..h {
        for xx in 0..w {
            let c = to_rgba8(pixels.get(ox + xx, oy + yy));
            fillable[idx(xx, yy)] = (0..4).all(|i| (c[i] as i32 - seed[i] as i32).abs() <= tolerance as i32);
        }
    }
    const N8: [(i32, i32); 8] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];
    // поиск в ширину по 8 соседям от `from` на `steps` шагов по точкам, где `pass`
    let grow = |from: &mut Vec<bool>, steps: u32, pass: &dyn Fn(usize) -> bool| {
        let mut front: Vec<(i32, i32)> = (0..n).filter(|&i| from[i]).map(|i| (i as i32 % w, i as i32 / w)).collect();
        for _ in 0..steps {
            let mut next = Vec::new();
            for (px, py) in front {
                for (dx, dy) in N8 {
                    let (nx, ny) = (px + dx, py + dy);
                    if nx >= 0 && ny >= 0 && nx < w && ny < h && !from[idx(nx, ny)] && pass(idx(nx, ny)) {
                        from[idx(nx, ny)] = true;
                        next.push((nx, ny));
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            front = next;
        }
    };
    // 1. контур, утолщённый на gap
    let mut thick: Vec<bool> = fillable.iter().map(|f| !f).collect();
    grow(&mut thick, gap, &|_| true);
    // 2. связная область вне утолщённого контура (если щелчок у самой линии — без закрытия)
    let start_ok = !thick[idx(x, y)];
    let mut region = vec![false; n];
    let mut stack = vec![(x, y)];
    while let Some((px, py)) = stack.pop() {
        let i = idx(px, py);
        if region[i] || !fillable[i] || (start_ok && thick[i]) {
            continue;
        }
        region[i] = true;
        for (nx, ny) in [(px + 1, py), (px - 1, py), (px, py + 1), (px, py - 1)] {
            if nx >= 0 && ny >= 0 && nx < w && ny < h {
                stack.push((nx, ny));
            }
        }
    }
    // 3. назад к краю линии — только по заливаемым точкам
    if start_ok {
        grow(&mut region, gap, &|i| fillable[i]);
    }
    // 3½. мелкие пропуски у кончиков: полупрозрачное рядом с залитым, не дальше 2 px
    if omissions {
        let light: Vec<bool> = (0..n).map(|i| to_rgba8(pixels.get(ox + i as i32 % w, oy + i as i32 / w))[3] <= 204).collect();
        grow(&mut region, 2, &|i| light[i]);
    }
    // 4. запас под линию
    grow(&mut region, expand, &|_| true);
    let mut out = MaskMap::new();
    for (i, &r) in region.iter().enumerate() {
        if r {
            out.set(ox + i as i32 % w, oy + i as i32 / w, ONE as u16);
        }
    }
    // заливка дошла до края рамки, за которым пустота холста, — залита и вся она
    if has_outside {
        let edge = |xx: i32, yy: i32| region[idx(xx, yy)] && ((xx == 0 && ox > 0) || (yy == 0 && oy > 0) || (xx == w - 1 && ox + w < cw) || (yy == h - 1 && oy + h < ch));
        if (0..w).any(|xx| edge(xx, 0) || edge(xx, h - 1)) || (0..h).any(|yy| edge(0, yy) || edge(w - 1, yy)) {
            fill_outside(&mut out, cw, ch, (ox, oy, ox + w, oy + h));
        }
    }
    out
}

/// Что считается «заливаемым» у заливки лассо с замкнутыми областями (как в SAI 2026).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FillRegion {
    /// Прозрачное (непрозрачность не больше допуска).
    #[default]
    Transparent,
    /// Прозрачное с допуском: полупрозрачное заливается частично (тем сильнее, чем прозрачнее).
    TransparentFuzzy,
    /// Цвет, похожий на цвет в начале обводки (в пределах допуска).
    Similar,
    /// Непрозрачное (непрозрачность больше допуска).
    Opaque,
}

/// Настройки заливки лассо.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LassoFill {
    /// Заливать только замкнутые области внутри обводки (не касающиеся её края).
    pub enclosed: bool,
    pub region: FillRegion,
    /// Допуск 0..=255.
    pub range: u8,
    /// Зазоры в линиях до `2·gap` px считаются закрытыми.
    pub gap: u32,
    /// Расширить заливку на столько px (в пределах обводки).
    pub dilate: u32,
    /// Не заливать то, что уже закрашено (только заливаемые точки).
    pub ignore_painted: bool,
}

/// Заливка лассо: доля заливки каждой точки (0..=ONE). `lasso` — маска обводки (со сглаженным
/// краем), `src` — по чему искать области (слой или сведённое), `seed` — точка начала обводки
/// (для «похожего цвета»).
///
/// Без замкнутых областей — просто обводка. С ними: от края обводки заливается «внешняя» часть
/// сквозь линии, утолщённые на `gap`, и возвращается к краю линий на те же `gap`; залито всё
/// остальное внутри обводки — замкнутые области вместе с их линиями (как в SAI: кольцо залито до
/// наружного края, зазор закрыт почти прямым отрезком между концами линии).
pub fn lasso_fill(lasso: &MaskMap, src: &ColorMap, width: u32, height: u32, seed: (i32, i32), o: LassoFill) -> MaskMap {
    if !o.enclosed {
        return lasso.clone();
    }
    let Some((x0, y0, x1, y1)) = mask_bounds(lasso) else { return MaskMap::new() };
    // рамка с полем в 1 px: всё за обводкой и за краем холста — «снаружи»
    let (x0, y0, x1, y1) = (x0.max(0), y0.max(0), x1.min(width as i32), y1.min(height as i32));
    if x1 <= x0 || y1 <= y0 {
        return MaskMap::new();
    }
    let (bx0, by0, bx1, by1) = (x0 - 1, y0 - 1, x1 + 1, y1 + 1);
    let (w, h) = ((bx1 - bx0) as usize, (by1 - by0) as usize);
    let n = w * h;
    let at = |i: usize| (bx0 + (i % w) as i32, by0 + (i / w) as i32);
    let inside: Vec<bool> = (0..n).map(|i| {
        let (x, y) = at(i);
        x >= 0 && y >= 0 && x < width as i32 && y < height as i32 && lasso.get(x, y) as u32 * 2 > ONE
    }).collect();
    let seed_c = to_rgba8(src.get(seed.0, seed.1));
    let r = o.range as i32;
    // насколько точка «заливаемая»: 0..=1
    let weight: Vec<f32> = (0..n).map(|i| {
        let (x, y) = at(i);
        let c = to_rgba8(src.get(x, y));
        match o.region {
            FillRegion::Transparent => (c[3] as i32 <= r) as u8 as f32,
            FillRegion::TransparentFuzzy => {
                if c[3] as i32 <= r { 1.0 } else { 1.0 - (c[3] as i32 - r) as f32 / (255 - r).max(1) as f32 }
            }
            FillRegion::Similar => (0..4).all(|k| (c[k] as i32 - seed_c[k] as i32).abs() <= r) as u8 as f32,
            FillRegion::Opaque => (c[3] as i32 > r) as u8 as f32,
        }
    }).collect();
    let fillable: Vec<bool> = weight.iter().map(|&v| v > 0.0).collect();
    const N8: [(i32, i32); 8] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];
    let nb = |i: usize, dx: i32, dy: i32| -> Option<usize> {
        let (x, y) = ((i % w) as i32 + dx, (i / w) as i32 + dy);
        (x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h).then(|| y as usize * w + x as usize)
    };
    // рост на `steps` шагов по 8 соседям через точки, где `pass`
    let grow = |set: &mut Vec<bool>, steps: u32, pass: &dyn Fn(usize) -> bool| {
        let mut front: Vec<usize> = (0..n).filter(|&i| set[i]).collect();
        for _ in 0..steps {
            let mut next = Vec::new();
            for i in front {
                for (dx, dy) in N8 {
                    if let Some(j) = nb(i, dx, dy) {
                        if !set[j] && pass(j) {
                            set[j] = true;
                            next.push(j);
                        }
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            front = next;
        }
    };
    // 1. линии внутри обводки, утолщённые на gap
    let mut thick: Vec<bool> = (0..n).map(|i| inside[i] && !fillable[i]).collect();
    grow(&mut thick, o.gap, &|j| inside[j]);
    // 2. внешняя часть: всё за обводкой, оттуда — по заливаемым точкам вне утолщения
    let open = |i: usize| inside[i] && fillable[i] && !thick[i];
    let mut outer: Vec<bool> = inside.iter().map(|v| !v).collect();
    let mut stack: Vec<usize> = (0..n).filter(|&i| open(i) && [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(dx, dy)| nb(i, dx, dy).is_some_and(|j| outer[j]))).collect();
    while let Some(i) = stack.pop() {
        if outer[i] {
            continue;
        }
        outer[i] = true;
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            if let Some(j) = nb(i, dx, dy) {
                if !outer[j] && open(j) {
                    stack.push(j);
                }
            }
        }
    }
    // 3. назад к краю линий
    grow(&mut outer, o.gap, &|j| inside[j] && fillable[j]);
    // 4. заливка — остальное внутри обводки, но только куски, где есть заливаемые точки (сама
    //    линия без замкнутой пустоты не заливается — как в SAI при допуске 0)
    let cand: Vec<bool> = (0..n).map(|i| inside[i] && !outer[i]).collect();
    let mut keep = vec![false; n];
    let mut seen = vec![false; n];
    for s0 in 0..n {
        if !cand[s0] || seen[s0] {
            continue;
        }
        let (mut part, mut any, mut st) = (Vec::new(), false, vec![s0]);
        seen[s0] = true;
        while let Some(i) = st.pop() {
            part.push(i);
            any |= fillable[i];
            for (dx, dy) in N8 {
                if let Some(j) = nb(i, dx, dy) {
                    if cand[j] && !seen[j] {
                        seen[j] = true;
                        st.push(j);
                    }
                }
            }
        }
        if any {
            for i in part {
                keep[i] = true;
            }
        }
    }
    let mut fill: Vec<bool> = (0..n).map(|i| keep[i] && (!o.ignore_painted || fillable[i])).collect();
    grow(&mut fill, o.dilate, &|j| inside[j]);
    let mut out = MaskMap::new();
    for i in 0..n {
        if fill[i] {
            let (x, y) = at(i);
            let edge = lasso.get(x, y) as f32;
            let k = if o.region == FillRegion::TransparentFuzzy && !o.ignore_painted { 1.0 } else if o.region == FillRegion::TransparentFuzzy { weight[i] } else { 1.0 };
            let v = (edge * k).round() as u32;
            if v > 0 {
                out.set(x, y, v.min(ONE) as u16);
            }
        }
    }
    out
}

/// Рамка ненулевой части маски (x0, y0, x1, y1), x1/y1 — не включительно.
pub fn mask_bounds(m: &MaskMap) -> Option<(i32, i32, i32, i32)> {
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for (&(tx, ty), t) in &m.tiles {
        for (i, &v) in t.0.iter().enumerate() {
            if v != 0 {
                let (x, y) = (tx * TILE + i as i32 % TILE, ty * TILE + i as i32 / TILE);
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    (x0 < x1).then_some((x0, y0, x1, y1))
}

/// Маска, сдвинутая на целые (`dx`, `dy`).
pub fn shift_mask(src: &MaskMap, dx: i32, dy: i32) -> MaskMap {
    let mut out = MaskMap::new();
    for (&(tx, ty), t) in &src.tiles {
        for (i, &v) in t.0.iter().enumerate() {
            if v != 0 {
                out.set(tx * TILE + i as i32 % TILE + dx, ty * TILE + i as i32 / TILE + dy, v);
            }
        }
    }
    out
}

/// Пиксели слоя после сдвига на (`dx`, `dy`): без выделения сдвигается всё; с выделением
/// выделенная часть вырезается и кладётся поверх остального на новом месте.
pub fn shift_pixels(src: &ColorMap, sel: &Selection, dx: i32, dy: i32) -> ColorMap {
    let mut out = ColorMap::new();
    let whole = sel.mask.tiles.is_empty();
    if !whole {
        // то, что остаётся на месте
        for (&(tx, ty), t) in &src.tiles {
            for (i, &p) in t.0.iter().enumerate() {
                let (x, y) = (tx * TILE + i as i32 % TILE, ty * TILE + i as i32 / TILE);
                let keep = ONE - sel.amount(x, y);
                if keep > 0 && p[3] > 0 {
                    out.set(x, y, p.map(|c| ((c as u32 * keep + ONE / 2) / ONE) as u16));
                }
            }
        }
    }
    for (&(tx, ty), t) in &src.tiles {
        for (i, &p) in t.0.iter().enumerate() {
            if p[3] == 0 {
                continue;
            }
            let (x, y) = (tx * TILE + i as i32 % TILE, ty * TILE + i as i32 / TILE);
            let k = if whole { ONE } else { sel.amount(x, y) };
            if k == 0 {
                continue;
            }
            let m = p.map(|c| ((c as u32 * k + ONE / 2) / ONE) as u16);
            let (nx, ny) = (x + dx, y + dy);
            let under = out.get(nx, ny);
            // «поверх» в предумноженных значениях
            let inv = ONE - m[3] as u32;
            let v = [0, 1, 2, 3].map(|c| (m[c] as u32 + (under[c] as u32 * inv + ONE / 2) / ONE).min(ONE) as u16);
            out.set(nx, ny, v);
        }
    }
    out.tiles.retain(|_, t| t.0.iter().any(|p| p[3] != 0));
    out
}

/// Пиксели слоя после трансформации матрицей `m` (точка источника → точка результата):
/// без выделения — весь слой, с выделением — выделенная часть поверх остального. Выделение
/// трансформируется так же. Возвращает (пиксели, маска выделения).
pub fn transform_pixels(src: &ColorMap, sel: &Selection, m: [f64; 6], mode: crate::resample::Mode) -> (ColorMap, MaskMap) {
    warp_pixels(src, sel, Warp::Affine(m), mode)
}

/// Отображение точки источника в точку результата.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Warp {
    /// `x' = m0·x + m1·y + m2`, `y' = m3·x + m4·y + m5`.
    Affine([f64; 6]),
    /// Матрица 3×3 по строкам (перспектива).
    Projective([f64; 9]),
    /// Свободная деформация, как в SAI 2017: прямоугольник `src` (x0, y0, x1, y1) в
    /// четырёхугольник `quad` (по часовой от левого верхнего). Точка — смесь билинейного и
    /// перспективного отображения в доле `perspective` (0..=1, ползунок «Perspective» 0–100;
    /// сверено с живой SAI, docs/17-selection-move.md).
    Quad { src: [f64; 4], quad: [[f64; 2]; 4], perspective: f64 },
}

impl Warp {
    fn full(self) -> [f64; 9] {
        match self {
            Warp::Affine(m) => [m[0], m[1], m[2], m[3], m[4], m[5], 0.0, 0.0, 1.0],
            Warp::Projective(m) => m,
            Warp::Quad { src, quad, .. } => {
                let [x0, y0, x1, y1] = src;
                let to_unit = [1.0 / (x1 - x0).max(1e-9), 0.0, -x0 / (x1 - x0).max(1e-9), 0.0, 1.0 / (y1 - y0).max(1e-9), -y0 / (y1 - y0).max(1e-9), 0.0, 0.0, 1.0];
                crate::resample::mul3(crate::resample::square_to_quad(quad), to_unit)
            }
        }
    }

    pub fn apply(self, x: f64, y: f64) -> (f64, f64) {
        let m = self.full();
        let w = m[6] * x + m[7] * y + m[8];
        let proj = ((m[0] * x + m[1] * y + m[2]) / w, (m[3] * x + m[4] * y + m[5]) / w);
        match self {
            Warp::Quad { src, quad, perspective } if perspective < 1.0 => {
                let u = (x - src[0]) / (src[2] - src[0]).max(1e-9);
                let v = (y - src[1]) / (src[3] - src[1]).max(1e-9);
                let [a, b, c, d] = quad;
                let bil = |i: usize| (1.0 - u) * (1.0 - v) * a[i] + u * (1.0 - v) * b[i] + u * v * c[i] + (1.0 - u) * v * d[i];
                let t = perspective;
                ((1.0 - t) * bil(0) + t * proj.0, (1.0 - t) * bil(1) + t * proj.1)
            }
            _ => proj,
        }
    }

    /// Обратное отображение: точка результата → точка источника.
    fn inverse(self) -> Option<impl Fn(f64, f64) -> Option<(f64, f64)>> {
        let inv = crate::resample::invert3(self.full())?;
        let blend = matches!(self, Warp::Quad { perspective, .. } if perspective < 1.0);
        Some(move |x: f64, y: f64| {
            let w = inv[6] * x + inv[7] * y + inv[8];
            if w.abs() < 1e-12 {
                return None;
            }
            let mut p = ((inv[0] * x + inv[1] * y + inv[2]) / w, (inv[3] * x + inv[4] * y + inv[5]) / w);
            if !blend {
                return Some(p);
            }
            // смесь с билинейным: метод Ньютона от перспективного ответа
            for _ in 0..12 {
                let f = self.apply(p.0, p.1);
                let (ex, ey) = (f.0 - x, f.1 - y);
                if ex * ex + ey * ey < 1e-8 {
                    return Some(p);
                }
                let h = 1e-3;
                let fx = self.apply(p.0 + h, p.1);
                let fy = self.apply(p.0, p.1 + h);
                let (a, b, c, d) = ((fx.0 - f.0) / h, (fy.0 - f.0) / h, (fx.1 - f.1) / h, (fy.1 - f.1) / h);
                let det = a * d - b * c;
                if det.abs() < 1e-12 {
                    return None;
                }
                p = (p.0 - (d * ex - b * ey) / det, p.1 - (a * ey - c * ex) / det);
            }
            let f = self.apply(p.0, p.1);
            ((f.0 - x).hypot(f.1 - y) < 0.01).then_some(p)
        })
    }
}

/// Как `transform_pixels`, но с любым отображением `warp`.
pub fn warp_pixels(src: &ColorMap, sel: &Selection, warp: Warp, mode: crate::resample::Mode) -> (ColorMap, MaskMap) {
    let whole = sel.mask.tiles.is_empty();
    // область источника
    let bounds = if whole { content_bounds(src) } else { sel.bounds() };
    let Some((x0, y0, x1, y1)) = bounds else { return (src.clone(), sel.mask.clone()) };
    let Some(inverse) = warp.inverse() else { return (src.clone(), sel.mask.clone()) };
    // область результата — образ углов
    let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].map(|(x, y)| warp.apply(x as f64, y as f64));
    let dx0 = corners.iter().map(|c| c.0).fold(f64::MAX, f64::min).floor() as i32 - 1;
    let dy0 = corners.iter().map(|c| c.1).fold(f64::MAX, f64::min).floor() as i32 - 1;
    let dx1 = corners.iter().map(|c| c.0).fold(f64::MIN, f64::max).ceil() as i32 + 1;
    let dy1 = corners.iter().map(|c| c.1).fold(f64::MIN, f64::max).ceil() as i32 + 1;
    warp_core(src, sel, (x0, y0, x1, y1), (dx0, dy0, dx1, dy1), mode, &[&inverse])
}

/// Обратное отображение: точка результата → точка источника.
pub(crate) type BackMap<'a> = dyn Fn(f64, f64) -> Option<(f64, f64)> + Sync + 'a;

/// Общая часть трансформаций: источник — прямоугольник `bounds` (с выделением — его часть),
/// результат — в прямоугольнике `dest`; `layers` — обратные отображения по слоям наложения
/// (первый — снизу; у деформации сеткой со «Смешиванием» складки ложатся вторым слоем).
pub(crate) fn warp_core(
    src: &ColorMap,
    sel: &Selection,
    (x0, y0, x1, y1): (i32, i32, i32, i32),
    (dx0, dy0, dx1, dy1): (i32, i32, i32, i32),
    mode: crate::resample::Mode,
    layers: &[&BackMap<'_>],
) -> (ColorMap, MaskMap) {
    let whole = sel.mask.tiles.is_empty();
    let (sw, sh) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let k = 1.0 / ONE as f32;
    // источник и доля выделения — плотными буферами (строками плиток, а не поиском на точку)
    let amounts: Vec<u16> = if whole { Vec::new() } else { sel.mask.read_rect(x0, y0, sw, sh) };
    let f_at = |i: usize| if whole { 1.0 } else { amounts[i] as f32 / ONE as f32 };
    let part: Vec<[f32; 4]> = src.read_rect(x0, y0, sw, sh).into_iter().enumerate().map(|(i, p)| {
        let f = f_at(i);
        p.map(|c| c as f32 * k * f)
    }).collect();
    let mpart: Vec<[f32; 4]> = if whole { Vec::new() } else { (0..sw * sh).map(|i| [f_at(i); 4]).collect() };
    let mut out = ColorMap::new();
    if !whole {
        for (&(tx, ty), t) in &src.tiles {
            for (i, &p) in t.0.iter().enumerate() {
                let (x, y) = (tx * TILE + i as i32 % TILE, ty * TILE + i as i32 / TILE);
                let keep = ONE - sel.amount(x, y);
                if keep > 0 && p[3] > 0 {
                    out.set(x, y, p.map(|c| ((c as u32 * keep + ONE / 2) / ONE) as u16));
                }
            }
        }
    }
    let (dw, dh) = ((dx1 - dx0) as usize, (dy1 - dy0) as usize);
    let mut mask = MaskMap::new();
    // результат и маска — плотными буферами области результата, в плитки — в конце
    let mut dout: Vec<[u16; 4]> = out.read_rect(dx0, dy0, dw, dh);
    let mut dmask: Vec<u16> = if whole { Vec::new() } else { vec![0; dw * dh] };
    for inverse in layers {
        // обратное отображение в координаты буфера источника
        let back = |x: f64, y: f64| inverse(x, y).map(|(sx, sy)| (sx - x0 as f64, sy - y0 as f64));
        let moved = crate::resample::sample_map(&part, sw, sh, dx0, dy0, dw, dh, mode, back);
        // маска переноса нужна только при выделении (без него она не читается)
        let mmask = if whole { Vec::new() } else { crate::resample::sample_map(&mpart, sw, sh, dx0, dy0, dw, dh, mode, back) };
        for (i, p) in moved.iter().enumerate() {
            if p[3] > 0.0 {
                let mv = p.map(|c| (c.clamp(0.0, 1.0) * ONE as f32 + 0.5) as u32);
                let under = dout[i];
                let inv_a = ONE - mv[3].min(ONE);
                dout[i] = [0, 1, 2, 3].map(|c| (mv[c] + (under[c] as u32 * inv_a + ONE / 2) / ONE).min(ONE) as u16);
            }
            if !whole {
                let a = (mmask[i][3].clamp(0.0, 1.0) * ONE as f32 + 0.5) as u16;
                dmask[i] = dmask[i].max(a);
            }
        }
    }
    out.write_rect(dx0, dy0, dw, dh, &dout);
    if !whole {
        mask.write_rect(dx0, dy0, dw, dh, &dmask);
    }
    out.tiles.retain(|_, t| t.0.iter().any(|p| p[3] != 0));
    mask.tiles.retain(|_, t| t.0.iter().any(|&v| v != 0));
    (out, mask)
}

/// Выделение по непрозрачности пикселей: доля выделения = альфа.
pub fn from_opacity(src: &ColorMap) -> MaskMap {
    let mut out = MaskMap::new();
    for (k, t) in &src.tiles {
        if t.0.iter().all(|p| p[3] == 0) {
            continue;
        }
        let dst = out.tile_mut(*k);
        for (d, p) in dst.0.iter_mut().zip(t.0.iter()) {
            *d = p[3];
        }
    }
    out
}

/// Границы непустых пикселей слоя.
pub fn content_bounds(src: &ColorMap) -> Option<(i32, i32, i32, i32)> {
    let mut b: Option<(i32, i32, i32, i32)> = None;
    for (&(tx, ty), t) in &src.tiles {
        for (i, p) in t.0.iter().enumerate() {
            if p[3] == 0 {
                continue;
            }
            let (x, y) = (tx * TILE + i as i32 % TILE, ty * TILE + i as i32 / TILE);
            b = Some(match b {
                None => (x, y, x + 1, y + 1),
                Some((a, c, d, e)) => (a.min(x), c.min(y), d.max(x + 1), e.max(y + 1)),
            });
        }
    }
    b
}

/// Ограничивает покрытие `m` (0..=ONE) выделением: доля `sel` (0..=ONE).
#[inline]
pub fn limit(m: u32, sel: u16) -> u32 {
    m * sel as u32 / ONE
}

#[cfg(test)]
mod tests {

    /// Кольцо с разрывом (как в живой проверке SAI 2026): без замкнутых областей — залита вся
    /// обводка; с ними и допуском 0 — ничего (кольцо открыто); с допуском больше половины
    /// разрыва — внутренность до наружного края линии, снаружи пусто.
    #[test]
    fn lasso_fill_enclosed_ring_with_gap() {
        let (w, h) = (300u32, 300u32);
        let mut layer = ColorMap::new();
        // сплошное кольцо r = 98…102 без дуги ±15° справа (зазор ≈ 52 px)
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let (dx, dy) = (x as f64 + 0.5 - 150.0, y as f64 + 0.5 - 150.0);
                let r = (dx * dx + dy * dy).sqrt();
                if (98.0..=102.0).contains(&r) && dy.atan2(dx).to_degrees().abs() > 15.0 {
                    layer.set(x, y, [0, 0, 0, ONE as u16]);
                }
            }
        }
        let lasso = polygon(&[[20.0, 20.0], [280.0, 20.0], [280.0, 280.0], [20.0, 280.0]]);
        let count = |m: &MaskMap| m.tiles.values().map(|t| t.0.iter().filter(|&&v| v as u32 * 2 > ONE).count()).sum::<usize>();
        let plain = lasso_fill(&lasso, &layer, w, h, (20, 20), LassoFill::default());
        assert_eq!(count(&plain), count(&lasso));
        let mut o = LassoFill { enclosed: true, ..Default::default() };
        assert_eq!(count(&lasso_fill(&lasso, &layer, w, h, (20, 20), o)), 0, "открытое кольцо не заливается");
        o.gap = 30;
        let m = lasso_fill(&lasso, &layer, w, h, (20, 20), o);
        assert!(m.get(150, 150) as u32 == ONE, "внутри залито");
        assert!(m.get(150, 51) as u32 == ONE, "линия покрыта");
        assert_eq!(m.get(40, 40), 0, "снаружи пусто");
        assert_eq!(m.get(150, 46), 0, "за линией пусто");
        // зазор закрыт у концов линии: точка в середине зазора залита, далеко за ним — нет
        assert!(m.get(240, 150) as u32 == ONE, "середина зазора");
        assert_eq!(m.get(262, 150), 0, "за зазором");
        // «не трогать закрашенное» — линия не покрыта
        o.ignore_painted = true;
        assert_eq!(lasso_fill(&lasso, &layer, w, h, (20, 20), o).get(150, 50), 0);
    }

    use super::*;

    #[test]
    fn rectangle_and_modes() {
        let mut s = Selection::default();
        assert!(s.is_empty());
        assert_eq!(s.amount(5, 5), ONE);
        s.combine(&rect(0.0, 0.0, 10.0, 10.0), Combine::Replace);
        assert_eq!(s.amount(5, 5), ONE);
        assert_eq!(s.amount(15, 5), 0);
        s.combine(&rect(5.0, 0.0, 20.0, 10.0), Combine::Add);
        assert_eq!(s.amount(15, 5), ONE);
        s.combine(&rect(0.0, 0.0, 3.0, 10.0), Combine::Subtract);
        assert_eq!(s.amount(1, 5), 0);
        s.combine(&rect(10.0, 0.0, 30.0, 10.0), Combine::Intersect);
        assert_eq!((s.amount(8, 5), s.amount(12, 5)), (0, ONE));
        assert_eq!(s.bounds(), Some((10, 0, 20, 10)));
    }

    #[test]
    fn selection_from_layer_opacity() {
        let mut px = ColorMap::new();
        px.set(3, 4, crate::pixel::from_rgba8([10, 20, 30, 128]));
        let m = from_opacity(&px);
        assert_eq!(m.get(3, 4), px.get(3, 4)[3]);
        assert_eq!(m.get(4, 4), 0);
    }

    #[test]
    fn grow_and_shrink_by_one_pixel() {
        let mut s = Selection::default();
        s.combine(&rect(5.0, 5.0, 10.0, 10.0), Combine::Replace);
        s.grow(true);
        assert_eq!(s.bounds(), Some((4, 4, 11, 11)));
        s.grow(false);
        s.grow(false);
        assert_eq!(s.bounds(), Some((6, 6, 9, 9)));
    }

    #[test]
    fn grow_by_cuts_corners_like_sai() {
        // SAI 2017: прямоугольник +25 px — стороны сдвигаются на 25, углы срезаны по 12 px
        let mut s = Selection::default();
        s.combine(&rect(100.0, 100.0, 200.0, 180.0), Combine::Replace);
        s.grow_by(25, true);
        assert_eq!(s.bounds(), Some((75, 75, 225, 205)));
        let top: Vec<i32> = (0..300).filter(|&x| s.amount(x, 75) > 0).collect();
        assert_eq!((top[0], *top.last().unwrap()), (75 + 12, 224 - 12));
        s.grow_by(25, false);
        assert_eq!(s.bounds(), Some((100, 100, 200, 180)));
    }

    #[test]
    fn half_pixel_edge_is_half_selected() {
        let m = rect(0.0, 0.0, 4.5, 4.0);
        assert_eq!(m.get(4, 1) as u32, ONE / 2);
    }

    #[test]
    fn invert_and_outline() {
        let mut s = Selection::default();
        s.combine(&rect(2.0, 2.0, 5.0, 4.0), Combine::Replace);
        assert_eq!(s.outline().len(), 4);
        s.invert(8, 8);
        assert_eq!((s.amount(3, 3), s.amount(0, 0)), (0, ONE));
    }

    #[test]
    fn wand_takes_connected_similar_colour() {
        let mut px = ColorMap::new();
        let red = crate::pixel::from_rgba8([255, 0, 0, 255]);
        for x in 0..10 {
            px.set(x, 0, red);
            px.set(x, 2, red);
        }
        let m = magic_wand(&px, 10, 3, 3, 0, 0);
        assert_eq!(m.get(9, 0) as u32, ONE);
        assert_eq!(m.get(3, 2), 0, "несвязная полоса не выделена");
        assert_eq!(m.get(3, 1), 0);
    }
}

#[cfg(test)]
mod closing_tests {
    use super::*;
    use crate::pixel::from_rgba8;

    /// Квадратная рамка с щелью 3 px: обычная заливка вытекает наружу, с закрытием — нет.
    #[test]
    fn closing_gaps_keeps_fill_inside() {
        let (w, h) = (40u32, 40u32);
        let mut m = ColorMap::new();
        for y in 0..40 {
            for x in 0..40 {
                m.set(x, y, from_rgba8([255, 255, 255, 255]));
            }
        }
        for i in 10..30 {
            for (x, y) in [(i, 10), (i, 29), (10, i), (29, i)] {
                if !(x == 29 && (18..21).contains(&y)) {
                    m.set(x, y, from_rgba8([0, 0, 0, 255]));
                }
            }
        }
        let count = |mask: &MaskMap| mask.tiles.values().map(|t| t.0.iter().filter(|&&v| v > 0).count()).sum::<usize>();
        let leak = magic_wand(&m, w, h, 20, 20, 10);
        assert!(leak.get(2, 2) > 0, "без закрытия заливка вытекает");
        let shut = magic_wand_closing(&m, w, h, 20, 20, WandOptions { tolerance: 10, gap: 2, expand: 0, omissions: false });
        assert_eq!(shut.get(2, 2), 0, "с закрытием наружу не вытекает");
        assert!(shut.get(11, 11) > 0 && shut.get(28, 28) > 0, "углы внутри залиты до линии");
        // внутри рамки 18×18 залито целиком; в щель и за неё — не дальше gap
        assert!((11..29).all(|y| (11..29).all(|x| shut.get(x, y) > 0)));
        assert!((18 * 18..=18 * 18 + 3 * 2 + 4).contains(&count(&shut)), "лишнее: {}", count(&shut) - 18 * 18);
        let grown = magic_wand_closing(&m, w, h, 20, 20, WandOptions { tolerance: 10, gap: 2, expand: 1, omissions: false });
        assert!(grown.get(10, 15) > 0 && count(&grown) > count(&shut), "запас уходит под линию");
    }

    /// «Уменьшить мелкие пропуски»: полупрозрачный хвост мазка (непрозрачность ≤ 80 %) у залитой
    /// области заливается, сплошная линия — нет.
    #[test]
    fn omissions_fill_faint_tips() {
        let (w, h) = (20u32, 20u32);
        let mut m = ColorMap::new();
        // сплошная линия x = 10 и бледный хвост у её конца: (11..13, 5)
        for y in 5..20 {
            m.set(10, y, from_rgba8([0, 0, 0, 255]));
        }
        for x in 11..13 {
            m.set(x, 5, from_rgba8([0, 0, 0, 90]));
        }
        let plain = magic_wand_closing(&m, w, h, 2, 2, WandOptions { tolerance: 0, gap: 0, expand: 0, omissions: false });
        let reduced = magic_wand_closing(&m, w, h, 2, 2, WandOptions { tolerance: 0, gap: 0, expand: 0, omissions: true });
        assert_eq!(plain.get(11, 5), 0);
        assert!(reduced.get(11, 5) > 0 && reduced.get(12, 5) > 0, "бледный хвост залит");
        assert_eq!(reduced.get(10, 10), 0, "сплошная линия не залита");
    }
}
