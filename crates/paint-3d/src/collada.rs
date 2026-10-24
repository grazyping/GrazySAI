//! COLLADA (`.dae`, 1.4 и 1.5: SketchUp, старые Cinema 4D, 3ds Max, Maya, Blender до 5.0): узлы с
//! преобразованиями, сетки (`triangles`, `polylist`, `polygons`), материалы — цвет и текстура.
//! Скелетные сетки (`instance_controller`) берутся в позе привязки, без костей.
//! Ось «вверх» и единица из `<asset>` приводятся к glTF: Y вверх, метры.

use crate::model::{err, Error, Image, Mesh, Model, Node, Prim, Res, MAX_TEXTURE_PIXELS, MAX_TRIANGLES, MAX_VERTICES};
use glam::{Mat4, Quat, Vec2, Vec3};
use roxmltree::{Document, Node as X};
use std::collections::HashMap;

fn kids<'a, 'i>(n: X<'a, 'i>, tag: &'static str) -> impl Iterator<Item = X<'a, 'i>> {
    n.children().filter(move |c| c.is_element() && c.tag_name().name() == tag)
}

fn kid<'a, 'i>(n: X<'a, 'i>, tag: &'static str) -> Option<X<'a, 'i>> {
    kids(n, tag).next()
}

fn floats(n: X) -> Vec<f32> {
    n.text().unwrap_or("").split_ascii_whitespace().map(|s| s.parse().unwrap_or(0.0)).collect()
}

fn url(s: &str) -> &str {
    s.strip_prefix('#').unwrap_or(s)
}

/// Матрица узла из `matrix`, `translate`, `rotate`, `scale` по порядку.
fn node_matrix(n: X) -> Mat4 {
    let mut m = Mat4::IDENTITY;
    for c in n.children().filter(|c| c.is_element()) {
        let v = floats(c);
        let t = match (c.tag_name().name(), v.len()) {
            ("matrix", 16) => Mat4::from_cols_array(&v.try_into().unwrap_or([0.0; 16])).transpose(),
            ("translate", 3) => Mat4::from_translation(Vec3::new(v[0], v[1], v[2])),
            ("rotate", 4) => {
                let axis = Vec3::new(v[0], v[1], v[2]);
                if axis.length_squared() > 0.0 { Mat4::from_axis_angle(axis.normalize(), v[3].to_radians()) } else { Mat4::IDENTITY }
            }
            ("scale", 3) => Mat4::from_scale(Vec3::new(v[0], v[1], v[2])),
            _ => continue,
        };
        m *= t;
    }
    m
}

struct Ctx<'a, 'i> {
    by_id: HashMap<&'a str, X<'a, 'i>>,
    files: &'a [(String, Vec<u8>)],
    /// Картинка по id `<image>` (None — не прочиталась).
    images: HashMap<String, Option<usize>>,
    /// Сетка по (геометрия, привязка материалов).
    meshes: HashMap<(String, Vec<(String, String)>), usize>,
    pixels: u64,
    vertices: usize,
}

impl<'a, 'i> Ctx<'a, 'i> {
    fn get(&self, id: &str) -> Option<X<'a, 'i>> {
        self.by_id.get(url(id)).copied()
    }

    /// Картинка `<image id>` → номер в модели.
    fn image(&mut self, id: &str, m: &mut Model) -> Res<Option<usize>> {
        if let Some(&i) = self.images.get(id) {
            return Ok(i);
        }
        let path = self.get(id).and_then(|img| img.descendants().find(|d| d.tag_name().name() == "init_from")).and_then(|f| {
            // 1.5: <init_from><ref>…</ref></init_from>
            kid(f, "ref").and_then(|r| r.text()).or(f.text()).map(str::trim)
        });
        let i = path.and_then(|p| Model::file(self.files, p.strip_prefix("file://").unwrap_or(p))).and_then(|b| paint_core::io::decode_image(b).ok()).filter(|(w, h, _)| *w > 0 && *h > 0).map(|(w, h, rgba)| {
            self.pixels += w as u64 * h as u64;
            m.images.push(Image { w, h, rgba });
            m.images.len() - 1
        });
        if self.pixels > MAX_TEXTURE_PIXELS {
            return err("слишком большие текстуры");
        }
        self.images.insert(id.to_string(), i);
        Ok(i)
    }

