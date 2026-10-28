//! Запись `.sai2` — обратная сторона чтения (docs/03-sai2-format.md). Раскладка повторяет файлы,
//! которые пишет сама программа-образец: заголовок, оглавление, миниатюра `thum` (у холстов от
//! 512 px), сведённое изображение `intg`, описания слоёв `layr` сверху вниз, пиксели `lpix`.
//!
//! Пишутся растровые слои и папки. Слои контура и фигур пишутся растровыми (их кривые пока не
//! сохраняются).

use paint_core::blend::BlendMode;
use paint_core::pixel::Px;
use paint_core::tile::TILE;
use paint_core::{Document, Layer};
use std::io;
use std::path::Path;

const SAI_ONE: u16 = 0x4000;
const BLOCK: usize = 32;

/// Метка режима наложения, как в файле (обратное к `sai2_doc::blend_from_tag`).
pub fn blend_tag(mode: BlendMode) -> &'static [u8; 4] {
    use BlendMode::*;
    match mode {
        Normal => b"norm",
        Multiply => b"mult",
        Screen => b"scrn",
        Overlay => b"over",
        Shade => b"sub ",
        Shine => b"add ",
        ShadeShine => b"sbad",
        Burn => b"burn",
        Dodge => b"ddge",
        BurnDodge => b"bndg",
        Darken => b"dark",
        ColorBurn => b"cbrn",
        LinearBurn => b"lbrn",
        DarkerColor => b"drkc",
        Lighten => b"litn",
        ColorDodge => b"cddg",
        LinearDodge => b"lddg",
        LighterColor => b"litc",
        SoftLight => b"slit",
        HardLight => b"hlit",
        VividLight => b"vlit",
        LinearLight => b"llit",
        PinLight => b"plit",
        HardMix => b"hmix",
        Difference => b"diff",
        Exclusion => b"excl",
        Subtract => b"fsub",
        Divide => b"fdiv",
        #[allow(unreachable_patterns, reason = "на случай новых режимов наложения")]
        _ => b"norm",
    }
}

/// Запись битов от младшего к старшему.
#[derive(Default)]
struct BitWriter {
    out: Vec<u8>,
    acc: u64,
    have: u32,
}

impl BitWriter {
    fn put(&mut self, value: u32, n: u32) {
        if n == 0 {
            return;
        }
        self.acc |= ((value as u64) & ((1u64 << n) - 1)) << self.have;
        self.have += n;
        while self.have >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.have -= 8;
        }
    }

    /// Номер кода: `z = k/2` нулей, единица, затем младший бит `k`.
    fn code(&mut self, k: u32) {
        self.put(1 << (k / 2), k / 2 + 1);
        self.put(k & 1, 1);
    }

    fn finish(mut self) -> Vec<u8> {
        if self.have > 0 {
            self.out.push(self.acc as u8);
        }
        self.out
    }
}

/// Кодирует поправки одного канала (те же коды, что читает `decode_residuals`).
fn encode_plane(w: &mut BitWriter, values: impl Iterator<Item = i32>) {
    let values: Vec<i32> = values.collect();
    let mut i = 0;
    while i < values.len() {
        let r = values[i];
        if r == 0 {
            let run = values[i..].iter().take(135).take_while(|v| **v == 0).count();
            if run >= 8 {
                w.code(15);
                w.put((run - 8) as u32, 7);
                i += run;
            } else {
                w.code(0);
                i += 1;
            }
            continue;
        }
        let m = r.unsigned_abs() + 1;
        let k = 31 - m.leading_zeros(); // 2^k ≤ m < 2^(k+1)
        w.code(k);
        w.put(m - (1 << k), k);
        w.put((r < 0) as u32, 1);
        i += 1;
    }
}

/// Поправки блока 32×32 (BGRA, 0..=0x4000, цвет не больше альфы) и их битовый поток.
fn encode_block(px: &[[u16; 4]]) -> Vec<u8> {
    let mut res = vec![[0i32; 4]; BLOCK * BLOCK];
    for y in 0..BLOCK {
        let mut left = [0u16; 4];
        let mut up_left = [0u16; 4];
        for x in 0..BLOCK {
            let up = if y == 0 { [0u16; 4] } else { px[(y - 1) * BLOCK + x] };
            let v = px[y * BLOCK + x];
            for c in 0..4 {
                let predicted = left[c].wrapping_add(up[c]).saturating_sub(up_left[c]).min(SAI_ONE);
                res[y * BLOCK + x][c] = v[c] as i32 - predicted as i32;
            }
            left = v;
            up_left = up;
        }
    }
    let mut w = BitWriter::default();
    for c in 0..4 {
        encode_plane(&mut w, res.iter().map(|r| r[c]));
    }
    w.finish()
}

