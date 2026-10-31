//! Отрисовка сцены на процессоре: поза и скелет → треугольники на экране → полосы строк
//! параллельно (z-буфер, текстуры с поправкой на перспективу, свет по вершинам) → контур →
//! сглаживание (рисуем в `ss` раз крупнее и уменьшаем).

use crate::model::{Image, Model};
use crate::scene::{Display, Scene};
use glam::{Mat4, Vec2, Vec3, Vec4};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Высота полосы строк, которую рисует один поток.
const BAND: usize = 32;

struct Mat {
    color: [f32; 4],
    tex: Option<usize>,
    cutoff: Option<f32>,
    double: bool,
    model: usize,
}

/// Треугольники сцены в мире.
#[derive(Default)]
pub(crate) struct Geo {
    pub(crate) pos: Vec<Vec3>,
    nrm: Vec<Vec3>,
    uv: Vec<Vec2>,
    /// Вершины, у которых нет своих нормалей, — тогда нормаль грани.
    has_nrm: Vec<bool>,
    pub(crate) tris: Vec<[u32; 3]>,
    mat_of: Vec<u32>,
    mats: Vec<Mat>,
    /// Номер части (сетки) — для контура между частями.
    part_of: Vec<u32>,
}

/// Вершины сцены в мире (поза, скелет, морфы).
pub(crate) fn geometry(scene: &Scene, models: &[Arc<Model>]) -> Geo {
    let mut g = Geo::default();
    let mut part = 0u32;
    for (oi, o) in scene.objects.iter().enumerate() {
        let Some(m) = models.get(oi) else { continue };
        if !o.visible {
            continue;
        }
        let globals = o.globals(m);
        let hidden = o.hidden_nodes(m);
        let skin_mats: Vec<Vec<Mat4>> = m.skins.iter().map(|s| s.joints.iter().zip(&s.inv_bind).map(|(&j, ib)| globals[j] * *ib).collect()).collect();
        for (ni, node) in m.nodes.iter().enumerate() {
            let Some(mi) = node.mesh else { continue };
            if hidden[ni] {
                continue;
            }
            let mesh = &m.meshes[mi];
            for p in &mesh.prims {
                part += 1;
                let base = g.pos.len() as u32;
                let weights: Vec<f32> = (0..p.morphs.len()).map(|k| o.morph(m, mi, k)).collect();
                let skin = node.skin.filter(|_| !p.joints.is_empty()).map(|s| &skin_mats[s]);
                let mat_node = globals[ni];
                for v in 0..p.pos.len() {
                    let mut pos = p.pos[v];
                    for (k, w) in weights.iter().enumerate() {
                        if *w != 0.0 {
                            pos += p.morphs[k][v] * *w;
                        }
                    }
                    let mtx = match skin {
                        Some(sm) => {
                            let (j, w) = (p.joints[v], p.weights[v]);
                            let total: f32 = w.iter().sum();
                            if total <= 1e-6 {
                                mat_node
                            } else {
                                let mut acc = Mat4::ZERO;
                                for k in 0..4 {
                                    if w[k] != 0.0 {
                                        if let Some(jm) = sm.get(j[k] as usize) {
                                            acc += *jm * (w[k] / total);
                                        }
                                    }
                                }
                                acc
                            }
                        }
                        None => mat_node,
                    };
                    g.pos.push(mtx.transform_point3(pos));
                    let has = !p.nrm.is_empty();
                    g.nrm.push(if has { mtx.transform_vector3(p.nrm[v]).normalize_or_zero() } else { Vec3::ZERO });
                    g.has_nrm.push(has);
                    g.uv.push(p.uv.get(v).copied().unwrap_or(Vec2::ZERO));
                }
                g.mats.push(Mat { color: p.color, tex: p.tex, cutoff: p.cutoff, double: p.double, model: oi });
                let mid = g.mats.len() as u32 - 1;
                for t in p.idx.as_chunks::<3>().0 {
                    g.tris.push([base + t[0], base + t[1], base + t[2]]);
                    g.mat_of.push(mid);
                    g.part_of.push(part);
                }
            }
        }
    }
    g
}

