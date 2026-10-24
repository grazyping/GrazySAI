//! Слой 3D GrazySAI: модели glTF/GLB и OBJ, сцена (положение, поза, морфы, камера), отрисовка на
//! процессоре в обычные пиксели слоя. План — `docs/39-3d-reference.md`.

pub mod model;
pub mod pose;
pub mod render;
pub mod scene;
pub mod vp;

pub use model::{Error, Model};
pub use scene::{Camera, Display, Object, Scene};

use paint_core::layer::{Asset3d, Scene3dData};
use paint_core::tile::ColorMap;
use std::sync::{Arc, Mutex};

/// Разобранные модели по номеру (разбор большой модели — секунды, делается один раз).
fn cache() -> &'static Mutex<Vec<(u64, Arc<Model>)>> {
    static C: Mutex<Vec<(u64, Arc<Model>)>> = Mutex::new(Vec::new());
    &C
}

/// Модель по файлу слоя (из кэша или разобрать).
pub fn model_of(a: &Asset3d) -> Result<Arc<Model>, Error> {
    if let Some(m) = cache().lock().unwrap().iter().find(|(id, _)| *id == a.id).map(|(_, m)| m.clone()) {
        return Ok(m);
    }
    let m = Arc::new(model::load(&a.files)?);
    let mut c = cache().lock().unwrap();
    c.retain(|(id, _)| *id != a.id);
    c.push((a.id, m.clone()));
    // в памяти — последние 8 моделей
    if c.len() > 8 {
        c.remove(0);
    }
    Ok(m)
}

/// Модели предметов сцены по порядку (нет файла — пустая модель, предмет не рисуется).
pub fn models_for(scene: &Scene, data: &Scene3dData) -> Result<Vec<Arc<Model>>, Error> {
    scene
        .objects
        .iter()
        .map(|o| match data.assets.iter().find(|a| a.id == o.asset) {
            Some(a) => model_of(a),
            None => Ok(Arc::new(Model::default())),
        })
        .collect()
}

/// Новый номер модели.
pub fn new_asset_id() -> u64 {
    paint_core::layer::next_uid()
}

/// Сцена слоя → пиксели слоя размером с холст.
pub fn render_layer(data: &Scene3dData, w: u32, h: u32, ss: u32) -> Result<ColorMap, Error> {
    let scene = Scene::parse(&data.state);
    let models = models_for(&scene, data)?;
    Ok(to_colormap(&render::render(&scene, &models, w, h, ss), w, h))
}

/// RGBA без предумножения → плитки слоя (пустые плитки не создаются).
pub fn to_colormap(rgba: &[u8], w: u32, h: u32) -> ColorMap {
    let mut m = ColorMap::new();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let o = (y as usize * w as usize + x as usize) * 4;
            if rgba[o + 3] != 0 {
                m.set(x, y, paint_core::pixel::from_rgba8([rgba[o], rgba[o + 1], rgba[o + 2], rgba[o + 3]]));
            }
        }
    }
    m
}

/// Поставить камеру так, чтобы все предметы были видны целиком (вид чуть сбоку и сверху).
pub fn frame(scene: &mut Scene, models: &[Arc<Model>]) {
    let (mut lo, mut hi) = (glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN));
    for (o, m) in scene.objects.iter().zip(models) {
        let g = o.globals(m);
        for (ni, n) in m.nodes.iter().enumerate() {
            let Some(mi) = n.mesh else { continue };
            for p in &m.meshes[mi].prims {
                for v in &p.pos {
                    let w = g[ni].transform_point3(*v);
                    lo = lo.min(w);
                    hi = hi.max(w);
                }
            }
        }
    }
    if lo.x > hi.x {
        return;
    }
    let c = &mut scene.camera;
    c.target = (lo + hi) / 2.0;
    let r = ((hi - lo).length() / 2.0).max(1e-3);
    c.distance = r / (c.fov.to_radians() / 2.0).sin() * 1.05;
}

/// Слой 3D из файла модели: главный файл и соседние (буферы, текстуры).
pub fn new_layer_data(files: Vec<(String, Vec<u8>)>) -> Result<(Scene3dData, String), Error> {
    let name = files.first().map(|f| f.0.clone()).unwrap_or_default();
    let asset = Asset3d { id: new_asset_id(), name: name.clone(), files };
    let m = model_of(&asset)?;
    let mut scene = Scene::default();
    scene.objects.push(Object::new(asset.id));
    frame(&mut scene, std::slice::from_ref(&m));
    Ok((Scene3dData { assets: vec![Arc::new(asset)], state: scene.to_text() }, name))
}

/// Расширения файлов моделей.
pub const MODEL_EXTS: [&str; 3] = ["glb", "gltf", "obj"];

/// Прочитать модель с диска вместе с соседними файлами, на которые она ссылается (буферы и
/// текстуры glTF, материалы и текстуры OBJ). Берутся только файлы из той же папки, по имени.
pub fn gather(path: &std::path::Path) -> Result<Vec<(String, Vec<u8>)>, Error> {
    const MAX: u64 = 2 << 30;
    let read = |p: &std::path::Path| -> Option<Vec<u8>> {
        let m = std::fs::metadata(p).ok()?;
        (m.is_file() && m.len() <= MAX).then(|| std::fs::read(p).ok()).flatten()
    };
    let main = read(path).ok_or_else(|| Error(format!("не удалось прочитать {}", path.display())))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "model".into());
    let dir = path.parent().map(std::path::Path::to_path_buf).unwrap_or_default();
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mut wanted: Vec<String> = Vec::new();
    let base = |s: &str| s.rsplit(['/', '\\']).next().unwrap_or(s).trim().to_string();
    if ext == "gltf" {
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&main) {
            for key in ["buffers", "images"] {
                for item in v.get(key).and_then(|x| x.as_array()).into_iter().flatten() {
                    if let Some(u) = item.get("uri").and_then(|u| u.as_str()).filter(|u| !u.starts_with("data:")) {
                        wanted.push(base(u));
                    }
                }
            }
        }
    } else if ext == "obj" {
        let text = String::from_utf8_lossy(&main);
        for line in text.lines() {
            if let Some(rest) = line.trim().strip_prefix("mtllib") {
                wanted.extend(rest.split_whitespace().map(base));
            }
        }
    }
    let mut files = vec![(name, main)];
    let mut i = 0;
    while i < wanted.len() && files.len() < 200 {
        let w = wanted[i].clone();
        i += 1;
        if w.is_empty() || files.iter().any(|(n, _)| n.eq_ignore_ascii_case(&w)) {
            continue;
        }
        let Some(data) = read(&dir.join(&w)) else { continue };
        if w.to_lowercase().ends_with(".mtl") {
            for line in String::from_utf8_lossy(&data).lines() {
                let t = line.trim();
                if t.starts_with("map_") || t.starts_with("bump") || t.starts_with("disp") {
                    if let Some(last) = t.split_whitespace().last() {
                        wanted.push(base(last));
                    }
                }
            }
        }
        files.push((w, data));
    }
    Ok(files)
}
