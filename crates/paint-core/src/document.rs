//! Документ: холст, стопка слоёв, отмена и повтор, сведение в картинку.

use crate::blend::composite;
use crate::layer::{Layer, TileBackup};
use crate::pixel::{from_f32, to_f32, to_rgba8, Px};
use crate::stroke::Stroke;
use crate::tile::{Key, Tile, TILE, TILE_LEN};
use std::collections::HashMap;

/// Шаг истории. Обратный шаг получается при выполнении (`Document::apply_op`).
enum Op {
    /// Вернуть плитки слоя к сохранённому виду.
    Tiles { path: Vec<usize>, tiles: TileBackup },
    /// Вставить слой на место `path` (последний номер — место в стопке).
    Insert { path: Vec<usize>, layer: Box<Layer> },
    /// Убрать слой с места `path`.
    Remove { path: Vec<usize> },
}

/// Одно действие пользователя: шаги выполняются по порядку, отмена — обратными шагами в обратном
/// порядке.
struct HistoryEntry(Vec<Op>);

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
        self.push(vec![Op::Tiles { path: vec![layer], tiles }]);
    }

    /// Показывает на слое новые отпечатки незавершённого штриха. Возвращает изменившиеся плитки.
    pub fn live_update(&mut self, live: &mut LiveStroke) -> Vec<Key> {
        self.live_paint(live, false)
    }

    fn live_paint(&mut self, live: &mut LiveStroke, finish: bool) -> Vec<Key> {
        if live.stroke.is_mixing() {
            // Смешивающая кисть рисует прямо в слой: краска зависит от того, что уже на холсте.
            let Some(layer) = self.layer_mut(&live.path) else { return Vec::new() };
            let preserve = layer.preserve_opacity;
            let keys = live.stroke.paint_mixed(&mut layer.pixels, live.rgb, preserve, &mut live.originals, finish);
            live.stroke.take_touched();
            return keys;
        }
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
        let keys = self.live_paint(&mut live, true);
        if !live.originals.is_empty() {
            self.push(vec![Op::Tiles { path: live.path, tiles: live.originals.into_iter().collect() }]);
        }
        keys
    }

    fn push(&mut self, ops: Vec<Op>) {
        self.undo.push(HistoryEntry(ops));
        self.redo.clear();
    }

    /// Стопка, в которой лежит слой `path`, и его место в ней.
    fn stack_mut(&mut self, path: &[usize]) -> Option<(&mut Vec<Layer>, usize)> {
        let (last, parent) = path.split_last()?;
        let stack = if parent.is_empty() { &mut self.layers } else { self.layer_mut(parent)?.children.as_mut()? };
        Some((stack, *last))
    }

    /// Выполняет шаг; возвращает обратный шаг и плитки, которые надо перерисовать.
    fn apply_op(&mut self, op: Op) -> Option<(Op, Vec<Key>)> {
        match op {
            Op::Tiles { path, tiles } => {
                let keys = tiles.iter().map(|(k, _)| *k).collect();
                let back = self.layer_mut(&path)?.restore(tiles);
                Some((Op::Tiles { path, tiles: back }, keys))
            }
            Op::Insert { path, layer } => {
                let keys = layer_keys(&layer);
                let (stack, at) = self.stack_mut(&path)?;
                stack.insert(at.min(stack.len()), *layer);
                Some((Op::Remove { path }, keys))
            }
            Op::Remove { path } => {
                let (stack, at) = self.stack_mut(&path)?;
                if at >= stack.len() {
                    return None;
                }
                let layer = stack.remove(at);
                let keys = layer_keys(&layer);
                Some((Op::Insert { path, layer: Box::new(layer) }, keys))
            }
        }
    }

    /// Выполняет шаги по порядку; возвращает обратную запись и изменившиеся плитки.
    fn apply_entry(&mut self, ops: Vec<Op>) -> (Vec<Op>, Vec<Key>) {
        let mut back = Vec::with_capacity(ops.len());
        let mut keys = Vec::new();
        for op in ops {
            if let Some((inv, k)) = self.apply_op(op) {
                back.push(inv);
                keys.extend(k);
            }
        }
        back.reverse();
        (back, keys)
    }

    /// Выполняет действие и записывает его в историю.
    fn perform(&mut self, ops: Vec<Op>) -> Vec<Key> {
        let (back, keys) = self.apply_entry(ops);
        if !back.is_empty() {
            self.push(back);
        }
        keys
    }

    /// Отменяет последнее действие; возвращает плитки, которые изменились.
    pub fn undo(&mut self) -> Option<Vec<Key>> {
        let HistoryEntry(ops) = self.undo.pop()?;
        let (back, keys) = self.apply_entry(ops);
        self.redo.push(HistoryEntry(back));
        Some(keys)
    }

    pub fn redo(&mut self) -> Option<Vec<Key>> {
        let HistoryEntry(ops) = self.redo.pop()?;
        let (back, keys) = self.apply_entry(ops);
        self.undo.push(HistoryEntry(back));
        Some(keys)
    }

    /// Вставляет слой на место `path` (с отменой).
    pub fn insert_layer(&mut self, path: &[usize], layer: Layer) -> Vec<Key> {
        self.perform(vec![Op::Insert { path: path.to_vec(), layer: Box::new(layer) }])
    }

    /// Удаляет слой (с отменой).
    pub fn delete_layer(&mut self, path: &[usize]) -> Vec<Key> {
        self.perform(vec![Op::Remove { path: path.to_vec() }])
    }

    /// Копия слоя над ним (с отменой).
    pub fn duplicate_layer(&mut self, path: &[usize]) -> Vec<Key> {
        let Some(layer) = self.layer(path) else { return Vec::new() };
        let mut copy = layer.clone();
        copy.name = format!("{} (копия)", layer.name);
        let mut at = path.to_vec();
        *at.last_mut().unwrap() += 1;
        self.insert_layer(&at, copy)
    }

    /// Очищает обычный слой (с отменой).
    pub fn clear_layer(&mut self, path: &[usize]) -> Vec<Key> {
        let Some(layer) = self.layer(path) else { return Vec::new() };
        if layer.children.is_some() {
            return Vec::new();
        }
        let tiles: TileBackup = layer.pixels.tiles.keys().map(|k| (*k, None)).collect();
        self.perform(vec![Op::Tiles { path: path.to_vec(), tiles }])
    }

    /// Переставляет слой на `delta` мест в той же стопке (+1 — выше). Возвращает новый путь.
    pub fn move_layer(&mut self, path: &[usize], delta: isize) -> Option<(Vec<usize>, Vec<Key>)> {
        let (last, parent) = path.split_last()?;
        let len = if parent.is_empty() { self.layers.len() } else { self.layer(parent)?.children.as_ref()?.len() };
        let to = *last as isize + delta;
        if to < 0 || to >= len as isize || delta == 0 {
            return None;
        }
        let layer = self.layer(path)?.clone();
        let mut dst = parent.to_vec();
        dst.push(to as usize);
        let keys = self.perform(vec![Op::Remove { path: path.to_vec() }, Op::Insert { path: dst.clone(), layer: Box::new(layer) }]);
        Some((dst, keys))
    }

    /// Сливает обычный слой с нижним обычным слоем той же стопки (с отменой): нижний получает
    /// результат наложения верхнего с его режимом и непрозрачностью, верхний убирается.
    /// Возвращает путь нижнего слоя.
    pub fn merge_down(&mut self, path: &[usize]) -> Option<(Vec<usize>, Vec<Key>)> {
        self.combine_down(path, false)
    }

    /// «Перенести на нижний», как в образце: рисунок слоя накладывается на нижний слой, а сам
    /// слой остаётся пустым (с отменой). Возвращает путь нижнего слоя.
    pub fn transfer_down(&mut self, path: &[usize]) -> Option<(Vec<usize>, Vec<Key>)> {
        self.combine_down(path, true)
    }

    fn combine_down(&mut self, path: &[usize], keep_upper: bool) -> Option<(Vec<usize>, Vec<Key>)> {
        let (last, parent) = path.split_last()?;
        if *last == 0 {
            return None;
        }
        let mut below = parent.to_vec();
        below.push(last - 1);
        let upper = self.layer(path)?;
        let lower = self.layer(&below)?;
        if upper.children.is_some() || lower.children.is_some() {
            return None;
        }
        // Нижний слой — как обычный непрозрачный «фон» пары; верхний — со своими свойствами.
        let mut base = lower.clone();
        base.mode = crate::blend::BlendMode::Normal;
        base.opacity = 1.0;
        base.visible = true;
        base.clipping = false;
        let mut top = upper.clone();
        top.visible = true;
        let pair = [base, top];
        let mut keys: Vec<Key> = lower.pixels.tiles.keys().chain(upper.pixels.tiles.keys()).copied().collect();
        keys.sort_unstable();
        keys.dedup();
        let mut tiles: TileBackup = Vec::with_capacity(keys.len());
        for key in &keys {
            let mut buf = vec![[0f32; 4]; TILE_LEN];
            render_group(&pair, *key, &mut buf);
            let tile: Vec<Px> = buf.iter().map(|c| from_f32(*c)).collect();
            if tile.iter().all(|p| p[3] == 0) {
                tiles.push((*key, None));
            } else {
                tiles.push((*key, Some(Tile(tile.into_boxed_slice()))));
            }
        }
        let second = if keep_upper {
            let upper_tiles: TileBackup = self.layer(path)?.pixels.tiles.keys().map(|k| (*k, None)).collect();
            Op::Tiles { path: path.to_vec(), tiles: upper_tiles }
        } else {
            Op::Remove { path: path.to_vec() }
        };
        let k = self.perform(vec![Op::Tiles { path: below.clone(), tiles }, second]);
        Some((below, k))
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
/// Плитки слоя (у папки — всех вложенных).
fn layer_keys(layer: &Layer) -> Vec<Key> {
    match &layer.children {
        Some(children) => children.iter().flat_map(layer_keys).collect(),
        None => layer.pixels.tiles.keys().copied().collect(),
    }
}

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