    /// Цвет и текстура материала (`<material>` → `<effect>` → `diffuse`).
    fn material(&mut self, mat_id: &str, m: &mut Model) -> Res<([f32; 4], Option<usize>, bool)> {
        let Some(eff) = self.get(mat_id).and_then(|mt| kid(mt, "instance_effect")).and_then(|ie| ie.attribute("url")).and_then(|u| self.get(u)) else {
            return Ok(([0.8, 0.8, 0.8, 1.0], None, false));
        };
        let double = eff.descendants().any(|d| d.tag_name().name() == "double_sided" && d.text().is_some_and(|t| t.trim() == "1"));
        let Some(diffuse) = eff.descendants().find(|d| d.tag_name().name() == "diffuse") else { return Ok(([0.8, 0.8, 0.8, 1.0], None, double)) };
        let mut color = [0.8, 0.8, 0.8, 1.0];
        if let Some(c) = kid(diffuse, "color") {
            let v = floats(c);
            if v.len() >= 3 {
                color = [v[0], v[1], v[2], v.get(3).copied().unwrap_or(1.0)];
            }
        }
        let mut tex = None;
        if let Some(t) = kid(diffuse, "texture").and_then(|t| t.attribute("texture")) {
            // texture → newparam sampler2D → (source → newparam surface → init_from | instance_image) → image
            let param = |sid: &str| eff.descendants().find(|d| d.tag_name().name() == "newparam" && d.attribute("sid") == Some(sid));
            let mut image = None;
            if let Some(s) = param(t) {
                let samp = s.descendants().find(|d| d.tag_name().name() == "sampler2D");
                if let Some(ii) = samp.and_then(|s| kid(s, "instance_image")).and_then(|i| i.attribute("url")) {
                    image = Some(url(ii).to_string());
                } else if let Some(src) = samp.and_then(|s| kid(s, "source")).and_then(|s| s.text()) {
                    image = param(src.trim()).and_then(|p| p.descendants().find(|d| d.tag_name().name() == "init_from")).and_then(|i| i.text()).map(|s| s.trim().to_string());
                }
            } else {
                // SketchUp и др.: прямо id картинки
                image = Some(t.to_string());
            }
            if let Some(id) = image {
                tex = self.image(&id, m)?;
                if tex.is_some() {
                    color = [1.0, 1.0, 1.0, color[3]];
                }
            }
        }
        Ok((color, tex, double))
    }

