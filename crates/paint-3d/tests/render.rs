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

/// «В линии»: у фигуры есть очертание, все точки — на холсте и в рамке её силуэта; сгибы
/// прибавляют линий; на 1500×2000 — быстро.
#[test]
fn figure_to_lines() {
    let (s, models) = figure();
    let (w, h) = (1500u32, 2000u32);
    let t = std::time::Instant::now();
    let outline = paint_3d::lines::lines(&s, &models, w, h, 180.0);
    let ms = t.elapsed().as_millis();
    let creases = paint_3d::lines::lines(&s, &models, w, h, 30.0);
    let img = paint_3d::render::render(&s, &models, w, h, 1);
    let (x0, y0, x1, y1, _) = bounds(&img, w as usize);
    let pts: Vec<[f32; 2]> = outline.iter().flatten().copied().collect();
    assert!(pts.len() > 20, "{}", pts.len());
    for p in &pts {
        assert!(p[0] >= x0 as f32 - 3.0 && p[0] <= x1 as f32 + 3.0 && p[1] >= y0 as f32 - 3.0 && p[1] <= y1 as f32 + 3.0, "{p:?}");
    }
    let len = |l: &Vec<Vec<[f32; 2]>>| l.iter().map(|p| p.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum::<f32>()).sum::<f32>();
    assert!(len(&creases) > len(&outline));
    println!("линий {} / {}, {ms} мс", outline.len(), creases.len());
    if let Some(out) = std::env::var_os("GS_LINES_PNG") {
        let mut im = img.clone();
        for l in &creases {
            for s in l.windows(2) {
                let n = ((s[1][0] - s[0][0]).hypot(s[1][1] - s[0][1]).ceil() as usize).max(1);
                for k in 0..=n {
                    let t = k as f32 / n as f32;
                    let (x, y) = ((s[0][0] + (s[1][0] - s[0][0]) * t) as usize, (s[0][1] + (s[1][1] - s[0][1]) * t) as usize);
                    if x < w as usize && y < h as usize {
                        im[(y * w as usize + x) * 4..][..4].copy_from_slice(&[255, 0, 0, 255]);
                    }
                }
            }
        }
        paint_core::io::save_image(std::path::Path::new(&out), w, h, &im).unwrap();
    }
    assert!(ms < 3000, "{ms} мс");
}

