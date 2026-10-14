//! Общая часть проверки пера: холст, текущий штрих, журнал событий. От системы не зависит.

use paint_core::tile::TILE;
use paint_core::{io, BrushParams, Document, InputPoint, Stroke};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::Instant;

pub const CANVAS_W: u32 = 1920;
pub const CANVAS_H: u32 = 1200;

pub fn exe_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap_or_default()
}

pub fn timestamp() -> String {
    // Без внешних библиотек: секунды с 1970 года переводим в дату вручную (UTC).
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (days, rem) = (secs / 86400, secs % 86400);
    let z = days as i64 + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}utc", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Одно событие пера в едином для всех систем виде.
pub struct PenEvent<'a> {
    pub source: &'a str,
    pub event: &'a str,
    pub x: f32,
    pub y: f32,
    pub pressure_raw: u32,
    pub pressure: f32,
    pub tilt: (i32, i32),
    pub rotation: i32,
    pub flags: u32,
    pub buttons: u32,
    pub dev_time: u64,
}

pub struct App {
    pub doc: Document,
    /// Сведённая картинка без текущего штриха, BGRA.
    base: Vec<u8>,
    pub stroke: Option<Stroke>,
    stroke_is_eraser: bool,
    pub size: f32,
    log: BufWriter<File>,
    pub log_path: PathBuf,
    started: Instant,
    pub events: u64,
    window_start: Instant,
    window_events: u32,
    pub rate: u32,
    pub last: String,
    pub last_pressure: f32,
    #[cfg_attr(not(windows), allow(dead_code))]
    pub mode: String,
    /// Область холста, которую нужно перерисовать: лево, верх, право, низ (право и низ не включаются).
    pub dirty: Option<[i32; 4]>,
    stroke_box: Option<[i32; 4]>,
}

fn grow(b: &mut Option<[i32; 4]>, r: [i32; 4]) {
    *b = Some(match *b {
        Some(o) => [o[0].min(r[0]), o[1].min(r[1]), o[2].max(r[2]), o[3].max(r[3])],
        None => r,
    });
}

impl App {
    pub fn new(mode: &str) -> std::io::Result<Self> {
        // Имя с режимом и, при совпадении, с номером — чтобы запуски в одну секунду не затирали друг друга.
        let stamp = timestamp();
        let mut log_path = exe_dir().join(format!("pen-log-{stamp}-{mode}.csv"));
        let mut n = 2;
        while log_path.exists() {
            log_path = exe_dir().join(format!("pen-log-{stamp}-{mode}-{n}.csv"));
            n += 1;
        }
        let mut log = BufWriter::new(File::create(&log_path)?);
        writeln!(log, "# pen-test {} mode={mode}", env!("CARGO_PKG_VERSION"))?;
        writeln!(log, "t_ms,source,event,x,y,pressure_raw,pressure,tilt_x,tilt_y,rotation,flags,buttons,dev_time")?;
        let mut app = Self {
            base: Vec::new(),
            doc: Document::new(CANVAS_W, CANVAS_H),
            stroke: None,
            stroke_is_eraser: false,
            size: 12.0,
            log,
            log_path,
            started: Instant::now(),
            events: 0,
            window_start: Instant::now(),
            window_events: 0,
            rate: 0,
            last: String::from("нет событий"),
            last_pressure: 0.0,
            mode: mode.to_string(),
            dirty: None,
            stroke_box: None,
        };
        app.rebuild_base();
        Ok(app)
    }

    pub fn note(&mut self, text: &str) {
        let _ = writeln!(self.log, "# {text}");
        let _ = self.log.flush();
    }

    pub fn rebuild_base(&mut self) {
        let mut img = self.doc.flatten();
        for px in img.as_chunks_mut::<4>().0 {
            px.swap(0, 2); // RGBA -> BGRA
        }
        self.base = img;
    }

    pub fn mark_all_dirty(&mut self) {
        self.dirty = Some([0, 0, CANVAS_W as i32, CANVAS_H as i32]);
    }

    pub fn record(&mut self, e: &PenEvent) {
        let t = self.started.elapsed().as_secs_f64() * 1000.0;
        let _ = writeln!(
            self.log,
            "{t:.2},{},{},{:.2},{:.2},{},{:.4},{},{},{},{},{},{}",
            e.source, e.event, e.x, e.y, e.pressure_raw, e.pressure, e.tilt.0, e.tilt.1, e.rotation, e.flags, e.buttons, e.dev_time
        );
        self.events += 1;
        self.window_events += 1;
        self.tick();
        self.last_pressure = e.pressure;
        self.last = format!(
            "{} {}  x={:.1} y={:.1}  нажим={:.3} ({})  наклон={},{}  поворот={}  флаги={:#x}",
            e.source, e.event, e.x, e.y, e.pressure, e.pressure_raw, e.tilt.0, e.tilt.1, e.rotation, e.flags
        );
    }

    /// Обновляет счётчик событий в секунду; вызывать и без событий, чтобы он спадал до нуля.
    pub fn tick(&mut self) {
        if self.window_start.elapsed().as_millis() >= 1000 {
            self.rate = self.window_events;
            self.window_events = 0;
            self.window_start = Instant::now();
        }
    }

    pub fn pen_down(&mut self, eraser: bool) {
        let params = BrushParams { size: if eraser { self.size * 2.5 } else { self.size }, erase: eraser, min_size: 0.05, ..Default::default() };
        self.stroke = Some(Stroke::new(params));
        self.stroke_is_eraser = eraser;
        self.stroke_box = None;
    }