/// Сцена в мире для видеокарты: вершины (x, y, z, нормаль, u, v) и куски по материалам.
#[derive(Default)]
pub struct GpuMesh {
    pub vertices: Vec<[f32; 8]>,
    pub indices: Vec<u32>,
    pub draws: Vec<GpuDraw>,
    /// Границы вершин (для дальней плоскости).
    pub min: [f32; 3],
    pub max: [f32; 3],
}

/// Кусок треугольников одного материала.
pub struct GpuDraw {
    pub first: u32,
    pub count: u32,
    pub color: [f32; 4],
    /// Номер модели и картинки в ней.
    pub tex: Option<(usize, usize)>,
    pub cutoff: Option<f32>,
    pub double: bool,
}

/// Поза, скелет и морфы посчитаны, камера ещё нет: пока вращают камеру, это не меняется.
pub fn gpu_mesh(scene: &Scene, models: &[Arc<Model>]) -> GpuMesh {
    let g = geometry(scene, models);
    let mut m = GpuMesh { vertices: Vec::with_capacity(g.pos.len()), indices: Vec::with_capacity(g.tris.len() * 3), ..Default::default() };
    for i in 0..g.pos.len() {
        let (p, n, uv) = (g.pos[i], g.nrm[i], g.uv[i]);
        m.vertices.push([p.x, p.y, p.z, n.x, n.y, n.z, uv.x, uv.y]);
    }
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in &g.pos {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    (m.min, m.max) = (lo.to_array(), hi.to_array());
    let mut ti = 0;
    while ti < g.tris.len() {
        let mid = g.mat_of[ti];
        let first = m.indices.len() as u32;
        while ti < g.tris.len() && g.mat_of[ti] == mid {
            let t = g.tris[ti];
            ti += 1;
            let [a, b, c] = t.map(|i| i as usize);
            let face = (g.pos[b] - g.pos[a]).cross(g.pos[c] - g.pos[a]);
            if face.length_squared() == 0.0 || !face.is_finite() {
                continue;
            }
            if g.has_nrm[a] && g.has_nrm[b] && g.has_nrm[c] {
                m.indices.extend_from_slice(&t);
            } else {
                // своих нормалей нет — нормаль грани: вершины не общие
                let f = face.normalize();
                for i in [a, b, c] {
                    let (p, uv) = (g.pos[i], g.uv[i]);
                    m.indices.push(m.vertices.len() as u32);
                    m.vertices.push([p.x, p.y, p.z, f.x, f.y, f.z, uv.x, uv.y]);
                }
            }
        }
        let mat = &g.mats[mid as usize];
        m.draws.push(GpuDraw { first, count: m.indices.len() as u32 - first, color: mat.color, tex: mat.tex.map(|t| (mat.model, t)), cutoff: mat.cutoff, double: mat.double });
    }
    m
}

/// Камера для видеокарты: мир → экран и направление света, как в `render`.
pub fn gpu_camera(scene: &Scene, mesh: &GpuMesh, w: u32, h: u32) -> (Mat4, Vec3) {
    let cam = &scene.camera;
    let view = cam.view();
    let mut far = 0f32;
    for k in 0..8 {
        let pick = |i: usize| if k >> i & 1 == 0 { mesh.min[i] } else { mesh.max[i] };
        far = far.max(-(view.transform_point3(Vec3::new(pick(0), pick(1), pick(2)))).z);
    }
    let far = far.max(1e-3) * 1.05 + 1e-3;
    let near = (far * 1e-4).max(1e-5);
    let proj = glam::camera::rh::proj::directx::perspective(cam.fov.clamp(1.0, 150.0).to_radians(), w.max(1) as f32 / h.max(1) as f32, near, far);
    let (ly, lp) = (scene.light[0].to_radians(), scene.light[1].to_radians());
    let light = view.inverse().transform_vector3(Vec3::new(lp.cos() * ly.sin(), lp.sin(), lp.cos() * ly.cos())).normalize_or(Vec3::Y);
    (proj * view, light)
}

/// Вершина треугольника на экране: x, y (px), z (0..1), 1/w и деленные на w: u, v, свет.
#[derive(Clone, Copy, Default)]
struct SV {
    x: f32,
    y: f32,
    z: f32,
    iw: f32,
    u: f32,
    v: f32,
    l: f32,
}

struct STri {
    v: [SV; 3],
    mat: u32,
    part: u32,
    y0: f32,
    y1: f32,
}

/// Сцена, отрисованная в рамку `w`×`h` (RGBA по 8 бит без предумножения; прозрачно, где пусто).
pub fn render(scene: &Scene, models: &[Arc<Model>], w: u32, h: u32, ss: u32) -> Vec<u8> {
    let ss = ss.clamp(1, 4) as usize;
    let (w, h) = (w.max(1) as usize, h.max(1) as usize);
    let (fw, fh) = (w * ss, h * ss);
    let g = geometry(scene, models);
    if g.tris.is_empty() {
        return vec![0; w * h * 4];
    }
    let cam = &scene.camera;
    let view = cam.view();
    // ближняя и дальняя плоскости — по тому, что действительно есть в кадре
    let mut far = 0f32;
    for p in &g.pos {
        far = far.max(-(view.transform_point3(*p)).z);
    }
    let far = far.max(1e-3) * 1.05 + 1e-3;
    let near = (far * 1e-4).max(1e-5);
    let proj = glam::camera::rh::proj::directx::perspective(cam.fov.clamp(1.0, 150.0).to_radians(), fw as f32 / fh as f32, near, far);
    let vp = proj * view;
    let eye = cam.eye();
    // свет — относительно камеры
    let inv_view = view.inverse();
    let (ly, lp) = (scene.light[0].to_radians(), scene.light[1].to_radians());
    let light = inv_view.transform_vector3(Vec3::new(lp.cos() * ly.sin(), lp.sin(), lp.cos() * ly.cos())).normalize_or(Vec3::Y);
    let clip: Vec<Vec4> = g.pos.iter().map(|p| vp * p.extend(1.0)).collect();
    let shade = |n: Vec3| 0.32 + 0.68 * n.dot(light).max(0.0);
    // треугольники на экран: отсечение ближней плоскостью, задние стороны
    let mut tris: Vec<STri> = Vec::with_capacity(g.tris.len());
    for (ti, t) in g.tris.iter().enumerate() {
        let mat = &g.mats[g.mat_of[ti] as usize];
        let [a, b, c] = t.map(|i| i as usize);
        let face = (g.pos[b] - g.pos[a]).cross(g.pos[c] - g.pos[a]);
        if face.length_squared() == 0.0 || !face.is_finite() {
            continue;
        }
        let face = face.normalize();
        let toward = face.dot(eye - g.pos[a]) >= 0.0;
        if !toward && !mat.double {
            continue;
        }
        // нормаль к камере: у двусторонних задняя сторона светится как передняя
        let flip = if toward { 1.0 } else { -1.0 };
        let corner = |i: usize| -> (Vec4, Vec2, f32) {
            let n = if g.has_nrm[i] { g.nrm[i] } else { face };
            (clip[i], g.uv[i], shade(n * flip))
        };
        let poly = clip_near([corner(a), corner(b), corner(c)]);
        for k in 1..poly.len().saturating_sub(1) {
            let sv = |(cp, uv, l): (Vec4, Vec2, f32)| -> SV {
                let iw = 1.0 / cp.w;
                SV { x: (cp.x * iw * 0.5 + 0.5) * fw as f32, y: (0.5 - cp.y * iw * 0.5) * fh as f32, z: cp.z * iw, iw, u: uv.x * iw, v: uv.y * iw, l: l * iw }
            };
            let v = [sv(poly[0]), sv(poly[k]), sv(poly[k + 1])];
            let (y0, y1) = (v.iter().map(|p| p.y).fold(f32::MAX, f32::min), v.iter().map(|p| p.y).fold(f32::MIN, f32::max));
            if y1 < 0.0 || y0 > fh as f32 || !(y0.is_finite() && y1.is_finite()) {
                continue;
            }
            let (x0, x1) = (v.iter().map(|p| p.x).fold(f32::MAX, f32::min), v.iter().map(|p| p.x).fold(f32::MIN, f32::max));
            if x1 < 0.0 || x0 > fw as f32 {
                continue;
            }
            tris.push(STri { v, mat: g.mat_of[ti], part: g.part_of[ti], y0, y1 });
        }
    }
    // раскладка по полосам строк
    let bands = fh.div_ceil(BAND);
    let mut bins: Vec<Vec<u32>> = vec![Vec::new(); bands];
    for (i, t) in tris.iter().enumerate() {
        let b0 = (t.y0.max(0.0) as usize / BAND).min(bands - 1);
        let b1 = (t.y1.max(0.0) as usize / BAND).min(bands - 1);
        for b in &mut bins[b0..=b1] {
            b.push(i as u32);
        }
    }
    let mut depth = vec![f32::INFINITY; fw * fh];
    let mut color = vec![[0f32; 4]; fw * fh];
    let mut part = vec![0u32; fw * fh];
    let mut vdepth = vec![0f32; fw * fh];
    let mode = scene.display;
    let ink = [scene.ink[0] as f32 / 255.0, scene.ink[1] as f32 / 255.0, scene.ink[2] as f32 / 255.0, 1.0];
    let images: Vec<&[Image]> = models.iter().map(|m| m.images.as_slice()).collect();
    {
        let next = AtomicUsize::new(0);
        let chunks: Vec<_> = depth.chunks_mut(BAND * fw).zip(color.chunks_mut(BAND * fw)).zip(part.chunks_mut(BAND * fw)).zip(vdepth.chunks_mut(BAND * fw)).map(std::sync::Mutex::new).collect();
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(bands);
        std::thread::scope(|s| {
            for _ in 0..threads {
                s.spawn(|| loop {
                    let b = next.fetch_add(1, Ordering::Relaxed);
                    let Some(chunk) = chunks.get(b) else { break };
                    let mut guard = chunk.lock().unwrap();
                    let (((d, c), p), vd) = &mut *guard;
                    raster_band(b, &bins[b], &tris, &g.mats, &images, mode, ink, fw, fh, d, c, p, vd);
                });
            }
        });
    }
    // контур: граница фигуры, стык частей, скачок глубины
    if mode == Display::Outline {
        let r = (scene.line * ss as f32 * 0.5).max(0.5);
        let edge = |i: usize, j: usize| -> bool {
            let (a, b) = (depth[i].is_finite(), depth[j].is_finite());
            if a != b {
                return true;
            }
            if !a {
                return false;
            }
            part[i] != part[j] || (vdepth[i] - vdepth[j]).abs() > 0.04 * vdepth[i].min(vdepth[j])
        };
        let mut lines = vec![false; fw * fh];
        let ri = r.ceil() as i64;
        for y in 0..fh {
            for x in 0..fw {
                let i = y * fw + x;
                let e = (x + 1 < fw && edge(i, i + 1)) || (y + 1 < fh && edge(i, i + fw));
                if !e {
                    continue;
                }
                for dy in -ri..=ri {
                    for dx in -ri..=ri {
                        if ((dx * dx + dy * dy) as f32) > r * r + 0.25 {
                            continue;
                        }
                        let (xx, yy) = (x as i64 + dx, y as i64 + dy);
                        if xx >= 0 && yy >= 0 && (xx as usize) < fw && (yy as usize) < fh {
                            lines[yy as usize * fw + xx as usize] = true;
                        }
                    }
                }
            }
        }
        for (c, l) in color.iter_mut().zip(&lines) {
            *c = if *l { ink } else { [0.0; 4] };
        }
    }
    // уменьшение в ss раз: среднее с предумножением
    let mut out = vec![0u8; w * h * 4];
    let n = (ss * ss) as f32;
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0f32; 4];
            for sy in 0..ss {
                for sx in 0..ss {
                    let c = color[(y * ss + sy) * fw + x * ss + sx];
                    acc[0] += c[0] * c[3];
                    acc[1] += c[1] * c[3];
                    acc[2] += c[2] * c[3];
                    acc[3] += c[3];
                }
            }
            let a = acc[3] / n;
            let o = (y * w + x) * 4;
            if a > 0.0 {
                for k in 0..3 {
                    out[o + k] = ((acc[k] / acc[3]).clamp(0.0, 1.0) * 255.0).round() as u8;
                }
                out[o + 3] = (a.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
    out
}

/// Отсечь треугольник плоскостью z ≥ 0 (ближняя в glam `perspective_rh`): 0, 3 или 4 вершины.
fn clip_near(t: [(Vec4, Vec2, f32); 3]) -> Vec<(Vec4, Vec2, f32)> {
    let inside = |p: &Vec4| p.z >= 0.0 && p.w > 0.0;
    if t.iter().all(|v| inside(&v.0)) {
        return t.to_vec();
    }
    let mut out = Vec::with_capacity(4);
    for i in 0..3 {
        let (a, b) = (t[i], t[(i + 1) % 3]);
        let (ia, ib) = (inside(&a.0), inside(&b.0));
        if ia {
            out.push(a);
        }
        if ia != ib {
            let k = a.0.z / (a.0.z - b.0.z);
            if k.is_finite() {
                out.push((a.0.lerp(b.0, k), a.1.lerp(b.1, k), a.2 + (b.2 - a.2) * k));
            }
        }
    }
    out.retain(|v| v.0.w > 0.0);
    out
}

fn sample(img: &Image, u: f32, v: f32) -> [f32; 4] {
    let (w, h) = (img.w as usize, img.h as usize);
    let fx = (u - u.floor()) * w as f32 - 0.5;
    let fy = (v - v.floor()) * h as f32 - 0.5;
    let (x0, y0) = (fx.floor(), fy.floor());
    let (tx, ty) = (fx - x0, fy - y0);
    let px = |x: i64, y: i64| -> [f32; 4] {
        let (x, y) = (x.rem_euclid(w as i64) as usize, y.rem_euclid(h as i64) as usize);
        let o = (y * w + x) * 4;
        let p = &img.rgba[o..o + 4];
        [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0, p[3] as f32 / 255.0]
    };
    let (x0, y0) = (x0 as i64, y0 as i64);
    let (a, b, c, d) = (px(x0, y0), px(x0 + 1, y0), px(x0, y0 + 1), px(x0 + 1, y0 + 1));
    std::array::from_fn(|k| (a[k] * (1.0 - tx) + b[k] * tx) * (1.0 - ty) + (c[k] * (1.0 - tx) + d[k] * tx) * ty)
}

#[allow(clippy::too_many_arguments, reason = "внутренняя функция горячего цикла: параметры раздельно")]
fn raster_band(b: usize, bin: &[u32], tris: &[STri], mats: &[Mat], images: &[&[Image]], mode: Display, ink: [f32; 4], fw: usize, fh: usize, depth: &mut [f32], color: &mut [[f32; 4]], part: &mut [u32], vdepth: &mut [f32]) {
    let (by0, by1) = (b * BAND, ((b + 1) * BAND).min(fh));
    for &ti in bin {
        let t = &tris[ti as usize];
        let [p0, p1, p2] = t.v;
        let area = (p1.x - p0.x) * (p2.y - p0.y) - (p1.y - p0.y) * (p2.x - p0.x);
        if area.abs() < 1e-12 {
            continue;
        }
        let inv = 1.0 / area;
        let x0 = (p0.x.min(p1.x).min(p2.x).floor().max(0.0) as usize).min(fw);
        let x1 = (p0.x.max(p1.x).max(p2.x).ceil().max(0.0) as usize).min(fw);
        let y0 = (t.y0.floor().max(by0 as f32) as usize).max(by0);
        let y1 = (t.y1.ceil().max(0.0) as usize).min(by1);
        let mat = &mats[t.mat as usize];
        let tex = mat.tex.and_then(|i| images.get(mat.model).and_then(|im| im.get(i)));
        for y in y0..y1 {
            let py = y as f32 + 0.5;
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let w0 = ((p1.x - px) * (p2.y - py) - (p1.y - py) * (p2.x - px)) * inv;
                let w1 = ((p2.x - px) * (p0.y - py) - (p2.y - py) * (p0.x - px)) * inv;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = w0 * p0.z + w1 * p1.z + w2 * p2.z;
                let i = (y - by0) * fw + x;
                if z >= depth[i] || !(0.0..=1.0).contains(&z) {
                    continue;
                }
                let iw = w0 * p0.iw + w1 * p1.iw + w2 * p2.iw;
                let pw = 1.0 / iw;
                let l = (w0 * p0.l + w1 * p1.l + w2 * p2.l) * pw;
                let mut c = mat.color;
                if let Some(img) = tex {
                    let (u, v) = ((w0 * p0.u + w1 * p1.u + w2 * p2.u) * pw, (w0 * p0.v + w1 * p1.v + w2 * p2.v) * pw);
                    let s = sample(img, u, v);
                    c = [c[0] * s[0], c[1] * s[1], c[2] * s[2], c[3] * s[3]];
                }
                if let Some(cut) = mat.cutoff {
                    if c[3] < cut {
                        continue;
                    }
                }
                depth[i] = z;
                part[i] = t.part;
                vdepth[i] = pw;
                color[i] = match mode {
                    Display::Lit => [c[0] * l, c[1] * l, c[2] * l, 1.0],
                    Display::Clay => [0.82 * l, 0.8 * l, 0.78 * l, 1.0],
                    Display::Silhouette | Display::Outline => ink,
                };
            }
        }
    }
}


/// Точка мира → точка холста `w`×`h` (та же камера, что у `render`); `None` — за камерой.
pub fn project(scene: &Scene, w: u32, h: u32, p: Vec3) -> Option<Vec2> {
    let cam = &scene.camera;
    let proj = glam::camera::rh::proj::directx::perspective(cam.fov.clamp(1.0, 150.0).to_radians(), w.max(1) as f32 / h.max(1) as f32, 0.01, 100.0);
    let c = proj * cam.view() * p.extend(1.0);
    (c.w > 1e-6).then(|| Vec2::new((c.x / c.w * 0.5 + 0.5) * w as f32, (0.5 - c.y / c.w * 0.5) * h as f32))
}

/// Точка холста → точка мира на плоскости, которая проходит через `at` поперёк взгляда (та же
/// камера, что у `project`).
pub fn unproject(scene: &Scene, w: u32, h: u32, p: Vec2, at: Vec3) -> Option<Vec3> {
    let cam = &scene.camera;
    let proj = glam::camera::rh::proj::directx::perspective(cam.fov.clamp(1.0, 150.0).to_radians(), w.max(1) as f32 / h.max(1) as f32, 0.01, 100.0);
    let inv = (proj * cam.view()).inverse();
    let (x, y) = (p.x / w.max(1) as f32 * 2.0 - 1.0, 1.0 - p.y / h.max(1) as f32 * 2.0);
    let a = inv.project_point3(Vec3::new(x, y, 0.0));
    let b = inv.project_point3(Vec3::new(x, y, 0.5));
    let dir = (b - a).normalize_or_zero();
    let n = (cam.target - cam.eye()).normalize_or(Vec3::NEG_Z);
    let den = dir.dot(n);
    if den.abs() < 1e-6 {
        return None;
    }
    let t = (at - a).dot(n) / den;
    (t.is_finite() && t > 0.0).then(|| a + dir * t)
}

/// Точки, за которые можно взять предмет: суставы скелета, а без скелета — узлы с сетками и их
/// родители (части сцены). (узел, точка в мире, родитель-точка).
pub fn handles(o: &crate::scene::Object, m: &Model) -> Vec<(usize, Vec3, Option<usize>)> {
    let g = o.globals(m);
    let pick: Vec<bool> = if m.nodes.iter().any(|n| n.joint) {
        m.nodes.iter().map(|n| n.joint).collect()
    } else {
        m.nodes.iter().map(|n| n.mesh.is_some() || !n.children.is_empty()).collect()
    };
    (0..m.nodes.len())
        .filter(|&i| pick[i])
        .map(|i| {
            let mut p = m.nodes[i].parent;
            while let Some(q) = p.filter(|&q| !pick[q]) {
                p = m.nodes[q].parent;
            }
            (i, g[i].w_axis.truncate(), p)
        })
        .collect()
}

/// RGBA `sw`×`sh` → вдвое (и т. п.) крупнее до `w`×`h` (предпросмотр при перетаскивании).
pub fn upscale(src: &[u8], sw: u32, sh: u32, w: u32, h: u32) -> Vec<u8> {
    let mut out = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        let sy = ((y as u64 * sh as u64) / h.max(1) as u64).min(sh as u64 - 1) as u32;
        for x in 0..w {
            let sx = ((x as u64 * sw as u64) / w.max(1) as u64).min(sw as u64 - 1) as u32;
            let s = ((sy * sw + sx) * 4) as usize;
            let d = ((y * w + x) * 4) as usize;
            out[d..d + 4].copy_from_slice(&src[s..s + 4]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unproject_inverts_project() {
        let scene = Scene::default();
        let p = Vec3::new(0.3, 1.2, -0.4);
        let s = project(&scene, 800, 600, p).unwrap();
        let back = unproject(&scene, 800, 600, s, p).unwrap();
        assert!((back - p).length() < 1e-3, "{back:?}");
    }
}
