//! Пиксели растрового слоя `.sai2` (блок `lpix`, сжатие `dpcm`).
//!
//! Устройство блока:
//! - метка `dpcm`, затем по одному u32 на каждую строку блоков — длина её данных;
//! - строка блоков — цепочка записей с двухбайтным маркером `FF xy`, где `y` — младшие четыре
//!   бита номера блока по горизонтали, а `x` — вид записи:
//!   `0` — пропуск: u16 `n`, пусто `n + 1` блоков;
//!   `5` — блок одного цвета: один пиксель (4 × u16);
//!   `A` — блок 32×32 с картинкой: u16 длина, затем битовый поток;
//!   `F` — конец строки.
//! - пиксель: синий, зелёный, красный, альфа; 0x4000 = 1.0; цвет предумножен на альфу.
//!
//! Картинка блока хранится разностями: значение предсказывается по соседям слева, сверху и
//! сверху-слева, в поток пишется только поправка — кодом переменной длины.

use crate::sai2::{Entry, LayerInfo};
use paint_core::pixel::Px;
use paint_core::tile::{ColorMap, TILE};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

pub const BLOCK: i32 = 32;
const BLOCK_LEN: usize = (BLOCK * BLOCK) as usize;
const SAI_ONE: u16 = 0x4000;

fn bad(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

/// Чтение битов от младшего к старшему.
struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    acc: u64,
    have: u32,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0, acc: 0, have: 0 }
    }

    /// Сколько байт потока уже затронуто чтением.
    fn bytes_used(&self) -> usize {
        self.pos - (self.have / 8) as usize
    }

    fn refill(&mut self) {
        while self.have <= 56 {
            let byte = self.data.get(self.pos).copied().unwrap_or(0);
            self.pos += 1;
            self.acc |= (byte as u64) << self.have;
            self.have += 8;
        }
    }

    fn take(&mut self, n: u32) -> u32 {
        let v = (self.acc & ((1u64 << n) - 1)) as u32;
        self.acc >>= n;
        self.have -= n;
        v
    }

    /// Сколько нулевых битов до первой единицы (её тоже забирает).
    fn zeros_then_one(&mut self) -> Option<u32> {
        let z = (self.acc as u32).trailing_zeros();
        if z >= 8 {
            return None; // таких кодов в формате нет
        }
        self.take(z + 1);
        Some(z)
    }
}

/// Раскодирует поправки: `planes` каналов подряд, в каждом `count` значений. В выходном массиве
/// каналы чередуются с шагом 4. Вторым числом возвращает, сколько байт потока прочитано.
fn decode_residuals(data: &[u8], count: usize, planes: usize) -> io::Result<(Vec<i16>, usize)> {
    let mut out = vec![0i16; count * 4];
    let mut bits = Bits::new(data);
    for channel in 0..planes {
        let mut i = 0;
        while i < count {
            bits.refill();
            let zeros = bits.zeros_then_one().ok_or_else(|| bad("битый код в блоке пикселей"))?;
            let code = zeros * 2 + bits.take(1);
            match code {
                0 => i += 1,
                1..=14 => {
                    let low = bits.take(code);
                    let negative = bits.take(1) == 1;
                    let magnitude = ((1u32 << code) + low - 1) as i32;
                    out[i * 4 + channel] = (if negative { -magnitude } else { magnitude }) as i16;
                    i += 1;
                }
                _ => i += (bits.take(7) + 8) as usize, // серия нулей
            }
        }
    }
    Ok((out, bits.bytes_used()))
}

