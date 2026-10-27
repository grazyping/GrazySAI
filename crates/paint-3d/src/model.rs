//! Модель, прочитанная из glTF/GLB/VRM, OBJ, FBX, PMX или COLLADA: узлы, сетки, скелеты, морфы, текстуры.
//!
//! Файлы чужие и непроверенные: каждый индекс и размер проверяется, объёмы ограничены, ошибки —
//! сообщения, а не падение программы.

use glam::{Mat4, Quat, Vec2, Vec3};
use std::fmt;

/// Не больше стольких вершин, треугольников и точек текстур в одной модели.
pub const MAX_VERTICES: usize = 20_000_000;
pub const MAX_TRIANGLES: usize = 20_000_000;
pub const MAX_TEXTURE_PIXELS: u64 = 256 << 20;

#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub(crate) fn err<T>(s: impl Into<String>) -> Result<T, Error> {
    Err(Error(s.into()))
}

pub type Res<T> = Result<T, Error>;

#[derive(Clone, Debug)]
pub struct Node {
    pub name: String,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    /// Положение в покое: сдвиг, поворот, размер относительно родителя.
    pub t: Vec3,
    pub r: Quat,
    pub s: Vec3,
    pub mesh: Option<usize>,
    pub skin: Option<usize>,
    /// Узел — сустав какого-то скелета.
    pub joint: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Prim {
    pub pos: Vec<Vec3>,
    /// Пусто — нормали считаются по граням.
    pub nrm: Vec<Vec3>,
    pub uv: Vec<Vec2>,
    pub joints: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
    pub idx: Vec<u32>,
    /// Сдвиги вершин морфов (по одному списку на морф сетки).
    pub morphs: Vec<Vec<Vec3>>,
    /// Цвет материала RGBA 0..1 (без предумножения).
    pub color: [f32; 4],
    pub tex: Option<usize>,
    /// Прозрачное ниже порога не рисуется (`MASK`); `BLEND` рисуется как непрозрачное.
    pub cutoff: Option<f32>,
    /// Видны обе стороны треугольников (иначе задние не рисуются).
    pub double: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub name: String,
    pub prims: Vec<Prim>,
    pub morph_names: Vec<String>,
    pub weights: Vec<f32>,
}

#[derive(Clone, Debug)]
pub struct Skin {
    pub joints: Vec<usize>,
    pub inv_bind: Vec<Mat4>,
}

#[derive(Clone, Debug)]
pub struct Image {
    pub w: u32,
    pub h: u32,
    /// RGBA по 8 бит без предумножения.
    pub rgba: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct Model {
    pub nodes: Vec<Node>,
    pub roots: Vec<usize>,
    pub meshes: Vec<Mesh>,
    pub skins: Vec<Skin>,
    pub images: Vec<Image>,
    pub triangles: usize,
}

impl Model {
    /// Узлы-суставы по порядку (для списка суставов).
    pub fn joints(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.nodes.len()).filter(|&i| self.nodes[i].joint)
    }

    /// Найти файл по имени из списка (без путей; регистр не важен).
    pub(crate) fn file<'a>(files: &'a [(String, Vec<u8>)], uri: &str) -> Option<&'a [u8]> {
        let name = percent_decode(uri);
        let base = name.rsplit(['/', '\\']).next().unwrap_or(&name).to_lowercase();
        files.iter().find(|(n, _)| n.to_lowercase() == base).map(|(_, d)| d.as_slice())
    }
}

/// Разобрать модель: главный файл первым (`.glb`, `.gltf`, `.vrm`, `.obj`, `.fbx`, `.pmx`, `.dae`), рядом — его
/// файлы.
pub fn load(files: &[(String, Vec<u8>)]) -> Res<Model> {
    let Some((name, data)) = files.first() else { return err("нет файла модели") };
    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    let m = match ext.as_str() {
        "obj" => load_obj(data, files)?,
        "fbx" => crate::fbx::load(data, files)?,
        "pmx" => crate::pmx::load(data, files)?,
        "dae" => crate::collada::load(data, files)?,
        _ if data.starts_with(b"glTF") || ext == "gltf" || ext == "glb" || data.first() == Some(&b'{') => load_gltf(data, files)?,
        _ => return err(format!("неизвестный вид модели: {name}")),
    };
    if m.triangles == 0 {
        return err("в модели нет ни одного треугольника");
    }
    Ok(m)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() && s.is_char_boundary(i + 1) && s.is_char_boundary(i + 3) {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn base64(s: &str) -> Res<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        } as u32)
    };
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes() {
        if c == b'=' {
            break;
        }
        if c.is_ascii_whitespace() {
            continue;
        }
        let Some(v) = val(c) else { return err("испорченные данные base64") };
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Ok(out)
}

