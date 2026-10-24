//! Модель → линии: очертания (край между лицевой и задней гранью), края открытых сеток и сгибы
//! (соседние грани под углом больше заданного), только видимые — за другими гранями
//! прячутся. Результат — ломаные в точках холста, их кладут на слой контура.

use crate::model::Model;
use crate::render::{geometry, project};
use crate::scene::Scene;
use glam::{Vec2, Vec3};
use std::collections::HashMap;
use std::sync::Arc;

/// Наибольшая сторона буфера глубины (больший холст — проверка видимости в уменьшенном).
const DEPTH_SIDE: u32 = 2048;

/// Буфер глубины: расстояние вдоль взгляда до ближайшей грани в каждой точке.
struct Depth {
    w: usize,
    h: usize,
    k: f32,
    z: Vec<f32>,
}

impl Depth {
    fn new(w: u32, h: u32) -> Self {
        let k = (DEPTH_SIDE as f32 / w.max(h) as f32).min(1.0);
        let (w, h) = (((w as f32 * k).ceil() as usize).max(1), ((h as f32 * k).ceil() as usize).max(1));
        Self { w, h, k, z: vec![f32::INFINITY; w * h] }
    }

    /// Треугольник: точки холста и глубина вершин (1/z меняется по экрану линейно).
    fn tri(&mut self, p: [Vec2; 3], z: [f32; 3]) {
        let p = p.map(|v| v * self.k);
        let area = (p[1] - p[0]).perp_dot(p[2] - p[0]);
        if area.abs() < 1e-9 {
            return;
        }
        let x0 = p.iter().map(|v| v.x).fold(f32::INFINITY, f32::min).floor().max(0.0) as usize;
        let x1 = (p.iter().map(|v| v.x).fold(f32::NEG_INFINITY, f32::max).ceil() as usize).min(self.w);
        let y0 = p.iter().map(|v| v.y).fold(f32::INFINITY, f32::min).floor().max(0.0) as usize;
        let y1 = (p.iter().map(|v| v.y).fold(f32::NEG_INFINITY, f32::max).ceil() as usize).min(self.h);
        let iz = z.map(|v| 1.0 / v);
        for y in y0..y1 {
            for x in x0..x1 {
                let c = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let b0 = (p[1] - c).perp_dot(p[2] - c) / area;
                let b1 = (p[2] - c).perp_dot(p[0] - c) / area;
                let b2 = 1.0 - b0 - b1;
                if b0 < 0.0 || b1 < 0.0 || b2 < 0.0 {
                    continue;
                }
                let d = 1.0 / (b0 * iz[0] + b1 * iz[1] + b2 * iz[2]);
                let o = y * self.w + x;
                if d < self.z[o] {
                    self.z[o] = d;
                }
            }
        }
    }

    /// Видна ли точка глубины `z`: не дальше ближайшей грани около неё (3×3 — чтобы края самих
    /// граней не прятали свою же линию).
    fn sees(&self, p: Vec2, z: f32) -> bool {
        let (x, y) = ((p.x * self.k) as isize, (p.y * self.k) as isize);
        let mut far = 0.0f32;
        let mut any = false;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (xx, yy) = (x + dx, y + dy);
                if xx < 0 || yy < 0 || xx as usize >= self.w || yy as usize >= self.h {
                    continue;
                }
                any = true;
                far = far.max(self.z[yy as usize * self.w + xx as usize]);
            }
        }
        !any || z <= far * 1.002 + 1e-4
    }
}

