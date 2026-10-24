//! FBX (Autodesk; выводят Cinema 4D, Maya, 3ds Max, Blender, Mixamo, Daz) через библиотеку ufbx:
//! узлы, сетки по материалам, скелет, морфы (blend shapes), цвет и текстура материала.
//! Оси и единицы приводятся к glTF: Y вверх, метры.

use crate::model::{err, Error, Image, Mesh, Model, Node, Prim, Res, Skin, MAX_TEXTURE_PIXELS, MAX_TRIANGLES, MAX_VERTICES};
use glam::{Mat4, Quat, Vec2, Vec3};
use std::collections::HashMap;

/// Памяти на разбор FBX, байт (на временную и на итоговую части).
const MEMORY_LIMIT: usize = 4 << 30;

fn mat4(m: &ufbx::Matrix) -> Mat4 {
    Mat4::from_cols_array(&[
        m.m00 as f32, m.m10 as f32, m.m20 as f32, 0.0,
        m.m01 as f32, m.m11 as f32, m.m21 as f32, 0.0,
        m.m02 as f32, m.m12 as f32, m.m22 as f32, 0.0,
        m.m03 as f32, m.m13 as f32, m.m23 as f32, 1.0,
    ])
}

fn v3(v: ufbx::Vec3) -> Vec3 {
    Vec3::new(v.x as f32, v.y as f32, v.z as f32)
}

/// Имена внешних (не встроенных) текстур FBX — чтобы взять их с диска рядом с моделью.
pub fn texture_files(data: &[u8]) -> Vec<String> {
    let opts = ufbx::LoadOpts {
        ignore_geometry: true,
        ignore_animation: true,
        load_external_files: false,
        node_depth_limit: 512,
        temp_allocator: ufbx::AllocatorOpts { memory_limit: MEMORY_LIMIT, ..Default::default() },
        result_allocator: ufbx::AllocatorOpts { memory_limit: MEMORY_LIMIT, ..Default::default() },
        ..Default::default()
    };
    let Ok(scene) = ufbx::load_memory(data, opts) else { return Vec::new() };
    scene.textures.iter().filter(|t| t.content.is_empty()).map(|t| if t.relative_filename.is_empty() { t.filename.to_string() } else { t.relative_filename.to_string() }).filter(|n| !n.is_empty()).collect()
}