/// Пиксель ядра (RGBA, 15 бит, предумножен) → файла (BGRA, 0x4000).
fn to_sai(p: Px) -> [u16; 4] {
    let a = (p[3] / 2).min(SAI_ONE);
    [(p[2] / 2).min(a), (p[1] / 2).min(a), (p[0] / 2).min(a), a]
}

/// Блок `lpix` слоя на области `bw`×`bh` блоков от левого верхнего угла холста.
fn encode_lpix(layer: &Layer, bw: usize, bh: usize) -> Vec<u8> {
    // строки блоков независимы — по потокам; результат собирается по порядку
    let threads = paint_core::threads();
    let per = bh.div_ceil(threads).max(1);
    let rows: Vec<Vec<u8>> = std::thread::scope(|scope| {
        let parts: Vec<_> = (0..bh).step_by(per).map(|b0| scope.spawn(move || (b0..(b0 + per).min(bh)).map(|by| lpix_row(layer, bw, by)).collect::<Vec<_>>())).collect();
        parts.into_iter().flat_map(|p| p.join().unwrap()).collect()
    });
    let mut out = b"dpcm".to_vec();
    for r in &rows {
        out.extend((r.len() as u32).to_le_bytes());
    }
    for r in rows {
        out.extend(r);
    }
    out
}

/// Строка блоков `by` слоя. Блок 32×32 лежит внутри одной плитки 64×64: где плитки нет, блок пуст
/// без перебора точек, а точки есть — берутся из плитки строками, без поиска на каждую.
fn lpix_row(layer: &Layer, bw: usize, by: usize) -> Vec<u8> {
    {
        let mut row = Vec::new();
        let mut empty_run = 0usize;
        let flush_empty = |row: &mut Vec<u8>, run: &mut usize, bx_end: usize| {
            // пропуски по 65536 блоков максимум; номер — первого блока после пропуска
            while *run > 0 {
                let n = (*run).min(65536);
                let start = bx_end - *run;
                row.extend([0xFF, (start & 0x0F) as u8]);
                row.extend(((n - 1) as u16).to_le_bytes());
                *run -= n;
            }
        };
        let mut px = vec![[0u16; 4]; BLOCK * BLOCK];
        for bx in 0..bw {
            let (gx, gy) = ((bx * BLOCK) as i32, (by * BLOCK) as i32);
            let Some(tile) = layer.pixels.tiles.get(&paint_core::tile::key_of(gx, gy)) else {
                empty_run += 1;
                continue;
            };
            let (ox, oy) = (gx.rem_euclid(TILE) as usize, gy.rem_euclid(TILE) as usize);
            let mut any = false;
            for y in 0..BLOCK {
                let src = &tile.0[(oy + y) * TILE as usize + ox..][..BLOCK];
                for (x, p) in src.iter().enumerate() {
                    let v = to_sai(*p);
                    any |= v[3] != 0;
                    px[y * BLOCK + x] = v;
                }
            }
            if !any {
                empty_run += 1;
                continue;
            }
            flush_empty(&mut row, &mut empty_run, bx);
            if px.iter().all(|p| *p == px[0]) {
                row.extend([0xFF, 0x50 | (bx & 0x0F) as u8]);
                for c in px[0] {
                    row.extend(c.to_le_bytes());
                }
            } else {
                let stream = encode_block(&px);
                row.extend([0xFF, 0xA0 | (bx & 0x0F) as u8]);
                row.extend((stream.len() as u16).to_le_bytes());
                row.extend(stream);
            }
        }
        flush_empty(&mut row, &mut empty_run, bw);
        row.extend([0xFF, 0xF0 | (bw & 0x0F) as u8]);
        row
    }
}

