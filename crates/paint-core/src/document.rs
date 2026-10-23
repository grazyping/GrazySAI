//! Документ: холст, стопка слоёв, отмена и повтор, сведение в картинку.

use crate::blend::composite;
use crate::layer::{Layer, TileBackup};
use crate::pixel::{from_f32, to_f32, to_rgba8, Px, ONE};
use crate::selection::{Combine, Selection};
use crate::stroke::Stroke;
use crate::tile::{ColorMap, Key, MaskMap, Tile, TILE, TILE_LEN};
use std::collections::HashMap;

/// Шаг истории. Обратный шаг получается при выполнении (`Document::apply_op`).
enum Op {
    /// Вернуть плитки слоя к сохранённому виду.
    Tiles { path: Vec<usize>, tiles: TileBackup },
    /// То же, плитки выгружены на диск (`history_disk`).
    TilesOnDisk { path: Vec<usize>, spill: crate::history_disk::Spill },
    /// Вставить слой на место `path` (последний номер — место в стопке).
    Insert { path: Vec<usize>, layer: Box<Layer> },
    /// Убрать слой с места `path`.
    Remove { path: Vec<usize> },
    /// Заменить выделение (обратный шаг — прежнее выделение).
    Selection { mask: MaskMap },
    /// Заменить кривые слоя контура или фигур (обратный шаг — прежние кривые).
    Vector { path: Vec<usize>, vector: Option<crate::layer::VectorData> },
    /// Заменить сцену слоя 3D (обратный шаг — прежняя сцена).
    Scene { path: Vec<usize>, scene: Option<Box<crate::layer::Scene3dData>> },
    /// Заменить маску слоя (обратный шаг — прежняя маска).
    Mask { path: Vec<usize>, mask: Option<crate::layer::LayerMask> },
    /// Заменить весь холст: размер, слои, выделение (для действий над холстом).
    Canvas { width: u32, height: u32, layers: Vec<Layer>, selection: MaskMap },
}

/// Вид шага истории (по убыванию важности для подписи).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepKind {
    Canvas,
    NewLayer,
    DeleteLayer,
    Mask,
    Vector,
    Scene3d,
    Paint,
    Selection,
    Other,
}

/// Подпись шага: вид и слой, если он понятен.
#[derive(Clone, Debug, PartialEq)]
pub struct StepInfo {
    pub kind: StepKind,
    pub layer: Option<String>,
}

/// Одно действие пользователя: шаги выполняются по порядку, отмена — обратными шагами в обратном
/// порядке.
struct HistoryEntry(Vec<Op>);

/// Шагов отмены не больше этого — как в SAI (100).
pub const HISTORY_STEPS: usize = 100;
/// И не больше этой памяти на шаги, которые нельзя выгрузить на диск (слои целиком, холст);
/// плитки сверх `history_disk::ram_budget` уходят на диск.
pub const HISTORY_BYTES: usize = 2 << 30;

impl HistoryEntry {
    /// Сколько памяти занимает шаг (плитки и слои — по их пикселям).
    fn bytes(&self) -> usize {
        fn layer_bytes(l: &Layer) -> usize {
            l.pixels.tiles.len() * TILE_LEN * 8 + l.children.as_ref().map_or(0, |c| c.iter().map(layer_bytes).sum())
        }
        self.0
            .iter()
            .map(|op| match op {
                Op::Tiles { tiles, .. } => tiles.iter().filter(|(_, t)| t.is_some()).count() * TILE_LEN * 8,
                Op::Insert { layer, .. } => layer_bytes(layer),
                Op::Canvas { layers, selection, .. } => layers.iter().map(layer_bytes).sum::<usize>() + selection.tiles.len() * TILE_LEN * 2,
                Op::Selection { mask } => mask.tiles.len() * TILE_LEN * 2,
                Op::Vector { vector, .. } => vector.as_ref().map_or(0, |v| v.raw.len()),
                Op::Scene { scene, .. } => scene.as_ref().map_or(0, |s| s.state.len()),
                Op::Remove { .. } | Op::TilesOnDisk { .. } => 0,
                Op::Mask { mask, .. } => mask.as_ref().map_or(0, |m| m.hide.tiles.len() * TILE_LEN * 2),
            })
            .sum::<usize>()
            + 64
    }
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

/// Что изменилось в документе — для записи процесса (`Document::journal`).
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    /// У слоя `uid` изменились плитки `keys`.
    Tiles { uid: u64, keys: Vec<Key> },
    /// Слой `uid` целиком (вставлен, маска, кривые): записать заново.
    Layer(u64),
    /// Весь холст (размер, сведение, поворот…): записать всё заново.
    Canvas,
}

pub struct Document {
    /// Журнал изменений для записи процесса; `None` — запись не идёт (ничего не копится).
    pub journal: Option<Vec<Change>>,
    pub width: u32,
    pub height: u32,
    /// Слои снизу вверх.
    pub layers: Vec<Layer>,
    /// Цвет фона холста; `None` — прозрачный (следует из `bg_code`, см. `set_background`).
    pub background: Option<[u8; 3]>,
    /// Фон холста, как байт 0x11 заголовка `.sai2` (поле холста +0x249 в SAI 2017): младшие
    /// 4 бита — 0 прозрачный, 1 непрозрачный; старшие — вид. Прозрачный показывается: 0 — светлой
    /// шашкой, 1 — тёмной, 2 — белым, 3 — чёрным, 4 — цветом `bg_color`; непрозрачный: 0 — белый,
    /// 1 — чёрный, 2 — цвет `bg_color` (docs/03-sai2-format.md).
    pub bg_code: u8,
    /// Цвет «Canvas Background Color» (смещение 0x38 заголовка).
    pub bg_color: [u8; 3],
    /// «Layer Effect Color» (0x3C заголовка): `norm`, `vivd`, `deep`, `mult`, `ver1` — хранится как был.
    pub effect_color: [u8; 4],
    /// Разрешение печати, как в `.sai2` (0x1C): точек на сантиметр × 65536; 96 dpi по умолчанию.
    pub resolution: u32,
    /// Выделение; пустое — рисовать можно везде.
    pub selection: Selection,
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
}

