//! Действия над всем холстом и над слоем: отражение, поворот на 90°, размер и разрешение.

use crate::layer::Layer;
use crate::pixel::ONE;
use crate::resample::{resize, Mode};
use crate::tile::{ColorMap, MaskMap, TileMap, TILE};

/// Перенос пикселей по правилу `f(x, y) -> (x', y')` (целые координаты).
pub fn remap<T: Copy + Default + PartialEq>(src: &TileMap<T>, f: impl Fn(i32, i32) -> (i32, i32)) -> TileMap<T> {
    let mut out = TileMap::new();
    for (&(tx, ty), t) in &src.tiles {
        for (i, &p) in t.0.iter().enumerate() {
            if p == T::default() {
                continue;
            }
            let (x, y) = f(tx * TILE + i as i32 % TILE, ty * TILE + i as i32 / TILE);
            out.set(x, y, p);
        }
    }
    out
}

/// Применить перенос ко всем слоям дерева.
pub fn remap_layers(layers: &mut [Layer], f: &dyn Fn(i32, i32) -> (i32, i32)) {
    for l in layers {
        if let Some(m) = l.mask.as_mut() {
            m.hide = remap(&m.hide, f);
        }
        match l.children.as_mut() {
            Some(ch) => remap_layers(ch, f),
            None => l.pixels = remap(&l.pixels, f),
        }
    }
}

/// Пересэмплировать все слои холста `w`×`h` в `nw`×`nh`.
pub fn resample_layers(layers: &mut [Layer], w: u32, h: u32, nw: u32, nh: u32, mode: Mode) {
    for l in layers {
        if let Some(m) = l.mask.as_mut() {
            m.hide = resample_mask(&m.hide, w, h, nw, nh, mode);
        }
        if let Some(ch) = l.children.as_mut() {
            resample_layers(ch, w, h, nw, nh, mode);
            continue;
        }
        if l.pixels.tiles.is_empty() {
            continue;
        }
        let k = 1.0 / ONE as f32;
        let mut buf = vec![[0f32; 4]; (w * h) as usize];
        for (i, v) in buf.iter_mut().enumerate() {
            let p = l.pixels.get(i as i32 % w as i32, i as i32 / w as i32);
            *v = p.map(|c| c as f32 * k);
        }
        let out = resize(&buf, w as usize, h as usize, nw as usize, nh as usize, mode);
        let mut px = ColorMap::new();
        for (i, v) in out.iter().enumerate() {
            if v[3] > 0.0 {
                let q = v.map(|c| (c.clamp(0.0, 1.0) * ONE as f32 + 0.5) as u16);
                px.set(i as i32 % nw as i32, i as i32 / nw as i32, q);
            }
        }
        l.pixels = px;
    }
}

/// Пересэмплировать маску (0..=16384) так же, как слой.
fn resample_mask(hide: &MaskMap, w: u32, h: u32, nw: u32, nh: u32, mode: Mode) -> MaskMap {
    let mut out = MaskMap::new();
    if hide.tiles.is_empty() {
        return out;
    }
    let mut buf = vec![[0f32; 4]; (w * h) as usize];
    for (i, v) in buf.iter_mut().enumerate() {
        v[3] = hide.get(i as i32 % w as i32, i as i32 / w as i32) as f32 / 16384.0;
    }
    for (i, v) in resize(&buf, w as usize, h as usize, nw as usize, nh as usize, mode).iter().enumerate() {
        let m = (v[3].clamp(0.0, 1.0) * 16384.0).round() as u16;
        if m > 0 {
            out.set(i as i32 % nw as i32, i as i32 / nw as i32, m);
        }
    }
    out
}

/// Сдвиг для нового размера холста по привязке: `anchor` — 0, 1 или 2 по каждой оси
/// (лево/центр/право, верх/центр/низ).
pub fn anchor_offset(w: u32, h: u32, nw: u32, nh: u32, anchor: (u8, u8)) -> (i32, i32) {
    let d = |old: u32, new: u32, a: u8| (new as i32 - old as i32) * a as i32 / 2;
    (d(w, nw, anchor.0), d(h, nh, anchor.1))
}