/// Блок маски `mpix` (один канал, 0..=0x4000) на области `bw`×`bh` блоков от угла холста.
/// Пустые блоки пропускаются, остальные пишутся картинкой (как у образца).
fn encode_mpix(hide: &paint_core::tile::MaskMap, bw: usize, bh: usize) -> Vec<u8> {
    let mut rows: Vec<Vec<u8>> = Vec::with_capacity(bh);
    for by in 0..bh {
        let mut row = Vec::new();
        let mut empty_run = 0usize;
        let flush_empty = |row: &mut Vec<u8>, run: &mut usize, bx_end: usize| {
            while *run > 0 {
                let n = (*run).min(65536);
                let start = bx_end - *run;
                row.extend([0xFF, (start & 0x0F) as u8]);
                row.extend(((n - 1) as u16).to_le_bytes());
                *run -= n;
            }
        };
        for bx in 0..bw {
            let mut v = vec![0u16; BLOCK * BLOCK];
            for y in 0..BLOCK {
                for x in 0..BLOCK {
                    v[y * BLOCK + x] = hide.get((bx * BLOCK + x) as i32, (by * BLOCK + y) as i32).min(SAI_ONE);
                }
            }
            if v.iter().all(|m| *m == 0) {
                empty_run += 1;
                continue;
            }
            flush_empty(&mut row, &mut empty_run, bx);
            let mut res = vec![0i32; BLOCK * BLOCK];
            for y in 0..BLOCK {
                let (mut left, mut up_left) = (0u16, 0u16);
                for x in 0..BLOCK {
                    let up = if y == 0 { 0 } else { v[(y - 1) * BLOCK + x] };
                    let predicted = left.wrapping_add(up).saturating_sub(up_left).min(SAI_ONE);
                    res[y * BLOCK + x] = v[y * BLOCK + x] as i32 - predicted as i32;
                    left = v[y * BLOCK + x];
                    up_left = up;
                }
            }
            let mut w = BitWriter::default();
            encode_plane(&mut w, res.into_iter());
            let stream = w.finish();
            row.extend([0xFF, 0xA0 | (bx & 0x0F) as u8]);
            row.extend((stream.len() as u16).to_le_bytes());
            row.extend(stream);
        }
        flush_empty(&mut row, &mut empty_run, bw);
        row.extend([0xFF, 0xF0 | (bw & 0x0F) as u8]);
        rows.push(row);
    }
    let mut out = b"dpcm".to_vec();
    for r in &rows {
        out.extend((r.len() as u32).to_le_bytes());
    }
    for r in rows {
        out.extend(r);
    }
    out
}