    pub fn pen_move(&mut self, x: f32, y: f32, pressure: f32) {
        let Some(s) = &mut self.stroke else { return };
        let before = s.last_point();
        s.add_point(InputPoint { x, y, pressure });
        let pad = s.params.size + 3.0;
        let (px, py) = before.map(|p| (p.x, p.y)).unwrap_or((x, y));
        let r = [(x.min(px) - pad).floor() as i32, (y.min(py) - pad).floor() as i32, (x.max(px) + pad).ceil() as i32, (y.max(py) + pad).ceil() as i32];
        grow(&mut self.dirty, r);
        grow(&mut self.stroke_box, r);
    }

    pub fn pen_up(&mut self) {
        if let Some(s) = self.stroke.take() {
            self.doc.commit_stroke(0, &s, [20, 20, 20]);
            self.rebuild_base();
            if let Some(b) = self.stroke_box.take() {
                grow(&mut self.dirty, b);
            }
        }
        let _ = self.log.flush();
    }

    pub fn clear(&mut self) {
        self.doc = Document::new(CANVAS_W, CANVAS_H);
        self.stroke = None;
        self.rebuild_base();
        self.mark_all_dirty();
        self.note("холст очищен");
    }

    pub fn undo(&mut self) {
        self.doc.undo();
        self.rebuild_base();
        self.mark_all_dirty();
    }

    pub fn save_png(&mut self) {
        let path = exe_dir().join(format!("pen-test-{}.png", timestamp()));
        let result = io::save_png(&path, CANVAS_W, CANVAS_H, &self.doc.flatten());
        self.note(&format!("сохранение {}: {}", path.display(), if result.is_ok() { "успешно" } else { "ошибка" }));
    }

    /// Обрезает прямоугольник по холсту; `None`, если от него ничего не осталось.
    pub fn clip(r: [i32; 4]) -> Option<[i32; 4]> {
        let c = [r[0].max(0), r[1].max(0), r[2].min(CANVAS_W as i32), r[3].min(CANVAS_H as i32)];
        (c[0] < c[2] && c[1] < c[3]).then_some(c)
    }

    /// Кусок картинки для показа (BGRA): готовое изображение плюс незавершённый штрих.
    /// Прямоугольник должен лежать внутри холста.
    pub fn frame_rect(&self, r: [i32; 4]) -> Vec<u8> {
        let (w, h) = ((r[2] - r[0]) as usize, (r[3] - r[1]) as usize);
        let mut img = vec![0u8; w * h * 4];
        for row in 0..h {
            let src = ((r[1] as usize + row) * CANVAS_W as usize + r[0] as usize) * 4;
            img[row * w * 4..(row + 1) * w * 4].copy_from_slice(&self.base[src..src + w * 4]);
        }
        let Some(s) = &self.stroke else { return img };
        let color: [u32; 3] = if self.stroke_is_eraser { [255, 255, 255] } else { [20, 20, 20] };
        for (&(tx, ty), tile) in &s.mask.tiles {
            let (ox, oy) = (tx * TILE, ty * TILE);
            if ox >= r[2] || oy >= r[3] || ox + TILE <= r[0] || oy + TILE <= r[1] {
                continue;
            }
            for (i, &m) in tile.0.iter().enumerate() {
                if m == 0 {
                    continue;
                }
                let (x, y) = (ox + i as i32 % TILE, oy + i as i32 / TILE);
                if x < r[0] || y < r[1] || x >= r[2] || y >= r[3] {
                    continue;
                }
                let o = ((y - r[1]) as usize * w + (x - r[0]) as usize) * 4;
                let m = m as u32;
                for c in 0..3 {
                    img[o + c] = ((color[c] * m + img[o + c] as u32 * (32768 - m)) / 32768) as u8;
                }
            }
        }
        img
    }

    pub fn frame(&self) -> Vec<u8> {
        self.frame_rect([0, 0, CANVAS_W as i32, CANVAS_H as i32])
    }

    pub fn status_line(&self) -> String {
        format!("событий: {}   в секунду: {}   нажим: {:.3}   кисть: {:.0} px", self.events, self.rate, self.last_pressure, self.size)
    }
}

/// Проверка без окна: рисует линию с нажимом, пишет журнал и картинку.
pub fn selftest(out: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new("selftest")?;
    app.note("самопроверка без окна");
    app.pen_down(false);
    for i in 0..=200 {
        let t = i as f32 / 200.0;
        let (x, y, p) = (100.0 + t * 800.0, 300.0 + (t * 9.0).sin() * 120.0, (t * std::f32::consts::PI).sin());
        app.record(&PenEvent { source: "selftest", event: if i == 0 { "down" } else { "move" }, x, y, pressure_raw: (p * 1024.0) as u32, pressure: p, tilt: (0, 0), rotation: 0, flags: 0, buttons: 0, dev_time: 0 });
        app.pen_move(x, y, p);
    }
    // Кусок кадра с незавершённым штрихом должен совпадать с тем же местом полного кадра.
    let part = App::clip(app.dirty.unwrap_or([0, 0, 1, 1])).ok_or("пустая область перерисовки")?;
    let (full, piece) = (app.frame(), app.frame_rect(part));
    let w = (part[2] - part[0]) as usize;
    for row in 0..(part[3] - part[1]) as usize {
        let src = ((part[1] as usize + row) * CANVAS_W as usize + part[0] as usize) * 4;
        if full[src..src + w * 4] != piece[row * w * 4..(row + 1) * w * 4] {
            return Err("кусок кадра не совпал с полным кадром".into());
        }
    }
    if !piece.as_chunks::<4>().0.iter().any(|p| p[0] < 100) {
        return Err("в области перерисовки нет штриха".into());
    }
    app.pen_up();
    io::save_png(std::path::Path::new(out), CANVAS_W, CANVAS_H, &app.doc.flatten())?;
    let events = app.events;
    app.note(&format!("самопроверка: событий {events}, картинка {out}"));
    Ok(())
}