/// Данные по ссылке: `data:` внутри файла или соседний файл.
fn uri_data(uri: &str, files: &[(String, Vec<u8>)]) -> Res<Vec<u8>> {
    if let Some(rest) = uri.strip_prefix("data:") {
        let Some((_, b64)) = rest.split_once(";base64,") else { return err("ссылка data: без base64") };
        return base64(b64);
    }
    match Model::file(files, uri) {
        Some(d) => Ok(d.to_vec()),
        None => err(format!("нет файла «{uri}» рядом с моделью")),
    }
}

/// Значения доступа (accessor) как f64 по компонентам: `count × dims` чисел, с нормализацией и
/// разреженными заменами. Все смещения проверяются.
fn read_acc(acc: &gltf::Accessor, bufs: &[Vec<u8>]) -> Res<(Vec<f64>, usize)> {
    use gltf::accessor::DataType as D;
    let dims = acc.dimensions().multiplicity();
    let count = acc.count();
    if count > MAX_VERTICES * 3 {
        return err("слишком много значений в модели");
    }
    let (csize, norm) = (acc.data_type().size(), acc.normalized());
    let decode = |b: &[u8], t: D| -> f64 {
        match t {
            D::F32 => f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64,
            D::U32 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64,
            D::U16 => {
                let v = u16::from_le_bytes([b[0], b[1]]) as f64;
                if norm { v / 65535.0 } else { v }
            }
            D::I16 => {
                let v = i16::from_le_bytes([b[0], b[1]]) as f64;
                if norm { (v / 32767.0).max(-1.0) } else { v }
            }
            D::U8 => {
                let v = b[0] as f64;
                if norm { v / 255.0 } else { v }
            }
            D::I8 => {
                let v = b[0] as i8 as f64;
                if norm { (v / 127.0).max(-1.0) } else { v }
            }
        }
    };
    let elem = csize * dims;
    let mut out = vec![0f64; count * dims];
    if let Some(view) = acc.view() {
        let buf = bufs.get(view.buffer().index()).ok_or_else(|| Error("нет буфера модели".into()))?;
        let stride = view.stride().unwrap_or(elem).max(elem);
        let start = view.offset() + acc.offset();
        let end_view = view.offset().checked_add(view.length()).ok_or_else(|| Error("испорченный вид буфера".into()))?;
        if count > 0 {
            let last = start + stride * (count - 1) + elem;
            if last > end_view || end_view > buf.len() {
                return err("данные модели выходят за буфер");
            }
        }
        for i in 0..count {
            let o = start + i * stride;
            for c in 0..dims {
                out[i * dims + c] = decode(&buf[o + c * csize..], acc.data_type());
            }
        }
    }
    if let Some(sp) = acc.sparse() {
        let n = sp.count();
        let (iv, vv) = (sp.indices(), sp.values());
        let ib = bufs.get(iv.view().buffer().index()).ok_or_else(|| Error("нет буфера модели".into()))?;
        let vb = bufs.get(vv.view().buffer().index()).ok_or_else(|| Error("нет буфера модели".into()))?;
        let it = match iv.index_type() {
            gltf::accessor::sparse::IndexType::U8 => D::U8,
            gltf::accessor::sparse::IndexType::U16 => D::U16,
            gltf::accessor::sparse::IndexType::U32 => D::U32,
        };
        let isz = it.size();
        let i0 = iv.view().offset() + iv.offset();
        let v0 = vv.view().offset() + vv.offset();
        if i0 + n * isz > ib.len() || v0 + n * elem > vb.len() {
            return err("разреженные данные модели выходят за буфер");
        }
        for k in 0..n {
            let b = &ib[i0 + k * isz..];
            let idx = match it {
                D::U8 => b[0] as usize,
                D::U16 => u16::from_le_bytes([b[0], b[1]]) as usize,
                _ => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize,
            };
            if idx >= count {
                return err("разреженная замена вне данных");
            }
            for c in 0..dims {
                out[idx * dims + c] = decode(&vb[v0 + k * elem + c * csize..], acc.data_type());
            }
        }
    }
    Ok((out, dims))
}

