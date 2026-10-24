//! Слой 3D в программе: окно «3D» (камера, вид, предметы, части и суставы, морфы), управление
//! мышью на холсте, отрисовка в своём потоке, запись в историю одним шагом после правки.

use crate::i18n::{tr, trf};
use crate::{files, PaintApp, ViewXf};
use eframe::egui::{self, Color32, Vec2};
use glam::{EulerRot, Quat, Vec3};
use paint_3d::{Display, Model, Object, Scene};
use paint_core::layer::{Asset3d, Scene3dData};
use paint_core::tile::ColorMap;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Что делает мышь на холсте, пока открыт слой 3D.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MouseMode {
    Off,
    Camera,
    Pose,
}

enum Drag {
    /// Камера: последняя точка (px холста).
    Camera(Vec2),
    /// Поворот узла `node` вокруг оси взгляда: центр на холсте и прошлая точка.
    Joint { node: usize, center: Vec2, last: Vec2 },
    /// «Тянуть за конец»: узел `node` идёт за указателем, суставы `chain` над ним поворачиваются.
    Reach { node: usize, chain: Vec<usize> },
}

/// Сколько суставов над концом поворачивает «тянуть за конец» (у кисти — предплечье и плечо).
const REACH_CHAIN: usize = 2;

struct Job {
    gen: u64,
    full: bool,
    rx: Receiver<(Result<ColorMap, String>, f32)>,
}

/// Правка слоя 3D.
pub struct Edit3d {
    uid: u64,
    path: Vec<usize>,
    /// Сцена до правки (для одного шага истории).
    orig: Scene3dData,
    orig_px: ColorMap,
    pub scene: Scene,
    assets: Vec<Arc<Asset3d>>,
    models: Vec<Arc<Model>>,
    pub obj: usize,
    pub node: Option<usize>,
    drag: Option<Drag>,
    /// Номер правки: растёт с каждым изменением.
    gen: u64,
    shown_gen: u64,
    full_gen: u64,
    job: Option<Job>,
    changed_at: Option<Instant>,
    /// Сколько занял последний полный кадр, мс (большие сцены при перетаскивании — вполовину).
    last_ms: f32,
    /// В режиме позы — тянуть за конец (кисть, стопу), а не поворачивать сустав.
    pub reach: bool,
    /// Имя позы, которую сейчас запоминают (поле ввода открыто).
    pose_name: Option<String>,
    /// «В линии»: сгиб — грани под углом больше этого, градусы.
    pub crease: f32,
}

impl Edit3d {
    fn data(&self) -> Scene3dData {
        Scene3dData { assets: self.assets.clone(), state: self.scene.to_text() }
    }

    fn changed(&mut self) {
        self.gen += 1;
        self.changed_at = Some(Instant::now());
    }
}

fn find_uid(layers: &[paint_core::Layer], uid: u64, path: &mut Vec<usize>) -> bool {
    for (i, l) in layers.iter().enumerate() {
        path.push(i);
        if l.uid == uid {
            return true;
        }
        if let Some(ch) = &l.children {
            if find_uid(ch, uid, path) {
                return true;
            }
        }
        path.pop();
    }
    false
}

/// Поворот в градусах по осям X, Y, Z ↔ кватернион.
fn euler(q: Quat) -> [f32; 3] {
    let (x, y, z) = q.to_euler(EulerRot::XYZ);
    [x.to_degrees(), y.to_degrees(), z.to_degrees()]
}

fn from_euler(e: [f32; 3]) -> Quat {
    Quat::from_euler(EulerRot::XYZ, e[0].to_radians(), e[1].to_radians(), e[2].to_radians())
}

impl PaintApp {
    /// Выбранный слой — слой 3D? Тогда начать (или продолжить) его правку; иначе закончить.
    pub(crate) fn s3d_sync(&mut self) {
        let sel = self.doc.layer(&self.selected).filter(|l| l.scene3d.is_some()).map(|l| l.uid);
        if let Some(e) = &self.s3d {
            // слой мог переехать в стопке или исчезнуть (новый холст, удаление)
            let mut p = Vec::new();
            if !find_uid(&self.doc.layers, e.uid, &mut p) || self.doc.layer(&p).is_none_or(|l| l.scene3d.is_none()) {
                self.s3d = None;
            } else if p != e.path {
                self.s3d.as_mut().unwrap().path = p;
            }
        }
        let same = self.s3d.as_ref().map(|e| e.uid) == sel && sel.is_some();
        if same {
            // сцену поменяли снаружи (отмена, повтор) — перечитать
            let e = self.s3d.as_ref().unwrap();
            if e.changed_at.is_none() && e.job.is_none() {
                if let Some(d) = self.doc.layer(&e.path).and_then(|l| l.scene3d.as_deref()) {
                    if *d != e.orig {
                        // начать заново (режим мыши и выбранный сустав остаются)
                        let (path, obj, node) = (e.path.clone(), e.obj, e.node);
                        self.s3d_begin(path);
                        if let Some(e) = self.s3d.as_mut() {
                            e.obj = obj.min(e.scene.objects.len().saturating_sub(1));
                            e.node = node;
                        }
                    }
                }
            }
            return;
        }
        self.s3d_finish();
        if sel.is_some() {
            let path = self.selected.clone();
            self.s3d_begin(path);
            self.show_3d = true;
        }
    }

