//! Документ: холст, стопка слоёв, отмена и повтор, сведение в картинку.

use crate::blend::composite;
use crate::layer::{Layer, TileBackup};
use crate::pixel::{from_f32, to_f32, to_rgba8, Px};
use crate::stroke::Stroke;
use crate::tile::{Key, Tile, TILE, TILE_LEN};
use std::collections::HashMap;

struct HistoryEntry {
    /// Путь к слою: номера в стопках от верхнего уровня вглубь папок.
    path: Vec<usize>,
    tiles: TileBackup,
}

/// Штрих, который ещё рисуется: слой уже показывает его, но отмена пока не записана.
pub struct LiveStroke {
    pub path: Vec<usize>,
    pub stroke: Stroke,
    pub rgb: [u8; 3],
    originals: HashMap<Key, Option<Tile<Px>>>,
}

impl LiveStroke {
    pub fn new(path: Vec<usize>, stroke: Stroke, rgb: [u8; 3]) -> Self {
        Self { path, stroke, rgb, originals: HashMap::new() }
    }
}

pub struct Document {
    pub width: u32,
    pub height: u32,
    /// Слои снизу вверх.
    pub layers: Vec<Layer>,
    /// Цвет фона холста; `None` — прозрачный.
    pub background: Option<[u8; 3]>,
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
}

impl Document {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            layers: vec![Layer::new("Слой 1")],
            background: Some([255, 255, 255]),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    pub fn add_layer(&mut self, name: &str) -> usize {
        self.layers.push(Layer::new(name));
        self.layers.len() - 1
    }

    pub fn layer(&self, path: &[usize]) -> Option<&Layer> {
        let (first, rest) = path.split_first()?;
        let mut layer = self.layers.get(*first)?;
        for i in rest {
            layer = layer.children.as_ref()?.get(*i)?;
        }
        Some(layer)
    }

    pub fn layer_mut(&mut self, path: &[usize]) -> Option<&mut Layer> {
        let (first, rest) = path.split_first()?;
        let mut layer = self.layers.get_mut(*first)?;
        for i in rest {
            layer = layer.children.as_mut()?.get_mut(*i)?;
        }
        Some(layer)
    }

    /// Завершает штрих: переносит его на слой и запоминает для отмены.
    pub fn commit_stroke(&mut self, layer: usize, stroke: &Stroke, rgb: [u8; 3]) {
        let tiles = self.layers[layer].apply_stroke(stroke, rgb);
        self.undo.push(HistoryEntry { path: vec![layer], tiles });
        self.redo.clear();
    }

    /// Показывает на слое новые отпечатки незавершённого штриха. Возвращает изменившиеся плитки.
    pub fn live_update(&mut self, live: &mut LiveStroke) -> Vec<Key> {
        let touched: Vec<Key> = live.stroke.take_touched().into_iter().collect();
        let Some(layer) = self.layer_mut(&live.path) else { return Vec::new() };
        for key in &touched {
            // Плитку возвращаем к исходному виду и накладываем покрытие штриха заново целиком.
            let original = live.originals.entry(*key).or_insert_with(|| layer.pixels.tiles.get(key).cloned());
            match original {
                Some(t) => {
                    layer.pixels.tiles.insert(*key, t.clone());
                }
                None => {
                    layer.pixels.tiles.remove(key);
                }
            }
            if let Some(mask) = live.stroke.mask.tiles.get(key) {
                layer.apply_mask_tile(*key, mask, live.stroke.params.erase, live.rgb);
            }
        }
        touched
    }

    /// Перо поднято: записывает штрих в историю.
    pub fn live_finish(&mut self, mut live: LiveStroke) -> Vec<Key> {
        let keys = self.live_update(&mut live);
        if !live.originals.is_empty() {
            self.undo.push(HistoryEntry { path: live.path, tiles: live.originals.into_iter().collect() });
            self.redo.clear();
        }
        keys
    }

    fn swap_history(&mut self, e: HistoryEntry) -> Option<(HistoryEntry, Vec<Key>)> {
        let layer = self.layer_mut(&e.path)?;
        let keys = e.tiles.iter().map(|(k, _)| *k).collect();
        let tiles = layer.restore(e.tiles);
        Some((HistoryEntry { path: e.path, tiles }, keys))
    }

    /// Отменяет последнее действие; возвращает плитки, которые изменились.
    pub fn undo(&mut self) -> Option<Vec<Key>> {
        let e = self.undo.pop()?;
        let (back, keys) = self.swap_history(e)?;
        self.redo.push(back);
        Some(keys)
    }