pub fn load(data: &[u8], files: &[(String, Vec<u8>)]) -> Res<Model> {
    let opts = ufbx::LoadOpts {
        target_axes: ufbx::CoordinateAxes::right_handed_y_up(),
        target_unit_meters: 1.0,
        geometry_transform_handling: ufbx::GeometryTransformHandling::ModifyGeometry,
        inherit_mode_handling: ufbx::InheritModeHandling::Compensate,
        generate_missing_normals: true,
        ignore_animation: true,
        // соседние файлы — только из списка рядом с моделью, сама библиотека диск не читает
        load_external_files: false,
        // испорченный файл не должен развернуться в огромное дерево и съесть всю память
        node_depth_limit: 512,
        temp_allocator: ufbx::AllocatorOpts { memory_limit: MEMORY_LIMIT, ..Default::default() },
        result_allocator: ufbx::AllocatorOpts { memory_limit: MEMORY_LIMIT, ..Default::default() },
        ..Default::default()
    };
    let scene = ufbx::load_memory(data, opts).map_err(|e| Error(format!("FBX: {}", &*e.description)))?;
    let mut m = Model::default();

    // узлы: номер в списке ufbx (`typed_id`) — номер у нас
    let n_nodes = scene.nodes.len();
    for node in &scene.nodes {
        let (s, r, t) = mat4(&node.node_to_parent).to_scale_rotation_translation();
        let name = &*node.element.name;
        m.nodes.push(Node {
            name: if name.is_empty() { format!("node{}", node.element.typed_id) } else { name.to_string() },
            parent: node.parent.as_ref().map(|p| p.element.typed_id as usize).filter(|&p| p < n_nodes),
            children: node.children.iter().map(|c| c.element.typed_id as usize).filter(|&c| c < n_nodes).collect(),
            t,
            r: if r.is_finite() { r.normalize() } else { Quat::IDENTITY },
            s,
            mesh: None,
            skin: None,
            joint: false,
        });
    }
    m.roots = (0..m.nodes.len()).filter(|&i| m.nodes[i].parent.is_none()).collect();

    // картинки: внутри файла или соседний файл по имени
    let mut pixels = 0u64;
    let mut tex_of: HashMap<u32, Option<usize>> = HashMap::new();
    let mut image_of = |tex: &ufbx::Texture, m: &mut Model| -> Res<Option<usize>> {
        if let Some(&i) = tex_of.get(&tex.element.typed_id) {
            return Ok(i);
        }
        let bytes: Option<Vec<u8>> = if !tex.content.is_empty() {
            Some(tex.content.to_vec())
        } else {
            [&*tex.relative_filename, &*tex.filename, &*tex.absolute_filename].into_iter().filter(|n| !n.is_empty()).find_map(|n| Model::file(files, n).map(<[u8]>::to_vec))
        };
        let i = match bytes.and_then(|b| paint_core::io::decode_image(&b).ok()) {
            Some((w, h, rgba)) if w > 0 && h > 0 => {
                pixels += w as u64 * h as u64;
                if pixels > MAX_TEXTURE_PIXELS {
                    return err("слишком большие текстуры");
                }
                m.images.push(Image { w, h, rgba });
                Some(m.images.len() - 1)
            }
            _ => None,
        };
        tex_of.insert(tex.element.typed_id, i);
        Ok(i)
    };

    // сетки
    let mut vertices = 0usize;
    for mesh in &scene.meshes {
        m.triangles += mesh.num_triangles;
        if m.triangles > MAX_TRIANGLES {
            return err("слишком много треугольников (больше 20 млн)");
        }
        let skin = mesh.skin_deformers.first();
        // скелет сетки: суставы — узлы кластеров, обратная привязка — из геометрии в сустав
        let mut skin_index = None;
        if let Some(sk) = skin {
            let joints: Vec<usize> = sk.clusters.iter().map(|c| c.bone_node.as_ref().map_or(0, |b| b.element.typed_id as usize).min(n_nodes.saturating_sub(1))).collect();
            if !joints.is_empty() {
                for &j in &joints {
                    m.nodes[j].joint = true;
                }
                m.skins.push(Skin { joints, inv_bind: sk.clusters.iter().map(|c| mat4(&c.geometry_to_bone)).collect() });
                skin_index = Some(m.skins.len() - 1);
            }
        }
        // морфы: каналы с целевой формой
        let channels: Vec<(String, f32, &ufbx::BlendShape)> = mesh
            .blend_deformers
            .iter()
            .flat_map(|d| d.channels.iter())
            .filter_map(|c| c.target_shape.as_ref().map(|s| (c.element.name.to_string(), c.weight as f32, &**s)))
            .collect();
        let mut out = Mesh {
            name: mesh.element.name.to_string(),
            morph_names: channels.iter().map(|c| c.0.clone()).collect(),
            weights: channels.iter().map(|c| c.1).collect(),
            prims: Vec::new(),
        };
        // части по материалам (без материалов — одна часть из всех граней)
        let all: Vec<u32> = (0..mesh.num_faces as u32).collect();
        let parts: Vec<(Option<u32>, &[u32])> = if mesh.material_parts.is_empty() {
            vec![(None, &all[..])]
        } else {
            mesh.material_parts.iter().map(|p| (Some(p.index), p.face_indices.as_ref())).collect()
        };
        let mut tri = Vec::new();
        for (mat_index, faces) in parts {
            let mut prim = Prim { color: [0.8, 0.8, 0.8, 1.0], double: true, ..Default::default() };
            let mut seen: HashMap<u32, u32> = HashMap::new();
            let mut orig: Vec<u32> = Vec::new();
            for &f in faces {
                let Some(face) = mesh.faces.get(f as usize) else { continue };
                ufbx::triangulate_face_vec(&mut tri, mesh, *face);
                for &ix in &tri {
                    let ix = ix as usize;
                    if ix >= mesh.num_indices {
                        return err("FBX: номер угла вне сетки");
                    }
                    // угол → вершина у нас (одинаковые положение, нормаль и развёртка — одна)
                    let key = ix as u32;
                    let v = match seen.get(&key) {
                        Some(&v) => v,
                        None => {
                            let v = prim.pos.len() as u32;
                            prim.pos.push(v3(mesh.vertex_position[ix]));
                            if mesh.vertex_normal.exists {
                                prim.nrm.push(v3(mesh.vertex_normal[ix]));
                            }
                            if mesh.vertex_uv.exists {
                                let uv = mesh.vertex_uv[ix];
                                // в FBX развёртка снизу вверх, в glTF — сверху вниз
                                prim.uv.push(Vec2::new(uv.x as f32, 1.0 - uv.y as f32));
                            }
                            orig.push(mesh.vertex_indices[ix]);
                            seen.insert(key, v);
                            v
                        }
                    };
                    prim.idx.push(v);
                }
            }
            if prim.idx.is_empty() {
                continue;
            }
            vertices += prim.pos.len();
            if vertices > MAX_VERTICES {
                return err("слишком много вершин");
            }
            // одинаковые углы — общая вершина
            weld(&mut prim, &mut orig);
            if let (Some(sk), Some(_)) = (skin, skin_index) {
                let nc = sk.clusters.len();
                for &ov in &orig {
                    let (mut j, mut w) = ([0u16; 4], [0f32; 4]);
                    if let Some(sv) = sk.vertices.get(ov as usize) {
                        let (b, n) = (sv.weight_begin as usize, sv.num_weights as usize);
                        // веса у ufbx — от большего к меньшему: берём четыре первых
                        for (k, sw) in sk.weights.get(b..b + n).unwrap_or(&[]).iter().take(4).enumerate() {
                            if (sw.cluster_index as usize) < nc {
                                j[k] = sw.cluster_index as u16;
                                w[k] = sw.weight as f32;
                            }
                        }
                    }
                    prim.joints.push(j);
                    prim.weights.push(w);
                }
            }
            for (_, _, shape) in &channels {
                prim.morphs.push(orig.iter().map(|&ov| v3(ufbx::get_blend_shape_vertex_offset(shape, ov as usize))).collect());
            }
            if let Some(mat) = mat_index.and_then(|i| mesh.materials.get(i as usize)) {
                let (pbr, fbx) = (&mat.pbr.base_color, &mat.fbx.diffuse_color);
                let map = if pbr.has_value || pbr.texture.is_some() { pbr } else { fbx };
                if map.has_value {
                    let c = map.value_vec4;
                    let a = if map.value_components >= 4 { c.w as f32 } else { 1.0 };
                    prim.color = [c.x as f32, c.y as f32, c.z as f32, a.clamp(0.0, 1.0)];
                }
                let tex = pbr.texture.as_ref().or(fbx.texture.as_ref());
                if let Some(t) = tex {
                    prim.tex = image_of(t, &mut m)?;
                    if prim.tex.is_some() {
                        // с текстурой цвет материала — множитель; у FBX он часто тёмный по умолчанию
                        prim.color = [1.0, 1.0, 1.0, prim.color[3]];
                    }
                }
            }
            out.prims.push(prim);
        }
        let mi = m.meshes.len();
        m.meshes.push(out);
        for inst in &mesh.element.instances {
            let ni = inst.element.typed_id as usize;
            if ni < n_nodes {
                m.nodes[ni].mesh = Some(mi);
                m.nodes[ni].skin = skin_index;
            }
        }
    }
    Ok(m)
}