    fn s3d_begin(&mut self, path: Vec<usize>) {
        let Some(l) = self.doc.layer(&path) else { return };
        let Some(d) = l.scene3d.as_deref() else { return };
        let scene = Scene::parse(&d.state);
        let (models, err) = match paint_3d::models_for(&scene, d) {
            Ok(m) => (m, None),
            Err(e) => (scene.objects.iter().map(|_| Arc::new(Model::default())).collect(), Some(e.to_string())),
        };
        if let Some(e) = err {
            self.status = trf("Модель слоя 3D не прочитана: {0}", &[&e]);
        }
        self.s3d = Some(Edit3d {
            uid: l.uid,
            path,
            orig: d.clone(),
            orig_px: l.pixels.clone(),
            scene,
            assets: d.assets.clone(),
            models,
            obj: 0,
            node: None,
            drag: None,
            gen: 0,
            shown_gen: 0,
            full_gen: 0,
            job: None,
            changed_at: None,
            last_ms: 0.0,
            reach: false,
            pose_name: None,
            crease: 40.0,
        });
    }

    /// Закончить правку: если что-то менялось — дорисовать начисто и записать одним шагом.
    pub(crate) fn s3d_finish(&mut self) {
        let Some(mut e) = self.s3d.take() else { return };
        e.job = None;
        if e.gen == 0 && e.changed_at.is_none() {
            return;
        }
        let data = e.data();
        if data == e.orig {
            return;
        }
        let (w, h) = (self.doc.width, self.doc.height);
        let px = paint_3d::to_colormap(&paint_3d::render::render(&e.scene, &e.models, w, h, 2), w, h);
        self.s3d_commit(&e.path, e.orig.clone(), e.orig_px.clone(), data, px);
    }

    /// Шаг истории: от сцены до правки к новой.
    fn s3d_commit(&mut self, path: &[usize], orig: Scene3dData, orig_px: ColorMap, data: Scene3dData, px: ColorMap) {
        let k1 = self.doc.set_scene_quiet(path, Some(orig), orig_px);
        let k2 = self.doc.set_scene_layer(path, data, px);
        self.invalidate(k1);
        self.invalidate(k2);
        self.thumbs.clear();
        self.dirty = true;
    }

