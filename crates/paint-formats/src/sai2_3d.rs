//! Слой 3D в `.sai2`: состояние сцены — блок `gs3d` в описании слоя, модели — блоки файла `gs3a`.
//! SAI 2 видит такой слой обычным растровым (проверено в SAI пользователя, `docs/39`).

use paint_core::layer::{Asset3d, Scene3dData};
use paint_core::Layer;
use std::collections::HashMap;
use std::sync::Arc;

pub const LAYER_TAG: [u8; 4] = *b"gs3d";
pub const FILE_TAG: [u8; 4] = *b"gs3a";

/// Блок слоя: версия, номера моделей, текст сцены.
pub fn layer_block(s: &Scene3dData) -> Vec<u8> {
    let mut d = 1u32.to_le_bytes().to_vec();
    d.extend((s.assets.len() as u32).to_le_bytes());
    for a in &s.assets {
        d.extend(a.id.to_le_bytes());
    }
    d.extend(s.state.as_bytes());
    d
}

/// Блок файла с моделью.
pub fn asset_block(a: &Asset3d) -> Vec<u8> {
    let mut d = a.id.to_le_bytes().to_vec();
    let name = a.name.as_bytes();
    d.extend((name.len().min(u16::MAX as usize) as u16).to_le_bytes());
    d.extend(&name[..name.len().min(u16::MAX as usize)]);
    d.extend((a.files.len() as u32).to_le_bytes());
    for (n, data) in &a.files {
        let n = n.as_bytes();
        let n = &n[..n.len().min(u16::MAX as usize)];
        d.extend((n.len() as u16).to_le_bytes());
        d.extend(n);
        d.extend((data.len() as u64).to_le_bytes());
        d.extend(data);
    }
    d
}

pub fn parse_asset(d: &[u8]) -> Option<Asset3d> {
    let mut p = 0usize;
    let mut take = |n: usize| -> Option<&[u8]> {
        let s = d.get(p..p.checked_add(n)?)?;
        p += n;
        Some(s)
    };
    let id = u64::from_le_bytes(take(8)?.try_into().ok()?);
    let nl = u16::from_le_bytes(take(2)?.try_into().ok()?) as usize;
    let name = String::from_utf8_lossy(take(nl)?).into_owned();
    let count = u32::from_le_bytes(take(4)?.try_into().ok()?) as usize;
    if count > 10_000 {
        return None;
    }
    let mut files = Vec::with_capacity(count);
    for _ in 0..count {
        let fl = u16::from_le_bytes(take(2)?.try_into().ok()?) as usize;
        let fname = String::from_utf8_lossy(take(fl)?).into_owned();
        let len = u64::from_le_bytes(take(8)?.try_into().ok()?);
        let data = take(usize::try_from(len).ok()?)?.to_vec();
        files.push((fname, data));
    }
    Some(Asset3d { id, name, files })
}

/// Блоки моделей всех слоёв 3D (каждая модель — один раз).
pub fn file_blocks(layers: &[Layer]) -> Vec<([u8; 4], u32, Vec<u8>)> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    fn walk(layers: &[Layer], seen: &mut Vec<u64>, out: &mut Vec<([u8; 4], u32, Vec<u8>)>) {
        for l in layers {
            if let Some(s) = &l.scene3d {
                for a in &s.assets {
                    if !seen.contains(&a.id) {
                        seen.push(a.id);
                        out.push((FILE_TAG, out.len() as u32 + 1, asset_block(a)));
                    }
                }
            }
            if let Some(ch) = &l.children {
                walk(ch, seen, out);
            }
        }
    }
    walk(layers, &mut seen, &mut out);
    out
}

/// После чтения: блок `gs3d` из прочего описания слоя → сцена с моделями из `assets`.
/// Возвращает имена слоёв, у которых не нашлось модели.
pub fn attach(layers: &mut [Layer], assets: &HashMap<u64, Arc<Asset3d>>) -> Vec<String> {
    let mut missing = Vec::new();
    for l in layers {
        if let Some(k) = l.extra.iter().position(|(t, _)| *t == LAYER_TAG) {
            let (_, body) = l.extra.remove(k);
            if body.len() >= 8 && u32::from_le_bytes(body[0..4].try_into().unwrap()) == 1 {
                let n = (u32::from_le_bytes(body[4..8].try_into().unwrap()) as usize).min(10_000);
                let ids_end = 8 + n * 8;
                if body.len() >= ids_end {
                    let ids: Vec<u64> = (0..n).map(|i| u64::from_le_bytes(body[8 + i * 8..16 + i * 8].try_into().unwrap())).collect();
                    let found: Vec<Arc<Asset3d>> = ids.iter().filter_map(|id| assets.get(id).cloned()).collect();
                    if found.len() < ids.len() {
                        missing.push(l.name.clone());
                    }
                    l.scene3d = Some(Box::new(Scene3dData { assets: found, state: String::from_utf8_lossy(&body[ids_end..]).into_owned() }));
                }
            }
        }
        if let Some(ch) = l.children.as_mut() {
            missing.extend(attach(ch, assets));
        }
    }
    missing
}