impl Document {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            journal: None,
            width,
            height,
            layers: vec![Layer::new("Слой 1")],
            background: Some([255, 255, 255]),
            bg_code: 0x01,
            bg_color: [0x80, 0x80, 0x80],
            effect_color: *b"norm",
            resolution: (96.0f64 / 2.54 * 65536.0).round() as u32,
            selection: Selection::default(),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Копия холста без истории (для записи в фоне).
    pub fn snapshot(&self) -> Document {
        Document {
            journal: None,
            width: self.width,
            height: self.height,
            layers: self.layers.clone(),
            background: self.background,
            bg_code: self.bg_code,
            bg_color: self.bg_color,
            effect_color: self.effect_color,
            resolution: self.resolution,
            selection: self.selection.clone(),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Документ из картинки RGBA по 8 бит (один слой; фон — белый, как у нового холста).
    pub fn from_rgba(width: u32, height: u32, rgba: &[u8]) -> Self {
        let mut doc = Document::new(width, height);
        for (i, p) in rgba.chunks(4).enumerate() {
            if p[3] > 0 {
                doc.layers[0].pixels.set((i as u32 % width) as i32, (i as u32 / width) as i32, crate::pixel::from_rgba8([p[0], p[1], p[2], p[3]]));
            }
        }
        doc
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
        let tiles = self.layers[layer].apply_stroke(stroke, rgb, &self.selection);
        self.push(vec![Op::Tiles { path: vec![layer], tiles }]);
    }

    /// Показывает на слое новые отпечатки незавершённого штриха. Возвращает изменившиеся плитки.
    pub fn live_update(&mut self, live: &mut LiveStroke) -> Vec<Key> {
        self.live_paint(live, false)
    }

    fn live_paint(&mut self, live: &mut LiveStroke, finish: bool) -> Vec<Key> {
        if live.stroke.is_mixing() {
            // Смешивающая кисть рисует прямо в слой: краска зависит от того, что уже на холсте.
            let sel = std::mem::take(&mut self.selection);
            let Some(layer) = self.layer_mut(&live.path) else {
                self.selection = sel;
                return Vec::new();
            };
            let preserve = layer.preserve_opacity;
            let keys = live.stroke.paint_mixed(&mut layer.pixels, live.rgb, preserve, &sel, &mut live.originals, finish);
            self.selection = sel;
            live.stroke.take_touched();
            return keys;
        }
        let touched: Vec<Key> = live.stroke.take_touched().into_iter().collect();
        let sel: HashMap<Key, Option<Tile<u16>>> = touched.iter().map(|k| (*k, self.selection.tile(*k))).collect();
        let Some(layer) = self.layer_mut(&live.path) else { return Vec::new() };
        // Плитку возвращаем к исходному виду и накладываем покрытие штриха заново целиком;
        // плитки независимы — у огромных кистей их тысячи, считаются параллельно.
        let mut jobs: Vec<(Key, Option<Tile<Px>>)> = touched
            .iter()
            .map(|key| (*key, live.originals.entry(*key).or_insert_with(|| layer.pixels.tiles.get(key).cloned()).clone()))
            .collect();
        let (erase, rgb, preserve, mask, effect) = (live.stroke.params.erase, live.rgb, layer.preserve_opacity, &live.stroke.mask, live.stroke.params.effect);
        let run = |job: &mut (Key, Option<Tile<Px>>)| {
            if let Some(m) = mask.tiles.get(&job.0) {
                let t = job.1.get_or_insert_with(|| Tile::filled(Px::default()));
                crate::layer::mask_into_tile(t, m, erase, rgb, preserve, sel[&job.0].as_ref(), effect);
            }
        };
        let threads = crate::threads();
        if jobs.len() < 32 || threads < 2 {
            jobs.iter_mut().for_each(run);
        } else {
            let chunk = jobs.len().div_ceil(threads);
            std::thread::scope(|scope| {
                for part in jobs.chunks_mut(chunk) {
                    let run = &run;
                    scope.spawn(move || part.iter_mut().for_each(run));
                }
            });
        }
        for (key, tile) in jobs {
            match tile {
                Some(t) => {
                    layer.pixels.tiles.insert(key, t);
                }
                None => {
                    layer.pixels.tiles.remove(&key);
                }
            }
        }
        touched
    }

    /// Убрать штрих, который ещё рисуется: плитки слоя — как до него, в историю ничего.
    pub fn live_cancel(&mut self, live: LiveStroke) -> Vec<Key> {
        let Some(layer) = self.layer_mut(&live.path) else { return Vec::new() };
        let keys: Vec<Key> = live.originals.keys().copied().collect();
        for (key, original) in live.originals {
            match original {
                Some(t) => {
                    layer.pixels.tiles.insert(key, t);
                }
                None => {
                    layer.pixels.tiles.remove(&key);
                }
            }
        }
        keys
    }

    /// Перо поднято: записывает штрих в историю.
    pub fn live_finish(&mut self, mut live: LiveStroke) -> Vec<Key> {
        let keys = self.live_paint(&mut live, true);
        if !live.originals.is_empty() {
            self.push(vec![Op::Tiles { path: live.path, tiles: live.originals.into_iter().collect() }]);
        }
        keys
    }

    /// Записать в журнал то, что сделали шаги `ops`, если запись идёт. `inverse` — это обратные
    /// шаги (история хранит их): `Remove` значит «вставили», `Insert` — «убрали».
    fn journal_ops(&mut self, ops: &[Op], inverse: bool) {
        if self.journal.is_none() {
            return;
        }
        let mut out = Vec::new();
        for op in ops {
            match op {
                Op::Tiles { path, tiles } => {
                    if let Some(l) = self.layer(path) {
                        out.push(Change::Tiles { uid: l.uid, keys: tiles.iter().map(|(k, _)| *k).collect() });
                    }
                }
                Op::TilesOnDisk { path, .. } | Op::Vector { path, .. } | Op::Scene { path, .. } | Op::Mask { path, .. } => {
                    if let Some(l) = self.layer(path) {
                        out.push(Change::Layer(l.uid));
                    }
                }
                Op::Remove { path } if inverse => {
                    if let Some(l) = self.layer(path) {
                        out.push(Change::Layer(l.uid));
                    }
                }
                Op::Insert { path, .. } if !inverse => {
                    if let Some(l) = self.layer(path) {
                        out.push(Change::Layer(l.uid));
                    }
                }
                Op::Canvas { .. } => out.push(Change::Canvas),
                _ => {}
            }
        }
        if let Some(j) = self.journal.as_mut() {
            j.extend(out);
        }
    }

    fn push(&mut self, ops: Vec<Op>) {
        self.journal_ops(&ops, true);
        self.push_raw(ops);
    }

    fn push_raw(&mut self, ops: Vec<Op>) {
        self.undo.push(HistoryEntry(ops));
        self.redo.clear();
        // как в SAI: не больше 100 шагов
        while self.undo.len() > HISTORY_STEPS {
            self.undo.remove(0);
        }
        // сверх бюджета памяти — старые плитки на диск, начиная с самых старых шагов
        let budget = crate::history_disk::ram_budget();
        let mut total: usize = self.undo.iter().map(HistoryEntry::bytes).sum();
        let mut i = 0;
        while total > budget && i + 1 < self.undo.len() {
            let before = self.undo[i].bytes();
            for op in self.undo[i].0.iter_mut() {
                if let Op::Tiles { path, tiles } = op {
                    if let Some(spill) = crate::history_disk::spill(tiles) {
                        *op = Op::TilesOnDisk { path: std::mem::take(path), spill };
                    }
                }
            }
            total = total - before + self.undo[i].bytes();
            i += 1;
        }
        // и общий предел памяти для того, что на диск не ушло
        while total > HISTORY_BYTES && self.undo.len() > 1 {
            total -= self.undo.remove(0).bytes();
        }
    }

    /// Сколько шагов отмены сейчас доступно.
    pub fn undo_steps(&self) -> usize {
        self.undo.len()
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
            Op::TilesOnDisk { path, spill } => {
                let tiles = crate::history_disk::load(&spill)?;
                self.apply_op(Op::Tiles { path, tiles })
            }
            Op::Insert { path, layer } => {
                let keys = layer_keys(&layer);
                let (stack, at) = self.stack_mut(&path)?;
                stack.insert(at.min(stack.len()), *layer);
                Some((Op::Remove { path }, keys))
            }
            Op::Canvas { width, height, layers, selection } => {
                let back = Op::Canvas {
                    width: self.width,
                    height: self.height,
                    layers: std::mem::replace(&mut self.layers, layers),
                    selection: std::mem::replace(&mut self.selection.mask, selection),
                };
                let mut keys: Vec<Key> = Vec::new();
                for (w, h) in [(self.width, self.height), (width, height)] {
                    for ty in 0..(h as i32 + TILE - 1) / TILE {
                        for tx in 0..(w as i32 + TILE - 1) / TILE {
                            keys.push((tx, ty));
                        }
                    }
                }
                self.width = width;
                self.height = height;
                keys.sort_unstable();
                keys.dedup();
                Some((back, keys))
            }
            Op::Vector { path, vector } => {
                let layer = self.layer_mut(&path)?;
                let old = std::mem::replace(&mut layer.vector, vector);
                Some((Op::Vector { path, vector: old }, Vec::new()))
            }
            Op::Scene { path, scene } => {
                let layer = self.layer_mut(&path)?;
                let old = std::mem::replace(&mut layer.scene3d, scene);
                Some((Op::Scene { path, scene: old }, Vec::new()))
            }
            Op::Mask { path, mask } => {
                let layer = self.layer_mut(&path)?;
                let old = std::mem::replace(&mut layer.mask, mask);
                let mut keys = layer_keys(layer);
                keys.extend(old.iter().chain(layer.mask.iter()).flat_map(|m| m.hide.tiles.keys().copied()));
                keys.sort_unstable();
                keys.dedup();
                Some((Op::Mask { path, mask: old }, keys))
            }
            Op::Selection { mask } => {
                let old = std::mem::replace(&mut self.selection.mask, mask);
                Some((Op::Selection { mask: old }, Vec::new()))
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
                // в журнал — по уже применённому шагу (его обратный — `inv`)
                self.journal_ops(std::slice::from_ref(&inv), true);
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
            // в журнал уже записано по шагам (apply_entry)
            self.push_raw(back);
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
        copy.fresh_uid();
        copy.name = format!("{} (копия)", layer.name);
        let mut at = path.to_vec();
        *at.last_mut().unwrap() += 1;
        self.insert_layer(&at, copy)
    }

    /// Очищает обычный слой (с отменой).
    /// Очищает слой; при выделении — только выделенное (как Delete в SAI).
    pub fn clear_layer(&mut self, path: &[usize]) -> Vec<Key> {
        let Some(layer) = self.layer(path) else { return Vec::new() };
        if layer.children.is_some() || !layer.paintable() {
            return Vec::new();
        }
        let tiles: TileBackup = if self.selection.is_empty() {
            layer.pixels.tiles.keys().map(|k| (*k, None)).collect()
        } else {
            let mut out = Vec::new();
            for (key, tile) in &layer.pixels.tiles {
                let Some(sel) = self.selection.tile(*key) else { continue };
                if sel.0.iter().all(|&v| v == 0) {
                    continue;
                }
                let mut t = tile.clone();
                for (px, &s) in t.0.iter_mut().zip(sel.0.iter()) {
                    let keep = ONE - s as u32;
                    for c in px.iter_mut() {
                        *c = ((*c as u32 * keep + ONE / 2) / ONE) as u16;
                    }
                }
                out.push((*key, Some(t)));
            }
            out
        };
        self.perform(vec![Op::Tiles { path: path.to_vec(), tiles }])
    }

    /// Соединяет область с выделением (`selection::Combine`) — одна запись истории.
    pub fn select(&mut self, area: &MaskMap, mode: Combine) {
        let mut next = self.selection.clone();
        next.combine(area, mode);
        if next != self.selection {
            self.perform(vec![Op::Selection { mask: next.mask }]);
        }
    }

    pub fn select_all(&mut self) {
        let all = Selection::all(self.width, self.height);
        self.select(&all, Combine::Replace);
    }

    pub fn deselect(&mut self) {
        if !self.selection.is_empty() {
            self.perform(vec![Op::Selection { mask: MaskMap::new() }]);
        }
    }

    /// Показ сдвига без записи в историю: пиксели слоя `path` и выделение берутся из исходных
    /// (`original`, `orig_sel`) и сдвигаются на (`dx`, `dy`); маска слоя (`orig_mask` — какой она
    /// была до сдвига) едет вместе с ними. Возвращает плитки для перерисовки.
    pub fn move_preview(&mut self, path: &[usize], original: &ColorMap, orig_mask: Option<&MaskMap>, orig_sel: &Selection, dx: i32, dy: i32) -> Vec<Key> {
        let moved = crate::selection::shift_pixels(original, orig_sel, dx, dy);
        let mask = orig_mask.map(|m| color_as_mask(&crate::selection::shift_pixels(&mask_as_color(m), orig_sel, dx, dy)));
        let new_sel = if orig_sel.mask.tiles.is_empty() { MaskMap::new() } else { crate::selection::shift_mask(&orig_sel.mask, dx, dy) };
        self.selection.mask = new_sel;
        self.replace_preview(path, moved, mask)
    }

    /// Показ трансформации без записи в историю (как `move_preview`): матрица `m` переводит точку
    /// исходного слоя в точку результата.
    pub fn transform_preview(&mut self, path: &[usize], original: &ColorMap, orig_mask: Option<&MaskMap>, orig_sel: &Selection, m: [f64; 6], mode: crate::resample::Mode) -> Vec<Key> {
        self.warp_preview(path, original, orig_mask, orig_sel, crate::selection::Warp::Affine(m), mode)
    }

    /// Как `transform_preview`, но с любым отображением (в том числе перспективой).
    pub fn warp_preview(&mut self, path: &[usize], original: &ColorMap, orig_mask: Option<&MaskMap>, orig_sel: &Selection, warp: crate::selection::Warp, mode: crate::resample::Mode) -> Vec<Key> {
        let (pixels, sel) = crate::selection::warp_pixels(original, orig_sel, warp, mode);
        let mask = orig_mask.map(|m| color_as_mask(&crate::selection::warp_pixels(&mask_as_color(m), orig_sel, warp, mode).0));
        self.selection.mask = sel;
        self.replace_preview(path, pixels, mask)
    }

    /// Поставить слою пиксели (и маску, если есть) без записи в историю; плитки для перерисовки.
    fn replace_preview(&mut self, path: &[usize], pixels: ColorMap, mask: Option<MaskMap>) -> Vec<Key> {
        let Some(layer) = self.layer_mut(path) else { return Vec::new() };
        let mut keys: Vec<Key> = layer.pixels.tiles.keys().copied().collect();
        keys.extend(pixels.tiles.keys().copied());
        layer.pixels = pixels;
        if let (Some(new), Some(m)) = (mask, layer.mask.as_mut()) {
            keys.extend(m.hide.tiles.keys().copied());
            keys.extend(new.tiles.keys().copied());
            m.hide = new;
        }
        keys.sort_unstable();
        keys.dedup();
        keys
    }

    /// Заменить пиксели слоя без записи в историю (показ фильтра); возвращает плитки для перерисовки.
    pub fn set_layer_pixels(&mut self, path: &[usize], pixels: ColorMap) -> Vec<Key> {
        let Some(layer) = self.layer_mut(path) else { return Vec::new() };
        let mut keys: Vec<Key> = layer.pixels.tiles.keys().copied().collect();
        keys.extend(pixels.tiles.keys().copied());
        layer.pixels = pixels;
        keys.sort_unstable();
        keys.dedup();
        keys
    }

    /// Закончить сдвиг или трансформацию: одна запись истории, отмена возвращает исходные
    /// пиксели, маску (`orig_mask`, если она двигалась) и выделение.
    pub fn move_commit(&mut self, path: &[usize], original: ColorMap, orig_mask: Option<MaskMap>, orig_sel: Selection) {
        let Some(layer) = self.layer(path) else { return };
        let mask_changed = match (&orig_mask, &layer.mask) {
            (Some(o), Some(m)) => *o != m.hide,
            _ => false,
        };
        if layer.pixels == original && self.selection == orig_sel && !mask_changed {
            return;
        }
        let mut keys: Vec<Key> = layer.pixels.tiles.keys().copied().collect();
        keys.extend(original.tiles.keys().copied());
        keys.sort_unstable();
        keys.dedup();
        let tiles: TileBackup = keys.into_iter().map(|k| (k, original.tiles.get(&k).cloned())).collect();
        let mut ops = vec![Op::Tiles { path: path.to_vec(), tiles }, Op::Selection { mask: orig_sel.mask }];
        if mask_changed {
            let flags = layer.mask.as_ref().map_or([1, 1, 1, 0], |m| m.flags);
            ops.push(Op::Mask { path: path.to_vec(), mask: orig_mask.map(|hide| crate::layer::LayerMask { hide, flags }) });
        }
        self.push(ops);
    }

    /// Маска слоя такой, какая она сейчас (для `move_preview` и `move_commit`).
    pub fn mask_snapshot(&self, path: &[usize]) -> Option<MaskMap> {
        self.layer(path)?.mask.as_ref().map(|m| m.hide.clone())
    }

    /// Сдвиг слоя (или выделенного на нём) на целые пиксели одной записью истории; маска — вместе.
    pub fn move_layer_pixels(&mut self, path: &[usize], dx: i32, dy: i32) -> Vec<Key> {
        let Some(layer) = self.layer(path) else { return Vec::new() };
        if layer.children.is_some() {
            return Vec::new();
        }
        let original = layer.pixels.clone();
        let mask = self.mask_snapshot(path);
        let orig_sel = self.selection.clone();
        let keys = self.move_preview(path, &original, mask.as_ref(), &orig_sel, dx, dy);
        self.move_commit(path, original, mask, orig_sel);
        keys
    }

    /// Новый холст той же истории: размер, слои (выделение снимается).
    fn replace_canvas(&mut self, width: u32, height: u32, layers: Vec<Layer>) -> Vec<Key> {
        self.perform(vec![Op::Canvas { width, height, layers, selection: MaskMap::new() }])
    }

    /// Отразить холст по горизонтали (`horizontal`) или вертикали.
    pub fn flip_canvas(&mut self, horizontal: bool) -> Vec<Key> {
        let (w, h) = (self.width as i32, self.height as i32);
        let mut layers = self.layers.clone();
        if horizontal {
            crate::canvas_ops::remap_layers(&mut layers, &|x, y| (w - 1 - x, y));
        } else {
            crate::canvas_ops::remap_layers(&mut layers, &|x, y| (x, h - 1 - y));
        }
        self.replace_canvas(self.width, self.height, layers)
    }

    /// Повернуть холст на 90° по часовой (`clockwise`) или против.
    pub fn rotate_canvas(&mut self, clockwise: bool) -> Vec<Key> {
        let (w, h) = (self.width as i32, self.height as i32);
        let mut layers = self.layers.clone();
        if clockwise {
            crate::canvas_ops::remap_layers(&mut layers, &|x, y| (h - 1 - y, x));
        } else {
            crate::canvas_ops::remap_layers(&mut layers, &|x, y| (y, w - 1 - x));
        }
        self.replace_canvas(self.height, self.width, layers)
    }

    /// Новый размер холста; рисунок не масштабируется, а сдвигается по привязке.
    pub fn resize_canvas(&mut self, width: u32, height: u32, anchor: (u8, u8)) -> Vec<Key> {
        let (dx, dy) = crate::canvas_ops::anchor_offset(self.width, self.height, width, height, anchor);
        self.reframe_canvas(width, height, dx, dy)
    }

    /// «Обрезать по выделению» (Trim Canvas by Selection в SAI 2017): холст — прямоугольник
    /// выделения. Без выделения ничего не делает.
    pub fn trim_to_selection(&mut self) -> Vec<Key> {
        let Some((x0, y0, x1, y1)) = self.selection.bounds() else { return Vec::new() };
        let (x0, y0) = (x0.max(0), y0.max(0));
        let (x1, y1) = (x1.min(self.width as i32), y1.min(self.height as i32));
        if x0 >= x1 || y0 >= y1 {
            return Vec::new();
        }
        self.reframe_canvas((x1 - x0) as u32, (y1 - y0) as u32, -x0, -y0)
    }

    /// Новый размер холста, рисунок сдвигается на (dx, dy); вышедшее за холст отбрасывается.
    pub fn reframe_canvas(&mut self, width: u32, height: u32, dx: i32, dy: i32) -> Vec<Key> {
        let mut layers = self.layers.clone();
        crate::canvas_ops::remap_layers(&mut layers, &|x, y| (x + dx, y + dy));
        // всё, что вышло за новый холст, отбрасывается
        let (w, h) = (width as i32, height as i32);
        fn crop(layers: &mut [Layer], w: i32, h: i32) {
            for l in layers {
                match l.children.as_mut() {
                    Some(ch) => crop(ch, w, h),
                    None => {
                        let keys: Vec<Key> = l.pixels.tiles.keys().copied().collect();
                        for k in keys {
                            let t = l.pixels.tile_mut(k);
                            for (i, p) in t.0.iter_mut().enumerate() {
                                let (x, y) = (k.0 * TILE + i as i32 % TILE, k.1 * TILE + i as i32 / TILE);
                                if x < 0 || y < 0 || x >= w || y >= h {
                                    *p = [0; 4];
                                }
                            }
                        }
                        l.pixels.tiles.retain(|_, t| t.0.iter().any(|p| p[3] != 0));
                        if let Some(m) = l.mask.as_mut() {
                            let keys: Vec<Key> = m.hide.tiles.keys().copied().collect();
                            for k in keys {
                                let t = m.hide.tile_mut(k);
                                for (i, v) in t.0.iter_mut().enumerate() {
                                    let (x, y) = (k.0 * TILE + i as i32 % TILE, k.1 * TILE + i as i32 / TILE);
                                    if x < 0 || y < 0 || x >= w || y >= h {
                                        *v = 0;
                                    }
                                }
                            }
                            m.hide.tiles.retain(|_, t| t.0.iter().any(|&v| v != 0));
                        }
                    }
                }
            }
        }
        crop(&mut layers, w, h);
        self.replace_canvas(width, height, layers)
    }

    /// Новое разрешение холста: все слои пересэмплируются способом `mode`.
    pub fn resample_canvas(&mut self, width: u32, height: u32, mode: crate::resample::Mode) -> Vec<Key> {
        let mut layers = self.layers.clone();
        crate::canvas_ops::resample_layers(&mut layers, self.width, self.height, width, height, mode);
        self.replace_canvas(width, height, layers)
    }

    /// Повернуть слой на 90° (`cw` — по часовой) вокруг середины его рисунка, как «Rotate Layer
    /// 90deg.» SAI 2017 (с выделением — выделенное, вокруг его середины). Середина — целая точка,
    /// чтобы пиксели легли на сетку.
    pub fn rotate_layer(&mut self, path: &[usize], cw: bool) -> Vec<Key> {
        let Some(layer) = self.layer(path) else { return Vec::new() };
        if layer.children.is_some() {
            return Vec::new();
        }
        let sel = self.selection.clone();
        let bounds = if sel.mask.tiles.is_empty() { crate::selection::content_bounds(&layer.pixels) } else { sel.bounds() };
        let Some((x0, y0, x1, y1)) = bounds else { return Vec::new() };
        let (cx, cy) = (((x0 + x1) / 2) as f64, ((y0 + y1) / 2) as f64);
        // по часовой (y вниз): (dx, dy) → (−dy, dx)
        let m = if cw { [0.0, -1.0, cx + cy, 1.0, 0.0, cy - cx] } else { [0.0, 1.0, cx - cy, -1.0, 0.0, cx + cy] };
        let original = layer.pixels.clone();
        let mask = self.mask_snapshot(path);
        let keys = self.transform_preview(path, &original, mask.as_ref(), &sel, m, crate::resample::Mode::Nearest);
        self.move_commit(path, original, mask, sel);
        keys
    }

    /// «Слить видимые» (Merge Visible Layers в SAI 2017): видимые слои верхнего уровня сводятся в
    /// новый слой `name` на месте самого верхнего из них; скрытые остаются. Возвращает его номер.
    pub fn merge_visible(&mut self, name: &str) -> Option<(usize, Vec<Key>)> {
        let top = self.layers.iter().rposition(|l| l.visible)?;
        let mut merged = Layer::new(name);
        let (tw, th) = ((self.width as i32 + TILE - 1) / TILE, (self.height as i32 + TILE - 1) / TILE);
        for ty in 0..th {
            for tx in 0..tw {
                let mut buf = vec![[0f32; 4]; TILE_LEN];
                render_group(&self.layers, (tx, ty), &mut buf, self.size(), self.fx());
                if buf.iter().any(|p| p[3] > 0.0) {
                    merged.pixels.tiles.insert((tx, ty), Tile(buf.iter().map(|p| from_f32(*p)).collect()));
                }
            }
        }
        let mut layers = Vec::new();
        let mut index = 0;
        for (i, l) in self.layers.iter().enumerate() {
            if !l.visible {
                layers.push(l.clone());
            }
            if i == top {
                index = layers.len();
                layers.push(merged.clone());
            }
        }
        let selection = self.selection.mask.clone();
        let keys = self.perform(vec![Op::Canvas { width: self.width, height: self.height, layers, selection }]);
        Some((index, keys))
    }

    /// Отразить слой `path` (с выделением — только выделенное, вокруг его середины).
    pub fn flip_layer(&mut self, path: &[usize], horizontal: bool) -> Vec<Key> {
        let Some(layer) = self.layer(path) else { return Vec::new() };
        if layer.children.is_some() {
            return Vec::new();
        }
        let sel = self.selection.clone();
        let (x0, y0, x1, y1) = if sel.mask.tiles.is_empty() {
            (0, 0, self.width as i32, self.height as i32)
        } else {
            sel.bounds().unwrap_or((0, 0, self.width as i32, self.height as i32))
        };
        let m = if horizontal {
            [-1.0, 0.0, (x0 + x1) as f64, 0.0, 1.0, 0.0]
        } else {
            [1.0, 0.0, 0.0, 0.0, -1.0, (y0 + y1) as f64]
        };
        let original = layer.pixels.clone();
        let mask = self.mask_snapshot(path);
        let keys = self.transform_preview(path, &original, mask.as_ref(), &sel, m, crate::resample::Mode::Nearest);
        self.move_commit(path, original, mask, sel);
        keys
    }

    /// «Обводка» слоя, как в SAI 2017 (сверено с живой SAI, docs/31-gradient-selection.md):
    /// основной цвет поверх слоя с долей «расширенная на `outside` непрозрачность минус суженная на
    /// `inside`». Снаружи — (N, 0), внутри — (0, N), по центру — (⌊N/2⌋, ⌈N/2⌉). `canvas_edges` —
    /// «Apply to Canvas Edges too»: за краем холста прозрачно, иначе непрозрачно (у края нет обводки).
    pub fn outline(&mut self, path: &[usize], outside: u32, inside: u32, canvas_edges: bool, rgb: [u8; 3]) -> Vec<Key> {
        let Some(layer) = self.layer(path) else { return Vec::new() };
        if layer.children.is_some() || !layer.paintable() {
            return Vec::new();
        }
        let tiles: TileBackup = self.outline_tiles(&layer.pixels, layer.preserve_opacity, outside, inside, canvas_edges, rgb).into_iter().map(|(k, t)| (k, Some(t))).collect();
        let keys = tiles.iter().map(|(k, _)| *k).collect();
        if !tiles.is_empty() {
            self.perform(vec![Op::Tiles { path: path.to_vec(), tiles }]);
        }
        keys
    }

    /// Плитки слоя `src` после обводки (для предпросмотра и для `outline`).
    pub fn outline_tiles(&self, src: &ColorMap, preserve: bool, outside: u32, inside: u32, canvas_edges: bool, rgb: [u8; 3]) -> Vec<(Key, Tile<Px>)> {
        let Some((cx0, cy0, cx1, cy1)) = crate::selection::content_bounds(src) else { return Vec::new() };
        let pad = outside as i32 + 1;
        let (x0, y0) = ((cx0 - pad).max(0), (cy0 - pad).max(0));
        let (x1, y1) = ((cx1 + pad).min(self.width as i32), (cy1 + pad).min(self.height as i32));
        if x0 >= x1 || y0 >= y1 {
            return Vec::new();
        }
        let (bw, bh) = ((x1 - x0) as usize, (y1 - y0) as usize);
        let alpha: Vec<u16> = (0..bw * bh).map(|i| src.get(x0 + (i % bw) as i32, y0 + (i / bw) as i32)[3]).collect();
        let grown = crate::selection::morph(alpha.clone(), bw, bh, outside, true, 0);
        // за краем карты, но внутри холста — пустой запас (pad), так что `edge` действует только у края холста
        let edge = if canvas_edges { 0 } else { ONE as u16 };
        let shrunk = crate::selection::morph(alpha, bw, bh, inside, false, edge);
        let mut tiles: std::collections::HashMap<Key, Tile<Px>> = std::collections::HashMap::new();
        for (i, (&g, &s)) in grown.iter().zip(&shrunk).enumerate() {
            let band = g.saturating_sub(s) as u32;
            if band == 0 {
                continue;
            }
            let (x, y) = (x0 + (i % bw) as i32, y0 + (i / bw) as i32);
            let m = (band * self.selection.amount(x, y) + ONE / 2) / ONE;
            if m == 0 {
                continue;
            }
            let key = crate::tile::key_of(x, y);
            let t = tiles.entry(key).or_insert_with(|| src.tiles.get(&key).cloned().unwrap_or_else(|| Tile::filled([0; 4])));
            let j = crate::tile::index_of(x, y);
            t.0[j] = blend_paint(t.0[j], rgb, m, preserve);
        }
        tiles.into_iter().collect()
    }

    /// Залить слой цветом `rgb` в пределах `area` (доля 0..=ONE) и выделения — одна запись истории.
    /// С «сохранять непрозрачность» меняется только цвет.
    pub fn fill(&mut self, path: &[usize], area: &MaskMap, rgb: [u8; 3]) -> Vec<Key> {
        self.fill_with(path, area, rgb, true)
    }

    /// То же; `clip` — только в пределах выделения (иначе выделение не учитывается).
    pub fn fill_with(&mut self, path: &[usize], area: &MaskMap, rgb: [u8; 3], clip: bool) -> Vec<Key> {
        let Some(layer) = self.layer(path) else { return Vec::new() };
        if layer.children.is_some() || !layer.paintable() {
            return Vec::new();
        }
        let mut tiles: TileBackup = Vec::new();
        for (key, a) in &area.tiles {
            let sel = if clip { self.selection.tile(*key) } else { None };
            let mut t = layer.pixels.tiles.get(key).cloned().unwrap_or_else(|| Tile::filled([0; 4]));
            let mut any = false;
            for (i, px) in t.0.iter_mut().enumerate() {
                let m = match &sel {
                    Some(s) => crate::selection::limit(a.0[i] as u32, s.0[i]),
                    None => a.0[i] as u32,
                };
                if m == 0 {
                    continue;
                }
                any = true;
                *px = blend_paint(*px, rgb, m, layer.preserve_opacity);
            }
            if any {
                tiles.push((*key, Some(t)));
            }
        }
        let keys = tiles.iter().map(|(k, _)| *k).collect();
        if !tiles.is_empty() {
            self.perform(vec![Op::Tiles { path: path.to_vec(), tiles }]);
        }
        keys
    }

    /// Откуда палочка и заливка берут цвет: сведённые слои с «Источником выбора», а если таких
    /// нет — пиксели слоя `path`.
    pub fn pick_source(&self, path: &[usize]) -> ColorMap {
        fn only_sources(layers: &[Layer]) -> (Vec<Layer>, bool) {
            let mut any = false;
            let out = layers
                .iter()
                .map(|l| {
                    let mut c = l.clone();
                    match &l.children {
                        Some(ch) => {
                            let (inner, a) = only_sources(ch);
                            any |= a || l.selection_source;
                            if !l.selection_source {
                                c.children = Some(inner);
                            }
                        }
                        None => {
                            any |= l.selection_source;
                            if !l.selection_source {
                                c.visible = false;
                            }
                        }
                    }
                    c
                })
                .collect();
            (out, any)
        }
        let (layers, any) = only_sources(&self.layers);
        if !any {
            return self.layer(path).map(|l| l.pixels.clone()).unwrap_or_default();
        }
        let mut out = ColorMap::new();
        let (tw, th) = ((self.width as i32 + TILE - 1) / TILE, (self.height as i32 + TILE - 1) / TILE);
        for ty in 0..th {
            for tx in 0..tw {
                let mut buf = vec![[0f32; 4]; TILE_LEN];
                render_group(&layers, (tx, ty), &mut buf, self.size(), self.fx());
                if buf.iter().any(|p| p[3] > 0.0) {
                    out.tiles.insert((tx, ty), Tile(buf.iter().map(|p| from_f32(*p)).collect()));
                }
            }
        }
        out
    }

    /// Сведённое изображение всех видимых слоёв (без фона холста) — плитками.
    pub fn merged_pixels(&self) -> ColorMap {
        let mut out = ColorMap::new();
        let (tw, th) = ((self.width as i32 + TILE - 1) / TILE, (self.height as i32 + TILE - 1) / TILE);
        for ty in 0..th {
            for tx in 0..tw {
                let mut buf = vec![[0f32; 4]; TILE_LEN];
                render_group(&self.layers, (tx, ty), &mut buf, self.size(), self.fx());
                if buf.iter().any(|p| p[3] > 0.0) {
                    out.tiles.insert((tx, ty), Tile(buf.iter().map(|p| from_f32(*p)).collect()));
                }
            }
        }
        out
    }

    /// «Яркость в прозрачность»: белое становится прозрачным, тёмное остаётся, цвет линий
    /// сохраняется (как «цвет в альфу» по белому). С выделением — только выделенное.
    pub fn luminance_to_alpha(&mut self, path: &[usize]) -> Vec<Key> {
        let Some(layer) = self.layer(path) else { return Vec::new() };
        if layer.children.is_some() || !layer.paintable() {
            return Vec::new();
        }
        let mut tiles: TileBackup = Vec::new();
        for (key, t) in &layer.pixels.tiles {
            let sel = self.selection.tile(*key);
            let mut nt = t.clone();
            for (i, px) in nt.0.iter_mut().enumerate() {
                let s = sel.as_ref().map_or(ONE, |s| s.0[i] as u32);
                if s == 0 || px[3] == 0 {
                    continue;
                }
                let f = to_f32(*px);
                let a = f[3];
                let c = [f[0] / a, f[1] / a, f[2] / a];
                // новая альфа — насколько цвет темнее белого; цвет — такой, чтобы поверх белого было то же
                let na = (1.0 - c[0]).max(1.0 - c[1]).max(1.0 - c[2]);
                let out = if na <= 1e-6 {
                    [0.0; 4]
                } else {
                    let nc = c.map(|v| ((v - (1.0 - na)) / na).clamp(0.0, 1.0));
                    let aa = a * na;
                    [nc[0] * aa, nc[1] * aa, nc[2] * aa, aa]
                };
                let k = s as f32 / ONE as f32;
                let mixed = [0, 1, 2, 3].map(|c| f[c] + (out[c] - f[c]) * k);
                *px = from_f32(mixed);
            }
            tiles.push((*key, Some(nt)));
        }
        let keys = tiles.iter().map(|(k, _)| *k).collect();
        if !tiles.is_empty() {
            self.perform(vec![Op::Tiles { path: path.to_vec(), tiles }]);
        }
        keys
    }

    /// Новые кривые слоя и их пиксели — одна запись истории.
    pub fn set_vector_layer(&mut self, path: &[usize], vector: crate::layer::VectorData, pixels: ColorMap) -> Vec<Key> {
        let Some(layer) = self.layer(path) else { return Vec::new() };
        let mut keys: Vec<Key> = layer.pixels.tiles.keys().copied().collect();
        keys.extend(pixels.tiles.keys().copied());
        keys.sort_unstable();
        keys.dedup();
        let tiles: TileBackup = keys.iter().map(|k| (*k, pixels.tiles.get(k).cloned())).collect();
        self.perform(vec![Op::Vector { path: path.to_vec(), vector: Some(vector) }, Op::Tiles { path: path.to_vec(), tiles }])
    }

    /// Новая сцена слоя 3D и её пиксели — одна запись истории.
    pub fn set_scene_layer(&mut self, path: &[usize], scene: crate::layer::Scene3dData, pixels: ColorMap) -> Vec<Key> {
        let Some(layer) = self.layer(path) else { return Vec::new() };
        let mut keys: Vec<Key> = layer.pixels.tiles.keys().copied().collect();
        keys.extend(pixels.tiles.keys().copied());
        keys.sort_unstable();
        keys.dedup();
        let tiles: TileBackup = keys.iter().map(|k| (*k, pixels.tiles.get(k).cloned())).collect();
        self.perform(vec![Op::Scene { path: path.to_vec(), scene: Some(Box::new(scene)) }, Op::Tiles { path: path.to_vec(), tiles }])
    }

    /// Сцена и пиксели слоя 3D без записи в историю (живая правка: камера, поза). Итог — потом
    /// `set_scene_layer` от прежнего состояния.
    pub fn set_scene_quiet(&mut self, path: &[usize], scene: Option<crate::layer::Scene3dData>, pixels: ColorMap) -> Vec<Key> {
        let Some(layer) = self.layer_mut(path) else { return Vec::new() };
        let mut keys: Vec<Key> = layer.pixels.tiles.keys().copied().collect();
        keys.extend(pixels.tiles.keys().copied());
        keys.sort_unstable();
        keys.dedup();
        layer.scene3d = scene.map(Box::new);
        layer.pixels = pixels;
        keys
    }

    /// Вставить слой без записи в историю — для живой правки (итог потом записывается обычным
    /// шагом, а этот снимается `remove_layer_quiet`).
    pub fn insert_layer_quiet(&mut self, path: &[usize], layer: Layer) -> Vec<Key> {
        self.apply_op(Op::Insert { path: path.to_vec(), layer: Box::new(layer) }).map_or(Vec::new(), |(_, k)| k)
    }

    /// Убрать слой без записи в историю.
    pub fn remove_layer_quiet(&mut self, path: &[usize]) -> (Option<Layer>, Vec<Key>) {
        match self.apply_op(Op::Remove { path: path.to_vec() }) {
            Some((Op::Insert { layer, .. }, k)) => (Some(*layer), k),
            _ => (None, Vec::new()),
        }
    }

    /// Заменить кривые и пиксели слоя без записи в историю (живая правка надписи).
    pub fn set_vector_quiet(&mut self, path: &[usize], vector: Option<crate::layer::VectorData>, pixels: ColorMap) -> Vec<Key> {
        let Some(layer) = self.layer_mut(path) else { return Vec::new() };
        let mut keys: Vec<Key> = layer.pixels.tiles.keys().copied().collect();
        keys.extend(pixels.tiles.keys().copied());
        keys.sort_unstable();
        keys.dedup();
        layer.vector = vector;
        layer.pixels = pixels;
        keys
    }

    /// Растеризовать слой контура или фигур: кривые отбрасываются, пиксели остаются; отмена —
    /// одним шагом.
    pub fn rasterize_layer(&mut self, path: &[usize]) -> Vec<Key> {
        if self.layer(path).is_none_or(|l| l.vector.is_none() && l.scene3d.is_none()) {
            return Vec::new();
        }
        let mut layers = self.layers.clone();
        fn at<'a>(ls: &'a mut [Layer], path: &[usize]) -> Option<&'a mut Layer> {
            let (first, rest) = path.split_first()?;
            let l = ls.get_mut(*first)?;
            if rest.is_empty() {
                Some(l)
            } else {
                at(l.children.as_deref_mut()?, rest)
            }
        }
        if let Some(l) = at(&mut layers, path) {
            l.vector = None;
            l.scene3d = None;
        }
        let selection = self.selection.mask.clone();
        self.perform(vec![Op::Canvas { width: self.width, height: self.height, layers, selection }])
    }

    /// «Свести всё»: все слои в один обычный слой (фон холста не входит).
    pub fn flatten_layers(&mut self) -> Vec<Key> {
        let mut layer = Layer::new("Слой 1");
        let (tw, th) = ((self.width as i32 + TILE - 1) / TILE, (self.height as i32 + TILE - 1) / TILE);
        for ty in 0..th {
            for tx in 0..tw {
                let mut buf = vec![[0f32; 4]; TILE_LEN];
                render_group(&self.layers, (tx, ty), &mut buf, self.size(), self.fx());
                if buf.iter().any(|p| p[3] > 0.0) {
                    layer.pixels.tiles.insert((tx, ty), Tile(buf.iter().map(|p| from_f32(*p)).collect()));
                }
            }
        }
        let selection = self.selection.mask.clone();
        self.perform(vec![Op::Canvas { width: self.width, height: self.height, layers: vec![layer], selection }])
    }

    /// Сдвинуть только рамку выделения (Nudge в SAI при выделении) — одна запись истории.
    pub fn nudge_selection(&mut self, dx: i32, dy: i32) {
        if self.selection.is_empty() || (dx, dy) == (0, 0) {
            return;
        }
        let mask = crate::selection::shift_mask(&self.selection.mask, dx, dy);
        self.perform(vec![Op::Selection { mask }]);
    }

    /// Есть ли на слое `path` непрозрачные точки внутри выделения (без выделения — на всём слое).
    pub fn has_pixels_in_selection(&self, path: &[usize]) -> bool {
        let Some(layer) = self.layer(path) else { return false };
        let all = self.selection.mask.tiles.is_empty();
        layer.pixels.tiles.iter().any(|(key, t)| {
            if all {
                return t.0.iter().any(|p| p[3] > 0);
            }
            let Some(s) = self.selection.mask.tiles.get(key) else { return false };
            t.0.iter().zip(s.0.iter()).any(|(p, &m)| p[3] > 0 && m > 0)
        })
    }

    /// Расширить или сузить выделение на 1 px.
    pub fn grow_selection(&mut self, grow: bool) {
        let mut next = self.selection.clone();
        next.grow(grow);
        if next != self.selection {
            self.perform(vec![Op::Selection { mask: next.mask }]);
        }
    }

    /// Расширить или сузить выделение на `n` px (как в SAI: углы срезаются под 45°).
    pub fn grow_selection_by(&mut self, n: u32, grow: bool) {
        let mut next = self.selection.clone();
        next.grow_by(n, grow);
        if next != self.selection {
            self.perform(vec![Op::Selection { mask: next.mask }]);
        }
    }

    pub fn invert_selection(&mut self) {
        let mut next = self.selection.clone();
        next.invert(self.width, self.height);
        self.perform(vec![Op::Selection { mask: next.mask }]);
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

    /// Переносит слой в стопку `parent` на место `index` (номер считается до переноса, как
    /// видно в списке; +1 — выше), в том числе в другую папку (с отменой). Возвращает новый путь.
    pub fn move_layer_to(&mut self, from: &[usize], parent: &[usize], index: usize) -> Option<(Vec<usize>, Vec<Key>)> {
        let (last, from_parent) = from.split_last()?;
        if parent.starts_with(from) {
            return None; // папку в саму себя
        }
        let len = if parent.is_empty() { self.layers.len() } else { self.layer(parent)?.children.as_ref()?.len() };
        if index > len {
            return None;
        }
        // после изъятия слоя номера за ним в той же стопке сдвигаются на один
        let mut dst = parent.to_vec();
        let n = from_parent.len();
        if dst.len() > n && dst.starts_with(from_parent) && dst[n] > *last {
            dst[n] -= 1;
        }
        let at = if parent == from_parent && index > *last { index - 1 } else { index };
        dst.push(at);
        if dst == from {
            return None;
        }
        let layer = self.layer(from)?.clone();
        let keys = self.perform(vec![Op::Remove { path: from.to_vec() }, Op::Insert { path: dst.clone(), layer: Box::new(layer) }]);
        Some((dst, keys))
    }

    /// Задать или убрать маску слоя (с отменой).
    pub fn set_mask(&mut self, path: &[usize], mask: Option<crate::layer::LayerMask>) -> Vec<Key> {
        if self.layer(path).is_none() {
            return Vec::new();
        }
        self.perform(vec![Op::Mask { path: path.to_vec(), mask }])
    }

    /// Новая маска слоя: пустая, слой виден целиком (как в SAI).
    pub fn new_mask(&mut self, path: &[usize]) -> Vec<Key> {
        if self.layer(path).is_none_or(|l| l.mask.is_some()) {
            return Vec::new();
        }
        self.set_mask(path, Some(crate::layer::LayerMask { hide: MaskMap::new(), flags: [1, 1, 1, 0] }))
    }

    /// Применить маску: скрытое по маске стирается из пикселей слоя, маска убирается (с отменой).
    /// Только у обычных слоёв.
    pub fn apply_mask(&mut self, path: &[usize]) -> Vec<Key> {
        let Some(layer) = self.layer(path) else { return Vec::new() };
        let Some(mask) = &layer.mask else { return Vec::new() };
        if layer.children.is_some() {
            return Vec::new();
        }
        let mut pixels = layer.pixels.clone();
        for (key, tile) in pixels.tiles.iter_mut() {
            if let Some(m) = mask.hide.tiles.get(key) {
                for (px, &h) in tile.0.iter_mut().zip(m.0.iter()) {
                    let k = 0x4000 - h.min(0x4000) as u32;
                    *px = px.map(|c| ((c as u32 * k + 0x2000) >> 14) as u16);
                }
            }
        }
        let tiles: TileBackup = pixels.tiles.into_iter().map(|(k, t)| (k, Some(t))).collect();
        self.perform(vec![Op::Tiles { path: path.to_vec(), tiles }, Op::Mask { path: path.to_vec(), mask: None }])
    }

    /// Мазок в маске без записи в историю (показ, пока перо не поднято): `orig` — маска до мазка,
    /// `cover` — покрытие мазка (0..=ONE), `target` — значение маски, к которому ведёт мазок
    /// (0 — показать, 16384 — скрыть). Возвращает изменившиеся плитки.
    pub fn mask_stroke_preview(&mut self, path: &[usize], orig: &MaskMap, cover: &MaskMap, target: u16) -> Vec<Key> {
        let Some(mask) = self.layer_mut(path).and_then(|l| l.mask.as_mut()) else { return Vec::new() };
        let target = target.min(0x4000) as i32;
        let mut keys = Vec::new();
        for (key, c) in &cover.tiles {
            let base = orig.tiles.get(key);
            let tile = mask.hide.tile_mut(*key);
            for (i, &cv) in c.0.iter().enumerate() {
                let o = base.map_or(0, |t| t.0[i]) as i32;
                tile.0[i] = (o + ((target - o) * cv as i32 + (ONE as i32) / 2) / ONE as i32).clamp(0, 0x4000) as u16;
            }
            keys.push(*key);
        }
        keys
    }

    /// Записать мазок в маске в историю: `orig` — маска до мазка.
    pub fn mask_stroke_commit(&mut self, path: &[usize], orig: crate::layer::LayerMask) {
        if self.layer(path).is_some() {
            self.push(vec![Op::Mask { path: path.to_vec(), mask: Some(orig) }]);
        }
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
        // слой 3D рисует сам себя: в него не сливают, и «перенести» его нельзя (сцена осталась бы)
        if upper.children.is_some() || lower.children.is_some() || lower.scene3d.is_some() || (keep_upper && upper.scene3d.is_some()) {
            return None;
        }
        // У нижнего слоя с маской маска сначала применяется (как «Применить маску»), иначе она
        // легла бы и на рисунок верхнего; у верхнего маска учитывается при наложении. Итоговый
        // нижний слой — без маски.
        let lower_masked = lower.mask.is_some();
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
        if let Some(m) = &lower.mask {
            keys.extend(m.hide.tiles.keys().copied());
        }
        keys.sort_unstable();
        keys.dedup();
        let mut tiles: TileBackup = Vec::with_capacity(keys.len());
        for key in &keys {
            let mut buf = vec![[0f32; 4]; TILE_LEN];
            render_group(&pair, *key, &mut buf, self.size(), self.fx());
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
        let mut ops = vec![Op::Tiles { path: below.clone(), tiles }];
        if lower_masked {
            ops.push(Op::Mask { path: below.clone(), mask: None });
        }
        ops.push(second);
        let k = self.perform(ops);
        Some((below, k))
    }

    /// Что делал шаг истории — для окна истории. В шаге отмены лежит обратная операция (возврат
    /// слоя — значит, его удалили), в шаге повтора — прямая (`forward`).
    fn step_info(&self, e: &HistoryEntry, forward: bool) -> StepInfo {
        let name = |path: &[usize]| self.layer(path).map(|l| l.name.clone());
        let mut kind = StepKind::Other;
        let mut layer = None;
        for op in &e.0 {
            let (k, path): (StepKind, Option<&[usize]>) = match op {
                Op::Tiles { path, .. } | Op::TilesOnDisk { path, .. } => (StepKind::Paint, Some(path)),
                Op::Insert { layer: l, .. } => {
                    layer = layer.or(Some(l.name.clone()));
                    (if forward { StepKind::NewLayer } else { StepKind::DeleteLayer }, None)
                }
                Op::Remove { path } => (if forward { StepKind::DeleteLayer } else { StepKind::NewLayer }, Some(path)),
                Op::Selection { .. } => (StepKind::Selection, None),
                Op::Vector { path, .. } => (StepKind::Vector, Some(path)),
                Op::Scene { path, .. } => (StepKind::Scene3d, Some(path)),
                Op::Mask { path, .. } => (StepKind::Mask, Some(path)),
                Op::Canvas { .. } => (StepKind::Canvas, None),
            };
            // главное действие шага: холст и слои важнее рисования и выделения
            if kind == StepKind::Other || (k as u8) < (kind as u8) {
                kind = k;
            }
            if layer.is_none() {
                layer = path.and_then(name);
            }
        }
        StepInfo { kind, layer }
    }

    /// Шаги отмены (самый старый первым) и повтора (ближайший первым).
    pub fn history_steps(&self) -> (Vec<StepInfo>, Vec<StepInfo>) {
        (self.undo.iter().map(|e| self.step_info(e, false)).collect(), self.redo.iter().rev().map(|e| self.step_info(e, true)).collect())
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    fn size(&self) -> (i32, i32) {
        (self.width as i32, self.height as i32)
    }

    /// Задать фон холста кодом SAI (`bg_code`) и цветом; неизвестный код — белый непрозрачный.
    pub fn set_background(&mut self, code: u8, color: [u8; 3]) {
        let (opaque, kind) = (code & 0xf != 0, code >> 4);
        let ok = if opaque { kind <= 2 } else { kind <= 4 };
        self.bg_code = if ok { code } else { 0x01 };
        self.bg_color = color;
        self.background = match (self.bg_code & 0xf != 0, self.bg_code >> 4) {
            (false, _) => None,
            (true, 1) => Some([0, 0, 0]),
            (true, 2) => Some(color),
            _ => Some([255, 255, 255]),
        };
    }

    /// Чем показывается прозрачный холст в точке (x, y), как в SAI: шашка клетками 16 px или цвет.
    fn display_bg(&self, x: i32, y: i32) -> [f32; 4] {
        let g = |v: f32| [v, v, v, 1.0];
        match self.bg_code >> 4 {
            k @ (0 | 1) => {
                let odd = ((x >> 4) + (y >> 4)) & 1 == 1;
                // светлая: 0x4000 и 0x3800 из 0x4000; тёмная: 0x2000 и 0x1800
                let (a, b) = if k == 0 { (1.0, 0.875) } else { (0.5, 0.375) };
                g(if odd { b } else { a })
            }
            2 => g(1.0),
            3 => g(0.0),
            _ => [self.bg_color[0] as f32 / 255.0, self.bg_color[1] as f32 / 255.0, self.bg_color[2] as f32 / 255.0, 1.0],
        }
    }

    /// Цвет эффектов слоя (номер для `effects`/`paper`).
    pub fn fx(&self) -> u8 {
        crate::paper::fx_of(&self.effect_color)
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
        if self.background.is_none() {
            // прозрачный холст на экране — шашкой или цветом, как выбрано в «Фоне холста»
            for (i, b) in buf.iter_mut().enumerate() {
                *b = self.display_bg(key.0 * TILE + i as i32 % TILE, key.1 * TILE + i as i32 / TILE);
            }
        }
        render_group(&self.layers, key, &mut buf, self.size(), self.fx());
        let mut out = vec![0u8; TILE_LEN * 4];
        for (px, o) in buf.iter().zip(out.as_chunks_mut::<4>().0) {
            *o = to_rgba8(from_f32(*px));
        }
        out
    }

    /// Сводит все видимые слои в RGBA по 8 бит на канал (без предумножения).
    pub fn flatten(&self) -> Vec<u8> {
        self.flatten_on(self.background_f32())
    }

    /// Сведение без подложки фона (прозрачное остаётся прозрачным).
    pub fn flatten_clear(&self) -> Vec<u8> {
        self.flatten_on([0.0; 4])
    }

    /// Сводит все видимые слои в RGBA по 16 бит на канал (без предумножения), с подложкой фона.
    pub fn flatten16(&self) -> Vec<u16> {
        self.flatten_into(self.background_f32(), crate::pixel::f32_to_rgba16)
    }

    /// Сведение без подложки фона по 16 бит на канал (без предумножения).
    pub fn flatten_clear16(&self) -> Vec<u16> {
        self.flatten_into([0.0; 4], crate::pixel::f32_to_rgba16)
    }

    fn flatten_on(&self, bg: [f32; 4]) -> Vec<u8> {
        self.flatten_into(bg, |c| to_rgba8(from_f32(c)))
    }

    fn flatten_into<T: Copy + Default + Send>(&self, bg: [f32; 4], conv: impl Fn([f32; 4]) -> [T; 4] + Sync) -> Vec<T> {
        let (w, h) = (self.width as i32, self.height as i32);
        let mut out = vec![T::default(); (w * h * 4) as usize];
        let rows = (h + TILE - 1) / TILE;
        let cols = (w + TILE - 1) / TILE;
        let row_bytes = (w * 4 * TILE) as usize;
        // полосы по высоте плитки — каждая в своём потоке
        let threads = crate::threads();
        let next = std::sync::atomic::AtomicI32::new(0);
        let strips: Vec<std::sync::Mutex<&mut [T]>> = out.chunks_mut(row_bytes).map(std::sync::Mutex::new).collect();
        std::thread::scope(|scope| {
            for _ in 0..threads.min(rows.max(1) as usize) {
                scope.spawn(|| {
                    let mut buf = vec![bg; TILE_LEN];
                    loop {
                        let ty = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if ty >= rows {
                            break;
                        }
                        let mut strip = strips[ty as usize].lock().unwrap();
                        for tx in 0..cols {
                            buf.fill(bg);
                            render_group(&self.layers, (tx, ty), &mut buf, self.size(), self.fx());
                            for y in 0..TILE {
                                if ty * TILE + y >= h {
                                    break;
                                }
                                for x in 0..TILE {
                                    let px = tx * TILE + x;
                                    if px >= w {
                                        break;
                                    }
                                    let c = conv(buf[(y * TILE + x) as usize]);
                                    let o = ((y * w + px) * 4) as usize;
                                    strip[o..o + 4].copy_from_slice(&c);
                                }
                            }
                        }
                    }
                });
            }
        });
        drop(strips);
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
fn layer_source(layer: &Layer, key: Key, size: (i32, i32), fx: u8) -> Option<Buf> {
    let mut buf = layer_content(layer, key, size, fx)?;
    if let Some(tile) = layer.mask.as_ref().and_then(|m| m.hide.tiles.get(&key)) {
        for (px, &m) in buf.iter_mut().zip(tile.0.iter()) {
            let k = 1.0 - m.min(0x4000) as f32 / 16384.0;
            *px = px.map(|v| v * k);
        }
    }
    Some(buf)
}

/// Содержимое слоя или папки без маски.
fn layer_content(layer: &Layer, key: Key, size: (i32, i32), fx: u8) -> Option<Buf> {
    match &layer.children {
        Some(children) => {
            if !children.iter().any(|c| c.visible && touches(c, key)) {
                return None;
            }
            let mut buf = vec![[0.0f32; 4]; TILE_LEN];
            render_group(children, key, &mut buf, size, fx);
            Some(buf)
        }
        None => layer.pixels.tiles.get(&key).map(|t| t.0.iter().map(|p| to_f32(*p)).collect()),
    }
}

/// Бумага слоя на его плитку.
fn apply_paper(layer: &Layer, key: Key, buf: &mut [[f32; 4]], fx: u8) {
    let Some(paper) = &layer.paper else { return };
    let Some(tex) = &paper.texture else { return };
    if paper.strength > 0 {
        let vals = crate::paper::sample(tex, paper.scale, key.0 * TILE, key.1 * TILE, TILE as usize, TILE as usize);
        crate::paper::apply(buf, &vals, paper.strength, fx);
    }
}

/// Накладывает стопку слоёв (снизу вверх) на `backdrop` в пределах одной плитки.
/// `size` — размер холста (кайма за краем холста продолжает крайние точки, как в SAI).
/// Маска как картинка (значение — во всех каналах), чтобы сдвигать и трансформировать её теми
/// же функциями, что и пиксели слоя. 0x4000 у маски — это ONE у пикселя.
fn mask_as_color(m: &MaskMap) -> ColorMap {
    let mut out = ColorMap::new();
    for (k, t) in &m.tiles {
        let tile = out.tile_mut(*k);
        for (p, &v) in tile.0.iter_mut().zip(t.0.iter()) {
            let a = (v.min(0x4000) as u32 * 2).min(ONE) as u16;
            *p = [a, a, a, a];
        }
    }
    out.tiles.retain(|_, t| t.0.iter().any(|p| p[3] != 0));
    out
}

fn color_as_mask(c: &ColorMap) -> MaskMap {
    let mut out = MaskMap::new();
    for (k, t) in &c.tiles {
        let tile = out.tile_mut(*k);
        for (v, p) in tile.0.iter_mut().zip(t.0.iter()) {
            *v = (p[3] as u32).div_ceil(2).min(0x4000) as u16;
        }
    }
    out.tiles.retain(|_, t| t.0.iter().any(|&v| v != 0));
    out
}

fn render_group(layers: &[Layer], key: Key, backdrop: &mut [[f32; 4]], size: (i32, i32), fx: u8) {
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
            render_group(base.children.as_deref().unwrap_or(&[]), key, &mut inner, size, fx);
            let k = base.opacity.clamp(0.0, 1.0);
            for (dst, src) in backdrop.iter_mut().zip(inner.iter()) {
                for c in 0..4 {
                    dst[c] += (src[c] - dst[c]) * k;
                }
            }
            continue;
        }
        let Some(mut src) = layer_source(base, key, size, fx) else { continue };
        // эффект и бумага — до прикреплённых слоёв (бумага сверена со сведённой картинкой образца)
        crate::effects::apply(base, key, &mut src, size, fx);
        apply_paper(base, key, &mut src, fx);
        for clip in clips.iter().filter(|c| c.visible && c.opacity > 0.0) {
            let Some(mut top) = layer_source(clip, key, size, fx) else { continue };
            crate::effects::apply(clip, key, &mut top, size, fx);
            apply_paper(clip, key, &mut top, fx);
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

/// Пиксель после наложения краски `rgb` с долей `m` (0..=ONE): «поверх»; при сохранении
/// непрозрачности — только цвет, альфа прежняя.
pub(crate) fn blend_paint(px: Px, rgb: [u8; 3], m: u32, preserve: bool) -> Px {
    let color = rgb.map(|c| (c as u32 * ONE + 127) / 255);
    let inv = ONE - m;
    let mut out = px;
    if preserve {
        let a = px[3] as u32;
        for i in 0..3 {
            let src = color[i] * a / ONE;
            out[i] = ((src * m + px[i] as u32 * inv + ONE / 2) / ONE) as u16;
        }
    } else {
        for i in 0..3 {
            out[i] = ((color[i] * m + px[i] as u32 * inv + ONE / 2) / ONE) as u16;
        }
        out[3] = (m + (px[3] as u32 * inv + ONE / 2) / ONE).min(ONE) as u16;
    }
    out
}