    /// Раз в кадр: забрать готовую отрисовку, начать следующую, записать законченную правку.
    pub(crate) fn s3d_tick(&mut self, ctx: &egui::Context) {
        let Some(e) = self.s3d.as_mut() else { return };
        if let Some(job) = &e.job {
            match job.rx.try_recv() {
                Ok((Ok(px), ms)) => {
                    let (gen, full) = (job.gen, job.full);
                    if full {
                        e.last_ms = ms;
                    }
                    e.job = None;
                    e.shown_gen = gen;
                    if full {
                        e.full_gen = gen;
                    }
                    let path = e.path.clone();
                    let data = e.data();
                    let settled = full && gen == e.gen && e.drag.is_none() && e.changed_at.is_some_and(|t| t.elapsed() > Duration::from_millis(350));
                    if settled {
                        // правка закончилась: один шаг истории
                        let (orig, orig_px) = (e.orig.clone(), e.orig_px.clone());
                        e.orig = data.clone();
                        e.orig_px = px.clone();
                        e.changed_at = None;
                        self.s3d_commit(&path, orig, orig_px, data, px);
                    } else {
                        let keys = self.doc.set_scene_quiet(&path, Some(data), px);
                        self.invalidate(keys);
                        self.thumbs.clear();
                    }
                }
                Ok((Err(msg), _)) => {
                    e.job = None;
                    self.status = trf("Не удалось нарисовать сцену 3D: {0}", &[&msg]);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(_) => e.job = None,
            }
        }
        let Some(e) = self.s3d.as_mut() else { return };
        if e.job.is_none() {
            let busy = e.drag.is_some() || e.changed_at.is_some_and(|t| t.elapsed() < Duration::from_millis(300));
            let want_preview = e.gen > e.shown_gen;
            let want_full = e.changed_at.is_some() && !busy && e.full_gen < e.gen.max(1);
            if want_preview || want_full {
                let full = !busy;
                // большие сцены при перетаскивании — вполовину
                let half = !full && e.last_ms > 60.0;
                let (w, h) = (self.doc.width, self.doc.height);
                let (rw, rh) = if half { ((w / 2).max(1), (h / 2).max(1)) } else { (w, h) };
                let (scene, models, gen) = (e.scene.clone(), e.models.clone(), e.gen);
                let (tx, rx) = channel();
                let c = ctx.clone();
                std::thread::spawn(move || {
                    let t = Instant::now();
                    let r = std::panic::catch_unwind(|| {
                        let img = paint_3d::render::render(&scene, &models, rw, rh, if full { 2 } else { 1 });
                        let img = if half { paint_3d::render::upscale(&img, rw, rh, w, h) } else { img };
                        paint_3d::to_colormap(&img, w, h)
                    });
                    let _ = tx.send((r.map_err(|_| "сбой отрисовки".to_string()), t.elapsed().as_secs_f32() * 1000.0));
                    c.request_repaint();
                });
                e.job = Some(Job { gen, full, rx });
            }
        }
        if self.s3d.as_ref().is_some_and(|e| e.changed_at.is_some() || e.job.is_some()) {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    /// Новый слой 3D из файла модели (над выбранным слоем).
    pub(crate) fn s3d_new_layer(&mut self, path: &std::path::Path) {
        let files = match paint_3d::gather(path) {
            Ok(f) => f,
            Err(e) => {
                self.status = trf("Не удалось открыть модель {0}: {1}", &[&path.display(), &e]);
                return;
            }
        };
        let started = Instant::now();
        match paint_3d::new_layer_data(files) {
            Ok((data, name)) => {
                let (w, h) = (self.doc.width, self.doc.height);
                let scene = Scene::parse(&data.state);
                let models = paint_3d::models_for(&scene, &data).unwrap_or_default();
                let tris: usize = models.iter().map(|m| m.triangles).sum();
                let mut l = paint_core::Layer::new(name.rsplit_once('.').map_or(name.as_str(), |(a, _)| a));
                l.pixels = paint_3d::to_colormap(&paint_3d::render::render(&scene, &models, w, h, 2), w, h);
                l.scene3d = Some(Box::new(data));
                let mut at = self.selected.clone();
                if let Some(last) = at.last_mut() {
                    *last += 1;
                } else {
                    at = vec![self.doc.layers.len()];
                }
                let keys = self.doc.insert_layer(&at, l);
                self.selected = at;
                self.after_structure(keys);
                self.show_3d = true;
                self.status = trf("Слой 3D: {0} — треугольников {1}, за {2} с", &[&name, &tris, &format!("{:.1}", started.elapsed().as_secs_f32())]);
            }
            Err(e) => self.status = trf("Не удалось открыть модель {0}: {1}", &[&path.display(), &e]),
        }
    }

    /// Добавить ещё одну модель в сцену выбранного слоя 3D.
    pub(crate) fn s3d_add_model(&mut self, path: &std::path::Path) {
        let files = match paint_3d::gather(path) {
            Ok(f) => f,
            Err(err) => {
                self.status = trf("Не удалось открыть модель {0}: {1}", &[&path.display(), &err]);
                return;
            }
        };
        let name = files[0].0.clone();
        let asset = Arc::new(Asset3d { id: paint_3d::new_asset_id(), name, files });
        match paint_3d::model_of(&asset) {
            Ok(m) => {
                let Some(e) = self.s3d.as_mut() else { return };
                let mut o = Object::new(asset.id);
                // рядом с прежними: правее на размер сцены
                o.pos = e.scene.camera.target + Vec3::X * e.scene.camera.distance * 0.3;
                e.assets.push(asset);
                e.models.push(m);
                e.scene.objects.push(o);
                e.obj = e.scene.objects.len() - 1;
                e.node = None;
                e.changed();
            }
            Err(err) => self.status = trf("Не удалось открыть модель {0}: {1}", &[&path.display(), &err]),
        }
    }

    /// Мышь управляет сценой: начать можно, если включён режим и нажата кнопка над холстом.
    /// «В линии»: очертания и сгибы модели — новый слой контура над слоем 3D (цвет и толщина —
    /// как у контура в «Виде»).
    fn s3d_to_lines(&mut self) {
        let Some(e) = self.s3d.as_ref() else { return };
        let (w, h) = (self.doc.width, self.doc.height);
        let lines = paint_3d::lines::lines(&e.scene, &e.models, w, h, e.crease);
        // каждая ломаная — точками не реже 2 px, чтобы кривая легла по ней
        let paths: Vec<(Vec<paint_core::vectorize::PathNode>, bool)> = lines
            .iter()
            .filter_map(|l| {
                let mut pts = Vec::new();
                for s in l.windows(2) {
                    let (a, b) = (s[0], s[1]);
                    let n = ((b[0] - a[0]).hypot(b[1] - a[1]) / 2.0).ceil().max(1.0) as usize;
                    for k in 0..n {
                        let t = k as f32 / n as f32;
                        pts.push(paint_core::vectorize::StrokePoint { at: [(a[0] + (b[0] - a[0]) * t) as f64, (a[1] + (b[1] - a[1]) * t) as f64], pressure: 1.0 });
                    }
                }
                let last = *l.last()?;
                pts.push(paint_core::vectorize::StrokePoint { at: [last[0] as f64, last[1] as f64], pressure: 1.0 });
                paint_core::vectorize::vectorize(&pts, 0.5)
            })
            .collect();
        if paths.is_empty() {
            self.status = tr("Линий не вышло: модели не видно в кадре").to_string();
            return;
        }
        let ink = e.scene.ink;
        let st = paint_formats::linework::LineStyle { rgb: ink, size: e.scene.line.max(1.0), density: 1.0, hardness: 4 };
        let mut layer = paint_core::Layer::new(&format!("{} — {}", self.doc.layer(&e.path).map(|l| l.name.clone()).unwrap_or_default(), tr("линии")));
        let raw = match paint_formats::linework::append_many(&paint_formats::linework::empty().raw, &paths, &st) {
            Ok(r) => r,
            Err(err) => {
                self.status = trf("Не удалось добавить кривую: {0}", &[&err]);
                return;
            }
        };
        layer.vector = Some(paint_core::layer::VectorData { kind: *b"liwk", raw });
        match paint_formats::linework::render(&layer) {
            Ok(px) => layer.pixels = px,
            Err(err) => {
                self.status = trf("Не удалось нарисовать кривую: {0}", &[&err]);
                return;
            }
        }
        let mut path = e.path.clone();
        let n = paths.len();
        self.s3d_finish();
        if let Some(i) = path.last_mut() {
            *i += 1;
        }
        let keys = self.doc.insert_layer(&path, layer);
        self.selected = path;
        self.after_structure(keys);
        self.status = trf("Слой контура из модели: кривых {0}", &[&n]);
    }

    pub(crate) fn s3d_takes_mouse(&self, start: bool) -> bool {
        self.s3d_mode != MouseMode::Off && self.s3d.as_ref().is_some_and(|e| e.drag.is_some() || start)
    }

    pub(crate) fn s3d_mouse(&mut self, at: Option<Vec2>, moves: &[Vec2], pressed: bool, released: bool, mods: egui::Modifiers) {
        let (w, h) = (self.doc.width as f32, self.doc.height as f32);
        let (zoom, mode) = (self.zoom, self.s3d_mode);
        let Some(e) = self.s3d.as_mut() else { return };
        if pressed && e.drag.is_none() {
            let Some(p) = at else { return };
            e.drag = Some(Drag::Camera(p));
            if mode == MouseMode::Pose {
                // ближайший сустав в 14 точках экрана
                let mut best: Option<(usize, usize, Vec2, f32)> = None;
                for (oi, (o, m)) in e.scene.objects.iter().zip(&e.models).enumerate() {
                    for (node, wp, _) in paint_3d::render::handles(o, m) {
                        if let Some(sp) = paint_3d::render::project(&e.scene, w as u32, h as u32, wp) {
                            let d = (Vec2::new(sp.x, sp.y) - p).length() * zoom;
                            if d < 14.0 && best.is_none_or(|b| d < b.3) {
                                best = Some((oi, node, Vec2::new(sp.x, sp.y), d));
                            }
                        }
                    }
                }
                match best {
                    Some((oi, node, center, _)) => {
                        e.obj = oi;
                        e.node = Some(node);
                        let chain = e.models.get(oi).map(|m| paint_3d::pose::chain(m, node, REACH_CHAIN)).unwrap_or_default();
                        e.drag = Some(if e.reach && !chain.is_empty() { Drag::Reach { node, chain } } else { Drag::Joint { node, center, last: p } });
                    }
                    None => e.drag = None,
                }
            }
        }
        let mut changed = false;
        match e.drag.as_mut() {
            Some(Drag::Camera(last)) => {
                for &p in moves {
                    let d = p - *last;
                    *last = p;
                    let c = &mut e.scene.camera;
                    if mods.shift {
                        // сдвиг в плоскости экрана
                        let view = c.view();
                        let inv = view.inverse();
                        let right = inv.transform_vector3(Vec3::X);
                        let up = inv.transform_vector3(Vec3::Y);
                        let k = 2.0 * c.distance * (c.fov.to_radians() / 2.0).tan() / h;
                        c.target += (-right * d.x + up * d.y) * k;
                    } else if mods.command {
                        c.distance = (c.distance * (d.y / h * 3.0).exp()).clamp(1e-3, 1e6);
                    } else {
                        c.yaw = (c.yaw - d.x / w.max(h) * 360.0 + 540.0).rem_euclid(360.0) - 180.0;
                        c.pitch = (c.pitch + d.y / w.max(h) * 360.0).clamp(-89.0, 89.0);
                    }
                    changed = true;
                }
            }
            Some(Drag::Joint { node, center, last }) => {
                let node = *node;
                let (ww, hh) = (w as u32, h as u32);
                for &p in moves {
                    let (obj, scene) = (e.obj, &mut e.scene);
                    let Some(m) = e.models.get(obj) else { break };
                    // куда смотрит кость после сустава: к первому дочернему суставу; у концевых —
                    // поворот «как ручку» вокруг сустава
                    let hs = paint_3d::render::handles(&scene.objects[obj], m);
                    let tip = hs.iter().find(|h| h.2 == Some(node)).and_then(|h| paint_3d::render::project(scene, ww, hh, h.1)).map(|v| Vec2::new(v.x, v.y));
                    let from = match tip {
                        Some(t) => t - *center,
                        None => *last - *center,
                    };
                    let to = p - *center;
                    *last = p;
                    if from.length() < 2.0 || to.length() < 4.0 {
                        continue;
                    }
                    let ang = to.y.atan2(to.x) - from.y.atan2(from.x);
                    let ang = (ang + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
                    let o = &mut scene.objects[obj];
                    // экран: y вниз, ось взгляда — от камеры; поворот по часовой на экране = вокруг неё
                    let axis = (scene.camera.target - scene.camera.eye()).normalize_or(Vec3::NEG_Z);
                    let world = Quat::from_axis_angle(axis, ang);
                    let g = o.globals(m);
                    let parent_rot = m.nodes[node].parent.map_or(o.rot, |pi| g[pi].to_scale_rotation_translation().1);
                    let rest = m.nodes[node].r;
                    let local = rest * o.pose.get(&node).copied().unwrap_or(Quat::IDENTITY);
                    let new_local = parent_rot.inverse() * world * parent_rot * local;
                    o.pose.insert(node, (rest.inverse() * new_local).normalize());
                    changed = true;
                }
            }
            Some(Drag::Reach { node, chain }) => {
                let (obj, scene) = (e.obj, &mut e.scene);
                if let (Some(&p), Some(m)) = (moves.last(), e.models.get(obj)) {
                    let at = scene.objects[obj].globals(m)[*node].w_axis.truncate();
                    if let Some(target) = paint_3d::render::unproject(scene, w as u32, h as u32, glam::Vec2::new(p.x, p.y), at) {
                        let o = &mut scene.objects[obj];
                        paint_3d::pose::reach(o, m, *node, chain, target);
                        changed = true;
                    }
                }
            }
            None => {}
        }
        if released {
            e.drag = None;
        }
        if changed || released {
            e.changed();
        }
    }

    /// Суставы и части на холсте (в режиме позы).
    pub(crate) fn s3d_overlay(&self, painter: &egui::Painter, xf: ViewXf) {
        let Some(e) = self.s3d.as_ref().filter(|_| self.s3d_mode == MouseMode::Pose) else { return };
        let (w, h) = (self.doc.width, self.doc.height);
        let accent = Color32::from_rgb(40, 140, 255);
        for (oi, (o, m)) in e.scene.objects.iter().zip(&e.models).enumerate() {
            if !o.visible {
                continue;
            }
            let pts = paint_3d::render::handles(o, m);
            let screen = |p: Vec3| paint_3d::render::project(&e.scene, w, h, p).map(|v| xf.to_screen(Vec2::new(v.x, v.y)));
            for (_, wp, parent) in &pts {
                if let (Some(a), Some(pp)) = (screen(*wp), parent.and_then(|q| pts.iter().find(|x| x.0 == q))) {
                    if let Some(b) = screen(pp.1) {
                        painter.line_segment([a, b], egui::Stroke::new(1.0_f32, accent.gamma_multiply(0.6)));
                    }
                }
            }
            for (node, wp, _) in &pts {
                if let Some(a) = screen(*wp) {
                    let sel = oi == e.obj && e.node == Some(*node);
                    painter.circle(a, if sel { 5.5 } else { 3.5 }, if sel { Color32::from_rgb(255, 170, 0) } else { Color32::WHITE }, egui::Stroke::new(1.2_f32, accent));
                }
            }
        }
    }

    /// Окно «3D».
    pub(crate) fn s3d_window(&mut self, ctx: &egui::Context) {
        if !self.show_3d || self.s3d.is_none() {
            return;
        }
        let mut open = true;
        let mut add = false;
        let mut frame_all = false;
        let mut to_lines = false;
        let layer_name = self.doc.layer(&self.s3d.as_ref().unwrap().path).map(|l| l.name.clone()).unwrap_or_default();
        let mut mode = self.s3d_mode;
        // точки схода показанной перспективной линейки — для камеры
        let vps: Option<Vec<[f32; 2]>> = (self.ruler.visible && matches!(self.ruler.kind, crate::ruler::Kind::Perspective(_))).then(|| self.ruler.vp[..self.ruler.vps().min(2)].to_vec());
        let (dw, dh) = (self.doc.width, self.doc.height);
        let e = self.s3d.as_mut().unwrap();
        let mut changed = false;
        let mut msg: Option<String> = None;
        egui::Window::new(format!("3D — {layer_name}")).id(egui::Id::new("window3d")).open(&mut open).default_width(330.0).resizable(true).show(ctx, |ui| {
            ui.spacing_mut().slider_width = 170.0;
            ui.horizontal(|ui| {
                ui.label(tr("Мышь на холсте:"));
                ui.selectable_value(&mut mode, MouseMode::Camera, tr("камера"));
                ui.selectable_value(&mut mode, MouseMode::Pose, tr("поза"));
                ui.selectable_value(&mut mode, MouseMode::Off, tr("рисование"));
            });
            ui.weak(match mode {
                MouseMode::Camera => tr("Тянуть — вращать, с Shift — сдвигать, с Ctrl — приближать"),
                MouseMode::Pose if e.reach => tr("Взять кисть, стопу или другой сустав и тянуть — два сустава над ним повернутся, чтобы он шёл за указателем"),
                MouseMode::Pose => tr("Взять сустав (точку) и тянуть — кость за ним повернётся к указателю; точные углы — ниже"),
                MouseMode::Off => tr("Мышь работает как обычно (рисуйте на слое выше)"),
            });
            if mode == MouseMode::Pose {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut e.reach, false, tr("поворачивать сустав"));
                    ui.selectable_value(&mut e.reach, true, tr("тянуть за конец"));
                });
            }
            egui::CollapsingHeader::new(tr("Камера")).default_open(true).show(ui, |ui| {
                let c = &mut e.scene.camera;
                changed |= ui.add(egui::Slider::new(&mut c.yaw, -180.0..=180.0).suffix("°").text(tr("Поворот"))).changed();
                changed |= ui.add(egui::Slider::new(&mut c.pitch, -89.0..=89.0).suffix("°").text(tr("Наклон"))).changed();
                let d0 = c.distance;
                changed |= ui.add(egui::Slider::new(&mut c.distance, (d0 * 0.2).max(1e-3)..=(d0 * 5.0)).logarithmic(true).text(tr("Расстояние"))).changed();
                changed |= ui.add(egui::Slider::new(&mut c.fov, 5.0..=120.0).suffix("°").text(tr("Угол обзора"))).on_hover_text(tr("Меньше — «телеобъектив», плоская перспектива; больше — «широкий угол»")).changed();
                changed |= ui.add(egui::Slider::new(&mut c.roll, -45.0..=45.0).suffix("°").text(tr("Горизонт"))).changed();
                ui.horizontal(|ui| {
                    if ui.button(tr("Вписать в кадр")).clicked() {
                        frame_all = true;
                    }
                    let r = ui.add_enabled(vps.is_some(), egui::Button::new(tr("По перспективной линейке")));
                    let r = r.on_hover_text(tr("Горизонт и точки схода линейки — наклон, поворот и угол обзора камеры (две точки — оси X и Z, одна — ось Z)"));
                    let r = r.on_disabled_hover_text(tr("Покажите перспективную линейку: «Линейка → Перспектива: 2 точки»"));
                    if r.clicked() {
                        match vps.as_deref().and_then(|v| paint_3d::vp::camera_from_vps(c, dw, dh, v)) {
                            Some(n) => {
                                *c = n;
                                changed = true;
                            }
                            None => msg = Some(tr("Точки схода не годятся: они должны быть по разные стороны от середины холста").to_string()),
                        }
                    }
                });
            });
            egui::CollapsingHeader::new(tr("Вид")).default_open(true).show(ui, |ui| {
                let name = |d: Display| match d {
                    Display::Lit => tr("цвет и свет"),
                    Display::Clay => tr("серая фигура"),
                    Display::Silhouette => tr("силуэт"),
                    Display::Outline => tr("контур"),
                };
                egui::ComboBox::from_id_salt("3d_display").selected_text(name(e.scene.display)).show_ui(ui, |ui| {
                    for d in Display::ALL {
                        changed |= ui.selectable_value(&mut e.scene.display, d, name(d)).changed();
                    }
                });
                if matches!(e.scene.display, Display::Silhouette | Display::Outline) {
                    let mut c = Color32::from_rgb(e.scene.ink[0], e.scene.ink[1], e.scene.ink[2]);
                    ui.horizontal(|ui| {
                        ui.label(tr("Цвет"));
                        if ui.color_edit_button_srgba(&mut c).changed() {
                            e.scene.ink = [c.r(), c.g(), c.b()];
                            changed = true;
                        }
                    });
                }
                ui.horizontal(|ui| {
                    if ui.button(tr("В линии")).on_hover_text(tr("Новый слой контура над слоем 3D: очертания и сгибы модели кривыми, их можно править пером контура")).clicked() {
                        to_lines = true;
                    }
                    ui.add(egui::Slider::new(&mut e.crease, 10.0..=120.0).suffix("°").text(tr("Сгиб от")));
                });
                if e.scene.display == Display::Outline {
                    changed |= ui.add(egui::Slider::new(&mut e.scene.line, 0.5..=8.0).suffix(" px").text(tr("Толщина"))).changed();
                } else if e.scene.display != Display::Silhouette {
                    changed |= ui.add(egui::Slider::new(&mut e.scene.light[0], -180.0..=180.0).suffix("°").text(tr("Свет: сбоку"))).changed();
                    changed |= ui.add(egui::Slider::new(&mut e.scene.light[1], -89.0..=89.0).suffix("°").text(tr("Свет: сверху"))).changed();
                }
            });
            egui::CollapsingHeader::new(tr("Предметы")).default_open(true).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let names: Vec<String> = e.scene.objects.iter().map(|o| e.assets.iter().find(|a| a.id == o.asset).map_or_else(|| "?".into(), |a| a.name.clone())).collect();
                    egui::ComboBox::from_id_salt("3d_obj").selected_text(names.get(e.obj).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                        for (i, n) in names.iter().enumerate() {
                            if ui.selectable_value(&mut e.obj, i, format!("{}. {n}", i + 1)).changed() {
                                e.node = None;
                            }
                        }
                    });
                    if ui.button(tr("Добавить…")).clicked() {
                        add = true;
                    }
                    if e.scene.objects.len() > 1 && ui.button(tr("Убрать")).clicked() {
                        let o = e.scene.objects.remove(e.obj);
                        e.models.remove(e.obj);
                        if !e.scene.objects.iter().any(|x| x.asset == o.asset) {
                            e.assets.retain(|a| a.id != o.asset);
                        }
                        e.obj = e.obj.min(e.scene.objects.len() - 1);
                        e.node = None;
                        changed = true;
                    }
                });
                if let Some(o) = e.scene.objects.get_mut(e.obj) {
                    changed |= ui.checkbox(&mut o.visible, tr("Виден")).changed();
                    ui.horizontal(|ui| {
                        ui.label(tr("Место"));
                        for v in [&mut o.pos.x, &mut o.pos.y, &mut o.pos.z] {
                            changed |= ui.add(egui::DragValue::new(v).speed(0.01)).changed();
                        }
                    });
                    let mut r = euler(o.rot);
                    ui.horizontal(|ui| {
                        ui.label(tr("Поворот"));
                        let mut any = false;
                        for v in &mut r {
                            any |= ui.add(egui::DragValue::new(v).speed(0.5).suffix("°")).changed();
                        }
                        if any {
                            o.rot = from_euler(r);
                            changed = true;
                        }
                    });
                    changed |= ui.add(egui::Slider::new(&mut o.scale, 0.01..=100.0).logarithmic(true).text(tr("Размер"))).changed();
                }
            });
            let Some(m) = e.models.get(e.obj).cloned() else { return };
            let Some(o) = e.scene.objects.get_mut(e.obj) else { return };
            let has_skin = m.nodes.iter().any(|n| n.joint);
            egui::CollapsingHeader::new(if has_skin { tr("Суставы и части") } else { tr("Части") }).default_open(true).show(ui, |ui| {
                // скрыть можно узел, под которым есть сетка (не привязанная к скелету: у сетки со
                // скелетом скрытие сустава ничего не прячет)
                let mut shows = vec![false; m.nodes.len()];
                fn mark(m: &Model, i: usize, shows: &mut [bool]) -> bool {
                    let mut any = m.nodes[i].mesh.is_some();
                    for &c in &m.nodes[i].children {
                        any |= mark(m, c, shows);
                    }
                    shows[i] = any;
                    any
                }
                for &r in &m.roots {
                    mark(&m, r, &mut shows);
                }
                changed |= pose_row(ui, o, &m, &mut e.pose_name, &mut msg);
                egui::ScrollArea::vertical().id_salt("3d_nodes").max_height(220.0).show(ui, |ui| {
                    let mut stack: Vec<(usize, usize)> = m.roots.iter().rev().map(|&r| (r, 0)).collect();
                    while let Some((i, depth)) = stack.pop() {
                        let n = &m.nodes[i];
                        ui.horizontal(|ui| {
                            ui.add_space(depth as f32 * 12.0);
                            let mut vis = !o.hidden.contains(&i);
                            if shows[i] && ui.checkbox(&mut vis, "").changed() {
                                if vis {
                                    o.hidden.remove(&i);
                                } else {
                                    o.hidden.insert(i);
                                }
                                changed = true;
                            }
                            let label = if n.joint { format!("● {}", n.name) } else { n.name.clone() };
                            let posed = o.pose.contains_key(&i) || o.offsets.contains_key(&i);
                            let text = if posed { egui::RichText::new(label).strong() } else { egui::RichText::new(label) };
                            if ui.selectable_label(e.node == Some(i), text).clicked() {
                                e.node = Some(i);
                            }
                        });
                        for &c in n.children.iter().rev() {
                            stack.push((c, depth + 1));
                        }
                    }
                });
                if let Some(i) = e.node.filter(|&i| i < m.nodes.len()) {
                    ui.separator();
                    ui.label(&m.nodes[i].name);
                    let mut r = euler(o.pose.get(&i).copied().unwrap_or(Quat::IDENTITY));
                    let mut any = false;
                    for (k, axis) in ["X", "Y", "Z"].iter().enumerate() {
                        any |= ui.add(egui::Slider::new(&mut r[k], -180.0..=180.0).suffix("°").text(format!("{} {axis}", tr("Поворот")))).changed();
                    }
                    if any {
                        o.pose.insert(i, from_euler(r));
                        changed = true;
                    }
                    if !m.nodes[i].joint {
                        let mut off = o.offsets.get(&i).copied().unwrap_or(Vec3::ZERO);
                        ui.horizontal(|ui| {
                            ui.label(tr("Сдвиг"));
                            let mut moved = false;
                            for v in [&mut off.x, &mut off.y, &mut off.z] {
                                moved |= ui.add(egui::DragValue::new(v).speed(0.01)).changed();
                            }
                            if moved {
                                o.offsets.insert(i, off);
                                changed = true;
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        if ui.button(tr("Сбросить")).clicked() {
                            o.pose.remove(&i);
                            o.offsets.remove(&i);
                            changed = true;
                        }
                        if ui.button(tr("Сбросить всю позу")).clicked() {
                            o.pose.clear();
                            o.offsets.clear();
                            changed = true;
                        }
                    });
                }
            });
            let morphs: Vec<(usize, usize, String)> = m.meshes.iter().enumerate().flat_map(|(mi, me)| {
                let n = me.prims.iter().map(|p| p.morphs.len()).max().unwrap_or(0);
                (0..n).map(move |k| (mi, k, me.morph_names.get(k).cloned().filter(|s| !s.is_empty()).unwrap_or_else(|| format!("{} {}", me.name, k + 1))))
            }).collect();
            if !morphs.is_empty() {
                egui::CollapsingHeader::new(tr("Морфы (форма)")).default_open(false).show(ui, |ui| {
                    egui::ScrollArea::vertical().id_salt("3d_morphs").max_height(200.0).show(ui, |ui| {
                        for (mi, k, name) in morphs {
                            let mut v = o.morph(&m, mi, k);
                            if ui.add(egui::Slider::new(&mut v, 0.0..=1.0).text(name)).changed() {
                                o.morphs.insert((mi, k), v);
                                changed = true;
                            }
                        }
                    });
                });
            }
        });
        if changed {
            e.changed();
        }
        if let Some(m) = msg {
            self.status = m;
        }
        if frame_all {
            let models = e.models.clone();
            paint_3d::frame(&mut e.scene, &models);
            e.changed();
        }
        if add {
            let dir = self.file.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default().join("x"));
            if !self.native_dialog(files::Ask::Model3d(true), &dir) {
                self.status = tr("Окно выбора файла недоступно; перетащите файл модели в окно").to_string();
            }
        }
        self.s3d_mode = mode;
        if !open {
            self.show_3d = false;
        }
        if to_lines {
            self.s3d_to_lines();
        }
    }
}

