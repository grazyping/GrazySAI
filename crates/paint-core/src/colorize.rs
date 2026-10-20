//! Раскраска по контуру (как «Colorize mask» в Krita, но результат — обычные пиксели): цвета
//! подсказок растекаются по рисунку, а линии контура служат преградой.
//!
//! Способ — заливка «водоразделом»: каждая точка получает цвет той подсказки, до которой можно
//! дойти, переступив через самую слабую линию. Точки обходятся по возрастанию «высоты» (темноты
//! линии), поэтому цвета сначала заполняют свои области целиком и встречаются на линиях, а не в
//! разрывах: небольшая щель в контуре не даёт цвету протечь, если по обе её стороны есть подсказки.

/// Высота у линий расплывается на `gap` пикселей с убыванием: щель уже 2·`gap` становится
/// преградой средней высоты, и цвет через неё проходит только после того, как своя область залита.
pub fn spread_walls(walls: &[u8], w: usize, h: usize, gap: usize) -> Vec<u8> {
    if gap == 0 {
        return walls.to_vec();
    }
    let k: Vec<u32> = (0..=gap).map(|d| (256 * (gap + 1 - d) / (gap + 1)) as u32).collect();
    let pass = |src: &[u8], step: usize, len: usize, lines: usize, stride: usize| -> Vec<u8> {
        let mut out = src.to_vec();
        for line in 0..lines {
            let base = line * stride;
            for i in 0..len {
                let mut best = src[base + i * step] as u32;
                for (d, &kd) in k.iter().enumerate().skip(1) {
                    let v = |j: usize| (src[base + j * step] as u32 * kd) >> 8;
                    if i >= d {
                        best = best.max(v(i - d));
                    }
                    if i + d < len {
                        best = best.max(v(i + d));
                    }
                }
                out[base + i * step] = best as u8;
            }
        }
        out
    };
    let rows = pass(walls, 1, w, h, w);
    pass(&rows, w, h, w, 1)
}

/// `walls` — высота каждой точки (0 — пусто, 255 — самая тёмная линия), `seeds` — номер подсказки
/// в точке (0 — нет). Возвращает номер подсказки для каждой точки (0 — ни одна не дошла: подсказок
/// нет вовсе).
pub fn watershed(walls: &[u8], seeds: &[u16], w: usize, h: usize) -> Vec<u16> {
    let n = w * h;
    assert!(walls.len() == n && seeds.len() == n);
    let mut label = seeds.to_vec();
    // очередь по уровням: внутри уровня — по порядку добавления (фронты идут ровно)
    let mut buckets: Vec<Vec<u32>> = vec![Vec::new(); 256];
    let mut read = [0usize; 256];
    for (i, &s) in seeds.iter().enumerate() {
        if s != 0 {
            buckets[walls[i] as usize].push(i as u32);
        }
    }
    let mut level = 0;
    while level < 256 {
        let Some(&i) = buckets[level].get(read[level]) else {
            buckets[level] = Vec::new();
            level += 1;
            continue;
        };
        read[level] += 1;
        let i = i as usize;
        let (x, y) = (i % w, i / w);
        let l = label[i];
        let mut visit = |j: usize, buckets: &mut Vec<Vec<u32>>| {
            if label[j] == 0 {
                label[j] = l;
                let lv = (walls[j] as usize).max(level);
                buckets[lv].push(j as u32);
            }
        };
        if x > 0 {
            visit(i - 1, &mut buckets);
        }
        if x + 1 < w {
            visit(i + 1, &mut buckets);
        }
        if y > 0 {
            visit(i - w, &mut buckets);
        }
        if y + 1 < h {
            visit(i + w, &mut buckets);
        }
        // новые точки могли лечь на этот же уровень — продолжаем с него
    }
    label
}

/// Раскраска целиком: `lines` и `hints` — RGBA без предумножения `w`×`h`. Каждый свой цвет
/// подсказок (непрозрачность от половины) — своя область. Возвращает непрозрачный RGBA результата и
/// число цветов; `None` — подсказок нет.
pub fn colorize(lines: &[u8], hints: &[u8], w: usize, h: usize, gap: usize) -> Option<(Vec<u8>, usize)> {
    let mut colors: Vec<[u8; 3]> = Vec::new();
    let mut seeds = vec![0u16; w * h];
    for (i, c) in hints.as_chunks::<4>().0.iter().enumerate() {
        if c[3] < 128 {
            continue;
        }
        let rgb = [c[0], c[1], c[2]];
        let n = match colors.iter().position(|&k| k == rgb) {
            Some(n) => n,
            None if colors.len() < u16::MAX as usize - 1 => {
                colors.push(rgb);
                colors.len() - 1
            }
            None => continue,
        };
        seeds[i] = n as u16 + 1;
    }
    if colors.is_empty() {
        return None;
    }
    let walls: Vec<u8> = lines.as_chunks::<4>().0.iter().map(|c| wall_of(*c)).collect();
    let labels = watershed(&spread_walls(&walls, w, h, gap), &seeds, w, h);
    let mut out = vec![0u8; w * h * 4];
    for (o, &l) in out.as_chunks_mut::<4>().0.iter_mut().zip(&labels) {
        if l != 0 {
            let c = colors[l as usize - 1];
            *o = [c[0], c[1], c[2], 255];
        }
    }
    Some((out, colors.len()))
}

