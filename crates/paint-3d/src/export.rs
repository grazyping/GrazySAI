//! Запись модели: GLB (glTF 2.0 одним файлом — узлы, сетки, скелеты, морфы, текстуры) и OBJ с
//! MTL (только сетка в позе покоя и цвета с текстурами). Анимаций в `Model` нет — их нет и в выводе.

use crate::model::{err, Model, Res};
use glam::{Mat4, Vec3};
use serde_json::{json, Value};

/// Двоичный буфер GLB с видами (bufferViews) и описаниями данных (accessors).
#[derive(Default)]
struct Bin {
    data: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
}

impl Bin {
    /// Новый вид буфера с выравниванием на 4 байта; номер вида.
    fn view(&mut self, bytes: &[u8], target: Option<u32>) -> usize {
        while !self.data.len().is_multiple_of(4) {
            self.data.push(0);
        }
        let mut v = json!({ "buffer": 0, "byteOffset": self.data.len(), "byteLength": bytes.len() });
        if let Some(t) = target {
            v["target"] = json!(t);
        }
        self.data.extend_from_slice(bytes);
        self.views.push(v);
        self.views.len() - 1
    }

    fn accessor(&mut self, bytes: &[u8], target: Option<u32>, component: u32, count: usize, kind: &str, minmax: Option<(Vec3, Vec3)>) -> usize {
        let view = self.view(bytes, target);
        let mut a = json!({ "bufferView": view, "componentType": component, "count": count, "type": kind });
        if let Some((lo, hi)) = minmax {
            a["min"] = json!([lo.x, lo.y, lo.z]);
            a["max"] = json!([hi.x, hi.y, hi.z]);
        }
        self.accessors.push(a);
        self.accessors.len() - 1
    }

    fn vec3(&mut self, v: &[Vec3], bounds: bool) -> usize {
        let bytes: Vec<u8> = v.iter().flat_map(|p| p.to_array()).flat_map(f32::to_le_bytes).collect();
        let mm = bounds.then(|| v.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), p| (lo.min(*p), hi.max(*p))));
        self.accessor(&bytes, Some(ARRAY), FLOAT, v.len(), "VEC3", mm)
    }
}

const ARRAY: u32 = 34962;
const ELEMENT: u32 = 34963;
const FLOAT: u32 = 5126;
const USHORT: u32 = 5123;
const UINT: u32 = 5125;