fn vec3s(acc: &gltf::Accessor, bufs: &[Vec<u8>]) -> Res<Vec<Vec3>> {
    let (v, d) = read_acc(acc, bufs)?;
    if d != 3 {
        return err("ожидались тройки чисел");
    }
    Ok(v.as_chunks::<3>().0.iter().map(|c| Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32)).collect())
}

fn load_gltf(data: &[u8], files: &[(String, Vec<u8>)]) -> Res<Model> {
    let g = gltf::Gltf::from_slice_without_validation(data).map_err(|e| Error(format!("glTF: {e}")))?;
    let doc = &g.document;
    for ext in doc.extensions_required() {
        if ext == "KHR_draco_mesh_compression" || ext == "EXT_meshopt_compression" || ext == "KHR_meshopt_compression" {
            return err(format!("сжатие сеток {ext} не поддерживается — сохраните модель без сжатия (в Blender: экспорт glTF без Draco)"));
        }
    }
    let mut bufs = Vec::new();
    for b in doc.buffers() {
        let d = match b.source() {
            gltf::buffer::Source::Bin => g.blob.clone().ok_or_else(|| Error("в GLB нет двоичной части".into()))?,
            gltf::buffer::Source::Uri(u) => uri_data(u, files)?,
        };
        if d.len() < b.length() {
            return err("буфер модели короче заявленного");
        }
        bufs.push(d);
    }
    let mut m = Model::default();
    // картинки
    let mut pixels = 0u64;
    for img in doc.images() {
        let bytes = match img.source() {
            gltf::image::Source::View { view, .. } => {
                let b = bufs.get(view.buffer().index()).ok_or_else(|| Error("нет буфера картинки".into()))?;
                b.get(view.offset()..view.offset() + view.length()).ok_or_else(|| Error("картинка выходит за буфер".into()))?.to_vec()
            }
            gltf::image::Source::Uri { uri, .. } => uri_data(uri, files)?,
        };
        // картинку, которую не прочитать, заменяем серым квадратиком: модель всё равно видна
        let im = match paint_core::io::decode_image(&bytes) {
            Ok((w, h, rgba)) if w > 0 && h > 0 => Image { w, h, rgba },
            _ => Image { w: 1, h: 1, rgba: vec![200, 200, 200, 255] },
        };
        pixels += im.w as u64 * im.h as u64;
        if pixels > MAX_TEXTURE_PIXELS {
            return err("слишком большие текстуры");
        }
        m.images.push(im);
    }
    // узлы и иерархия
    let n_nodes = doc.nodes().count();
    for node in doc.nodes() {
        let (t, r, s) = node.transform().decomposed();
        m.nodes.push(Node {
            name: node.name().map_or_else(|| format!("node{}", node.index()), str::to_string),
            parent: None,
            children: node.children().map(|c| c.index()).filter(|&c| c < n_nodes).collect(),
            t: Vec3::from(t),
            r: Quat::from_array(r).normalize(),
            s: Vec3::from(s),
            mesh: node.mesh().map(|x| x.index()),
            skin: node.skin().map(|x| x.index()),
            joint: false,
        });
    }
    for i in 0..m.nodes.len() {
        for c in m.nodes[i].children.clone() {
            if m.nodes[c].parent.is_some() || c == i {
                return err("в модели узел с двумя родителями");
            }
            m.nodes[c].parent = Some(i);
        }
    }
    // корни — без родителей; круги отсеиваются тем, что из корней обходим один раз
    m.roots = (0..m.nodes.len()).filter(|&i| m.nodes[i].parent.is_none()).collect();
    let mut seen = vec![false; m.nodes.len()];
    let mut stack = m.roots.clone();
    while let Some(i) = stack.pop() {
        if std::mem::replace(&mut seen[i], true) {
            return err("в модели круг из узлов");
        }
        stack.extend(&m.nodes[i].children);
    }
    if seen.iter().any(|s| !s) {
        return err("в модели круг из узлов");
    }
    // скелеты
    for skin in doc.skins() {
        let joints: Vec<usize> = skin.joints().map(|j| j.index()).collect();
        let inv = match skin.inverse_bind_matrices() {
            Some(a) => {
                let (v, d) = read_acc(&a, &bufs)?;
                if d != 16 || v.len() / 16 < joints.len() {
                    return err("испорченные матрицы скелета");
                }
                v.as_chunks::<16>().0.iter().map(|c| Mat4::from_cols_array(&std::array::from_fn(|k| c[k] as f32))).collect()
            }
            None => vec![Mat4::IDENTITY; joints.len()],
        };
        for &j in &joints {
            m.nodes[j].joint = true;
        }
        m.skins.push(Skin { joints, inv_bind: inv });
    }
    // сетки
    for mesh in doc.meshes() {
        let morph_names = mesh
            .extras()
            .as_ref()
            .and_then(|r| serde_json::from_str::<serde_json::Value>(r.get()).ok())
            .and_then(|v| v.get("targetNames").and_then(|t| t.as_array()).map(|a| a.iter().map(|x| x.as_str().unwrap_or("").to_string()).collect()))
            .unwrap_or_default();
        let mut out = Mesh { name: mesh.name().unwrap_or("mesh").to_string(), morph_names, weights: mesh.weights().map(<[f32]>::to_vec).unwrap_or_default(), prims: Vec::new() };
        for p in mesh.primitives() {
            use gltf::mesh::Mode;
            let Some(pa) = p.get(&gltf::Semantic::Positions) else { continue };
            let pos = vec3s(&pa, &bufs)?;
            let n = pos.len();
            if n > MAX_VERTICES {
                return err("слишком много вершин");
            }
            let raw_idx: Vec<u32> = match p.indices() {
                Some(a) => read_acc(&a, &bufs)?.0.iter().map(|&v| v as u32).collect(),
                None => (0..n as u32).collect(),
            };
            if raw_idx.iter().any(|&i| i as usize >= n) {
                return err("номер вершины вне сетки");
            }
            let idx: Vec<u32> = match p.mode() {
                Mode::Triangles => raw_idx[..raw_idx.len() / 3 * 3].to_vec(),
                Mode::TriangleStrip => (2..raw_idx.len()).flat_map(|k| if k % 2 == 0 { [raw_idx[k - 2], raw_idx[k - 1], raw_idx[k]] } else { [raw_idx[k - 1], raw_idx[k - 2], raw_idx[k]] }).collect(),
                Mode::TriangleFan => (2..raw_idx.len()).flat_map(|k| [raw_idx[0], raw_idx[k - 1], raw_idx[k]]).collect(),
                _ => continue,
            };
            m.triangles += idx.len() / 3;
            if m.triangles > MAX_TRIANGLES {
                return err("слишком много треугольников (больше 20 млн)");
            }
            let nrm = match p.get(&gltf::Semantic::Normals) {
                Some(a) => vec3s(&a, &bufs)?,
                None => Vec::new(),
            };
            let uv = match p.get(&gltf::Semantic::TexCoords(0)) {
                Some(a) => {
                    let (v, d) = read_acc(&a, &bufs)?;
                    if d != 2 {
                        return err("испорченная развёртка");
                    }
                    v.as_chunks::<2>().0.iter().map(|c| Vec2::new(c[0] as f32, c[1] as f32)).collect()
                }
                None => Vec::new(),
            };
            let (mut joints, mut weights) = (Vec::new(), Vec::new());
            if let (Some(ja), Some(wa)) = (p.get(&gltf::Semantic::Joints(0)), p.get(&gltf::Semantic::Weights(0))) {
                let (j, dj) = read_acc(&ja, &bufs)?;
                let (w, dw) = read_acc(&wa, &bufs)?;
                if dj != 4 || dw != 4 {
                    return err("испорченная привязка к скелету");
                }
                joints = j.as_chunks::<4>().0.iter().map(|c| std::array::from_fn(|k| c[k].clamp(0.0, 65535.0) as u16)).collect();
                weights = w.as_chunks::<4>().0.iter().map(|c| std::array::from_fn(|k| c[k] as f32)).collect();
            }
            let mut morphs = Vec::new();
            for t in p.morph_targets() {
                morphs.push(match t.positions() {
                    Some(a) => vec3s(&a, &bufs)?,
                    None => vec![Vec3::ZERO; n],
                });
            }
            if nrm.len() != n && !nrm.is_empty() || uv.len() != n && !uv.is_empty() || joints.len() != n && !joints.is_empty() || weights.len() != joints.len() || morphs.iter().any(|v| v.len() != n) {
                return err("у вершин разное число данных");
            }
            let mat = p.material();
            let pbr = mat.pbr_metallic_roughness();
            let tex = pbr.base_color_texture().map(|t| t.texture().source().index()).filter(|&i| i < m.images.len());
            let cutoff = match mat.alpha_mode() {
                gltf::material::AlphaMode::Mask => Some(mat.alpha_cutoff().unwrap_or(0.5)),
                _ => None,
            };
            out.prims.push(Prim { pos, nrm, uv, joints, weights, idx, morphs, color: pbr.base_color_factor(), tex, cutoff, double: mat.double_sided() });
        }
        m.meshes.push(out);
    }
    // ссылки узлов на сетки и скелеты, суставы скелетов
    for node in &mut m.nodes {
        node.mesh = node.mesh.filter(|&i| i < m.meshes.len());
        node.skin = node.skin.filter(|&i| i < m.skins.len());
    }
    for (si, s) in m.skins.iter().enumerate() {
        if s.joints.iter().any(|&j| j >= m.nodes.len()) {
            return err(format!("скелет {si}: сустав вне модели"));
        }
    }
    for node in &m.nodes {
        if let (Some(mi), Some(si)) = (node.mesh, node.skin) {
            let nj = m.skins[si].joints.len();
            if m.meshes[mi].prims.iter().any(|p| p.joints.iter().flatten().any(|&j| j as usize >= nj)) {
                return err("вершина привязана к несуществующему суставу");
            }
        }
    }
    Ok(m)
}

