//! Перо слоя контура: штрих превращается в путь из узлов с ручками «Normal» и точками нажима.
//! Правила — как у образца, своими словами в `docs/11-linework-shapes.md` (разделы 4 и 6).

use crate::vector::{cubic_at, segments, Node, Pt};

fn sub(a: Pt, b: Pt) -> Pt {
    [a[0] - b[0], a[1] - b[1]]
}
fn len(a: Pt) -> f64 {
    a[0].hypot(a[1])
}
fn dot(a: Pt, b: Pt) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

/// Ручки вида «Normal»: три прохода с коэффициентами 0,398, 0,552, 0,506. Острые узлы и концы
/// открытого пути — ручки в самой точке. Возвращает (входящая, исходящая) для каждого узла.
pub fn normal_handles(points: &[Pt], pointed: &[bool], closed: bool) -> Vec<(Pt, Pt)> {
    let n = points.len();
    let mut h: Vec<(Pt, Pt)> = points.iter().map(|&p| (p, p)).collect();
    if n < 2 {
        return h;
    }
    for coef in [0.398, 0.552, 0.506] {
        let prev = h.clone();
        for i in 0..n {
            if pointed.get(i).copied().unwrap_or(false) || (!closed && (i == 0 || i == n - 1)) {
                h[i] = (points[i], points[i]);
                continue;
            }
            let (ia, ib) = ((i + n - 1) % n, (i + 1) % n);
            let p = points[i];
            // первый проход — соседние точки, дальше — их ручки, смотрящие на этот узел
            let (a, b) = if coef == 0.398 { (points[ia], points[ib]) } else { (prev[ia].1, prev[ib].0) };
            let (da, db) = (len(sub(p, a)), len(sub(b, p)));
            if da < 1e-9 || db < 1e-9 {
                h[i] = (p, p);
                continue;
            }
            let ua = [(p[0] - a[0]) / da, (p[1] - a[1]) / da];
            let ub = [(b[0] - p[0]) / db, (b[1] - p[1]) / db];
            let delta = (ua[0] * ub[1] - ua[1] * ub[0]).atan2(dot(ua, ub));
            let m = (da + db) * 0.5;
            let k = delta.abs() / std::f64::consts::PI;
            let (a2, b2) = (da + (m - da) * k, db + (m - db) * k);
            let rot = delta * a2 / (a2 + b2);
            let (s, c) = rot.sin_cos();
            let t = [ua[0] * c - ua[1] * s, ua[0] * s + ua[1] * c];
            let mut li = dot(sub(p, a), t).abs() * coef;
            let mut lo = dot(sub(b, p), t).abs() * coef;
            let small = li.min(lo);
            if li > 0.0 {
                li *= (5.0 * small / li).min(1.0).sqrt();
            }
            if lo > 0.0 {
                lo *= (5.0 * small / lo).min(1.0).sqrt();
            }
            h[i] = ([p[0] - t[0] * li, p[1] - t[1] * li], [p[0] + t[0] * lo, p[1] + t[1] * lo]);
        }
    }
    h
}

/// Точка штриха: место и нажим 0..1.
#[derive(Clone, Copy, Debug)]
pub struct StrokePoint {
    pub at: Pt,
    pub pressure: f64,
}

/// Узел готового пути.
#[derive(Clone, Debug, PartialEq)]
pub struct PathNode {
    pub node: Node,
    pub pointed: bool,
    pub weight: f64,
    /// Точки нажима отрезка до следующего узла: (доля длины дуги, нажим).
    pub pressure: Vec<(f64, f64)>,
}

/// Длины дуги ломаной от начала.
fn arc(points: &[StrokePoint]) -> Vec<f64> {
    let mut s = vec![0.0];
    for w in points.windows(2) {
        s.push(s.last().unwrap() + len(sub(w[1].at, w[0].at)));
    }
    s
}

