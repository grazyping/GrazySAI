//! Пиксель: RGBA с предумноженной альфой, 15 бит на канал (0..=32768).

pub const ONE: u32 = 1 << 15;
pub type Px = [u16; 4];
pub const CLEAR: Px = [0; 4];

pub fn from_rgba8(c: [u8; 4]) -> Px {
    let a = (c[3] as u32 * ONE + 127) / 255;
    let f = |v: u8| -> u16 {
        let v = (v as u32 * ONE + 127) / 255;
        ((v * a + ONE / 2) / ONE) as u16
    };
    [f(c[0]), f(c[1]), f(c[2]), a as u16]
}

pub fn to_rgba8(p: Px) -> [u8; 4] {
    let a = p[3] as u32;
    if a == 0 {
        return [0; 4];
    }
    let f = |v: u16| -> u8 { (((v as u32).min(a) * 255 + a / 2) / a) as u8 };
    [f(p[0]), f(p[1]), f(p[2]), ((a * 255 + ONE / 2) / ONE) as u8]
}

/// RGBA по 16 бит без предумножения (0..=65535).
pub fn to_rgba16(p: Px) -> [u16; 4] {
    let a = p[3] as u64;
    if a == 0 {
        return [0; 4];
    }
    let f = |v: u16| -> u16 { (((v as u64).min(a) * 65535 + a / 2) / a) as u16 };
    [f(p[0]), f(p[1]), f(p[2]), ((a * 65535 + ONE as u64 / 2) / ONE as u64) as u16]
}

/// Предумноженные доли единицы — в RGBA по 16 бит без предумножения.
pub fn f32_to_rgba16(c: [f32; 4]) -> [u16; 4] {
    let a = c[3].clamp(0.0, 1.0);
    if a <= 0.0 {
        return [0; 4];
    }
    let f = |v: f32| -> u16 { ((v / a).clamp(0.0, 1.0) * 65535.0 + 0.5) as u16 };
    [f(c[0]), f(c[1]), f(c[2]), (a * 65535.0 + 0.5) as u16]
}

/// В числа с плавающей точкой: предумноженные, 0.0..=1.0.
pub fn to_f32(p: Px) -> [f32; 4] {
    let k = 1.0 / ONE as f32;
    [p[0] as f32 * k, p[1] as f32 * k, p[2] as f32 * k, p[3] as f32 * k]
}

pub fn from_f32(c: [f32; 4]) -> Px {
    let f = |v: f32| -> u16 { (v.clamp(0.0, 1.0) * ONE as f32 + 0.5) as u16 };
    let a = f(c[3]);
    [f(c[0]).min(a), f(c[1]).min(a), f(c[2]).min(a), a]
}