/// Марионетка слоя в `.sai2`: блок `gspp` — длина текста (u32), текст настроек и булавок, затем
/// сжатые плитки исходника (число, затем у каждой — x, y и 64×64 точки по 4 u16).
pub const PUPPET_TAG: [u8; 4] = *b"gspp";

pub fn puppet_block(p: &paint_core::puppet::Puppet) -> Vec<u8> {
    let text = p.to_text().into_bytes();
    let mut raw = Vec::new();
    raw.extend((p.source.tiles.len() as u32).to_le_bytes());
    let mut keys: Vec<_> = p.source.tiles.keys().copied().collect();
    keys.sort_unstable();
    for k in keys {
        raw.extend(k.0.to_le_bytes());
        raw.extend(k.1.to_le_bytes());
        for px in p.source.tiles[&k].0.iter() {
            for c in px {
                raw.extend(c.to_le_bytes());
            }
        }
    }
    let mut out = (text.len() as u32).to_le_bytes().to_vec();
    out.extend(text);
    out.extend(miniz_oxide::deflate::compress_to_vec(&raw, 6));
    out
}

pub fn puppet_from_block(b: &[u8]) -> Option<paint_core::puppet::Puppet> {
    use paint_core::tile::{Tile, TILE_LEN};
    let n = u32::from_le_bytes(b.get(0..4)?.try_into().ok()?) as usize;
    let text = std::str::from_utf8(b.get(4..4usize.checked_add(n)?)?).ok()?;
    // не больше 1 ГБ исходника
    let raw = miniz_oxide::inflate::decompress_to_vec_with_limit(b.get(4 + n..)?, 1 << 30).ok()?;
    let count = u32::from_le_bytes(raw.get(0..4)?.try_into().ok()?) as usize;
    let size = 8 + TILE_LEN * 8;
    if raw.len() != 4 + count.checked_mul(size)? {
        return None;
    }
    let mut src = paint_core::tile::ColorMap::new();
    for i in 0..count {
        let o = 4 + i * size;
        let x = i32::from_le_bytes(raw[o..o + 4].try_into().ok()?);
        let y = i32::from_le_bytes(raw[o + 4..o + 8].try_into().ok()?);
        let px: Vec<[u16; 4]> = raw[o + 8..o + size].as_chunks::<8>().0.iter().map(|c| [0, 1, 2, 3].map(|k| u16::from_le_bytes([c[k * 2], c[k * 2 + 1]]))).collect();
        src.tiles.insert((x, y), Tile(px.into_boxed_slice()));
    }
    let mut p = paint_core::puppet::Puppet::new(src);
    p.read_text(text);
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Слой 3D сохраняется и читается обратно: сцена, модели (одна на два слоя), пиксели.
    #[test]
    fn scene_layers_round_trip() {
        let a = Arc::new(Asset3d { id: 42, name: "m.glb".into(), files: vec![("m.glb".into(), vec![1, 2, 3]), ("tex.png".into(), vec![9; 1000])] });
        let mut doc = paint_core::Document::new(64, 64);
        let mut l = Layer::new("3D");
        l.scene3d = Some(Box::new(Scene3dData { assets: vec![a.clone()], state: "gs3d 1\ndisplay clay\n".into() }));
        l.pixels.set(5, 5, [100, 0, 0, 32768]);
        let mut l2 = l.clone();
        l2.name = "3D копия".into();
        doc.layers.push(l);
        doc.layers.push(l2);
        let dir = std::env::temp_dir().join(format!("grazysai-3d-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("s.sai2");
        crate::sai2_write::save(&doc, &p).unwrap();
        let info = crate::sai2::read_info(&p).unwrap();
        assert_eq!(info.entries.iter().filter(|e| e.tag == FILE_TAG).count(), 1, "модель записана один раз");
        let back = crate::sai2_doc::load(&p).unwrap();
        let ls = &back.document.layers;
        for i in [1, 2] {
            let s = ls[i].scene3d.as_ref().expect("сцена прочитана");
            assert_eq!(s.state, "gs3d 1\ndisplay clay\n");
            assert_eq!(*s.assets[0], *a);
            assert!(ls[i].extra.iter().all(|(t, _)| *t != LAYER_TAG));
            assert_eq!(ls[i].pixels.get(5, 5)[3], 32768);
        }
        // пересохранение не дублирует блоки
        crate::sai2_write::save(&back.document, &p).unwrap();
        let again = crate::sai2_doc::load(&p).unwrap();
        assert_eq!(again.document.layers[1].scene3d, back.document.layers[1].scene3d);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn broken_asset_is_none() {
        let a = Asset3d { id: 1, name: "x".into(), files: vec![("x".into(), vec![0; 10])] };
        let b = asset_block(&a);
        for cut in 0..b.len() {
            assert!(parse_asset(&b[..cut]).is_none());
        }
        assert_eq!(parse_asset(&b).unwrap(), a);
    }
}