/// Сведённое изображение `intg`: блоки 256×256, каждая строка пикселей — свой поток.
/// `rgba` — RGBA по 8 бит без предумножения; `planes` — 3 (непрозрачный фон) или 4.
/// Сведённая картинка `intg` полосами по 256 строк: блоки 256×256 кодируются независимо, поэтому
/// полосами получаются те же байты, что целиком, а в памяти — одна полоса.
/// Полоса `by` сведённой картинки `intg`: размеры блоков и их байты (с маркером конца строки
/// блоков). Одинаковые по всей площади блоки (пустота, заливка) кодируются один раз: их байты
/// зависят только от цвета и размера.
fn encode_band(rgba: &[u8], w: usize, h: usize, by: usize, planes: usize) -> (Vec<u32>, Vec<u8>) {
    let bw = w.div_ceil(256);
    let rows = (h - by * 256).min(256);
    let mut sizes = Vec::with_capacity(bw);
    let mut body = Vec::new();
    let mut same: std::collections::HashMap<([u8; 4], usize), Vec<u8>> = std::collections::HashMap::new();
    for bx in 0..bw {
        let cols = (w - bx * 256).min(256);
        let px = |x: usize, y: usize| -> [u8; 4] {
            let i = (y * w + bx * 256 + x) * 4;
            [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
        };
        let first = px(0, 0);
        let uniform = (0..rows).all(|y| rgba[(y * w + bx * 256) * 4..(y * w + bx * 256 + cols) * 4].as_chunks::<4>().0.iter().all(|p| *p == first));
        let block = |bx: usize| -> Vec<u8> {
            let mut out = vec![0xFF, (bx & 0xFF) as u8];
            let mut above = vec![[0i32; 4]; cols];
            for y in 0..rows {
                let mut res = vec![[0i32; 4]; cols];
                let mut left = [0i32; 4];
                let mut up_left = [0i32; 4];
                for x in 0..cols {
                    let p = px(x, y);
                    let v = [p[2] as i32, p[1] as i32, p[0] as i32, p[3] as i32];
                    let up = above[x];
                    for c in 0..4 {
                        let predicted = ((left[c] as u16).wrapping_add(up[c] as u16).saturating_sub(up_left[c] as u16)).min(255) as i32;
                        res[x][c] = v[c] - predicted;
                        left[c] = v[c];
                    }
                    up_left = up;
                    above[x] = v;
                }
                let mut wr = BitWriter::default();
                for c in 0..planes {
                    encode_plane(&mut wr, res.iter().map(|r| r[c]));
                }
                out.extend(wr.finish());
            }
            out
        };
        let bytes = if uniform {
            // маркер начала блока несёт его номер — остальное одинаково у одинаковых блоков
            let mut b = same.entry((first, cols)).or_insert_with(|| block(0)).clone();
            b[1] = (bx & 0xFF) as u8;
            b
        } else {
            block(bx)
        };
        sizes.push(bytes.len() as u32);
        body.extend(bytes);
    }
    body.extend([0xFF, (bw & 0xFF) as u8]);
    (sizes, body)
}

struct IntgEncoder {
    bw: usize,
    table: Vec<u32>,
    body: Vec<u8>,
    by: usize,
}

impl IntgEncoder {
    fn new(w: usize, h: usize) -> Self {
        let bw = w.div_ceil(256);
        Self { bw, table: vec![0; bw * h.div_ceil(256)], body: Vec::new(), by: 0 }
    }

    /// Готовая полоса из `encode_band` (полосы — по порядку).
    fn push(&mut self, sizes: Vec<u32>, body: Vec<u8>) {
        let base = self.by * self.bw;
        self.table[base..base + sizes.len()].copy_from_slice(&sizes);
        self.body.extend(body);
        self.by += 1;
    }

    fn finish(self) -> Vec<u8> {
        let mut out = b"dpcm".to_vec();
        for t in self.table {
            out.extend(t.to_le_bytes());
        }
        out.extend(self.body);
        out
    }
}

#[cfg(test)]
fn encode_intg(rgba: &[u8], w: usize, h: usize, planes: usize) -> Vec<u8> {
    let mut e = IntgEncoder::new(w, h);
    for by in 0..h.div_ceil(256) {
        let (sizes, body) = encode_band(&rgba[by * 256 * w * 4..((by + 1) * 256).min(h) * w * 4], w, h, by, planes);
        e.push(sizes, body);
    }
    e.finish()
}

/// Блоки миниатюры (у холстов от 512 px, как у SAI 2017) и сведённой картинки — полосами по 256
/// строк: у холста 100000×100000 сведение целиком заняло бы 40 ГБ. Миниатюра уменьшает в
/// 2^level ≤ 256 раз — её строки не делят полосу, байты те же, что из целой картинки.
fn flat_blocks(doc: &Document, w: usize, h: usize, opaque: bool, planes: usize) -> (Option<Vec<u8>>, Vec<u8>) {
    let level = crate::jssf::thumb_level(doc.width, doc.height);
    let bands = h.div_ceil(256);
    // полосы — параллельно, сколько помещается в ~512 МБ буферов; собираются по порядку
    let band_bytes = w * 256 * 4;
    let workers = paint_core::threads().min((512 << 20) / band_bytes.max(1)).clamp(1, bands.max(1));
    let one = |by: usize| {
        let (y0, y1) = ((by * 256) as u32, (((by + 1) * 256).min(h)) as u32);
        let band = doc.flatten_rows(y0, y1, opaque);
        let thumb = (level > 0).then(|| crate::jssf::thumbnail(&band, doc.width, y1 - y0, level));
        let (sizes, body) = encode_band(&band, w, h, by, planes);
        (thumb, sizes, body)
    };
    let mut intg = IntgEncoder::new(w, h);
    let (mut thumb, mut tw, mut th) = (Vec::new(), 0, 0);
    let mut by = 0;
    while by < bands {
        let n = workers.min(bands - by);
        let done: Vec<_> = std::thread::scope(|scope| {
            let hs: Vec<_> = (by..by + n).map(|b| scope.spawn(move || one(b))).collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        for (t, sizes, body) in done {
            if let Some((bw, bh, rgb)) = t {
                (tw, th) = (bw, th + bh);
                thumb.extend(rgb);
            }
            intg.push(sizes, body);
        }
        by += n;
    }
    ((level > 0).then(|| crate::jssf::thum_block(tw, th, &thumb)), intg.finish())
}

struct Flat<'a> {
    layer: &'a Layer,
    depth: u8,
}

/// Слои сверху вниз с глубиной вложенности, как они идут в файле.
fn flatten_tree<'a>(layers: &'a [Layer], depth: u8, out: &mut Vec<Flat<'a>>) {
    for layer in layers.iter().rev() {
        out.push(Flat { layer, depth });
        if let Some(children) = &layer.children {
            flatten_tree(children, depth + 1, out);
        }
    }
}

fn layer_record(f: &Flat, id: u32, order: u32, rect: [i32; 4], mask_id: Option<u32>, timeline: &paint_core::anim::Timeline) -> Vec<u8> {
    let l = f.layer;
    let mut d = Vec::with_capacity(96);
    d.extend(b"layr");
    d.extend(id.to_le_bytes());
    d.extend(0u32.to_le_bytes());
    d.extend(order.to_le_bytes());
    d.extend(match (&l.children, &l.vector) {
        (Some(_), _) => b"fold",
        (None, Some(v)) => &v.kind,
        _ => b"norm",
    });
    d.extend([0u8; 8]);
    for v in rect {
        d.extend(v.to_le_bytes());
    }
    d.extend(if l.children.is_some() && l.pass_through { b"pass" } else { blend_tag(l.mode) });
    d.extend(((l.opacity.clamp(0.0, 1.0) * 100.0).round() as u32).to_le_bytes());
    d.push(f.depth);
    // замки: бит 0 — сохранять непрозрачность, 1 — рисование, 2 — перемещение, 7 — всё
    d.push(l.preserve_opacity as u8 | (l.lock_paint as u8) << 1 | (l.lock_move as u8) << 2 | (l.lock_all as u8) << 7);
    d.push(l.visible as u8 | (l.selection_source as u8) << 7);
    d.push((l.clipping as u8) | if l.children.is_some() { 0x40 } else { 0 });
    // бумага (как у SAI — перед именем): сила, масштаб, флаг, число символов, имя UTF-16, дополнение до 4 байт
    if let Some(p) = &l.paper {
        let units: Vec<u16> = p.name.encode_utf16().take(255).collect();
        let mut body = Vec::new();
        for v in [p.strength, p.scale, p.flag, units.len() as u16] {
            body.extend(v.to_le_bytes());
        }
        for u in units {
            body.extend(u.to_le_bytes());
        }
        while !body.len().is_multiple_of(4) {
            body.push(0);
        }
        d.extend(b"ptex");
        d.extend((body.len() as u32).to_le_bytes());
        d.extend(body);
    }
    // имя: u16 число символов, UTF-16, дополнение до 4 байт
    let units: Vec<u16> = l.name.encode_utf16().take(255).collect();
    let mut name = (units.len() as u16).to_le_bytes().to_vec();
    for u in units {
        name.extend(u.to_le_bytes());
    }
    while !name.len().is_multiple_of(4) {
        name.push(0);
    }
    d.extend(b"name");
    d.extend((name.len() as u32).to_le_bytes());
    d.extend(name);
    // маска: номер блока mpix, область, признаки
    if let (Some(m), Some(mid)) = (&l.mask, mask_id) {
        d.extend(b"lmsk");
        d.extend(24u32.to_le_bytes());
        d.extend(mid.to_le_bytes());
        for v in rect {
            d.extend(v.to_le_bytes());
        }
        d.extend(m.flags);
    }
    let effect = l.effect.as_ref().map(|e| e.to_block());
    if let Some(s) = &l.scene3d {
        let body = crate::sai2_3d::layer_block(s);
        d.extend(crate::sai2_3d::LAYER_TAG);
        d.extend((body.len() as u32).to_le_bytes());
        d.extend(body);
    }
    // свои блоки: стили слоя и слой коррекции (текст)
    let mut own: Vec<([u8; 4], Vec<u8>)> = Vec::new();
    if let Some(st) = l.styles.as_deref().filter(|s| !s.is_empty()) {
        own.push((*b"gsst", st.to_text().into_bytes()));
    }
    if let Some(a) = &l.adjust {
        own.push((*b"gsad", a.to_text().into_bytes()));
    }
    if let Some(p) = l.puppet.as_deref() {
        own.push((crate::sai2_3d::PUPPET_TAG, crate::sai2_3d::puppet_block(p)));
    }
    if let Some(a) = l.anim.as_deref().filter(|_| l.children.is_some()) {
        own.push((*b"gsan", (timeline.to_text() + &a.to_text()).into_bytes()));
    }
    for (tag, body) in effect.iter().chain(&own).chain(&l.extra) {
        d.extend(tag);
        d.extend((body.len() as u32).to_le_bytes());
        d.extend(body);
    }
    d.extend(0u32.to_le_bytes());
    d
}

/// Сохраняет документ в `.sai2`.
pub fn save(doc: &Document, path: &Path) -> io::Result<()> {
    save_with_blocks(doc, path, Vec::new())
}

/// То же и свои блоки файла (вид, номер, данные) — в конце оглавления (например, модели слоёв 3D).
pub fn save_with_blocks(doc: &Document, path: &Path, own: Vec<([u8; 4], u32, Vec<u8>)>) -> io::Result<()> {
    let (w, h) = (doc.width as usize, doc.height as usize);
    let (bw, bh) = (w.div_ceil(BLOCK), h.div_ceil(BLOCK));
    let rect = [0, 0, bw as i32, bh as i32];
    let mut flat = Vec::new();
    flatten_tree(&doc.layers, 0, &mut flat);
    if flat.iter().any(|f| f.depth > 10) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "папки вложены глубже 10 уровней — образец так не читает"));
    }
    let count = flat.len() as u32;
    // номера слоёв с 2, как у образца; служебный номер — снизу вверх с 1
    let ids: Vec<u32> = (0..flat.len()).map(|i| i as u32 + 2).collect();
    // каналов в `intg` — как решает SAI по байту фона в заголовке (младшие 4 бита 0 — прозрачный,
    // 4 канала с альфой; иначе 3). Обозреватель файлов SAI читает `intg` именно так, и при
    // расхождении считает файл повреждённым (так было у файлов SOsai до 0.9.3).
    let opaque = doc.bg_code & 0x0F != 0;
    let planes = if opaque { 3 } else { 4 };
    let mut blocks: Vec<([u8; 4], u32, Vec<u8>)> = Vec::new();
    let (thum, intg) = flat_blocks(doc, w, h, opaque, planes);
    blocks.extend(thum.map(|t| (*b"thum", 0, t)));
    blocks.push((*b"intg", 0, intg));
    // маски — свои блоки с номерами после слоёв
    let mut mask_ids = vec![None; flat.len()];
    let mut next_id = flat.len() as u32 + 2;
    for (i, f) in flat.iter().enumerate() {
        if f.layer.mask.is_some() {
            mask_ids[i] = Some(next_id);
            next_id += 1;
        }
    }
    for (i, f) in flat.iter().enumerate() {
        // текст и линейки перспективы у образца без области пикселей
        let own = match &f.layer.vector {
            Some(v) if matches!(&v.kind, b"text" | b"pers" | b"grid") => [0; 4],
            _ => rect,
        };
        blocks.push((*b"layr", ids[i], layer_record(f, ids[i], count - i as u32, own, mask_ids[i], &doc.timeline)));
    }
    // пиксели слоёв кодируются параллельно, по слою на поток
    // слои контура и фигур — своим векторным блоком, без пикселей (как у образца)
    for (i, f) in flat.iter().enumerate() {
        if let Some(v) = &f.layer.vector {
            blocks.push((v.kind, ids[i], v.raw.clone()));
        }
    }
    let raster: Vec<(usize, &Layer)> = flat.iter().enumerate().filter(|(_, f)| f.layer.children.is_none() && f.layer.vector.is_none()).map(|(i, f)| (i, f.layer)).collect();
    let threads = paint_core::threads();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let encoded = std::sync::Mutex::new(vec![Vec::new(); raster.len()]);
    std::thread::scope(|scope| {
        for _ in 0..threads.min(raster.len().max(1)) {
            scope.spawn(|| loop {
                let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some((_, layer)) = raster.get(k) else { break };
                let data = encode_lpix(layer, bw, bh);
                encoded.lock().unwrap()[k] = data;
            });
        }
    });
    for ((i, _), data) in raster.iter().zip(encoded.into_inner().unwrap()) {
        blocks.push((*b"lpix", ids[*i], data));
    }
    for (i, f) in flat.iter().enumerate() {
        if let (Some(m), Some(mid)) = (&f.layer.mask, mask_ids[i]) {
            blocks.push((*b"mpix", mid, encode_mpix(&m.hide, bw, bh)));
        }
    }
    blocks.extend(crate::sai2_3d::file_blocks(&doc.layers));
    blocks.extend(own);
    let mut out = Vec::new();
    out.extend(crate::sai2::MAGIC);
    // признаки холста: байт 1 — фон (как поле +0x249 SAI 2017)
    out.extend([0, doc.bg_code, 0, 0]);
    out.extend((w as u32).to_le_bytes());
    out.extend((h as u32).to_le_bytes());
    out.extend(doc.resolution.to_le_bytes());
    out.extend((blocks.len() as u32).to_le_bytes());
    out.extend(ids.first().copied().unwrap_or(0).to_le_bytes());
    out.extend([0u8; 16]);
    out.extend(doc.bg_color);
    out.push(0xFF);
    out.extend(doc.effect_color);
    let mut offset = (64 + blocks.len() * 16) as u64;
    let mut toc = Vec::new();
    let mut data = Vec::new();
    for (tag, id, body) in &blocks {
        toc.extend(tag);
        toc.extend(id.to_le_bytes());
        toc.extend(offset.to_le_bytes());
        data.extend(body);
        while !data.len().is_multiple_of(4) {
            data.push(0);
        }
        offset = (64 + blocks.len() * 16 + data.len()) as u64;
    }
    out.extend(toc);
    out.extend(data);
    paint_core::io::write_atomic(path, &out)
}

