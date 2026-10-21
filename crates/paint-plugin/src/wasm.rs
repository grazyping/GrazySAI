//! Фильтры плагинов на WebAssembly (wasmi): для работы с каждой точкой, где Luau медленен.
//!
//! Модуль экспортирует `memory`, `gs_alloc(байты) -> указатель` и функцию фильтра
//! `имя(пиксели, ширина, высота, параметры, число_параметров)`: пиксели — RGBA по 8 бит без
//! предумножения, построчно; параметры — массив f64 в порядке полей команды. Программа режет
//! область на плитки (с полями `margin` вокруг — для фильтров с соседями), обрабатывает их в
//! нескольких потоках (у каждого свой экземпляр модуля) и записывает внутреннюю часть плитки,
//! смешивая с исходной по выделению. Песочница WebAssembly: модуль не видит ничего, кроме своей
//! памяти; импортов нет; память и число шагов ограничены.

use crate::{Canvas, Command, Limits, Plugin, Report};
use std::time::Instant;
use wasmi::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

const TILE: i32 = 256;

/// Готовые плитки одного потока: рамка и пиксели.
type Done = Result<Vec<([i32; 4], Vec<u8>)>, String>;

struct Data {
    limits: StoreLimits,
}

/// Выполнить команду с `wasm=` над текущим слоем.
pub fn run(plugin: &Plugin, command: &Command, params: &[(String, f64)], canvas: &mut dyn Canvas, limits: Limits) -> Result<Report, String> {
    let started = Instant::now();
    let file = command.wasm.as_deref().ok_or("у команды нет wasm")?;
    let bytes = plugin.files.iter().find(|(n, _)| n == file).map(|(_, d)| d.as_slice()).ok_or_else(|| format!("в пакете нет {file}"))?;
    let mut cfg = Config::default();
    cfg.consume_fuel(true);
    let engine = Engine::new(&cfg);
    let module = Module::new(&engine, bytes).map_err(|e| format!("{file}: {e}"))?;
    if module.imports().len() > 0 {
        return Err(format!("{file}: модулю нужны импорты ({}) — у фильтров их нет", module.imports().map(|i| format!("{}.{}", i.module(), i.name())).collect::<Vec<_>>().join(", ")));
    }
    let (w, h) = canvas.size();
    let area = canvas.selection().unwrap_or([0, 0, w as i32, h as i32]);
    let area = [area[0].max(0), area[1].max(0), area[2].min(w as i32), area[3].min(h as i32)];
    if area[2] <= area[0] || area[3] <= area[1] {
        return Ok(Report { log: Vec::new(), elapsed: started.elapsed() });
    }
    let m = command.margin as i32;
    // область с полями — в буфер (чтение холста — в одном потоке)
    let (bx0, by0, bx1, by1) = ((area[0] - m).max(0), (area[1] - m).max(0), (area[2] + m).min(w as i32), (area[3] + m).min(h as i32));
    let bw = (bx1 - bx0) as usize;
    let mut src = vec![0u8; bw * (by1 - by0) as usize * 4];
    for y in by0..by1 {
        for x in bx0..bx1 {
            let i = ((y - by0) as usize * bw + (x - bx0) as usize) * 4;
            src[i..i + 4].copy_from_slice(&canvas.get(x, y));
        }
    }
    let values: Vec<f64> = command.params.iter().map(|p| params.iter().find(|(k, _)| *k == p.id).map_or(p.default, |(_, v)| *v)).collect();
    let mut tiles = Vec::new();
    for ty in (area[1]..area[3]).step_by(TILE as usize) {
        for tx in (area[0]..area[2]).step_by(TILE as usize) {
            tiles.push([tx, ty, (tx + TILE).min(area[2]), (ty + TILE).min(area[3])]);
        }
    }
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 32).min(tiles.len());
    let next = std::sync::atomic::AtomicUsize::new(0);
    // топливо на плитку: щедро для честного фильтра (около 4·10⁸ шагов в секунду предела), но конечно
    let fuel = (limits.time.as_secs_f64() * 4.0e8).clamp(1.0e7, 4.0e9) as u64;
    let results: Vec<Done> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                let (engine, module, src, tiles, values, next) = (&engine, &module, &src, &tiles, &values, &next);
                scope.spawn(move || -> Done {
                    let mut store = Store::new(engine, Data { limits: StoreLimitsBuilder::new().memory_size(limits.memory).build() });
                    store.limiter(|d| &mut d.limits);
                    let inst = Linker::<Data>::new(engine).instantiate_and_start(&mut store, module).map_err(|e| e.to_string())?;
                    let memory = inst.get_memory(&store, "memory").ok_or("модуль не экспортирует memory")?;
                    let alloc = inst.get_typed_func::<u32, u32>(&store, "gs_alloc").map_err(|_| "модуль не экспортирует gs_alloc(u32) -> u32".to_string())?;
                    let f = inst.get_typed_func::<(u32, u32, u32, u32, u32), ()>(&store, command.function.as_str()).map_err(|_| format!("модуль не экспортирует {}(ptr, w, h, params, n)", command.function))?;
                    let max = ((TILE + 2 * m) * (TILE + 2 * m) * 4) as u32;
                    store.set_fuel(fuel).map_err(|e| e.to_string())?;
                    let pbuf = alloc.call(&mut store, max + values.len() as u32 * 8).map_err(|e| format!("gs_alloc: {e}"))?;
                    let vbuf = pbuf + max;
                    if (vbuf as usize + values.len() * 8) > memory.data_size(&store) {
                        return Err("gs_alloc вернул память за пределами модуля".to_string());
                    }
                    let pbytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
                    let mut out = Vec::new();
                    loop {
                        let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(t) = tiles.get(k) else { break };
                        // плитка с полями из буфера
                        let (x0, y0, x1, y1) = ((t[0] - m).max(bx0), (t[1] - m).max(by0), (t[2] + m).min(bx1), (t[3] + m).min(by1));
                        let tw = (x1 - x0) as usize;
                        let mut px = Vec::with_capacity(tw * (y1 - y0) as usize * 4);
                        for y in y0..y1 {
                            let i = ((y - by0) as usize * bw + (x0 - bx0) as usize) * 4;
                            px.extend_from_slice(&src[i..i + tw * 4]);
                        }
                        memory.write(&mut store, pbuf as usize, &px).map_err(|e| e.to_string())?;
                        memory.write(&mut store, vbuf as usize, &pbytes).map_err(|e| e.to_string())?;
                        store.set_fuel(fuel).map_err(|e| e.to_string())?;
                        f.call(&mut store, (pbuf, tw as u32, (y1 - y0) as u32, vbuf, values.len() as u32)).map_err(|e| {
                            let s = e.to_string();
                            if s.contains("fuel") { "фильтр работает слишком долго — остановлен".to_string() } else { format!("{}: {s}", command.function) }
                        })?;
                        memory.read(&store, pbuf as usize, &mut px).map_err(|e| e.to_string())?;
                        // внутренняя часть плитки (без полей)
                        let iw = (t[2] - t[0]) as usize;
                        let mut inner = Vec::with_capacity(iw * (t[3] - t[1]) as usize * 4);
                        for y in t[1]..t[3] {
                            let i = ((y - y0) as usize * tw + (t[0] - x0) as usize) * 4;
                            inner.extend_from_slice(&px[i..i + iw * 4]);
                        }
                        out.push((*t, inner));
                    }
                    Ok(out)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap_or_else(|_| Err("сбой потока фильтра".into()))).collect()
    });
    let mut done = Vec::new();
    for r in results {
        done.extend(r?);
        if started.elapsed() > limits.time {
            return Err("фильтр работает слишком долго — остановлен".into());
        }
    }
    // запись: смешивание с исходной точкой по выделению
    for (t, px) in done {
        let iw = (t[2] - t[0]) as usize;
        for y in t[1]..t[3] {
            for x in t[0]..t[2] {
                let i = ((y - t[1]) as usize * iw + (x - t[0]) as usize) * 4;
                let new = [px[i], px[i + 1], px[i + 2], px[i + 3]];
                let s = canvas.selected(x, y);
                if s <= 0.0 {
                    continue;
                }
                let c = if s >= 1.0 {
                    new
                } else {
                    let o = canvas.get(x, y);
                    std::array::from_fn(|k| (o[k] as f32 + (new[k] as f32 - o[k] as f32) * s).round() as u8)
                };
                canvas.set(x, y, c);
            }
        }
    }
    Ok(Report { log: Vec::new(), elapsed: started.elapsed() })
}