/// Линии сцены на холсте `w`×`h`: сгибы — у граней под углом больше `crease` градусов.
pub fn lines(scene: &Scene, models: &[Arc<Model>], w: u32, h: u32, crease: f32) -> Vec<Vec<[f32; 2]>> {
    let g = geometry(scene, models);
    if g.tris.is_empty() {
        return Vec::new();
    }
    let eye = scene.camera.eye();
    let fwd = (scene.camera.target - eye).normalize_or(Vec3::NEG_Z);
    let depth_of = |p: Vec3| (p - eye).dot(fwd);
    // вершины с одним местом — одна вершина (швы текстур делят вершины)
    let (lo, hi) = g.pos.iter().fold((Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)), |(a, b), p| (a.min(*p), b.max(*p)));
    let q = ((hi - lo).max_element() * 1e-5).max(1e-9);
    let mut weld: HashMap<[i64; 3], u32> = HashMap::new();
    let id: Vec<u32> = g
        .pos
        .iter()
        .map(|p| {
            let k = [(p.x / q).round() as i64, (p.y / q).round() as i64, (p.z / q).round() as i64];
            let n = weld.len() as u32;
            *weld.entry(k).or_insert(n)
        })
        .collect();
    let mut pos = vec![Vec3::ZERO; weld.len()];
    for (i, &k) in id.iter().enumerate() {
        pos[k as usize] = g.pos[i];
    }
    let screen: Vec<Option<Vec2>> = pos.iter().map(|p| if depth_of(*p) > 1e-6 { project(scene, w, h, *p) } else { None }).collect();
    // грани: нормаль и «смотрит на камеру»; заодно буфер глубины
    let mut depth = Depth::new(w, h);
    let mut normal = Vec::with_capacity(g.tris.len());
    let mut front = Vec::with_capacity(g.tris.len());
    let mut edges: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
    for (ti, t) in g.tris.iter().enumerate() {
        let v = t.map(|i| id[i as usize]);
        let p = v.map(|i| pos[i as usize]);
        let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
        normal.push(n);
        // у двусторонних граней «лицом» тоже считается сторона по обходу: иначе у них нет очертания
        front.push(n.dot(eye - p[0]) > 0.0);
        if let [Some(a), Some(b), Some(c)] = v.map(|i| screen[i as usize]) {
            depth.tri([a, b, c], p.map(depth_of));
        }
        for (a, b) in [(v[0], v[1]), (v[1], v[2]), (v[2], v[0])] {
            if a != b {
                edges.entry((a.min(b), a.max(b))).or_default().push(ti as u32);
            }
        }
    }
    let cos_crease = crease.clamp(0.0, 180.0).to_radians().cos();
    // кусок ребра, видимый целиком, — в граф (для сборки в длинные ломаные); видимый отчасти —
    // отдельными ломаными
    let mut graph: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut out: Vec<Vec<[f32; 2]>> = Vec::new();
    for (&(a, b), faces) in &edges {
        let line = match faces.as_slice() {
            [f] => front[*f as usize],
            [f, k] => {
                let (f, k) = (*f as usize, *k as usize);
                (front[f] != front[k]) || (front[f] && normal[f].dot(normal[k]) < cos_crease)
            }
            // больше двух граней на ребре — край частей, рисуется, если видна хоть одна
            many => many.iter().any(|&f| front[f as usize]),
        };
        if !line {
            continue;
        }
        let (Some(sa), Some(sb)) = (screen[a as usize], screen[b as usize]) else { continue };
        let (za, zb) = (depth_of(pos[a as usize]), depth_of(pos[b as usize]));
        let steps = ((sb - sa).length() / 1.5).ceil().clamp(1.0, 4000.0) as usize;
        let seen: Vec<bool> = (0..=steps)
            .map(|k| {
                let t = k as f32 / steps as f32;
                // глубина по экрану — через 1/z
                let z = 1.0 / ((1.0 - t) / za + t / zb);
                depth.sees(sa.lerp(sb, t), z)
            })
            .collect();
        if seen.iter().all(|&s| s) {
            graph.entry(a).or_default().push(b);
            graph.entry(b).or_default().push(a);
            continue;
        }
        // видимые отрезки ребра: прямые, хватает концов; короче 3 точек — край соседней грани
        let mut from: Option<usize> = None;
        for k in 0..=seen.len() {
            match (from, seen.get(k).copied().unwrap_or(false)) {
                (None, true) => from = Some(k),
                (Some(f), false) => {
                    let (p0, p1) = (sa.lerp(sb, f as f32 / steps as f32), sa.lerp(sb, (k - 1) as f32 / steps as f32));
                    if (p1 - p0).length() >= 3.0 {
                        out.push(vec![[p0.x, p0.y], [p1.x, p1.y]]);
                    }
                    from = None;
                }
                _ => {}
            }
        }
    }
    // сборка графа в ломаные: сначала от концов и развилок, потом замкнутые
    let mut used: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    let key = |a: u32, b: u32| (a.min(b), a.max(b));
    let mut starts: Vec<u32> = graph.iter().filter(|(_, n)| n.len() != 2).map(|(&v, _)| v).collect();
    starts.sort_unstable();
    let mut rest: Vec<u32> = graph.keys().copied().collect();
    rest.sort_unstable();
    for s in starts.into_iter().chain(rest) {
        let Some(ns) = graph.get(&s) else { continue };
        for &first in ns {
            if used.contains(&key(s, first)) {
                continue;
            }
            let mut poly = vec![s];
            let (mut prev, mut cur) = (s, first);
            used.insert(key(s, first));
            loop {
                poly.push(cur);
                let n = &graph[&cur];
                if n.len() != 2 {
                    break;
                }
                let next = if n[0] == prev { n[1] } else { n[0] };
                if used.contains(&key(cur, next)) {
                    if next == poly[0] {
                        poly.push(next);
                    }
                    break;
                }
                used.insert(key(cur, next));
                (prev, cur) = (cur, next);
            }
            out.push(poly.iter().filter_map(|&v| screen[v as usize]).map(|p| [p.x, p.y]).collect());
        }
    }
    if std::env::var_os("GS_LINES_DEBUG").is_some() {
        eprintln!("рёбер {} граф {} кусков {} лицевых {}/{}", edges.len(), graph.len(), out.len(), front.iter().filter(|&&f| f).count(), front.len());
    }
    // крошки короче точки не нужны
    out.retain(|l| l.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum::<f32>() >= 1.0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Mesh, Node, Prim};
    use crate::scene::Object;

    /// Куб 1×1×1 с центром в (0, 1, 0), грани наружу.
    fn cube() -> Model {
        let mut pos = Vec::new();
        let mut idx = Vec::new();
        for axis in 0..3 {
            for side in [-1.0f32, 1.0] {
                let n = Vec3::from_array(std::array::from_fn(|k| if k == axis { side } else { 0.0 }));
                let (u, v) = (Vec3::from_array(std::array::from_fn(|k| if k == (axis + 1) % 3 { 1.0 } else { 0.0 })), Vec3::from_array(std::array::from_fn(|k| if k == (axis + 2) % 3 { 1.0 } else { 0.0 })));
                let c = Vec3::new(0.0, 1.0, 0.0) + n * 0.5;
                let b = pos.len() as u32;
                for (x, y) in [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)] {
                    pos.push(c + u * x + v * y);
                }
                // наружу: (u × v) смотрит вдоль n при side > 0
                if side > 0.0 {
                    idx.extend([b, b + 1, b + 2, b, b + 2, b + 3]);
                } else {
                    idx.extend([b, b + 2, b + 1, b, b + 3, b + 2]);
                }
            }
        }
        let prim = Prim { pos, idx, color: [1.0; 4], ..Default::default() };
        let node = Node { name: "cube".into(), parent: None, children: vec![], t: Vec3::ZERO, r: glam::Quat::IDENTITY, s: Vec3::ONE, mesh: Some(0), skin: None, joint: false };
        Model { nodes: vec![node], roots: vec![0], meshes: vec![Mesh { name: "cube".into(), prims: vec![prim], ..Default::default() }], triangles: 12, ..Default::default() }
    }

    #[test]
    fn cube_has_its_visible_edges() {
        let mut scene = Scene::default();
        scene.objects.push(Object::new(1));
        scene.camera.yaw = 30.0;
        scene.camera.pitch = 25.0;
        let models = vec![Arc::new(cube())];
        let l = lines(&scene, &models, 800, 600, 40.0);
        // у куба сверху и под углом видно 9 рёбер из 12; диагонали граней — не сгибы
        let segs: usize = l.iter().map(|p| p.len() - 1).sum();
        assert_eq!(segs, 9, "{l:?}");
        // без сгибов — только очертание из 6 рёбер
        let l = lines(&scene, &models, 800, 600, 180.0);
        assert_eq!(l.iter().map(|p| p.len() - 1).sum::<usize>(), 6, "{l:?}");
    }
}