/// FBX той же фигуры (Blender, тот же скрипт) даёт ту же картинку, что glTF: оси, единицы,
/// текстура; поза локтя и морф живота работают так же.
#[test]
fn fbx_matches_gltf() {
    let load = |name: &str| {
        let m = Arc::new(model::load(&data(name)).unwrap());
        let mut s = Scene::default();
        s.objects.push(Object::new(1));
        s.camera.yaw = 0.0;
        s.camera.pitch = 0.0;
        let models = vec![m];
        paint_3d::frame(&mut s, &models);
        (s, models)
    };
    let (sg, mg) = load("figure.glb");
    let (sf, mf) = load("figure.fbx");
    let f = &mf[0];
    assert!(!f.images.is_empty(), "текстура FBX не прочитана");
    assert!(f.skins.len() == 1 && f.joints().count() >= 40, "скелет FBX: {} суставов", f.joints().count());
    let shot = |s: &Scene, m: &Vec<Arc<model::Model>>, name: &str| {
        let img = render::render(s, m, 300, 400, 1);
        save(name, &img, 300, 400);
        img
    };
    let (g, x) = (shot(&sg, &mg, "fbx-gltf.png"), shot(&sf, &mf, "fbx-fbx.png"));
    let (bg, bx) = (bounds(&g, 300), bounds(&x, 300));
    let near = |a: usize, b: usize, d: usize| a.abs_diff(b) <= d;
    assert!(near(bg.0, bx.0, 3) && near(bg.1, bx.1, 3) && near(bg.2, bx.2, 3) && near(bg.3, bx.3, 3), "рамка: glTF {bg:?}, FBX {bx:?}");
    assert!(near(bg.4, bx.4, bg.4 / 50), "площадь: glTF {}, FBX {}", bg.4, bx.4);
    // цвет текстуры почти тот же
    let diff: u64 = g.iter().zip(&x).map(|(a, b)| a.abs_diff(*b) as u64).sum::<u64>() / (300 * 400);
    assert!(diff < 6, "средняя разница {diff}");
    // поза и морф
    let bend = |s: &Scene, m: &Vec<Arc<model::Model>>| {
        let fore = m[0].nodes.iter().position(|n| n.name == "forearm.L").unwrap();
        let mut b = s.clone();
        b.objects[0].pose.insert(fore, glam::Quat::from_rotation_z(80f32.to_radians()));
        bounds(&render::render(&b, m, 300, 400, 1), 300)
    };
    let (pg, px) = (bend(&sg, &mg), bend(&sf, &mf));
    assert!(near(pg.1, px.1, 3) && near(pg.4, px.4, pg.4 / 50), "поза: glTF {pg:?}, FBX {px:?}");
    let belly = |s: &Scene, m: &Vec<Arc<model::Model>>| {
        let mi = m[0].meshes.iter().position(|x| x.morph_names.iter().any(|n| n == "Belly")).expect("нет морфа Belly");
        let mut b = s.clone();
        b.camera.yaw = 90.0;
        b.objects[0].morphs.insert((mi, 0), 1.0);
        bounds(&render::render(&b, m, 300, 400, 1), 300).4
    };
    let (mgb, mfb) = (belly(&sg, &mg), belly(&sf, &mf));
    assert!(near(mgb, mfb, mgb / 50), "морф: glTF {mgb}, FBX {mfb}");
}

/// Обрезанный или испорченный FBX — ошибка, а не падение.
#[test]
fn broken_fbx_is_an_error() {
    let f = data("figure.fbx");
    let d = &f[0].1;
    for n in (0..d.len()).step_by(997) {
        let _ = model::load(&[("figure.fbx".to_string(), d[..n].to_vec())]);
    }
    let mut bad = d.clone();
    for i in (200..bad.len()).step_by(61) {
        bad[i] ^= 0x5a;
    }
    let _ = model::load(&[("figure.fbx".to_string(), bad)]);
}

/// VRM — это GLB с расширениями VRoid: читается тем же путём.
#[test]
fn vrm_is_read_as_glb() {
    let mut f = data("figure.glb");
    f[0].0 = "figure.vrm".to_string();
    let m = model::load(&f).unwrap();
    assert!(m.joints().count() >= 40);
}