    /// Сетка геометрии с привязкой материалов (символ → id материала).
    fn mesh(&mut self, geo_id: &str, bind: Vec<(String, String)>, m: &mut Model) -> Res<Option<usize>> {
        let key = (geo_id.to_string(), bind.clone());
        if let Some(&i) = self.meshes.get(&key) {
            return Ok(Some(i));
        }
        let Some(geo) = self.get(geo_id) else { return Ok(None) };
        let Some(mesh) = kid(geo, "mesh") else { return Ok(None) };
        // источники: id → (числа, шаг)
        let mut sources: HashMap<&str, (Vec<f32>, usize)> = HashMap::new();
        for s in kids(mesh, "source") {
            let Some(id) = s.attribute("id") else { continue };
            let data = kid(s, "float_array").map(floats).unwrap_or_default();
            if data.len() > MAX_VERTICES * 4 {
                return err("слишком много вершин");
            }
            let stride = s.descendants().find(|d| d.tag_name().name() == "accessor").and_then(|a| a.attribute("stride")).and_then(|v| v.parse().ok()).unwrap_or(3usize).clamp(1, 64);
            sources.insert(id, (data, stride));
        }
        // <vertices id> → источник положений
        let mut vert_src: HashMap<&str, &str> = HashMap::new();
        for v in kids(mesh, "vertices") {
            if let (Some(id), Some(p)) = (v.attribute("id"), kids(v, "input").find(|i| i.attribute("semantic") == Some("POSITION")).and_then(|i| i.attribute("source"))) {
                vert_src.insert(id, url(p));
            }
        }
        let mut out = Mesh { name: geo.attribute("name").unwrap_or(geo_id).to_string(), ..Default::default() };
        for part in mesh.children().filter(|c| matches!(c.tag_name().name(), "triangles" | "polylist" | "polygons")) {
            let kind = part.tag_name().name();
            // входы: смысл, источник, смещение
            let inputs: Vec<(&str, &str, usize)> = kids(part, "input").filter_map(|i| Some((i.attribute("semantic")?, url(i.attribute("source")?), i.attribute("offset")?.parse().ok()?))).collect();
            let stride = inputs.iter().map(|i| i.2 + 1).max().unwrap_or(1);
            let src = |sem: &str| inputs.iter().find(|i| i.0 == sem).map(|i| (if sem == "VERTEX" { vert_src.get(i.1).copied().unwrap_or(i.1) } else { i.1 }, i.2));
            let Some((pos_src, pos_off)) = src("VERTEX") else { continue };
            let Some((pos, pstride)) = sources.get(pos_src) else { continue };
            let nrm = src("NORMAL").and_then(|(s, o)| sources.get(s).map(|d| (d, o)));
            let uv = src("TEXCOORD").and_then(|(s, o)| sources.get(s).map(|d| (d, o)));
            // многоугольники: число углов у каждого
            let mut polys: Vec<(Vec<u32>, usize)> = Vec::new();
            match kind {
                "triangles" => {
                    if let Some(p) = kid(part, "p") {
                        polys.push((p.text().unwrap_or("").split_ascii_whitespace().map(|s| s.parse().unwrap_or(0)).collect(), 0));
                    }
                }
                "polylist" => {
                    let vc: Vec<usize> = kid(part, "vcount").map(|v| v.text().unwrap_or("").split_ascii_whitespace().map(|s| s.parse().unwrap_or(0)).collect()).unwrap_or_default();
                    let p: Vec<u32> = kid(part, "p").map(|p| p.text().unwrap_or("").split_ascii_whitespace().map(|s| s.parse().unwrap_or(0)).collect()).unwrap_or_default();
                    let mut at = 0;
                    for n in vc {
                        let len = n.saturating_mul(stride);
                        if at + len > p.len() || n < 3 {
                            at += len;
                            continue;
                        }
                        polys.push((p[at..at + len].to_vec(), n));
                        at += len;
                    }
                }
                _ => {
                    for p in kids(part, "p") {
                        let v: Vec<u32> = p.text().unwrap_or("").split_ascii_whitespace().map(|s| s.parse().unwrap_or(0)).collect();
                        let n = v.len() / stride;
                        if n >= 3 {
                            polys.push((v, n));
                        }
                    }
                }
            }
            let mat_id = part.attribute("material").map(|sym| bind.iter().find(|b| b.0 == sym).map_or(sym.to_string(), |b| b.1.clone()));
            let (color, tex, double) = match &mat_id {
                Some(id) => self.material(id, m)?,
                None => ([0.8, 0.8, 0.8, 1.0], None, false),
            };
            let mut prim = Prim { color, tex, double, ..Default::default() };
            let mut seen: HashMap<Vec<u32>, u32> = HashMap::new();
            let mut corner = |c: &[u32], prim: &mut Prim| -> Res<u32> {
                if let Some(&v) = seen.get(c) {
                    return Ok(v);
                }
                let at = |data: &(Vec<f32>, usize), i: u32, k: usize| data.0.get(i as usize * data.1 + k).copied().unwrap_or(0.0);
                let pi = c[pos_off] as usize;
                if pi * pstride + 2 >= pos.len() {
                    return err("COLLADA: номер вершины вне данных");
                }
                prim.pos.push(Vec3::new(pos[pi * pstride], pos[pi * pstride + 1], pos[pi * pstride + 2]));
                if let Some((d, o)) = nrm {
                    prim.nrm.push(Vec3::new(at(d, c[o], 0), at(d, c[o], 1), at(d, c[o], 2)));
                }
                if let Some((d, o)) = uv {
                    // в COLLADA развёртка снизу вверх, в glTF — сверху вниз
                    prim.uv.push(Vec2::new(at(d, c[o], 0), 1.0 - at(d, c[o], 1)));
                }
                let v = prim.pos.len() as u32 - 1;
                seen.insert(c.to_vec(), v);
                Ok(v)
            };
            for (p, n) in polys {
                let corners: Vec<&[u32]> = p.chunks_exact(stride).collect();
                let groups: Vec<[usize; 3]> = if n == 0 {
                    (0..corners.len() / 3).map(|t| [t * 3, t * 3 + 1, t * 3 + 2]).collect()
                } else {
                    (2..corners.len()).map(|k| [0, k - 1, k]).collect()
                };
                for g in groups {
                    for k in g {
                        let v = corner(corners[k], &mut prim)?;
                        prim.idx.push(v);
                    }
                }
            }
            if prim.idx.is_empty() {
                continue;
            }
            self.vertices += prim.pos.len();
            m.triangles += prim.idx.len() / 3;
            if self.vertices > MAX_VERTICES {
                return err("слишком много вершин");
            }
            if m.triangles > MAX_TRIANGLES {
                return err("слишком много треугольников (больше 20 млн)");
            }
            out.prims.push(prim);
        }
        if out.prims.is_empty() {
            return Ok(None);
        }
        m.meshes.push(out);
        let i = m.meshes.len() - 1;
        self.meshes.insert(key, i);
        Ok(Some(i))
    }
}

