//! Отрисовка тестовых моделей (сделаны своим скриптом `tools/3d/make_test_models.py`).

use paint_3d::{model, render, Display, Object, Scene};
use std::sync::Arc;

fn data(name: &str) -> Vec<(String, Vec<u8>)> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name);
    vec![(name.to_string(), std::fs::read(p).unwrap())]
}

fn figure() -> (Scene, Vec<Arc<model::Model>>) {
    let m = Arc::new(model::load(&data("figure.glb")).unwrap());
    let mut s = Scene::default();
    s.objects.push(Object::new(1));
    s.camera.yaw = 0.0;
    s.camera.pitch = 0.0;
    let models = vec![m];
    paint_3d::frame(&mut s, &models);
    (s, models)
}

/// Прямоугольник непрозрачного (x0, y0, x1, y1) и число точек.
fn bounds(img: &[u8], w: usize) -> (usize, usize, usize, usize, usize) {
    let (mut b, mut n) = ((usize::MAX, usize::MAX, 0, 0), 0);
    for (i, p) in img.as_chunks::<4>().0.iter().enumerate() {
        if p[3] > 128 {
            let (x, y) = (i % w, i / w);
            b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
            n += 1;
        }
    }
    (b.0, b.1, b.2, b.3, n)
}

fn save(name: &str, img: &[u8], w: u32, h: u32) {
    if let Ok(dir) = std::env::var("RENDER_OUT") {
        paint_core::io::save_png(&std::path::Path::new(&dir).join(name), w, h, img).unwrap();
    }
}

#[test]
fn figure_fills_the_frame_in_every_mode() {
    let (mut s, models) = figure();
    for d in Display::ALL {
        s.display = d;
        let img = render::render(&s, &models, 300, 400, 2);
        save(&format!("figure-{}.png", d.key()), &img, 300, 400);
        let (x0, y0, x1, y1, n) = bounds(&img, 300);
        assert!(n > if d == Display::Outline { 800 } else { 2000 }, "{d:?}: мало точек {n}");
        // фигура в кадре целиком и по центру
        assert!(x0 > 0 && y0 > 0 && x1 < 299 && y1 < 399, "{d:?}: {x0},{y0}–{x1},{y1}");
        assert!(((x0 + x1) as i32 - 300).abs() < 40, "{d:?}: не по центру {x0}–{x1}");
    }
}

#[test]
fn pose_and_morph_change_pixels() {
    let (s, models) = figure();
    let m = &models[0];
    let base = render::render(&s, &models, 300, 400, 1);
    // локоть: предплечье поднято на 80°
    let fore = m.nodes.iter().position(|n| n.name == "forearm.L").unwrap();
    let mut bent = s.clone();
    bent.objects[0].pose.insert(fore, glam::Quat::from_rotation_z(80f32.to_radians()));
    let img = render::render(&bent, &models, 300, 400, 1);
    save("figure-bent.png", &img, 300, 400);
    let (_, y0a, ..) = bounds(&base, 300);
    let (_, y0b, ..) = bounds(&img, 300);
    assert!(y0b + 5 < y0a || base != img, "поза не изменила картинку");
    assert_ne!(base, img);
    // морф живота
    let mi = m.meshes.iter().position(|x| x.name == "Body").unwrap();
    let mut fat = s.clone();
    fat.objects[0].morphs.insert((mi, 0), 1.0);
    fat.camera.yaw = 90.0;
    let mut side = s.clone();
    side.camera.yaw = 90.0;
    side.objects[0].morphs.insert((mi, 0), 0.0);
    let ia = render::render(&side, &models, 300, 400, 1);
    let ib = render::render(&fat, &models, 300, 400, 1);
    save("figure-side.png", &ia, 300, 400);
    save("figure-belly.png", &ib, 300, 400);
    let a = bounds(&ia, 300).4;
    let b = bounds(&ib, 300).4;
    assert!(b > a + 50, "морф не добавил живота: {a} → {b}");
}

#[test]
fn hidden_parts_and_scene() {
    let m = Arc::new(model::load(&data("scene.glb")).unwrap());
    let mut s = Scene::default();
    s.objects.push(Object::new(1));
    let models = vec![m.clone()];
    paint_3d::frame(&mut s, &models);
    let all = bounds(&render::render(&s, &models, 400, 300, 1), 400).4;
    let house = m.nodes.iter().position(|n| n.name == "House3").unwrap();
    s.objects[0].hidden.insert(house);
    let less = bounds(&render::render(&s, &models, 400, 300, 1), 400).4;
    assert!(less < all, "скрытая часть видна: {all} → {less}");
}

/// Скорость: `HEAVY=путь/heavy.glb cargo test --release -p paint-3d --test render heavy -- --ignored --nocapture`.
#[test]
#[ignore]
fn heavy_scene_speed() {
    let path = std::env::var("HEAVY").unwrap();
    let t = std::time::Instant::now();
    let m = Arc::new(model::load(&[("heavy.glb".into(), std::fs::read(&path).unwrap())]).unwrap());
    eprintln!("load {} tris in {:?}", m.triangles, t.elapsed());
    let mut s = Scene::default();
    s.objects.push(Object::new(1));
    let models = vec![m];
    paint_3d::frame(&mut s, &models);
    for (w, h, ss) in [(1000, 1000, 1), (2000, 2000, 1), (2000, 2000, 2)] {
        let t = std::time::Instant::now();
        let img = render::render(&s, &models, w, h, ss);
        eprintln!("render {w}x{h} ss{ss}: {:?}, px {}", t.elapsed(), img.chunks(4).filter(|p| p[3] > 0).count());
    }
    s.display = Display::Outline;
    let t = std::time::Instant::now();
    render::render(&s, &models, 2000, 2000, 1);
    eprintln!("outline 2000: {:?}", t.elapsed());
}

/// `.gltf` с отдельным `.bin`: соседний файл подбирается по ссылке.
#[test]
fn separate_gltf_is_gathered() {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/scene.gltf");
    let files = paint_3d::gather(&p).unwrap();
    assert!(files.iter().any(|(n, _)| n == "scene.bin"), "{:?}", files.iter().map(|f| &f.0).collect::<Vec<_>>());
    let m = model::load(&files).unwrap();
    assert_eq!(m.triangles, model::load(&data("scene.glb")).unwrap().triangles);
}