/// Папка поз в своей папке программы.
fn poses_dir() -> Option<std::path::PathBuf> {
    Some(crate::settings::dir()?.join("poses"))
}

/// Запомненные позы по алфавиту.
fn pose_names() -> Vec<String> {
    let Some(rd) = poses_dir().and_then(|d| std::fs::read_dir(d).ok()) else { return Vec::new() };
    let mut out: Vec<String> = rd.flatten().filter_map(|e| {
        let p = e.path();
        p.extension().is_some_and(|x| x == "gspose").then(|| p.file_stem()?.to_str().map(str::to_string)).flatten()
    }).collect();
    out.sort_by_key(|n| n.to_lowercase());
    out
}

/// Позы: отразить, запомнить под именем, взять запомненную, удалить. `true` — поза изменилась.
fn pose_row(ui: &mut egui::Ui, o: &mut Object, m: &Model, name: &mut Option<String>, msg: &mut Option<String>) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        if ui.button(tr("Отразить позу")).on_hover_text(tr("Левая ↔ правая: суставы с «Left/Right», «.L/.R», «_L/_R» в имени меняются позами")).clicked() {
            paint_3d::pose::mirror(o, m);
            changed = true;
        }
        if ui.button(tr("Запомнить позу…")).clicked() {
            *name = Some(String::new());
        }
        let names = pose_names();
        if !names.is_empty() {
            egui::ComboBox::from_id_salt("3d_poses").selected_text(tr("Взять позу")).show_ui(ui, |ui| {
                for n in &names {
                    ui.horizontal(|ui| {
                        if ui.small_button("✕").on_hover_text(tr("Удалить")).clicked() {
                            if let Some(d) = poses_dir() {
                                let _ = std::fs::remove_file(d.join(format!("{n}.gspose")));
                            }
                        }
                        if ui.selectable_label(false, n).clicked() {
                            let text = poses_dir().and_then(|d| std::fs::read_to_string(d.join(format!("{n}.gspose"))).ok()).unwrap_or_default();
                            match paint_3d::pose::apply_text(o, m, &text) {
                                Ok((_, 0)) => *msg = Some(trf("Поза «{0}»", &[n])),
                                Ok((_, missed)) => *msg = Some(trf("Поза «{0}»: у этой модели нет суставов из позы: {1}", &[n, &missed])),
                                Err(e) => *msg = Some(trf("Поза «{0}» не открылась: {1}", &[n, &e])),
                            }
                            changed = true;
                        }
                    });
                }
            });
        }
    });
    let mut close = false;
    if let Some(n) = name.as_mut() {
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(n).hint_text(tr("Имя позы")).desired_width(150.0));
            r.request_focus();
            let ok = ui.button(tr("Запомнить")).clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            if ok && !n.trim().is_empty() {
                let file = crate::assets::clean_name(n);
                let res = poses_dir().ok_or_else(|| tr("нет папки настроек").to_string()).and_then(|d| {
                    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
                    paint_core::io::write_atomic(&d.join(format!("{file}.gspose")), paint_3d::pose::to_text(o, m).as_bytes()).map_err(|e| e.to_string())
                });
                *msg = Some(match res {
                    Ok(()) => trf("Поза «{0}» запомнена", &[&file]),
                    Err(e) => trf("Поза не запомнена: {0}", &[&e]),
                });
                close = true;
            }
            if ui.button(tr("Отмена")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                close = true;
            }
        });
    }
    if close {
        *name = None;
    }
    changed
}