/// Запись PMX 2.0 (UTF-8, номера по 4 байта) из модели — для проверки чтения: вершины в позе
/// покоя, кости — суставы скелета, морфы вершин, текстура в подпапке `tex`.
fn to_pmx(m: &model::Model) -> Vec<(String, Vec<u8>)> {
    let mut o: Vec<u8> = b"PMX ".to_vec();
    let f = |o: &mut Vec<u8>, v: f32| o.extend(v.to_le_bytes());
    let i = |o: &mut Vec<u8>, v: i32| o.extend(v.to_le_bytes());
    let t = |o: &mut Vec<u8>, s: &str| {
        o.extend((s.len() as i32).to_le_bytes());
        o.extend(s.as_bytes());
    };
    let to = |v: glam::Vec3| glam::Vec3::new(v.x, v.y, -v.z) / 0.08;
    f(&mut o, 2.0);
    o.push(8);
    o.extend([1, 0, 4, 4, 4, 4, 4, 4]);
    for _ in 0..4 {
        t(&mut o, "");
    }
    let obj = Object::new(1);
    let g = obj.globals(m);
    let skin = &m.skins[0];
    let rest: Vec<glam::Mat4> = skin.joints.iter().zip(&skin.inv_bind).map(|(&j, ib)| g[j] * *ib).collect();
    // вершины всех частей подряд
    let (mut verts, mut faces, mut mats, mut belly) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (ni, node) in m.nodes.iter().enumerate() {
        let Some(mi) = node.mesh else { continue };
        for p in &m.meshes[mi].prims {
            let base = verts.len() as i32;
            for v in 0..p.pos.len() {
                let (jj, ww) = if p.joints.is_empty() { ([0u16; 4], [0f32; 4]) } else { (p.joints[v], p.weights[v]) };
                let total: f32 = ww.iter().sum();
                let mtx = if total > 0.0 { (0..4).fold(glam::Mat4::ZERO, |a, k| a + rest[jj[k] as usize] * (ww[k] / total)) } else { g[ni] };
                let morph = p.morphs.first().map(|d| mtx.transform_vector3(d[v]));
                belly.push(morph.unwrap_or_default());
                verts.push((mtx.transform_point3(p.pos[v]), p.nrm.get(v).map_or(glam::Vec3::Y, |n| mtx.transform_vector3(*n).normalize()), p.uv.get(v).copied().unwrap_or_default(), jj, ww));
            }
            for tr in p.idx.as_chunks::<3>().0 {
                faces.extend([base + tr[0] as i32, base + tr[2] as i32, base + tr[1] as i32]);
            }
            mats.push((p.color, p.tex, p.double, p.idx.len() as i32));
        }
    }
    i(&mut o, verts.len() as i32);
    for (pos, n, uv, jj, ww) in &verts {
        for v in to(*pos).to_array().into_iter().chain(glam::Vec3::new(n.x, n.y, -n.z).to_array()).chain(uv.to_array()) {
            f(&mut o, v);
        }
        o.push(2);
        for j in jj {
            i(&mut o, *j as i32);
        }
        for w in ww {
            f(&mut o, *w);
        }
        f(&mut o, 1.0);
    }
    i(&mut o, faces.len() as i32);
    for x in &faces {
        i(&mut o, *x);
    }
    i(&mut o, 1);
    t(&mut o, "tex\\skin.png");
    i(&mut o, mats.len() as i32);
    for (c, tex, double, n) in &mats {
        t(&mut o, "mat");
        t(&mut o, "");
        for v in c {
            f(&mut o, *v);
        }
        for _ in 0..7 {
            f(&mut o, 0.0);
        }
        o.push(if *double { 1 } else { 0 });
        for _ in 0..5 {
            f(&mut o, 0.0);
        }
        i(&mut o, if tex.is_some() { 0 } else { -1 });
        i(&mut o, -1);
        o.extend([0, 1, 0]);
        t(&mut o, "");
        i(&mut o, *n);
    }
    i(&mut o, skin.joints.len() as i32);
    for &j in &skin.joints {
        t(&mut o, &m.nodes[j].name);
        t(&mut o, "");
        for v in to(g[j].w_axis.truncate()).to_array() {
            f(&mut o, v);
        }
        i(&mut o, m.nodes[j].parent.and_then(|p| skin.joints.iter().position(|&x| x == p)).map_or(-1, |p| p as i32));
        i(&mut o, 0);
        o.extend(0u16.to_le_bytes());
        for _ in 0..3 {
            f(&mut o, 0.0);
        }
    }
    i(&mut o, 1);
    t(&mut o, "Belly");
    t(&mut o, "");
    o.extend([4, 1]);
    let moved: Vec<(usize, glam::Vec3)> = belly.iter().enumerate().filter(|(_, d)| d.length() > 0.0).map(|(k, d)| (k, *d)).collect();
    i(&mut o, moved.len() as i32);
    for (k, d) in moved {
        i(&mut o, k as i32);
        for v in to(d).to_array() {
            f(&mut o, v);
        }
    }
    let img = &m.images[0];
    let png = paint_core::io::encode_png(img.w, img.h, &img.rgba).unwrap();
    vec![("figure.pmx".into(), o), ("skin.png".into(), png)]
}

