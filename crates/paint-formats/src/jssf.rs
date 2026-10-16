//! Миниатюра `.sai2` (блок `thum`, метка `jssf`) — JPEG-подобный поток SAI (docs/03-sai2-format.md).
//!
//! Раскладка: u16 ширина, u16 высота, u16 число каналов (3), две таблицы квантования по 64 байта
//! (в порядке зигзага: яркость, цвет), затем по каждой строке блоков 8 px — длина (u16; больше
//! 0x7FFF — сначала старшая половина с битом 0x8000) и данные строки. Данные — обычный JPEG без
//! прореживания цвета (4:4:4, Y Cb Cr, по блоку 8×8 на канал подряд), стандартные таблицы
//! Хаффмана (приложение K), после байта `FF` вставляется `00`, строка добивается единицами;
//! предсказание DC сбрасывается в начале каждой строки.

/// Таблицы квантования, которые пишет SAI при качестве 80 (порядок зигзага, как в файле).
const QUANT: [[u8; 64]; 2] = [
    [
        2, 2, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6,
        6, 6, 6, 6, 8, 8, 8, 8, 8, 8, 8, 9, 9, 9, 9, 9, 9, 10, 10, 10, 10, 10, 12, 12, 12, 12, 14, 14, 14, 16, 16, 16,
    ],
    [
        6, 6, 6, 6, 6, 6, 8, 8, 8, 8, 9, 9, 9, 9, 9, 12, 12, 12, 12, 12, 12, 16, 16, 16, 16, 16, 16, 16, 19, 19, 19, 19,
        19, 19, 19, 19, 23, 23, 23, 23, 23, 23, 23, 27, 27, 27, 27, 27, 27, 32, 32, 32, 32, 32, 37, 37, 37, 37, 37, 37, 37, 37, 37, 37,
    ],
];

const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45,
    38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