/// Модель целиком в GLB.
pub fn glb(m: &Model) -> Res<Vec<u8>> {
    let mut bin = Bin::default();
    let nodes: Vec<Value> = m
        .nodes
        .iter()
        .map(|n| {
            let mut v = json!({ "name": n.name });
            if !n.children.is_empty() {
                v["children"] = json!(n.children);
            }
            if n.t != Vec3::ZERO {
                v["translation"] = json!(n.t.to_array());
            }
            if n.r != glam::Quat::IDENTITY {
                // порядок glTF: x, y, z, w — как у glam
                v["rotation"] = json!(n.r.to_array());
            }
            if n.s != Vec3::ONE {
                v["scale"] = json!(n.s.to_array());
            }
            if let Some(i) = n.mesh {
                v["mesh"] = json!(i);
            }
            if let Some(i) = n.skin {
                v["skin"] = json!(i);
            }
            v
        })
        .collect();
    let mut materials = Vec::new();
    let mut meshes = Vec::new();
    for mesh in &m.meshes {
        let mut prims = Vec::new();
        for p in &mesh.prims {
            let n = p.pos.len();
            let mut attr = json!({ "POSITION": bin.vec3(&p.pos, true) });
            if p.nrm.len() == n {
                attr["NORMAL"] = json!(bin.vec3(&p.nrm, false));
            }
            if p.uv.len() == n {
                let bytes: Vec<u8> = p.uv.iter().flat_map(|u| u.to_array()).flat_map(f32::to_le_bytes).collect();
                attr["TEXCOORD_0"] = json!(bin.accessor(&bytes, Some(ARRAY), FLOAT, n, "VEC2", None));
            }
            if p.joints.len() == n && p.weights.len() == n {
                let j: Vec<u8> = p.joints.iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
                attr["JOINTS_0"] = json!(bin.accessor(&j, Some(ARRAY), USHORT, n, "VEC4", None));
                let w: Vec<u8> = p.weights.iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
                attr["WEIGHTS_0"] = json!(bin.accessor(&w, Some(ARRAY), FLOAT, n, "VEC4", None));
            }
            let idx: Vec<u8> = p.idx.iter().flat_map(|v| v.to_le_bytes()).collect();
            let indices = bin.accessor(&idx, Some(ELEMENT), UINT, p.idx.len(), "SCALAR", None);
            let mut pbr = json!({ "baseColorFactor": p.color, "metallicFactor": 0.0, "roughnessFactor": 1.0 });
            if let Some(t) = p.tex.filter(|&t| t < m.images.len()) {
                pbr["baseColorTexture"] = json!({ "index": t });
            }
            let mut mat = json!({ "pbrMetallicRoughness": pbr, "doubleSided": p.double });
            if let Some(c) = p.cutoff {
                mat["alphaMode"] = json!("MASK");
                mat["alphaCutoff"] = json!(c);
            } else if p.color[3] < 1.0 {
                mat["alphaMode"] = json!("BLEND");
            }
            materials.push(mat);
            let mut prim = json!({ "attributes": attr, "indices": indices, "material": materials.len() - 1, "mode": 4 });
            let targets: Vec<Value> = p.morphs.iter().filter(|d| d.len() == n).map(|d| json!({ "POSITION": bin.vec3(d, true) })).collect();
            if !targets.is_empty() {
                prim["targets"] = json!(targets);
            }
            prims.push(prim);
        }
        let mut v = json!({ "name": mesh.name, "primitives": prims });
        if !mesh.morph_names.is_empty() {
            v["extras"] = json!({ "targetNames": mesh.morph_names });
        }
        if !mesh.weights.is_empty() && mesh.prims.iter().all(|p| p.morphs.len() == mesh.weights.len()) {
            v["weights"] = json!(mesh.weights);
        }
        meshes.push(v);
    }
    let mut skins = Vec::new();
    for s in &m.skins {
        let n = s.joints.len().min(s.inv_bind.len());
        let bytes: Vec<u8> = s.inv_bind[..n].iter().flat_map(|mx| mx.to_cols_array()).flat_map(f32::to_le_bytes).collect();
        let ib = bin.accessor(&bytes, None, FLOAT, n, "MAT4", None);
        skins.push(json!({ "joints": &s.joints[..n], "inverseBindMatrices": ib }));
    }
    let mut images = Vec::new();
    for im in &m.images {
        let png = paint_core::io::encode_png(im.w, im.h, &im.rgba).map_err(|e| crate::model::Error(e.to_string()))?;
        let view = bin.view(&png, None);
        images.push(json!({ "bufferView": view, "mimeType": "image/png" }));
    }
    let textures: Vec<Value> = (0..images.len()).map(|i| json!({ "source": i, "sampler": 0 })).collect();
    while !bin.data.len().is_multiple_of(4) {
        bin.data.push(0);
    }
    let mut doc = json!({
        "asset": { "version": "2.0", "generator": "GrazySAI" },
        "scene": 0,
        "scenes": [{ "nodes": m.roots }],
        "nodes": nodes,
        "meshes": meshes,
        "materials": materials,
        "accessors": bin.accessors,
        "bufferViews": bin.views,
        "buffers": [{ "byteLength": bin.data.len() }],
    });
    if !skins.is_empty() {
        doc["skins"] = json!(skins);
    }
    if !images.is_empty() {
        doc["images"] = json!(images);
        doc["textures"] = json!(textures);
        doc["samplers"] = json!([{}]);
    }
    let mut text = serde_json::to_vec(&doc).map_err(|e| crate::model::Error(e.to_string()))?;
    while !text.len().is_multiple_of(4) {
        text.push(b' ');
    }
    let total = 12 + 8 + text.len() + 8 + bin.data.len();
    let Ok(total32) = u32::try_from(total) else { return err("модель больше 4 ГБ — в GLB не помещается") };
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&total32.to_le_bytes());
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&text);
    out.extend_from_slice(&(bin.data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&bin.data);
    Ok(out)
}

/// OBJ, MTL и текстуры PNG (имя файла, байты).
pub type ObjFiles = (String, String, Vec<(String, Vec<u8>)>);

/// Положения узлов в покое относительно мира.
fn rest_globals(m: &Model) -> Vec<Mat4> {
    let mut g = vec![Mat4::IDENTITY; m.nodes.len()];
    let mut stack: Vec<(usize, Mat4)> = m.roots.iter().map(|&r| (r, Mat4::IDENTITY)).collect();
    let mut seen = vec![false; m.nodes.len()];
    while let Some((i, parent)) = stack.pop() {
        if i >= m.nodes.len() || seen[i] {
            continue;
        }
        seen[i] = true;
        let n = &m.nodes[i];
        g[i] = parent * Mat4::from_scale_rotation_translation(n.s, n.r, n.t);
        stack.extend(n.children.iter().map(|&c| (c, g[i])));
    }
    g
}