/// PMX той же фигуры даёт ту же картинку, что glTF; морф и поза работают.
#[test]
fn pmx_matches_gltf() {
    let (sg, mg) = figure();
    let files = to_pmx(&mg[0]);
    let mp = Arc::new(model::load(&files).unwrap());
    assert!(!mp.images.is_empty(), "текстура PMX не прочитана");
    assert!(mp.joints().count() >= 40);
    let mut sp = Scene::default();
    sp.objects.push(Object::new(1));
    sp.camera.yaw = 0.0;
    sp.camera.pitch = 0.0;
    let mp = vec![mp];
    paint_3d::frame(&mut sp, &mp);
    let g = render::render(&sg, &mg, 300, 400, 1);
    let x = render::render(&sp, &mp, 300, 400, 1);
    save("pmx-pmx.png", &x, 300, 400);
    let (bg, bx) = (bounds(&g, 300), bounds(&x, 300));
    let near = |a: usize, b: usize, d: usize| a.abs_diff(b) <= d;
    assert!(near(bg.0, bx.0, 3) && near(bg.1, bx.1, 3) && near(bg.2, bx.2, 3) && near(bg.3, bx.3, 3), "рамка: glTF {bg:?}, PMX {bx:?}");
    let diff: u64 = g.iter().zip(&x).map(|(a, b)| a.abs_diff(*b) as u64).sum::<u64>() / (300 * 400);
    assert!(diff < 6, "средняя разница {diff}");
    // морф живота
    let belly = |s: &Scene, m: &Vec<Arc<model::Model>>| {
        let mi = m[0].meshes.iter().position(|x| x.morph_names.iter().any(|n| n == "Belly")).expect("нет морфа Belly");
        let mut b = s.clone();
        b.camera.yaw = 90.0;
        b.objects[0].morphs.insert((mi, 0), 1.0);
        bounds(&render::render(&b, m, 300, 400, 1), 300).4
    };
    let (a, b) = (belly(&sg, &mg), belly(&sp, &mp));
    assert!(near(a, b, a / 50), "морф: glTF {a}, PMX {b}");
    // поза: предплечье поворачивается
    let fore = mp[0].nodes.iter().position(|n| n.name == "forearm.L").unwrap();
    let mut bent = sp.clone();
    bent.objects[0].pose.insert(fore, glam::Quat::from_rotation_z(80f32.to_radians()));
    assert_ne!(render::render(&bent, &mp, 300, 400, 1), x);
    // обрезанный файл — ошибка, а не падение
    for n in (0..files[0].1.len()).step_by(331) {
        let _ = model::load(&[("figure.pmx".to_string(), files[0].1[..n].to_vec())]);
    }
}

