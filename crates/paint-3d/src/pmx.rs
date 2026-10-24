//! PMX (MikuMikuDance, PMX Editor; версии 2.0 и 2.1): вершины с весами костей, материалы с
//! текстурами, кости, морфы вершин. Физика, рамки показа и прочее не читаются.
//! MMD — левая система, Y вверх, единица ≈ 8 см: приводим к glTF (правая, метры).

use crate::model::{err, Error, Image, Mesh, Model, Node, Prim, Res, Skin, MAX_TEXTURE_PIXELS, MAX_TRIANGLES, MAX_VERTICES};
use glam::{Mat4, Quat, Vec2, Vec3};
use std::collections::HashMap;

/// Метров в единице MMD.
const UNIT: f32 = 0.08;

struct R<'a> {
    d: &'a [u8],
    p: usize,
    utf8: bool,
}

impl R<'_> {
    fn take(&mut self, n: usize) -> Res<&[u8]> {
        let end = self.p.checked_add(n).filter(|&e| e <= self.d.len()).ok_or_else(|| Error("PMX: файл обрывается".into()))?;
        let s = &self.d[self.p..end];
        self.p = end;
        Ok(s)
    }
    fn skip(&mut self, n: usize) -> Res<()> {
        self.take(n).map(|_| ())
    }
    fn u8(&mut self) -> Res<u8> {
        Ok(self.take(1)?[0])
    }
    fn i32(&mut self) -> Res<i32> {
        let b = self.take(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn f32(&mut self) -> Res<f32> {
        let b = self.take(4)?;
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn v3(&mut self) -> Res<Vec3> {
        Ok(Vec3::new(self.f32()?, self.f32()?, self.f32()?))
    }
    /// Число элементов: не больше, чем уместится в остатке файла по `min` байт на элемент.
    fn count(&mut self, min: usize) -> Res<usize> {
        let n = self.i32()?;
        if n < 0 || n as usize > (self.d.len() - self.p) / min.max(1) {
            return err("PMX: испорченное число элементов");
        }
        Ok(n as usize)
    }
    fn text(&mut self) -> Res<String> {
        let n = self.i32()?;
        if n < 0 {
            return err("PMX: испорченная строка");
        }
        let utf8 = self.utf8;
        let b = self.take(n as usize)?;
        Ok(if utf8 {
            String::from_utf8_lossy(b).into_owned()
        } else {
            String::from_utf16_lossy(&b.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect::<Vec<_>>())
        })
    }
    /// Номер размера `size` байт: вершины — без знака, остальное — со знаком (−1 — нет).
    fn index(&mut self, size: u8, unsigned: bool) -> Res<i64> {
        Ok(match (size, unsigned) {
            (1, true) => self.u8()? as i64,
            (2, true) => {
                let b = self.take(2)?;
                u16::from_le_bytes([b[0], b[1]]) as i64
            }
            (1, false) => self.u8()? as i8 as i64,
            (2, false) => {
                let b = self.take(2)?;
                i16::from_le_bytes([b[0], b[1]]) as i64
            }
            (4, _) => self.i32()? as i64,
            _ => return err("PMX: неизвестный размер номера"),
        })
    }
}

/// Перевод из MMD: зеркало по Z и метры.
fn conv(v: Vec3) -> Vec3 {
    Vec3::new(v.x, v.y, -v.z) * UNIT
}

struct Head {
    add_uv: usize,
    vi: u8,
    ti: u8,
    mi: u8,
    bi: u8,
    morph: u8,
}

fn header<'a>(d: &'a [u8]) -> Res<(R<'a>, Head)> {
    if !d.starts_with(b"PMX ") {
        return err("PMX: это не файл PMX (PMD — старый вид — не читается, пересохраните в PMX Editor)");
    }
    let mut r = R { d, p: 4, utf8: false };
    let _version = r.f32()?;
    let n = r.u8()? as usize;
    let g = r.take(n)?.to_vec();
    if g.len() < 8 {
        return err("PMX: испорченный заголовок");
    }
    r.utf8 = g[0] == 1;
    let h = Head { add_uv: (g[1] as usize).min(4), vi: g[2], ti: g[3], mi: g[4], bi: g[5], morph: g[6] };
    for _ in 0..4 {
        r.text()?; // имена и комментарии
    }
    Ok((r, h))
}

struct Vert {
    pos: Vec3,
    nrm: Vec3,
    uv: Vec2,
    joints: [i64; 4],
    weights: [f32; 4],
}

/// Пути текстур PMX (как записаны в файле, разделитель — `/`).
pub fn texture_files(data: &[u8]) -> Vec<String> {
    let read = || -> Res<Vec<String>> {
        let (mut r, h) = header(data)?;
        let nv = r.count(33)?;
        for _ in 0..nv {
            vertex(&mut r, &h)?;
        }
        let nf = r.count(h.vi as usize)?;
        r.skip(nf * h.vi as usize)?;
        let nt = r.count(4)?;
        (0..nt).map(|_| r.text().map(|s| s.replace('\\', "/"))).collect()
    };
    read().unwrap_or_default()
}

fn vertex(r: &mut R, h: &Head) -> Res<Vert> {
    let pos = r.v3()?;
    let nrm = r.v3()?;
    let uv = Vec2::new(r.f32()?, r.f32()?);
    r.skip(h.add_uv * 16)?;
    let bi = h.bi;
    let (joints, weights) = match r.u8()? {
        0 => ([r.index(bi, false)?, -1, -1, -1], [1.0, 0.0, 0.0, 0.0]),
        1 => {
            let (a, b) = (r.index(bi, false)?, r.index(bi, false)?);
            let w = r.f32()?;
            ([a, b, -1, -1], [w, 1.0 - w, 0.0, 0.0])
        }
        2 | 4 => {
            let j = [r.index(bi, false)?, r.index(bi, false)?, r.index(bi, false)?, r.index(bi, false)?];
            (j, [r.f32()?, r.f32()?, r.f32()?, r.f32()?])
        }
        3 => {
            // SDEF: как BDEF2 (сферическое смешивание не повторяем)
            let (a, b) = (r.index(bi, false)?, r.index(bi, false)?);
            let w = r.f32()?;
            r.skip(36)?;
            ([a, b, -1, -1], [w, 1.0 - w, 0.0, 0.0])
        }
        _ => return err("PMX: неизвестный вид привязки вершины"),
    };
    r.skip(4)?; // толщина обводки
    Ok(Vert { pos, nrm, uv, joints, weights })
}

pub fn load(data: &[u8], files: &[(String, Vec<u8>)]) -> Res<Model> {
    let (mut r, h) = header(data)?;
    let nv = r.count(33)?;
    if nv > MAX_VERTICES {
        return err("слишком много вершин");
    }
    let verts: Vec<Vert> = (0..nv).map(|_| vertex(&mut r, &h)).collect::<Res<_>>()?;
    let nf = r.count(h.vi as usize)?;
    if nf / 3 > MAX_TRIANGLES {
        return err("слишком много треугольников (больше 20 млн)");
    }
    let mut faces = Vec::with_capacity(nf);
    for _ in 0..nf {
        let i = r.index(h.vi, true)?;
        if i < 0 || i as usize >= nv {
            return err("PMX: номер вершины вне сетки");
        }
        faces.push(i as u32);
    }
    let nt = r.count(4)?;
    let tex_paths: Vec<String> = (0..nt).map(|_| r.text()).collect::<Res<_>>()?;
    // материалы: цвет, текстура, сколько номеров граней
    struct Mat {
        color: [f32; 4],
        tex: i64,
        double: bool,
        count: usize,
    }
    let nm = r.count(4)?;
    let mut mats = Vec::with_capacity(nm);
    for _ in 0..nm {
        r.text()?;
        r.text()?;
        let color = [r.f32()?, r.f32()?, r.f32()?, r.f32()?];
        r.skip(12 + 4 + 12)?; // блик, сила блика, окружение
        let flags = r.u8()?;
        r.skip(16 + 4)?; // цвет и толщина обводки
        let tex = r.index(h.ti, false)?;
        r.index(h.ti, false)?; // сферическая текстура
        r.u8()?;
        if r.u8()? == 0 {
            r.index(h.ti, false)?;
        } else {
            r.u8()?;
        }
        r.text()?;
        let count = r.i32()?.max(0) as usize; // номеров граней у материала (сами — выше)
        mats.push(Mat { color, tex, double: flags & 1 != 0, count });
    }
    // кости: положения в модели, родители
    let nb = r.count(4)?;
    let mut bones: Vec<(String, Vec3, i64)> = Vec::with_capacity(nb);
    for _ in 0..nb {
        let name = r.text()?;
        let name_en = r.text()?;
        let pos = r.v3()?;
        let parent = r.index(h.bi, false)?;
        r.i32()?; // слой
        let b = r.take(2)?;
        let flags = u16::from_le_bytes([b[0], b[1]]);
        if flags & 0x0001 != 0 {
            r.index(h.bi, false)?;
        } else {
            r.skip(12)?;
        }
        if flags & 0x0300 != 0 {
            r.index(h.bi, false)?;
            r.skip(4)?;
        }
        if flags & 0x0400 != 0 {
            r.skip(12)?;
        }
        if flags & 0x0800 != 0 {
            r.skip(24)?;
        }
        if flags & 0x2000 != 0 {
            r.skip(4)?;
        }
        if flags & 0x0020 != 0 {
            r.index(h.bi, false)?;
            r.skip(8)?;
            let links = r.count(1)?;
            for _ in 0..links {
                r.index(h.bi, false)?;
                if r.u8()? != 0 {
                    r.skip(24)?;
                }
            }
        }
        // английское имя — если японское пустое
        bones.push((if name.is_empty() { name_en } else { name }, pos, parent));
    }
    // морфы вершин (остальные виды пропускаются)
    let mut morphs: Vec<(String, HashMap<u32, Vec3>)> = Vec::new();
    let nmorph = r.count(4)?;
    for _ in 0..nmorph {
        let name = r.text()?;
        r.text()?;
        r.u8()?;
        let kind = r.u8()?;
        let n = r.count(1)?;
        let mut offs = HashMap::new();
        for _ in 0..n {
            match kind {
                0 | 9 => {
                    r.index(h.morph, false)?;
                    r.skip(4)?;
                }
                1 => {
                    let v = r.index(h.vi, true)?;
                    let d = r.v3()?;
                    if v >= 0 && (v as usize) < nv {
                        offs.insert(v as u32, Vec3::new(d.x, d.y, -d.z) * UNIT);
                    }
                }
                2 => {
                    r.index(h.bi, false)?;
                    r.skip(28)?;
                }
                3..=7 => {
                    r.index(h.vi, true)?;
                    r.skip(16)?;
                }
                8 => {
                    r.index(h.mi, false)?;
                    r.skip(1 + 16 + 12 + 4 + 12 + 16 + 4 + 16 * 3)?;
                }
                10 => {
                    r.index(4, false).ok();
                    r.skip(1 + 24)?;
                }
                _ => return err("PMX: неизвестный вид морфа"),
            }
        }
        if kind == 1 && !offs.is_empty() {
            morphs.push((name, offs));
        }
    }

    let mut m = Model::default();
    // узлы: кости, затем узел сетки
    let nb_ok = |i: i64| (i >= 0 && (i as usize) < nb).then_some(i as usize);
    for (i, (name, pos, parent)) in bones.iter().enumerate() {
        let parent = nb_ok(*parent).filter(|&p| p != i);
        let base = parent.map_or(Vec3::ZERO, |p| conv(bones[p].1));
        m.nodes.push(Node { name: name.clone(), parent, children: Vec::new(), t: conv(*pos) - base, r: Quat::IDENTITY, s: Vec3::ONE, mesh: None, skin: None, joint: true });
    }
    // круги из костей разрываем: такой кости — без родителя
    for i in 0..nb {
        let mut j = i;
        for _ in 0..=nb {
            match m.nodes[j].parent {
                Some(p) if p == i => {
                    m.nodes[i].parent = None;
                    m.nodes[i].t = conv(bones[i].1);
                    break;
                }
                Some(p) => j = p,
                None => break,
            }
        }
    }
    for i in 0..nb {
        if let Some(p) = m.nodes[i].parent {
            m.nodes[p].children.push(i);
        }
    }
    let skin = (nb > 0).then(|| {
        m.skins.push(Skin { joints: (0..nb).collect(), inv_bind: bones.iter().map(|b| Mat4::from_translation(-conv(b.1))).collect() });
        0
    });
    m.nodes.push(Node { name: "model".into(), parent: None, children: Vec::new(), t: Vec3::ZERO, r: Quat::IDENTITY, s: Vec3::ONE, mesh: Some(0), skin, joint: false });
    m.roots = (0..m.nodes.len()).filter(|&i| m.nodes[i].parent.is_none()).collect();

    // текстуры (одна картинка на путь)
    let mut pixels = 0u64;
    let mut images: HashMap<usize, Option<usize>> = HashMap::new();
    // сетка: часть на материал, вершины — свои у части
    let mut mesh = Mesh { name: "model".into(), morph_names: morphs.iter().map(|x| x.0.clone()).collect(), weights: vec![0.0; morphs.len()], prims: Vec::new() };
    let mut at = 0usize;
    for mat in &mats {
        let end = at.saturating_add(mat.count).min(faces.len());
        let idx = &faces[at..at + (end - at) / 3 * 3];
        at = end;
        let mut prim = Prim { color: mat.color, double: mat.double, ..Default::default() };
        let mut map: HashMap<u32, u32> = HashMap::new();
        let mut orig = Vec::new();
        // MMD — левая система: после зеркала по Z обход треугольника меняется
        for t in idx.as_chunks::<3>().0 {
            for &v in [t[0], t[2], t[1]].iter() {
                let k = *map.entry(v).or_insert_with(|| {
                    let x = &verts[v as usize];
                    prim.pos.push(conv(x.pos));
                    prim.nrm.push(Vec3::new(x.nrm.x, x.nrm.y, -x.nrm.z));
                    prim.uv.push(x.uv);
                    if skin.is_some() {
                        let mut j = [0u16; 4];
                        let mut w = [0f32; 4];
                        for k in 0..4 {
                            if let Some(b) = nb_ok(x.joints[k]) {
                                j[k] = b as u16;
                                w[k] = x.weights[k].max(0.0);
                            }
                        }
                        prim.joints.push(j);
                        prim.weights.push(w);
                    }
                    orig.push(v);
                    orig.len() as u32 - 1
                });
                prim.idx.push(k);
            }
        }
        if prim.idx.is_empty() {
            continue;
        }
        m.triangles += prim.idx.len() / 3;
        for (_, offs) in &morphs {
            prim.morphs.push(orig.iter().map(|v| offs.get(v).copied().unwrap_or(Vec3::ZERO)).collect());
        }
        if mat.tex >= 0 && (mat.tex as usize) < tex_paths.len() {
            let ti = mat.tex as usize;
            prim.tex = match images.get(&ti) {
                Some(&i) => i,
                None => {
                    let i = Model::file(files, &tex_paths[ti]).and_then(|b| paint_core::io::decode_image(b).ok()).filter(|(w, h, _)| *w > 0 && *h > 0).map(|(w, h, rgba)| {
                        pixels += w as u64 * h as u64;
                        m.images.push(Image { w, h, rgba });
                        m.images.len() - 1
                    });
                    if pixels > MAX_TEXTURE_PIXELS {
                        return err("слишком большие текстуры");
                    }
                    images.insert(ti, i);
                    i
                }
            };
            if prim.tex.is_some() {
                // цвет материала MMD — множитель к текстуре; часто тёмный: берём белый
                prim.color = [1.0, 1.0, 1.0, mat.color[3]];
            }
        }
        prim.cutoff = (mat.color[3] < 1.0 || prim.tex.is_some()).then_some(0.5);
        mesh.prims.push(prim);
    }
    m.meshes.push(mesh);
    Ok(m)
}