/// Точка кривой отрезка на доле `f` длины его дуги.
fn at_fraction(seg: &[Pt; 4], f: f64) -> Pt {
    const N: usize = 64;
    let mut pts = Vec::with_capacity(N + 1);
    let mut total = 0.0;
    let mut acc = vec![0.0];
    pts.push(seg[0]);
    for k in 1..=N {
        let p = cubic_at(seg, k as f64 / N as f64);
        total += len(sub(p, *pts.last().unwrap()));
        acc.push(total);
        pts.push(p);
    }
    let want = f.clamp(0.0, 1.0) * total;
    let j = acc.iter().position(|&a| a >= want).unwrap_or(N).max(1);
    let (a0, a1) = (acc[j - 1], acc[j]);
    let t = if a1 > a0 { (want - a0) / (a1 - a0) } else { 0.0 };
    [pts[j - 1][0] + (pts[j][0] - pts[j - 1][0]) * t, pts[j - 1][1] + (pts[j][1] - pts[j - 1][1]) * t]
}

/// Штрих → путь. `tolerance` — допуск в пикселях («Tolerance» в SAI). `None` — штрих слишком
/// короткий. Узлов не больше 100.
pub fn vectorize(raw: &[StrokePoint], tolerance: f64) -> Option<(Vec<PathNode>, bool)> {
    let tol = tolerance.max(0.1);
    // 1. повторы
    let mut pts: Vec<StrokePoint> = Vec::new();
    for p in raw {
        match pts.last_mut() {
            Some(q) if len(sub(q.at, p.at)) < 1e-6 => q.pressure = q.pressure.min(p.pressure),
            _ => pts.push(*p),
        }
    }
    let s = arc(&pts);
    // 2. слишком короткий
    if pts.len() < 2 || *s.last().unwrap() < tol.max(1.0) {
        return None;
    }
    // 3. крючки на концах
    let trim = |pts: &[StrokePoint]| -> usize {
        let s = arc(pts);
        let reach = 6.0 * tol;
        let Some(j) = s.iter().position(|&v| v >= reach) else { return 0 };
        let main = sub(pts[j].at, pts[0].at);
        let ml = len(main);
        if ml < 1e-9 {
            return 0;
        }
        let mut cut = 0;
        for i in 0..j {
            let d = sub(pts[i + 1].at, pts[i].at);
            let dl = len(d);
            if dl > 1e-9 && dot(d, main) / (dl * ml) <= 0.0 {
                cut = i + 1;
            }
        }
        cut
    };
    let head = trim(&pts);
    pts.drain(..head.min(pts.len().saturating_sub(2)));
    pts.reverse();
    let tail = trim(&pts);
    pts.drain(..tail.min(pts.len().saturating_sub(2)));
    pts.reverse();
    let s = arc(&pts);
    let closed = pts.len() > 3 && len(sub(pts[0].at, pts[pts.len() - 1].at)) < 0.1;
    // 5. резкие повороты — кандидаты в острые углы
    let mut sharp = vec![false; pts.len()];
    for i in 1..pts.len().saturating_sub(1) {
        let (a, b) = (sub(pts[i].at, pts[i - 1].at), sub(pts[i + 1].at, pts[i].at));
        let (la, lb) = (len(a), len(b));
        if la > 1e-9 && lb > 1e-9 && dot(a, b) / (la * lb) < 0.95 {
            sharp[i] = true;
        }
    }
    // 6. подбор узлов: начать с концов, добавлять худшую точку
    let last = pts.len() - 1;
    let mut idx: Vec<usize> = vec![0, last];
    if closed && !sharp.iter().any(|&v| v) {
        let far = (1..last).max_by(|&a, &b| len(sub(pts[a].at, pts[0].at)).total_cmp(&len(sub(pts[b].at, pts[0].at)))).unwrap_or(last / 2);
        sharp[far] = true;
        idx = vec![0, far, last];
    }
    let build = |idx: &[usize]| -> Vec<Node> {
        let points: Vec<Pt> = idx.iter().map(|&i| pts[i].at).collect();
        let pointed: Vec<bool> = idx.iter().map(|&i| sharp[i] && i != 0 && i != last).collect();
        normal_handles(&points, &pointed, false)
            .into_iter()
            .zip(points)
            .map(|((hi, ho), p)| Node { point: p, handle_in: hi, handle_out: ho })
            .collect()
    };
    loop {
        let nodes = build(&idx);
        let segs = segments(&nodes, false);
        let mut worst = (0.0, 0usize);
        for (k, seg) in segs.iter().enumerate() {
            let (i0, i1) = (idx[k], idx[k + 1]);
            let span = s[i1] - s[i0];
            for (i, p) in pts.iter().enumerate().take(i1).skip(i0 + 1) {
                let f = if span > 0.0 { (s[i] - s[i0]) / span } else { 0.0 };
                let d = len(sub(at_fraction(seg, f), p.at));
                if d > worst.0 {
                    worst = (d, i);
                }
            }
        }
        if worst.0 < tol || idx.len() >= 100 {
            break;
        }
        let pos = idx.partition_point(|&i| i < worst.1);
        idx.insert(pos, worst.1);
    }
    let nodes = build(&idx);
    // 7. нажим: в узлах — нажим штриха; точки нажима там, где кривая нажима расходится на 0,05+
    let mut out: Vec<PathNode> = idx
        .iter()
        .zip(&nodes)
        .map(|(&i, n)| PathNode { node: *n, pointed: sharp[i] && i != 0 && i != last, weight: pts[i].pressure, pressure: Vec::new() })
        .collect();
    for k in 0..idx.len() - 1 {
        let (i0, i1) = (idx[k], idx[k + 1]);
        let span = s[i1] - s[i0];
        for _ in 0..3 {
            let (w0, w1) = (out[k].weight, out[k + 1].weight);
            let mut knots = vec![(0.0, w0)];
            knots.extend(out[k].pressure.iter().copied());
            knots.push((1.0, w1));
            let curve = |f: f64| -> f64 {
                let j = knots.windows(2).position(|w| f <= w[1].0).unwrap_or(knots.len() - 2);
                let (a, b) = (knots[j], knots[j + 1]);
                if b.0 <= a.0 { b.1 } else { a.1 + (b.1 - a.1) * (f - a.0) / (b.0 - a.0) }
            };
            let mut worst = (0.0, 0.0, 0.0);
            for i in i0 + 1..i1 {
                let f = if span > 0.0 { (s[i] - s[i0]) / span } else { 0.0 };
                let d = (curve(f) - pts[i].pressure).abs();
                if d > worst.0 {
                    worst = (d, f, pts[i].pressure);
                }
            }
            if worst.0 < 0.05 {
                break;
            }
            let p = &mut out[k].pressure;
            let at = p.partition_point(|q| q.0 < worst.1);
            p.insert(at, (worst.1, worst.2));
        }
    }
    Some((out, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_stroke_becomes_two_nodes() {
        let raw: Vec<StrokePoint> = (0..=100).map(|i| StrokePoint { at: [i as f64, 10.0], pressure: 1.0 }).collect();
        let (nodes, _) = vectorize(&raw, 1.0).unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[1].node.point, [100.0, 10.0]);
    }

    #[test]
    fn arc_is_followed_within_tolerance() {
        let raw: Vec<StrokePoint> = (0..=180).map(|i| {
            let a = (i as f64).to_radians();
            StrokePoint { at: [100.0 + 80.0 * a.cos(), 100.0 + 80.0 * a.sin()], pressure: 0.5 + 0.5 * (i as f64 / 180.0) }
        }).collect();
        let (nodes, _) = vectorize(&raw, 1.0).unwrap();
        assert!(nodes.len() >= 3 && nodes.len() < 12, "узлов {}", nodes.len());
        let ns: Vec<Node> = nodes.iter().map(|n| n.node).collect();
        for seg in segments(&ns, false) {
            for k in 0..=10 {
                let p = cubic_at(&seg, k as f64 / 10.0);
                let r = len(sub(p, [100.0, 100.0]));
                assert!((r - 80.0).abs() < 2.5, "отклонение {}", r - 80.0);
            }
        }
        assert!((nodes[0].weight - 0.5).abs() < 0.01 && (nodes.last().unwrap().weight - 1.0).abs() < 0.01);
    }

    #[test]
    fn corner_becomes_a_pointed_node() {
        let mut raw: Vec<StrokePoint> = (0..=50).map(|i| StrokePoint { at: [i as f64, 0.0], pressure: 1.0 }).collect();
        raw.extend((1..=50).map(|i| StrokePoint { at: [50.0, i as f64], pressure: 1.0 }));
        let (nodes, _) = vectorize(&raw, 1.0).unwrap();
        assert!(nodes.iter().any(|n| n.pointed && (n.node.point[0] - 50.0).abs() < 1.5 && n.node.point[1].abs() < 1.5), "{nodes:?}");
    }
}
