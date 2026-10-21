//! Фильтр GrazySAI на WebAssembly: размытие квадратом и сепия.
//!
//! Программа вызывает `gs_alloc` один раз и кладёт в эту память плитку RGBA (по 8 бит, без
//! предумножения, построчно), затем зовёт функцию фильтра с указателем на плитку, её размером и
//! параметрами (f64 в порядке полей команды в `plugin.txt`). Фильтр меняет плитку на месте.

/// Память под плитку и параметры: отдаётся программе и больше не освобождается.
#[no_mangle]
pub extern "C" fn gs_alloc(size: u32) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(size as usize);
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

fn tile<'a>(ptr: *mut u8, w: u32, h: u32) -> &'a mut [u8] {
    unsafe { std::slice::from_raw_parts_mut(ptr, (w * h * 4) as usize) }
}

fn params<'a>(ptr: *const f64, n: u32) -> &'a [f64] {
    unsafe { std::slice::from_raw_parts(ptr, n as usize) }
}

/// Размытие квадратом радиуса `params[0]` (в `plugin.txt` у команды `margin` не меньше радиуса:
/// по краям плитки нужны соседние точки). Два прохода — по строкам и по столбцам.
#[no_mangle]
pub extern "C" fn blur(ptr: *mut u8, w: u32, h: u32, p: *const f64, n: u32) {
    let px = tile(ptr, w, h);
    let r = params(p, n).first().copied().unwrap_or(4.0).clamp(1.0, 64.0) as i32;
    let (w, h) = (w as i32, h as i32);
    // предумножить, чтобы прозрачные точки не красили соседей
    let mut buf: Vec<[f32; 4]> = px.chunks_exact(4).map(|c| {
        let a = c[3] as f32 / 255.0;
        [c[0] as f32 * a, c[1] as f32 * a, c[2] as f32 * a, c[3] as f32]
    }).collect();
    let pass = |src: &Vec<[f32; 4]>, horizontal: bool| -> Vec<[f32; 4]> {
        let mut out = vec![[0.0; 4]; src.len()];
        for y in 0..h {
            for x in 0..w {
                let mut s = [0.0f32; 4];
                let mut k = 0.0;
                for d in -r..=r {
                    let (sx, sy) = if horizontal { (x + d, y) } else { (x, y + d) };
                    if sx < 0 || sy < 0 || sx >= w || sy >= h {
                        continue;
                    }
                    let v = src[(sy * w + sx) as usize];
                    for c in 0..4 {
                        s[c] += v[c];
                    }
                    k += 1.0;
                }
                out[(y * w + x) as usize] = s.map(|v| v / k);
            }
        }
        out
    };
    buf = pass(&buf, true);
    buf = pass(&buf, false);
    for (c, v) in px.chunks_exact_mut(4).zip(buf) {
        let a = v[3] / 255.0;
        let un = |x: f32| if a > 0.0 { (x / a).round().clamp(0.0, 255.0) as u8 } else { 0 };
        c.copy_from_slice(&[un(v[0]), un(v[1]), un(v[2]), v[3].round().clamp(0.0, 255.0) as u8]);
    }
}

/// Сепия с силой `params[0]` (0..100 %).
#[no_mangle]
pub extern "C" fn sepia(ptr: *mut u8, w: u32, h: u32, p: *const f64, n: u32) {
    let k = (params(p, n).first().copied().unwrap_or(100.0) / 100.0).clamp(0.0, 1.0) as f32;
    for c in tile(ptr, w, h).chunks_exact_mut(4) {
        let (r, g, b) = (c[0] as f32, c[1] as f32, c[2] as f32);
        let t = [0.393 * r + 0.769 * g + 0.189 * b, 0.349 * r + 0.686 * g + 0.168 * b, 0.272 * r + 0.534 * g + 0.131 * b];
        for i in 0..3 {
            c[i] = (c[i] as f32 + (t[i].min(255.0) - c[i] as f32) * k).round() as u8;
        }
    }
}