/// PMX с диска: текстура берётся из подпапки по пути из файла.
#[test]
fn pmx_gathers_textures_from_subfolders() {
    let (_, mg) = figure();
    let files = to_pmx(&mg[0]);
    let dir = std::env::temp_dir().join(format!("gs-pmx-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("tex")).unwrap();
    std::fs::write(dir.join("figure.pmx"), &files[0].1).unwrap();
    std::fs::write(dir.join("tex/skin.png"), &files[1].1).unwrap();
    let got = paint_3d::gather(&dir.join("figure.pmx")).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    assert!(got.iter().any(|(n, _)| n == "skin.png"), "{:?}", got.iter().map(|f| &f.0).collect::<Vec<_>>());
    assert!(!model::load(&got).unwrap().images.is_empty());
}

/// COLLADA: куб из четырёхугольников (`polylist`), ось Z вверх, сантиметры, текстура через
/// sampler2D → surface → image, узел со сдвигом и `instance_node`.
#[test]
fn collada_cube() {
    let dae = r##"<?xml version="1.0" encoding="utf-8"?>
<COLLADA xmlns="http://www.collada.org/2005/11/COLLADASchema" version="1.4.1">
  <asset><unit name="centimeter" meter="0.01"/><up_axis>Z_UP</up_axis></asset>
  <library_images><image id="img"><init_from>textures/check.png</init_from></image></library_images>
  <library_effects><effect id="eff"><profile_COMMON>
    <newparam sid="surf"><surface type="2D"><init_from>img</init_from></surface></newparam>
    <newparam sid="samp"><sampler2D><source>surf</source></sampler2D></newparam>
    <technique sid="common"><lambert><diffuse><texture texture="samp" texcoord="UV"/></diffuse></lambert></technique>
  </profile_COMMON></effect></library_effects>
  <library_materials><material id="mat"><instance_effect url="#eff"/></material></library_materials>
  <library_geometries><geometry id="cube" name="Cube"><mesh>
    <source id="p"><float_array id="pa" count="24">-50 -50 0 50 -50 0 50 50 0 -50 50 0 -50 -50 100 50 -50 100 50 50 100 -50 50 100</float_array>
      <technique_common><accessor source="#pa" count="8" stride="3"/></technique_common></source>
    <source id="t"><float_array id="ta" count="8">0 0 1 0 1 1 0 1</float_array>
      <technique_common><accessor source="#ta" count="4" stride="2"/></technique_common></source>
    <vertices id="v"><input semantic="POSITION" source="#p"/></vertices>
    <polylist material="sym" count="6">
      <input semantic="VERTEX" source="#v" offset="0"/><input semantic="TEXCOORD" source="#t" offset="1" set="0"/>
      <vcount>4 4 4 4 4 4</vcount>
      <p>0 0 3 3 2 2 1 1  4 0 5 1 6 2 7 3  0 0 1 1 5 2 4 3  1 0 2 1 6 2 5 3  2 0 3 1 7 2 6 3  3 0 0 1 4 2 7 3</p>
    </polylist>
  </mesh></geometry></library_geometries>
  <library_nodes><node id="box"><instance_geometry url="#cube"><bind_material><technique_common>
    <instance_material symbol="sym" target="#mat"/></technique_common></bind_material></instance_geometry></node></library_nodes>
  <library_visual_scenes><visual_scene id="s"><node name="Up"><translate>0 0 200</translate><instance_node url="#box"/></node></visual_scene></library_visual_scenes>
  <scene><instance_visual_scene url="#s"/></scene>
</COLLADA>"##;
    let png = paint_core::io::encode_png(2, 2, &[255, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 255]).unwrap();
    let files = vec![("cube.dae".to_string(), dae.as_bytes().to_vec()), ("check.png".to_string(), png)];
    let m = model::load(&files).unwrap();
    assert_eq!(m.triangles, 12);
    assert_eq!(m.images.len(), 1, "текстура не прочитана");
    // низ куба — на высоте 2 м (Z вверх → Y вверх), ребро — 1 м
    let g = Object::new(1).globals(&m);
    let ni = m.nodes.iter().position(|n| n.mesh.is_some()).unwrap();
    let p = &m.meshes[m.nodes[ni].mesh.unwrap()].prims[0];
    let world: Vec<glam::Vec3> = p.pos.iter().map(|v| g[ni].transform_point3(*v)).collect();
    let (lo, hi) = world.iter().fold((glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN)), |(a, b), v| (a.min(*v), b.max(*v)));
    assert!((lo - glam::Vec3::new(-0.5, 2.0, -0.5)).length() < 1e-4 && (hi - glam::Vec3::new(0.5, 3.0, 0.5)).length() < 1e-4, "{lo} {hi}");
    // на диске текстура из подпапки по пути из файла
    let dir = std::env::temp_dir().join(format!("gs-dae-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("textures")).unwrap();
    std::fs::write(dir.join("cube.dae"), dae).unwrap();
    std::fs::write(dir.join("textures/check.png"), &files[1].1).unwrap();
    let got = paint_3d::gather(&dir.join("cube.dae")).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(model::load(&got).unwrap().images.len(), 1);
    // испорченный файл — ошибка, а не падение
    for n in (0..dae.len()).step_by(37) {
        let _ = model::load(&[("cube.dae".to_string(), dae.as_bytes()[..n].to_vec())]);
    }
}