/// Склеить вершины с одинаковыми положением, нормалью и развёрткой (у FBX данные — по углам граней).
fn weld(p: &mut Prim, orig: &mut Vec<u32>) {
    let key = |i: usize| -> [u32; 9] {
        let n = p.nrm.get(i).copied().unwrap_or(Vec3::ZERO);
        let u = p.uv.get(i).copied().unwrap_or(Vec2::ZERO);
        [p.pos[i].x.to_bits(), p.pos[i].y.to_bits(), p.pos[i].z.to_bits(), n.x.to_bits(), n.y.to_bits(), n.z.to_bits(), u.x.to_bits(), u.y.to_bits(), orig[i]]
    };
    let mut map: HashMap<[u32; 9], u32> = HashMap::new();
    let mut remap = Vec::with_capacity(p.pos.len());
    let mut keep = Vec::new();
    for i in 0..p.pos.len() {
        let k = key(i);
        let next = keep.len() as u32;
        let v = *map.entry(k).or_insert(next);
        if v == next {
            keep.push(i);
        }
        remap.push(v);
    }
    if keep.len() == p.pos.len() {
        return;
    }
    p.pos = keep.iter().map(|&i| p.pos[i]).collect();
    if !p.nrm.is_empty() {
        p.nrm = keep.iter().map(|&i| p.nrm[i]).collect();
    }
    if !p.uv.is_empty() {
        p.uv = keep.iter().map(|&i| p.uv[i]).collect();
    }
    *orig = keep.iter().map(|&i| orig[i]).collect();
    for i in &mut p.idx {
        *i = remap[*i as usize];
    }
}