// стандартные таблицы Хаффмана JPEG (приложение K): число кодов каждой длины и значения
const DC_L: ([u8; 16], &[u8]) = ([0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0], &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
const DC_C: ([u8; 16], &[u8]) = ([0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0], &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
const AC_L: ([u8; 16], &[u8]) = (
    [0, 2, 1, 3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 0x7d],
    &[
        0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07, 0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xa1, 0x08, 0x23, 0x42, 0xb1, 0xc1, 0x15, 0x52, 0xd1,
        0xf0, 0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0a, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x43, 0x44, 0x45, 0x46, 0x47,
        0x48, 0x49, 0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x83, 0x84, 0x85, 0x86,
        0x87, 0x88, 0x89, 0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba,
        0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xf1, 0xf2, 0xf3,
        0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa,
    ],
);
const AC_C: ([u8; 16], &[u8]) = (
    [0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1, 2, 0x77],
    &[
        0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21, 0x31, 0x06, 0x12, 0x41, 0x51, 0x07, 0x61, 0x71, 0x13, 0x22, 0x32, 0x81, 0x08, 0x14, 0x42, 0x91, 0xa1, 0xb1, 0xc1, 0x09, 0x23, 0x33, 0x52,
        0xf0, 0x15, 0x62, 0x72, 0xd1, 0x0a, 0x16, 0x24, 0x34, 0xe1, 0x25, 0xf1, 0x17, 0x18, 0x19, 0x1a, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x43, 0x44, 0x45, 0x46,
        0x47, 0x48, 0x49, 0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x82, 0x83, 0x84,
        0x85, 0x86, 0x87, 0x88, 0x89, 0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8,
        0xb9, 0xba, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xf2, 0xf3,
        0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa,
    ],
);

/// Код и длина для каждого значения таблицы.
fn codes(t: &([u8; 16], &[u8])) -> [(u16, u8); 256] {
    let mut out = [(0u16, 0u8); 256];
    let (mut code, mut k) = (0u16, 0usize);
    for (l, &n) in t.0.iter().enumerate() {
        for _ in 0..n {
            out[t.1[k] as usize] = (code, l as u8 + 1);
            code += 1;
            k += 1;
        }
        code <<= 1;
    }
    out
}

/// Сколько раз SAI уменьшает холст вдвое для миниатюры: пока большая сторона не меньше 512, не
/// больше 8 раз. 0 — миниатюры нет (SAI 2017 пишет её только у холстов от 512 px).
pub fn thumb_level(w: u32, h: u32) -> u32 {
    let m = w.max(h);
    let mut l = 0;
    while l < 8 && (m >> l) >= 512 {
        l += 1;
    }
    l
}

struct BitWriter {
    out: Vec<u8>,
    acc: u32,
    n: u32,
}

impl BitWriter {
    fn put(&mut self, v: u32, len: u8) {
        if len == 0 {
            return;
        }
        self.acc = (self.acc << len) | (v & ((1 << len) - 1));
        self.n += len as u32;
        while self.n >= 8 {
            self.n -= 8;
            let b = (self.acc >> self.n) as u8;
            self.out.push(b);
            if b == 0xff {
                self.out.push(0);
            }
        }
        self.acc &= (1 << self.n) - 1;
    }

    fn flush(&mut self) {
        if self.n > 0 {
            let pad = 8 - self.n as u8;
            self.put((1 << pad) - 1, pad);
        }
    }
}

fn category(v: i32) -> u8 {
    (32 - v.unsigned_abs().leading_zeros()) as u8
}

fn natural_quant(t: usize) -> [f32; 64] {
    let mut q = [1f32; 64];
    let src = QUANT[t];
    for i in 0..64 {
        q[ZIGZAG[i]] = src[i].max(1) as f32;
    }
    q
}

/// Прямое ДКП 8×8 (по определению, коэффициенты как в JPEG).
fn fdct(block: &[f32; 64]) -> [f32; 64] {
    let c = |u: usize, x: usize| {
        let a = if u == 0 { std::f32::consts::FRAC_1_SQRT_2 } else { 1.0 };
        a * (((2 * x + 1) * u) as f32 * std::f32::consts::PI / 16.0).cos()
    };
    let mut tmp = [0f32; 64];
    for y in 0..8 {
        for u in 0..8 {
            tmp[y * 8 + u] = (0..8).map(|x| block[y * 8 + x] * c(u, x)).sum::<f32>() / 2.0;
        }
    }
    let mut out = [0f32; 64];
    for v in 0..8 {
        for u in 0..8 {
            out[v * 8 + u] = (0..8).map(|y| tmp[y * 8 + u] * c(v, y)).sum::<f32>() / 2.0;
        }
    }
    out
}

/// Закодировать картинку `w×h` (RGB по 8 бит) потоком `jssf` — без 8 байт размера и метки.
pub fn encode(w: u32, h: u32, rgb: &[u8]) -> Vec<u8> {
    let (dcl, dcc, acl, acc) = (codes(&DC_L), codes(&DC_C), codes(&AC_L), codes(&AC_C));
    let q = [natural_quant(0), natural_quant(1)];
    let mut out = Vec::new();
    out.extend((w as u16).to_le_bytes());
    out.extend((h as u16).to_le_bytes());
    out.extend(3u16.to_le_bytes());
    out.extend(QUANT[0]);
    out.extend(QUANT[1]);
    let (bw, bh) = (w.div_ceil(8) as usize, h.div_ceil(8) as usize);
    let px = |x: usize, y: usize| {
        let (x, y) = (x.min(w as usize - 1), y.min(h as usize - 1));
        let i = (y * w as usize + x) * 3;
        [rgb[i] as f32, rgb[i + 1] as f32, rgb[i + 2] as f32]
    };
    for by in 0..bh {
        let mut bits = BitWriter { out: Vec::new(), acc: 0, n: 0 };
        let mut pred = [0i32; 3];
        for bx in 0..bw {
            let mut planes = [[0f32; 64]; 3];
            for y in 0..8 {
                for x in 0..8 {
                    let [r, g, b] = px(bx * 8 + x, by * 8 + y);
                    planes[0][y * 8 + x] = 0.299 * r + 0.587 * g + 0.114 * b - 128.0;
                    planes[1][y * 8 + x] = -0.168_736 * r - 0.331_264 * g + 0.5 * b;
                    planes[2][y * 8 + x] = 0.5 * r - 0.418_688 * g - 0.081_312 * b;
                }
            }
            for (c, plane) in planes.iter().enumerate() {
                let (dc_t, ac_t, qt) = if c == 0 { (&dcl, &acl, &q[0]) } else { (&dcc, &acc, &q[1]) };
                let f = fdct(plane);
                let coef: [i32; 64] = std::array::from_fn(|i| (f[ZIGZAG[i]] / qt[ZIGZAG[i]]).round() as i32);
                let diff = coef[0] - pred[c];
                pred[c] = coef[0];
                let s = category(diff);
                bits.put(dc_t[s as usize].0 as u32, dc_t[s as usize].1);
                bits.put((if diff < 0 { diff - 1 } else { diff }) as u32, s);
                let mut run = 0;
                for &v in &coef[1..] {
                    if v == 0 {
                        run += 1;
                        continue;
                    }
                    while run > 15 {
                        bits.put(ac_t[0xf0].0 as u32, ac_t[0xf0].1);
                        run -= 16;
                    }
                    let s = category(v);
                    let rs = (run << 4) | s as usize;
                    bits.put(ac_t[rs].0 as u32, ac_t[rs].1);
                    bits.put((if v < 0 { v - 1 } else { v }) as u32, s);
                    run = 0;
                }
                if coef[63] == 0 {
                    bits.put(ac_t[0].0 as u32, ac_t[0].1);
                }
            }
        }
        bits.flush();
        let n = bits.out.len();
        if n > 0x7fff {
            out.extend(((n >> 16) as u16 | 0x8000).to_le_bytes());
        }
        out.extend((n as u16).to_le_bytes());
        out.extend(bits.out);
    }
    out
}

/// Блок `thum` целиком: u32 ширина, u32 высота, метка `jssf`, поток.
pub fn thum_block(w: u32, h: u32, rgb: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(w.to_le_bytes());
    out.extend(h.to_le_bytes());
    out.extend(b"jssf");
    out.extend(encode(w, h, rgb));
    out
}

/// Миниатюра холста по сведённому RGBA (без предумножения): уменьшение вдвое `level` раз
/// средним по квадрату, прозрачное — на белом. Ширина и высота округляются вверх.
pub fn thumbnail(rgba: &[u8], w: u32, h: u32, level: u32) -> (u32, u32, Vec<u8>) {
    let k = 1usize << level;
    let (tw, th) = (w.div_ceil(k as u32), h.div_ceil(k as u32));
    let mut rgb = vec![0u8; (tw * th * 3) as usize];
    for ty in 0..th as usize {
        for tx in 0..tw as usize {
            let mut acc = [0f64; 4];
            let mut n = 0f64;
            for y in ty * k..((ty + 1) * k).min(h as usize) {
                for x in tx * k..((tx + 1) * k).min(w as usize) {
                    let p = &rgba[(y * w as usize + x) * 4..][..4];
                    let a = p[3] as f64 / 255.0;
                    for c in 0..3 {
                        acc[c] += p[c] as f64 * a;
                    }
                    acc[3] += a;
                    n += 1.0;
                }
            }
            let a = acc[3] / n;
            for c in 0..3 {
                rgb[(ty * tw as usize + tx) * 3 + c] = (acc[c] / n + 255.0 * (1.0 - a)).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    (tw, th, rgb)
}

/// Разобрать поток `jssf` (после метки) в RGB: ширина, высота, точки.
pub fn decode(data: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let u16_at = |i: usize| data.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let (w, h, nc) = (u16_at(0)? as usize, u16_at(2)? as usize, u16_at(4)?);
    if nc != 3 || w == 0 || h == 0 {
        return None;
    }
    let mut q = [[0f32; 64]; 2];
    for (t, qt) in q.iter_mut().enumerate() {
        let src = data.get(6 + 64 * t..6 + 64 * (t + 1))?;
        for i in 0..64 {
            qt[ZIGZAG[i]] = src[i] as f32;
        }
    }
    let lookup = |t: &([u8; 16], &[u8])| -> Lookup {
        let c = codes(t);
        let mut m = Lookup::new();
        for (v, &(code, len)) in c.iter().enumerate() {
            if len > 0 {
                m.insert((len, code), v as u8);
            }
        }
        m
    };
    let (dcl, dcc, acl, acc) = (lookup(&DC_L), lookup(&DC_C), lookup(&AC_L), lookup(&AC_C));
    let (bw, bh) = (w.div_ceil(8), h.div_ceil(8));
    let mut planes = vec![[0f32; 3]; bw * 8 * bh * 8];
    let mut pos = 134;
    for by in 0..bh {
        let mut n = u16_at(pos)? as usize;
        pos += 2;
        if n & 0x8000 != 0 {
            n = ((n & 0x7fff) << 16) | u16_at(pos)? as usize;
            pos += 2;
        }
        let raw = data.get(pos..pos + n)?;
        pos += n;
        let mut bytes = Vec::with_capacity(n);
        let mut i = 0;
        while i < raw.len() {
            bytes.push(raw[i]);
            i += if raw[i] == 0xff && raw.get(i + 1) == Some(&0) { 2 } else { 1 };
        }
        let mut rd = BitReader { bytes: &bytes, bit: 0 };
        let mut pred = [0i32; 3];
        for bx in 0..bw {
            for c in 0..3 {
                let (dt, at) = if c == 0 { (&dcl, &acl) } else { (&dcc, &acc) };
                let ext = |v: u32, s: u8| if s == 0 { 0 } else if v >= 1 << (s - 1) { v as i32 } else { v as i32 - (1 << s) + 1 };
                let mut coef = [0f32; 64];
                let s = rd.huff(dt)?;
                let v = rd.read(s)?;
                pred[c] += ext(v, s);
                coef[0] = pred[c] as f32;
                let mut k = 1;
                while k < 64 {
                    let rs = rd.huff(at)?;
                    let (r, s) = ((rs >> 4) as usize, rs & 15);
                    if s == 0 {
                        if r == 15 {
                            k += 16;
                            continue;
                        }
                        break;
                    }
                    k += r;
                    if k > 63 {
                        return None;
                    }
                    let v = rd.read(s)?;
                    coef[ZIGZAG[k]] = ext(v, s) as f32;
                    k += 1;
                }
                let qt = &q[(c > 0) as usize];
                let block: [f32; 64] = std::array::from_fn(|i| coef[i] * qt[i]);
                let img = idct(&block);
                for y in 0..8 {
                    for x in 0..8 {
                        planes[(by * 8 + y) * bw * 8 + bx * 8 + x][c] = img[y * 8 + x] + if c == 0 { 128.0 } else { 0.0 };
                    }
                }
            }
        }
    }
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            let [yy, cb, cr] = planes[y * bw * 8 + x];
            for v in [yy + 1.402 * cr, yy - 0.344_136 * cb - 0.714_136 * cr, yy + 1.772 * cb] {
                rgb.push(v.round().clamp(0.0, 255.0) as u8);
            }
        }
    }
    Some((w as u32, h as u32, rgb))
}

type Lookup = std::collections::HashMap<(u8, u16), u8>;

struct BitReader<'a> {
    bytes: &'a [u8],
    bit: usize,
}

impl BitReader<'_> {
    fn read(&mut self, len: u8) -> Option<u32> {
        let mut v = 0u32;
        for _ in 0..len {
            let b = (self.bytes.get(self.bit >> 3)? >> (7 - (self.bit & 7))) & 1;
            v = (v << 1) | b as u32;
            self.bit += 1;
        }
        Some(v)
    }

    fn huff(&mut self, t: &Lookup) -> Option<u8> {
        let mut code = 0u16;
        for len in 1..=16u8 {
            code = (code << 1) | self.read(1)? as u16;
            if let Some(v) = t.get(&(len, code)) {
                return Some(*v);
            }
        }
        None
    }
}

fn idct(block: &[f32; 64]) -> [f32; 64] {
    let c = |u: usize, x: usize| {
        let a = if u == 0 { std::f32::consts::FRAC_1_SQRT_2 } else { 1.0 };
        a * (((2 * x + 1) * u) as f32 * std::f32::consts::PI / 16.0).cos()
    };
    let mut tmp = [0f32; 64];
    for v in 0..8 {
        for x in 0..8 {
            tmp[v * 8 + x] = (0..8).map(|u| block[v * 8 + u] * c(u, x)).sum::<f32>() / 2.0;
        }
    }
    let mut out = [0f32; 64];
    for y in 0..8 {
        for x in 0..8 {
            out[y * 8 + x] = (0..8).map(|v| tmp[v * 8 + x] * c(v, y)).sum::<f32>() / 2.0;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_follow_sai() {
        assert_eq!(thumb_level(500, 500), 0);
        assert_eq!(thumb_level(1000, 1000), 1);
        assert_eq!(thumb_level(2300, 4096), 4);
        assert_eq!(thumbnail(&vec![255; 2300 * 64 * 4], 2300, 64, 4).0, 144);
    }

    #[test]
    fn round_trip_is_close() {
        let (w, h) = (37u32, 21u32);
        let mut rgb = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let edge = if (x / 6 + y / 5) % 2 == 0 { 255 } else { 0 };
                rgb.extend([(x * 7) as u8, edge, (y * 12) as u8]);
            }
        }
        let data = encode(w, h, &rgb);
        let (dw, dh, back) = decode(&data).expect("разбор своего потока");
        assert_eq!((dw, dh), (w, h));
        let mean = back.iter().zip(&rgb).map(|(a, b)| (*a as i32 - *b as i32).abs()).sum::<i32>() as f64 / rgb.len() as f64;
        assert!(mean < 6.0, "средняя разница {mean}");
    }
}