/// Восстанавливает пиксели блока по поправкам. Возвращает BGRA, 0..=0x4000.
fn reconstruct(residuals: &[i16]) -> Vec<[u16; 4]> {
    let mut out = vec![[0u16; 4]; BLOCK_LEN];
    for y in 0..BLOCK as usize {
        let mut left = [0u16; 4]; // значение слева до обрезки
        let mut up_left = [0u16; 4];
        for x in 0..BLOCK as usize {
            let up = if y == 0 { [0u16; 4] } else { out[(y - 1) * BLOCK as usize + x] };
            let mut value = [0u16; 4];
            for c in 0..4 {
                let predicted = left[c].wrapping_add(up[c]).saturating_sub(up_left[c]).min(SAI_ONE);
                value[c] = predicted.wrapping_add(residuals[(y * BLOCK as usize + x) * 4 + c] as u16);
            }
            let alpha = value[3] as i16;
            let px = &mut out[y * BLOCK as usize + x];
            for c in 0..4 {
                px[c] = (value[c] as i16).min(alpha).clamp(0, SAI_ONE as i16) as u16;
            }
            left = value;
            up_left = up;
        }
    }
    out
}

/// BGRA 14 бит → RGBA 15 бит ядра.
fn to_px(p: [u16; 4]) -> Px {
    [p[2] * 2, p[1] * 2, p[0] * 2, p[3] * 2]
}

fn put_block(map: &mut ColorMap, bx: i32, by: i32, pixels: &[[u16; 4]]) {
    let (x0, y0) = (bx * BLOCK, by * BLOCK);
    let tile = map.tile_mut((x0.div_euclid(TILE), y0.div_euclid(TILE)));
    let (ox, oy) = (x0.rem_euclid(TILE), y0.rem_euclid(TILE));
    for y in 0..BLOCK {
        for x in 0..BLOCK {
            tile.0[((oy + y) * TILE + ox + x) as usize] = to_px(pixels[(y * BLOCK + x) as usize]);
        }
    }
}

fn u16_at(d: &[u8], o: usize) -> io::Result<u16> {
    let s = d.get(o..o + 2).ok_or_else(|| bad("данные слоя оборваны"))?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}

/// Разбирает блок `lpix` целиком (уже прочитанный в память).
pub fn decode_lpix(data: &[u8], layer: &LayerInfo) -> io::Result<ColorMap> {
    let mut map = ColorMap::new();
    let [left, top, width, height] = layer.block_rect;
    if width <= 0 || height <= 0 {
        return Ok(map);
    }
    if data.get(0..4) != Some(b"dpcm") {
        return Err(bad("у слоя неизвестный способ сжатия"));
    }
    if !(-8192..=8192).contains(&left) || !(-8192..=8192).contains(&top) || width > 8192 || height > 8192 {
        return Err(bad("границы слоя неправдоподобны"));
    }
    let (right, bottom) = (left + width, top + height);
    let rows = (bottom - top) as usize;
    let mut o = 4 + rows * 4;
    for by in top..bottom {
        let mut bx = left;
        loop {
            let marker = data.get(o..o + 2).ok_or_else(|| bad("данные слоя оборваны"))?;
            if marker[0] != 0xFF || (marker[1] & 0x0F) as i32 != bx & 0x0F {
                return Err(bad(format!("сбой разметки в строке блоков {by}, блок {bx}")));
            }
            o += 2;
            match marker[1] >> 4 {
                0xF if bx == right => break,
                0x0 if bx < right => {
                    bx += u16_at(data, o)? as i32 + 1;
                    o += 2;
                }
                0x5 if bx < right => {
                    let raw = data.get(o..o + 8).ok_or_else(|| bad("данные слоя оборваны"))?;
                    o += 8;
                    let px: [u16; 4] = std::array::from_fn(|c| u16::from_le_bytes([raw[c * 2], raw[c * 2 + 1]]).min(SAI_ONE));
                    if px != [0; 4] {
                        put_block(&mut map, bx, by, &vec![px; BLOCK_LEN]);
                    }
                    bx += 1;
                }
                0xA if bx < right => {
                    let len = u16_at(data, o)? as usize;
                    o += 2;
                    if len > 0x3000 {
                        return Err(bad("блок пикселей слишком велик"));
                    }
                    let body = data.get(o..o + len).ok_or_else(|| bad("данные слоя оборваны"))?;
                    o += len;
                    put_block(&mut map, bx, by, &reconstruct(&decode_residuals(body, BLOCK_LEN, 4)?.0));
                    bx += 1;
                }
                _ => return Err(bad(format!("неизвестная запись в строке блоков {by}, блок {bx}"))),
            }
        }
    }
    Ok(map)
}