    pub fn redo(&mut self) -> Option<Vec<Key>> {
        let e = self.redo.pop()?;
        let (back, keys) = self.swap_history(e)?;
        self.undo.push(back);
        Some(keys)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    fn background_f32(&self) -> [f32; 4] {
        match self.background {
            Some(c) => [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0],
            None => [0.0; 4],
        }
    }

    /// Сведённая плитка для показа: RGBA по 8 бит, 64×64.
    pub fn render_tile(&self, key: Key) -> Vec<u8> {
        let mut buf = vec![self.background_f32(); TILE_LEN];
        render_group(&self.layers, key, &mut buf);
        let mut out = vec![0u8; TILE_LEN * 4];
        for (px, o) in buf.iter().zip(out.as_chunks_mut::<4>().0) {
            *o = to_rgba8(from_f32(*px));
        }
        out
    }

    /// Сводит все видимые слои в RGBA по 8 бит на канал (без предумножения).
    pub fn flatten(&self) -> Vec<u8> {
        let (w, h) = (self.width as i32, self.height as i32);
        let mut out = vec![0u8; (w * h * 4) as usize];
        let bg = self.background_f32();
        let mut buf = vec![bg; TILE_LEN];
        for ty in 0..(h + TILE - 1) / TILE {
            for tx in 0..(w + TILE - 1) / TILE {
                buf.fill(bg);
                render_group(&self.layers, (tx, ty), &mut buf);
                for y in 0..TILE {
                    let py = ty * TILE + y;
                    if py >= h {
                        break;
                    }
                    for x in 0..TILE {
                        let px = tx * TILE + x;
                        if px >= w {
                            break;
                        }
                        let c = to_rgba8(from_f32(buf[(y * TILE + x) as usize]));
                        let o = ((py * w + px) * 4) as usize;
                        out[o..o + 4].copy_from_slice(&c);
                    }
                }
            }
        }
        out
    }
}

type Buf = Vec<[f32; 4]>;

/// Есть ли в этом месте холста хоть что-то от слоя (для папки — от любого вложенного).
fn touches(layer: &Layer, key: Key) -> bool {
    match &layer.children {
        Some(children) => children.iter().any(|c| c.visible && touches(c, key)),
        None => layer.pixels.tiles.contains_key(&key),
    }
}

/// Содержимое слоя или папки в одной плитке, без учёта его режима и непрозрачности.
fn layer_source(layer: &Layer, key: Key) -> Option<Buf> {
    match &layer.children {
        Some(children) => {
            if !children.iter().any(|c| c.visible && touches(c, key)) {
                return None;
            }
            let mut buf = vec![[0.0f32; 4]; TILE_LEN];
            render_group(children, key, &mut buf);
            Some(buf)
        }
        None => layer.pixels.tiles.get(&key).map(|t| t.0.iter().map(|p| to_f32(*p)).collect()),
    }
}

/// Накладывает стопку слоёв (снизу вверх) на `backdrop` в пределах одной плитки.
fn render_group(layers: &[Layer], key: Key, backdrop: &mut [[f32; 4]]) {
    let mut i = 0;
    while i < layers.len() {
        let base = &layers[i];
        // Слои с обтравкой, лежащие сразу над этим, привязаны к нему.
        let mut end = i + 1;
        if !base.clipping {
            while end < layers.len() && layers[end].clipping {
                end += 1;
            }
        }
        let clips = &layers[i + 1..end];
        i = end;
        if !base.visible || base.opacity <= 0.0 {
            continue;
        }
        if base.children.is_some() && base.pass_through && clips.is_empty() {
            if !touches(base, key) {
                continue;
            }
            let mut inner = backdrop.to_vec();
            render_group(base.children.as_deref().unwrap_or(&[]), key, &mut inner);
            let k = base.opacity.clamp(0.0, 1.0);
            for (dst, src) in backdrop.iter_mut().zip(inner.iter()) {
                for c in 0..4 {
                    dst[c] += (src[c] - dst[c]) * k;
                }
            }
            continue;
        }
        let Some(mut src) = layer_source(base, key) else { continue };
        for clip in clips.iter().filter(|c| c.visible && c.opacity > 0.0) {
            let Some(top) = layer_source(clip, key) else { continue };
            for (b, t) in src.iter_mut().zip(top.iter()) {
                if b[3] <= 0.0 || t[3] <= 0.0 {
                    continue;
                }
                // Цвет основы считаем непрозрачным, накладываем на него слой, альфу основы сохраняем.
                let a = b[3];
                let opaque = [b[0] / a, b[1] / a, b[2] / a, 1.0];
                let mixed = composite(clip.mode, opaque, *t, clip.opacity);
                *b = [mixed[0] * a, mixed[1] * a, mixed[2] * a, a];
            }
        }
        for (dst, s) in backdrop.iter_mut().zip(src.iter()) {
            if s[3] > 0.0 {
                *dst = composite(base.mode, *dst, *s, base.opacity);
            }
        }
    }
}