#[cfg(test)]
mod band_tests {
    use super::*;

    /// Прежний кодер `intg` — по точкам, целой картинкой (эталон).
fn encode_intg_reference(rgba: &[u8], w: usize, h: usize, planes: usize) -> Vec<u8> {
        let (bw, bh) = (w.div_ceil(256), h.div_ceil(256));
        let mut table = vec![0u32; bw * bh];
        let mut body = Vec::new();
        for by in 0..bh {
            let rows = (h - by * 256).min(256);
            for bx in 0..bw {
                let start = body.len();
                body.extend([0xFF, (bx & 0xFF) as u8]);
                let cols = (w - bx * 256).min(256);
                let mut above = vec![[0i32; 4]; cols];
                for y in 0..rows {
                    let mut res = vec![[0i32; 4]; cols];
                    let mut left = [0i32; 4];
                    let mut up_left = [0i32; 4];
                    for x in 0..cols {
                        let i = ((by * 256 + y) * w + bx * 256 + x) * 4;
                        let v = [rgba[i + 2] as i32, rgba[i + 1] as i32, rgba[i] as i32, rgba[i + 3] as i32];
                        let up = above[x];
                        for c in 0..4 {
                            let predicted = ((left[c] as u16).wrapping_add(up[c] as u16).saturating_sub(up_left[c] as u16)).min(255) as i32;
                            res[x][c] = v[c] - predicted;
                            left[c] = v[c];
                        }
                        up_left = up;
                        above[x] = v;
                    }
                    let mut wr = BitWriter::default();
                    for c in 0..planes {
                        encode_plane(&mut wr, res.iter().map(|r| r[c]));
                    }
                    body.extend(wr.finish());
                }
                table[by * bw + bx] = (body.len() - start) as u32;
            }
            body.extend([0xFF, (bw & 0xFF) as u8]);
        }
        let mut out = b"dpcm".to_vec();
        for t in table {
            out.extend(t.to_le_bytes());
        }
        out.extend(body);
        out
    }

    #[test]
    fn banded_flat_blocks_match_the_whole_picture() {
        // 700×600: три полосы intg, миниатюра в 2 раза; с фоном и без
        for opaque in [true, false] {
            let mut doc = Document::new(700, 600);
            for y in 0..600 {
                for x in 0..700 {
                    if x < 300 && (x * 7 + y * 3) % 11 < 6 {
                        doc.layers[0].pixels.set(x, y, paint_core::pixel::from_rgba8([(x % 256) as u8, (y % 256) as u8, 90, ((x + y) % 256) as u8]));
                    }
                }
            }
            let planes = if opaque { 3 } else { 4 };
            let flat = if opaque { doc.flatten() } else { doc.flatten_clear() };
            let level = crate::jssf::thumb_level(700, 600);
            let (tw, th, rgb) = crate::jssf::thumbnail(&flat, 700, 600, level);
            let (thum, intg) = flat_blocks(&doc, 700, 600, opaque, planes);
            assert_eq!(intg, encode_intg_reference(&flat, 700, 600, planes));
            assert_eq!(intg, encode_intg(&flat, 700, 600, planes));
            assert_eq!(thum, Some(crate::jssf::thum_block(tw, th, &rgb)));
        }
    }
}