/// Читает пиксели слоя из файла; `entries` — оглавление из [`crate::sai2::read_info`].
pub fn read_layer(file: &mut File, entries: &[Entry], layer: &LayerInfo) -> io::Result<ColorMap> {
    let Some(e) = entries.iter().find(|e| &e.tag == b"lpix" && e.id == layer.id) else {
        return Ok(ColorMap::new());
    };
    if e.len > 1 << 31 {
        return Err(bad("слой слишком велик"));
    }
    let mut data = vec![0u8; e.len as usize];
    file.seek(SeekFrom::Start(e.offset))?;
    file.read_exact(&mut data)?;
    decode_lpix(&data, layer)
}

/// Сведённое изображение из блока `intg`: RGBA по 8 бит, строки сверху вниз.
///
/// Картинка разбита на блоки 256×256. После метки `dpcm` идёт по одному u32 на блок, затем блоки
/// по строкам: маркер `FF` и номер блока в строке, далее каждая строка пикселей отдельным битовым
/// потоком (те же коды, что у слоёв), выровненным по байту; в конце строки блоков — маркер с
/// числом блоков. Пиксель: синий, зелёный, красный, альфа по 8 бит; предсказание «слева + сверху −
/// сверху-слева» в пределах блока.
pub fn decode_intg(data: &[u8], width: u32, height: u32) -> io::Result<Vec<u8>> {
    let (w, h) = (width as usize, height as usize);
    if data.get(0..4) != Some(b"dpcm") {
        return Err(bad("сведённое изображение сохранено неизвестным способом"));
    }
    let (bw, bh) = (w.div_ceil(256), h.div_ceil(256));
    let start = 4 + bw * bh * 4;
    // Число каналов в файле не записано рядом — пробуем четыре, затем три.
    let mut last = bad("сведённое изображение не разобрано");
    for planes in [4usize, 3] {
        match decode_intg_with(data, start, w, h, planes) {
            Ok(img) => return Ok(img),
            Err(e) => last = e,
        }
    }
    Err(last)
}

fn decode_intg_with(data: &[u8], start: usize, w: usize, h: usize, planes: usize) -> io::Result<Vec<u8>> {
    let (bw, bh) = (w.div_ceil(256), h.div_ceil(256));
    let mut img = vec![0u8; w * h * 4];
    let mut o = start;
    let marker = |o: usize, n: usize| -> io::Result<()> {
        match data.get(o..o + 2) {
            Some(m) if m[0] == 0xFF && m[1] as usize == n & 0xFF => Ok(()),
            _ => Err(bad("сбой разметки сведённого изображения")),
        }
    };
    for by in 0..bh {
        let rows = (h - by * 256).min(256);
        for bx in 0..bw {
            marker(o, bx)?;
            o += 2;
            let cols = (w - bx * 256).min(256);
            let mut above = vec![[0i16; 4]; cols];
            for y in 0..rows {
                let (res, used) = decode_residuals(data.get(o..).ok_or_else(|| bad("сведённое изображение оборвано"))?, cols, planes)?;
                o += used;
                let mut left = [0i16; 4];
                let mut up_left = [0i16; 4];
                let row = (by * 256 + y) * w + bx * 256;
                for x in 0..cols {
                    let up = above[x];
                    for c in 0..4 {
                        let predicted = (left[c] as u16).wrapping_add(up[c] as u16).saturating_sub(up_left[c] as u16).min(255);
                        left[c] = (predicted as i16).wrapping_add(res[x * 4 + c]);
                        above[x][c] = left[c].clamp(0, 255);
                    }
                    up_left = up;
                    let p = above[x];
                    let alpha = if planes == 4 { p[3] as u8 } else { 255 };
                    img[(row + x) * 4..(row + x) * 4 + 4].copy_from_slice(&[p[2] as u8, p[1] as u8, p[0] as u8, alpha]);
                }
            }
        }
        marker(o, bw)?;
        o += 2;
    }
    Ok(img)
}