/// Высота линии по пикселю контура (RGBA без предумножения): тёмное и непрозрачное — выше.
pub fn wall_of(c: [u8; 4]) -> u8 {
    let lum = (c[0] as u32 * 77 + c[1] as u32 * 150 + c[2] as u32 * 29) >> 8;
    ((255 - lum) * c[3] as u32 / 255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Квадрат, разделённый линией с щелью посередине: подсказки по обе стороны — каждая половина
    /// своего цвета, щель не пропускает цвет.
    #[test]
    fn colors_meet_on_the_line_not_in_the_gap() {
        let (w, h) = (21, 9);
        let mut walls = vec![0u8; w * h];
        for y in 0..h {
            if y != 4 {
                walls[y * w + 10] = 255;
            }
        }
        let mut seeds = vec![0u16; w * h];
        seeds[2 * w + 2] = 1;
        seeds[6 * w + 18] = 2;
        let l = watershed(&spread_walls(&walls, w, h, 2), &seeds, w, h);
        for y in 0..h {
            for x in 0..w {
                if x < 10 {
                    assert_eq!(l[y * w + x], 1, "({x}, {y})");
                } else if x > 10 {
                    assert_eq!(l[y * w + x], 2, "({x}, {y})");
                }
            }
        }
    }

    /// Рисунок как у художника: два сглаженных круга (у одного щель 2 px) на прозрачном фоне,
    /// по пятну в каждом и одно снаружи. Каждый круг залит своим цветом, фон — третьим.
    #[test]
    fn circles_with_a_gap_keep_their_colors() {
        let (w, h) = (120usize, 70usize);
        let mut lines = vec![0u8; w * h * 4];
        for (cx, cy, gap) in [(32.0f32, 35.0f32, false), (88.0, 35.0, true)] {
            for y in 0..h {
                for x in 0..w {
                    let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                    let d = ((dx * dx + dy * dy).sqrt() - 24.0).abs();
                    let a = (2.0 - d).clamp(0.0, 1.0);
                    if gap && dy.abs() < 1.5 && dx > 0.0 {
                        continue;
                    }
                    let i = (y * w + x) * 4;
                    lines[i + 3] = lines[i + 3].max((a * 255.0) as u8);
                }
            }
        }
        let mut hints = vec![0u8; w * h * 4];
        for (x, y, c) in [(30, 30, [255, 0, 0]), (90, 40, [0, 0, 255]), (60, 5, [0, 255, 0])] {
            let i = (y * w + x) * 4;
            hints[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
        let (out, n) = colorize(&lines, &hints, w, h, 3).unwrap();
        assert_eq!(n, 3);
        let at = |x: usize, y: usize| [out[(y * w + x) * 4], out[(y * w + x) * 4 + 1], out[(y * w + x) * 4 + 2]];
        if let Ok(p) = std::env::var("COLORIZE_PNG") {
            crate::io::save_image(std::path::Path::new(&p), w as u32, h as u32, &out).unwrap();
        }
        for (x, y) in [(32, 35), (20, 20), (44, 50)] {
            assert_eq!(at(x, y), [255, 0, 0], "левый круг ({x}, {y})");
        }
        for (x, y) in [(88, 35), (100, 35), (76, 50)] {
            assert_eq!(at(x, y), [0, 0, 255], "правый круг со щелью ({x}, {y})");
        }
        for (x, y) in [(2, 2), (60, 35), (117, 35), (60, 68)] {
            assert_eq!(at(x, y), [0, 255, 0], "снаружи ({x}, {y})");
        }
    }

    #[test]
    fn darker_and_more_opaque_is_a_higher_wall() {
        assert_eq!(wall_of([0, 0, 0, 255]), 255);
        assert_eq!(wall_of([255, 255, 255, 255]), 0);
        assert_eq!(wall_of([0, 0, 0, 0]), 0);
        assert!(wall_of([0, 0, 0, 128]) < wall_of([0, 0, 0, 255]));
    }
}