fn load_obj(data: &[u8], files: &[(String, Vec<u8>)]) -> Res<Model> {
    let opts = tobj::LoadOptions { single_index: true, triangulate: true, ..Default::default() };
    let (models, mats) = tobj::load_obj_buf(&mut std::io::BufReader::new(data), &opts, |p| {
        let name = p.to_string_lossy();
        match Model::file(files, &name) {
            Some(d) => tobj::load_mtl_buf(&mut std::io::BufReader::new(d)),
            None => Err(tobj::LoadError::OpenFileFailed),
        }
    })
    .map_err(|e| Error(format!("OBJ: {e}")))?;
    let mats = mats.unwrap_or_default();
    let mut m = Model::default();
    let mut tex_of: Vec<(String, usize)> = Vec::new();
    for (k, om) in models.into_iter().enumerate() {
        let me = &om.mesh;
        let n = me.positions.len() / 3;
        if n > MAX_VERTICES || me.indices.iter().any(|&i| i as usize >= n) {
            return err("OBJ: испорченная сетка");
        }
        m.triangles += me.indices.len() / 3;
        if m.triangles > MAX_TRIANGLES {
            return err("слишком много треугольников (больше 20 млн)");
        }
        let pos = me.positions.as_chunks::<3>().0.iter().map(|c| Vec3::new(c[0], c[1], c[2])).collect();
        let nrm: Vec<Vec3> = me.normals.as_chunks::<3>().0.iter().map(|c| Vec3::new(c[0], c[1], c[2])).collect();
        // в OBJ развёртка снизу вверх, в glTF — сверху вниз
        let uv: Vec<Vec2> = me.texcoords.as_chunks::<2>().0.iter().map(|c| Vec2::new(c[0], 1.0 - c[1])).collect();
        let mat = me.material_id.and_then(|i| mats.get(i));
        let d = mat.and_then(|x| x.diffuse).unwrap_or([0.8, 0.8, 0.8]);
        let mut tex = None;
        if let Some(t) = mat.and_then(|x| x.diffuse_texture.clone()) {
            tex = match tex_of.iter().find(|(n, _)| *n == t) {
                Some((_, i)) => Some(*i),
                None => Model::file(files, &t).and_then(|b| paint_core::io::decode_image(b).ok()).map(|(w, h, rgba)| {
                    m.images.push(Image { w, h, rgba });
                    tex_of.push((t.clone(), m.images.len() - 1));
                    m.images.len() - 1
                }),
            };
        }
        let prim = Prim { nrm: if nrm.len() == n { nrm } else { Vec::new() }, uv: if uv.len() == n { uv } else { Vec::new() }, pos, idx: me.indices.clone(), color: [d[0], d[1], d[2], 1.0], tex, double: true, ..Default::default() };
        m.meshes.push(Mesh { name: om.name.clone(), prims: vec![prim], ..Default::default() });
        m.nodes.push(Node { name: if om.name.is_empty() { format!("part{k}") } else { om.name }, parent: None, children: Vec::new(), t: Vec3::ZERO, r: Quat::IDENTITY, s: Vec3::ONE, mesh: Some(k), skin: None, joint: false });
        m.roots.push(k);
    }
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<(String, Vec<u8>)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
        let mut v = vec![(name.to_string(), std::fs::read(dir.join(name)).unwrap())];
        if name.ends_with(".obj") {
            v.push(("figure.mtl".into(), std::fs::read(dir.join("figure.mtl")).unwrap()));
        }
        v
    }

    #[test]
    fn figure_has_skeleton_fingers_and_morph() {
        let m = load(&fixture("figure.glb")).unwrap();
        assert_eq!(m.skins.len(), 1);
        assert_eq!(m.skins[0].joints.len(), 46);
        assert!(m.nodes.iter().any(|n| n.name == "middle.3.L" && n.joint));
        let body = m.meshes.iter().find(|x| x.name == "Body").unwrap();
        assert_eq!(body.morph_names, ["Belly"]);
        assert_eq!(m.images.len(), 1);
        assert!(m.triangles > 1000);
    }

    #[test]
    fn scene_and_obj() {
        let s = load(&fixture("scene.glb")).unwrap();
        let street = s.nodes.iter().position(|n| n.name == "Street").unwrap();
        assert_eq!(s.nodes[street].children.len(), 4);
        let o = load(&fixture("figure.obj")).unwrap();
        assert!(o.triangles > 1000 && o.skins.is_empty());
    }

    /// Сжатые координаты (`KHR_mesh_quantization`): u16 с нормализацией, индексы u8, данные по data:.
    #[test]
    fn quantized_positions() {
        // три вершины u16 (с выравниванием по 8 байт) и индексы u8
        let mut buf = Vec::new();
        for v in [[0u16, 0, 0], [65535, 0, 0], [0, 65535, 0]] {
            for c in v {
                buf.extend(c.to_le_bytes());
            }
            buf.extend([0, 0]);
        }
        buf.extend([0u8, 1, 2, 0]);
        let enc = |d: &[u8]| -> String {
            const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let mut o = String::new();
            for c in d.chunks(3) {
                let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
                for k in 0..4 {
                    o.push(if k <= c.len() { T[(n >> (18 - 6 * k) & 63) as usize] as char } else { '=' });
                }
            }
            o
        };
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"extensionsUsed":["KHR_mesh_quantization"],"extensionsRequired":["KHR_mesh_quantization"],
            "buffers":[{{"byteLength":{},"uri":"data:application/octet-stream;base64,{}"}}],
            "bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":24,"byteStride":8}},{{"buffer":0,"byteOffset":24,"byteLength":3}}],
            "accessors":[{{"bufferView":0,"componentType":5123,"normalized":true,"count":3,"type":"VEC3","min":[0,0,0],"max":[65535,65535,0]}},
                         {{"bufferView":1,"componentType":5121,"count":3,"type":"SCALAR"}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1}}]}}],
            "nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#,
            buf.len(),
            enc(&buf)
        );
        let m = load(&[("q.gltf".into(), json.into_bytes())]).unwrap();
        let p = &m.meshes[0].prims[0];
        assert_eq!(p.idx, [0, 1, 2]);
        assert!((p.pos[1].x - 1.0).abs() < 1e-6 && (p.pos[2].y - 1.0).abs() < 1e-6);
    }

    /// Обрезанный и испорченный файл — ошибка, а не падение.
    #[test]
    fn broken_files_are_errors() {
        let full = fixture("figure.glb")[0].1.clone();
        for cut in [0, 12, 20, 100, full.len() / 2, full.len() - 1] {
            assert!(load(&[("x.glb".into(), full[..cut].to_vec())]).is_err(), "обрезка {cut}");
        }
        let mut bad = full;
        for i in (200..bad.len()).step_by(97) {
            bad[i] ^= 0x5a;
        }
        let _ = load(&[("x.glb".into(), bad)]);
        assert!(load(&[("x.obj".into(), b"v 1 2\nf 1 2 3 9\n".to_vec())]).is_err());
    }
}