/// OBJ и MTL: сетка в позе покоя в мировых координатах (скелет и морфы не переносятся — в OBJ их
/// нет), материалы — цвет и текстура. `name` — имя без расширения: им называются MTL и текстуры.
/// Возвращает (OBJ, MTL, текстуры PNG с именами файлов).
pub fn obj(m: &Model, name: &str) -> Res<ObjFiles> {
    use std::fmt::Write;
    let globals = rest_globals(m);
    let skins: Vec<Vec<Mat4>> = m.skins.iter().map(|s| s.joints.iter().zip(&s.inv_bind).map(|(&j, ib)| globals.get(j).copied().unwrap_or(Mat4::IDENTITY) * *ib).collect()).collect();
    let (mut o, mut mtl) = (format!("# GrazySAI\nmtllib {name}.mtl\n"), String::from("# GrazySAI\n"));
    let mut textures = Vec::new();
    for (i, im) in m.images.iter().enumerate() {
        let png = paint_core::io::encode_png(im.w, im.h, &im.rgba).map_err(|e| crate::model::Error(e.to_string()))?;
        textures.push((format!("{name}_tex{i}.png"), png));
    }
    // номера в OBJ — с единицы, общие на весь файл
    let mut base = 1usize;
    let mut mat_n = 0usize;
    for (ni, node) in m.nodes.iter().enumerate() {
        let Some(mesh) = node.mesh.and_then(|i| m.meshes.get(i)) else { continue };
        let _ = writeln!(o, "o {}", if mesh.name.is_empty() { &node.name } else { &mesh.name });
        for p in &mesh.prims {
            let skin = node.skin.filter(|_| !p.joints.is_empty()).and_then(|s| skins.get(s));
            let at = |v: usize| -> Mat4 {
                let Some(sm) = skin else { return globals[ni] };
                let (j, w) = (p.joints[v], p.weights.get(v).copied().unwrap_or([0.0; 4]));
                let total: f32 = w.iter().sum();
                if total <= 1e-6 {
                    return globals[ni];
                }
                (0..4).filter(|&k| w[k] != 0.0).filter_map(|k| sm.get(j[k] as usize).map(|jm| *jm * (w[k] / total))).fold(Mat4::ZERO, |a, b| a + b)
            };
            let n = p.pos.len();
            let (has_uv, has_nrm) = (p.uv.len() == n, p.nrm.len() == n);
            for v in 0..n {
                let mx = at(v);
                let q = mx.transform_point3(p.pos[v]);
                let _ = writeln!(o, "v {} {} {}", q.x, q.y, q.z);
                if has_uv {
                    // в OBJ начало текстуры — внизу
                    let _ = writeln!(o, "vt {} {}", p.uv[v].x, 1.0 - p.uv[v].y);
                }
                if has_nrm {
                    let d = mx.transform_vector3(p.nrm[v]).normalize_or_zero();
                    let _ = writeln!(o, "vn {} {} {}", d.x, d.y, d.z);
                }
            }
            mat_n += 1;
            let _ = writeln!(mtl, "newmtl m{mat_n}\nKd {} {} {}\nd {}", p.color[0], p.color[1], p.color[2], p.color[3]);
            if let Some((file, _)) = p.tex.and_then(|t| textures.get(t)) {
                let _ = writeln!(mtl, "map_Kd {file}");
            }
            let _ = writeln!(o, "usemtl m{mat_n}");
            for t in p.idx.as_chunks::<3>().0 {
                if t.iter().any(|&i| i as usize >= n) {
                    continue;
                }
                let f = |i: u32| {
                    let k = base + i as usize;
                    match (has_uv, has_nrm) {
                        (true, true) => format!("{k}/{k}/{k}"),
                        (true, false) => format!("{k}/{k}"),
                        (false, true) => format!("{k}//{k}"),
                        (false, false) => k.to_string(),
                    }
                };
                let _ = writeln!(o, "f {} {} {}", f(t[0]), f(t[1]), f(t[2]));
            }
            base += n;
        }
    }
    Ok((o, mtl, textures))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Mesh, Node, Prim};
    use glam::{Quat, Vec2};

    fn tri() -> Model {
        let prim = Prim {
            pos: vec![Vec3::ZERO, Vec3::X, Vec3::Y],
            uv: vec![Vec2::ZERO, Vec2::X, Vec2::Y],
            idx: vec![0, 1, 2],
            morphs: vec![vec![Vec3::Z; 3]],
            color: [1.0, 0.5, 0.25, 1.0],
            ..Default::default()
        };
        Model {
            nodes: vec![Node { name: "tri".into(), parent: None, children: vec![], t: Vec3::new(0.0, 0.0, 2.0), r: Quat::IDENTITY, s: Vec3::ONE, mesh: Some(0), skin: None, joint: false }],
            roots: vec![0],
            meshes: vec![Mesh { name: "tri".into(), prims: vec![prim], morph_names: vec!["up".into()], weights: vec![0.0] }],
            skins: vec![],
            images: vec![],
            triangles: 1,
        }
    }

    #[test]
    fn glb_reads_back() {
        let bytes = glb(&tri()).unwrap();
        assert_eq!(bytes.len() % 4, 0);
        let back = crate::model::load(&[("t.glb".into(), bytes)]).unwrap();
        assert_eq!(back.triangles, 1);
        assert_eq!(back.meshes[0].prims[0].morphs.len(), 1);
        assert_eq!(back.nodes[0].t, Vec3::new(0.0, 0.0, 2.0));
    }

    #[test]
    fn obj_is_in_world_space() {
        let (o, mtl, _) = obj(&tri(), "t").unwrap();
        assert!(o.contains("v 1 0 2"), "{o}");
        assert!(o.contains("vt 0 0") && o.contains("vt 0 1"), "v перевёрнута");
        assert!(o.contains("f 1/1 2/2 3/3"));
        assert!(mtl.contains("Kd 1 0.5 0.25"));
        let back = crate::model::load(&[("t.obj".into(), o.into_bytes()), ("t.mtl".into(), mtl.into_bytes())]).unwrap();
        assert_eq!(back.triangles, 1);
    }
}