/// Блок маски: номер по горизонтали и вертикали и 32×32 значения.
pub type MaskBlock = ((i32, i32), Vec<u16>);

/// Восстанавливает один канал блока маски по поправкам (то же предсказание, что у пикселей).
fn reconstruct_mask(residuals: &[i16]) -> Vec<u16> {
    let mut out = vec![0u16; BLOCK_LEN];
    for y in 0..BLOCK as usize {
        let (mut left, mut up_left) = (0u16, 0u16);
        for x in 0..BLOCK as usize {
            let up = if y == 0 { 0 } else { out[(y - 1) * BLOCK as usize + x] };
            let predicted = left.wrapping_add(up).saturating_sub(up_left).min(SAI_ONE);
            let value = predicted.wrapping_add(residuals[(y * BLOCK as usize + x) * 4] as u16);
            out[y * BLOCK as usize + x] = (value as i16).clamp(0, SAI_ONE as i16) as u16;
            left = value;
            up_left = up;
        }
    }
    out
}

/// Разбирает блок маски `mpix`: область `rect` в блоках 32 точки (лево, верх, ширина, высота).
/// Возвращает блоки (номер по горизонтали и вертикали) со значениями 0..=0x4000; пропущенные
/// блоки — нулевые.
pub fn decode_mpix(data: &[u8], rect: [i32; 4]) -> io::Result<Vec<MaskBlock>> {
    let [left, top, width, height] = rect;
    let mut out = Vec::new();
    if width <= 0 || height <= 0 {
        return Ok(out);
    }
    if data.get(0..4) != Some(b"dpcm") {
        return Err(bad("у маски неизвестный способ сжатия"));
    }
    if !(-8192..=8192).contains(&left) || !(-8192..=8192).contains(&top) || width > 8192 || height > 8192 {
        return Err(bad("границы маски неправдоподобны"));
    }
    let right = left + width;
    let mut o = 4 + height as usize * 4;
    for by in top..top + height {
        let mut bx = left;
        loop {
            let marker = data.get(o..o + 2).ok_or_else(|| bad("данные маски оборваны"))?;
            if marker[0] != 0xFF || (marker[1] & 0x0F) as i32 != bx & 0x0F {
                return Err(bad(format!("сбой разметки маски в строке блоков {by}, блок {bx}")));
            }
            o += 2;
            match marker[1] >> 4 {
                0xF if bx == right => break,
                0x0 if bx < right => {
                    bx += u16_at(data, o)? as i32 + 1;
                    o += 2;
                }
                0x5 if bx < right => {
                    let v = u16_at(data, o)?.min(SAI_ONE);
                    o += 2;
                    if v != 0 {
                        out.push(((bx, by), vec![v; BLOCK_LEN]));
                    }
                    bx += 1;
                }
                0xA if bx < right => {
                    let len = u16_at(data, o)? as usize;
                    o += 2;
                    let body = data.get(o..o + len).ok_or_else(|| bad("данные маски оборваны"))?;
                    o += len;
                    out.push(((bx, by), reconstruct_mask(&decode_residuals(body, BLOCK_LEN, 1)?.0)));
                    bx += 1;
                }
                k => return Err(bad(format!("неизвестная запись {k:X} маски в строке блоков {by}, блок {bx}"))),
            }
        }
    }
    Ok(out)
}