fn push_node(m: &mut Model, name: String, parent: Option<usize>, mat: Mat4, mesh: Option<usize>) -> usize {
    let (s, r, t) = mat.to_scale_rotation_translation();
    let i = m.nodes.len();
    m.nodes.push(Node { name, parent, children: Vec::new(), t, r: if r.is_finite() { r.normalize() } else { Quat::IDENTITY }, s, mesh, skin: None, joint: false });
    if let Some(p) = parent {
        m.nodes[p].children.push(i);
    }
    i
}

/// Узел сцены со всеми вложенными (и `instance_node`); `depth` — защита от кругов.
fn walk(cx: &mut Ctx, n: X, parent: usize, m: &mut Model, depth: u32) -> Res<()> {
    if depth > 256 || m.nodes.len() > 1_000_000 {
        return err("COLLADA: слишком глубокое дерево узлов");
    }
    let name = n.attribute("name").or(n.attribute("id")).unwrap_or("node").to_string();
    let me = push_node(m, name.clone(), Some(parent), node_matrix(n), None);
    for c in n.children().filter(|c| c.is_element()) {
        match c.tag_name().name() {
            "node" => walk(cx, c, me, m, depth + 1)?,
            "instance_node" => {
                if let Some(t) = c.attribute("url").and_then(|u| cx.get(u)) {
                    walk(cx, t, me, m, depth + 1)?;
                }
            }
            "instance_geometry" | "instance_controller" => {
                let bind: Vec<(String, String)> = c
                    .descendants()
                    .filter(|d| d.tag_name().name() == "instance_material")
                    .filter_map(|im| Some((im.attribute("symbol")?.to_string(), url(im.attribute("target")?).to_string())))
                    .collect();
                let Some(u) = c.attribute("url") else { continue };
                // скелетная сетка — её исходная геометрия в позе привязки
                let (geo, bind_shape) = if c.tag_name().name() == "instance_controller" {
                    let skin = cx.get(u).and_then(|ctl| kid(ctl, "skin"));
                    let Some(src) = skin.and_then(|s| s.attribute("source")) else { continue };
                    let bsm = skin.and_then(|s| kid(s, "bind_shape_matrix")).map(floats).filter(|v| v.len() == 16).map(|v| Mat4::from_cols_array(&v.try_into().unwrap_or([0.0; 16])).transpose());
                    (url(src).to_string(), bsm.unwrap_or(Mat4::IDENTITY))
                } else {
                    (url(u).to_string(), Mat4::IDENTITY)
                };
                if let Some(mi) = cx.mesh(&geo, bind, m)? {
                    push_node(m, format!("{name} {}", m.meshes[mi].name), Some(me), bind_shape, Some(mi));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub fn load(data: &[u8], files: &[(String, Vec<u8>)]) -> Res<Model> {
    let text = std::str::from_utf8(data).map_err(|_| Error("COLLADA: файл не в UTF-8".into()))?;
    let opts = roxmltree::ParsingOptions { allow_dtd: false, nodes_limit: 50_000_000 };
    let doc = Document::parse_with_options(text, opts).map_err(|e| Error(format!("COLLADA: {e}")))?;
    let root = doc.root_element();
    if root.tag_name().name() != "COLLADA" {
        return err("это не файл COLLADA");
    }
    let mut by_id = HashMap::new();
    for n in root.descendants().filter(|n| n.is_element()) {
        if let Some(id) = n.attribute("id") {
            by_id.entry(id).or_insert(n);
        }
    }
    let mut cx = Ctx { by_id, files, images: HashMap::new(), meshes: HashMap::new(), pixels: 0, vertices: 0 };
    // ось «вверх» и единица
    let asset = kid(root, "asset");
    let meter = asset.and_then(|a| kid(a, "unit")).and_then(|u| u.attribute("meter")).and_then(|v| v.parse::<f32>().ok()).filter(|v| v.is_finite() && *v > 0.0).unwrap_or(1.0);
    let up = asset.and_then(|a| kid(a, "up_axis")).and_then(|u| u.text()).unwrap_or("Y_UP").trim().to_string();
    let axes = match up.as_str() {
        "Z_UP" => Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2),
        "X_UP" => Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2),
        _ => Mat4::IDENTITY,
    };
    let mut m = Model::default();
    let top = push_node(&mut m, "scene".into(), None, axes * Mat4::from_scale(Vec3::splat(meter)), None);
    m.roots = vec![top];
    let scene_url = kid(root, "scene").and_then(|s| kid(s, "instance_visual_scene")).and_then(|i| i.attribute("url"));
    let vs = scene_url.and_then(|u| cx.get(u)).or_else(|| root.descendants().find(|d| d.tag_name().name() == "visual_scene"));
    let Some(vs) = vs else { return err("COLLADA: нет сцены") };
    for n in kids(vs, "node") {
        walk(&mut cx, n, top, &mut m, 0)?;
    }
    Ok(m)
}

/// Имена картинок COLLADA (`<image><init_from>`) — чтобы взять их с диска рядом с моделью.
pub fn texture_files(data: &[u8]) -> Vec<String> {
    let Ok(text) = std::str::from_utf8(data) else { return Vec::new() };
    let Ok(doc) = Document::parse_with_options(text, roxmltree::ParsingOptions { allow_dtd: false, nodes_limit: 50_000_000 }) else { return Vec::new() };
    doc.descendants()
        .filter(|d| d.tag_name().name() == "image")
        .filter_map(|img| img.descendants().find(|d| d.tag_name().name() == "init_from"))
        .filter_map(|f| kid(f, "ref").and_then(|r| r.text()).or(f.text()).map(|s| s.trim().strip_prefix("file://").unwrap_or(s.trim()).replace('\\', "/")))
        .collect()
}
