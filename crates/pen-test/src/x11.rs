//! Окно и ввод на Linux через X11 и расширение XInput 2 (нажим и наклон пера приходят «осями»
//! устройства). В сеансе Wayland программа работает через XWayland.

use crate::app::{App, PenEvent};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::xinput::{self, ConnectionExt as _, DeviceClassData, XIEventMask};
use x11rb::protocol::xproto::{self, ConnectionExt as _, *};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

type Res<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Copy)]
struct Axis {
    number: u16,
    min: f64,
    max: f64,
}

#[derive(Default)]
struct Device {
    name: String,
    pressure: Option<Axis>,
    tilt_x: Option<Axis>,
    tilt_y: Option<Axis>,
    /// Последние значения осей: X присылает только изменившиеся.
    values: HashMap<u16, f64>,
    eraser: bool,
}

fn fp3232(v: xinput::Fp3232) -> f64 {
    v.integral as f64 + v.frac as f64 / 4294967296.0
}

fn query_devices(conn: &RustConnection, app: &mut App) -> Res<HashMap<u16, Device>> {
    let mut out = HashMap::new();
    let reply = conn.xinput_xi_query_device(xinput::Device::ALL)?.reply()?;
    for info in reply.infos {
        let name = String::from_utf8_lossy(&info.name).to_string();
        let mut dev = Device { eraser: name.to_lowercase().contains("eraser"), name: name.clone(), ..Default::default() };
        let mut axes = Vec::new();
        for class in &info.classes {
            if let DeviceClassData::Valuator(v) = &class.data {
                let label = if v.label == 0 { String::new() } else { String::from_utf8_lossy(&conn.get_atom_name(v.label)?.reply()?.name).to_string() };
                let axis = Axis { number: v.number, min: fp3232(v.min), max: fp3232(v.max) };
                match label.as_str() {
                    "Abs Pressure" => dev.pressure = Some(axis),
                    "Abs Tilt X" => dev.tilt_x = Some(axis),
                    "Abs Tilt Y" => dev.tilt_y = Some(axis),
                    _ => {}
                }
                axes.push(format!("{}:{label}[{}..{}]", v.number, axis.min, axis.max));
            }
        }
        app.note(&format!("устройство X11 {} вид={:?} имя={name:?} оси={}", info.deviceid, info.type_, axes.join(" ")));
        out.insert(info.deviceid, dev);
    }
    Ok(out)
}

struct Win {
    conn: RustConnection,
    window: Window,
    gc: Gcontext,
    depth: u8,
    size: (u16, u16),
    net_wm_name: Atom,
    utf8: Atom,
}

impl Win {
    /// Отправляет на экран прямоугольник холста (координаты окна и холста совпадают).
    fn present(&self, app: &App, rect: [i32; 4]) -> Res<()> {
        let visible = [rect[0], rect[1], rect[2].min(self.size.0 as i32), rect[3].min(self.size.1 as i32)];
        let Some(r) = App::clip(visible) else { return Ok(()) };
        let img = app.frame_rect(r);
        let (w, h) = ((r[2] - r[0]) as usize, (r[3] - r[1]) as usize);
        // Большую картинку шлём полосами, чтобы уложиться в предельный размер запроса.
        let rows_per_chunk = ((self.conn.maximum_request_bytes() - 64) / (w * 4)).max(1);
        let mut row = 0;
        while row < h {
            let n = rows_per_chunk.min(h - row);
            self.conn.put_image(ImageFormat::Z_PIXMAP, self.window, self.gc, w as u16, n as u16, r[0] as i16, (r[1] as usize + row) as i16, 0, self.depth, &img[row * w * 4..(row + n) * w * 4])?;
            row += n;
        }
        Ok(())
    }

    fn set_title(&self, text: &str) -> Res<()> {
        self.conn.change_property8(PropMode::REPLACE, self.window, self.net_wm_name, self.utf8, text.as_bytes())?;
        Ok(())
    }
}

fn handle_pointer(app: &mut App, devices: &mut HashMap<u16, Device>, e: &xinput::ButtonPressEvent, kind: &str) {
    let (x, y) = (e.event_x as f32 / 65536.0, e.event_y as f32 / 65536.0);
    let dev = devices.entry(e.sourceid).or_default();
    // Оси перечислены битами маски; значения идут подряд только для выставленных битов.
    let mut next = 0;
    for (word, bits) in e.valuator_mask.iter().enumerate() {
        for bit in 0..32 {
            if bits & (1 << bit) != 0 {
                if let Some(v) = e.axisvalues.get(next) {
                    dev.values.insert((word * 32 + bit) as u16, fp3232(*v));
                }
                next += 1;
            }
        }
    }
    let norm = |axis: Option<Axis>, values: &HashMap<u16, f64>| -> Option<(f64, f64)> {
        let a = axis?;
        let v = *values.get(&a.number)?;
        Some((v, if a.max > a.min { (v - a.min) / (a.max - a.min) } else { 0.0 }))
    };
    let is_pen = dev.pressure.is_some();
    let (raw, pressure) = match norm(dev.pressure, &dev.values) {
        Some((v, n)) => (v as u32, n as f32),
        None => (0, if is_pen { 0.0 } else { 0.6 }), // у мыши нажима нет — рисуем средним
    };
    let tilt = (
        dev.tilt_x.and_then(|a| dev.values.get(&a.number)).copied().unwrap_or(0.0) as i32,
        dev.tilt_y.and_then(|a| dev.values.get(&a.number)).copied().unwrap_or(0.0) as i32,
    );
    let source = if is_pen { "x11-pen" } else { "x11-mouse" };
    let eraser = dev.eraser;
    let buttons = e.button_mask.first().copied().unwrap_or(0);
    let event = match kind {
        "press" if e.detail == 1 => "down",
        "release" if e.detail == 1 => "up",
        "press" | "release" => "button",
        _ => "move",
    };
    app.record(&PenEvent { source, event, x, y, pressure_raw: raw, pressure, tilt, rotation: 0, flags: e.sourceid as u32, buttons, dev_time: e.time as u64 });
    match event {
        "down" => {
            app.pen_down(eraser);
            app.pen_move(x, y, pressure);
        }
        "up" => app.pen_up(),
        "move" if app.stroke.is_some() => app.pen_move(x, y, pressure),
        _ => {}
    }
}

pub fn run(autoclose: Option<u32>) -> Res<()> {
    let (conn, screen_num) = x11rb::connect(None).map_err(|e| format!("нет связи с X-сервером (переменная DISPLAY): {e}"))?;
    let screen = conn.setup().roots[screen_num].clone();
    let depth = screen.root_depth;
    let bpp = conn.setup().pixmap_formats.iter().find(|f| f.depth == depth).map(|f| f.bits_per_pixel).unwrap_or(0);
    if bpp != 32 {
        return Err(format!("экран с глубиной {depth} и {bpp} бит на точку не поддерживается").into());
    }

    let mut app = App::new("x11")?;
    app.note(&format!("экран: {}x{}, глубина {depth}, сеанс {}", screen.width_in_pixels, screen.height_in_pixels, std::env::var("XDG_SESSION_TYPE").unwrap_or_default()));

    let window = conn.generate_id()?;
    let size = (1280u16.min(screen.width_in_pixels), 800u16.min(screen.height_in_pixels));
    let aux = CreateWindowAux::new().background_pixel(screen.white_pixel).event_mask(EventMask::EXPOSURE | EventMask::KEY_PRESS | EventMask::STRUCTURE_NOTIFY);
    conn.create_window(x11rb::COPY_DEPTH_FROM_PARENT, window, screen.root, 0, 0, size.0, size.1, 0, WindowClass::INPUT_OUTPUT, 0, &aux)?;
    let gc = conn.generate_id()?;
    conn.create_gc(gc, window, &CreateGCAux::new())?;
    let atom = |name: &str| -> Res<Atom> { Ok(conn.intern_atom(false, name.as_bytes())?.reply()?.atom) };
    let (wm_protocols, wm_delete, net_wm_name, utf8) = (atom("WM_PROTOCOLS")?, atom("WM_DELETE_WINDOW")?, atom("_NET_WM_NAME")?, atom("UTF8_STRING")?);
    conn.change_property32(PropMode::REPLACE, window, wm_protocols, AtomEnum::ATOM, &[wm_delete])?;
    conn.change_property8(PropMode::REPLACE, window, AtomEnum::WM_CLASS, AtomEnum::STRING, b"pen-test\0PaintRsPenTest\0")?;

    let version = conn.xinput_xi_query_version(2, 2)?.reply().map_err(|e| format!("X-сервер без XInput 2: {e}"))?;
    app.note(&format!("XInput {}.{}", version.major_version, version.minor_version));
    let mut devices = query_devices(&conn, &mut app)?;
    let pens = devices.values().filter(|d| d.pressure.is_some()).map(|d| d.name.clone()).collect::<Vec<_>>();
    app.note(&format!("устройств с нажимом: {} {:?}", pens.len(), pens));
    let mask = XIEventMask::MOTION | XIEventMask::BUTTON_PRESS | XIEventMask::BUTTON_RELEASE;
    conn.xinput_xi_select_events(window, &[xinput::EventMask { deviceid: xinput::Device::ALL_MASTER.into(), mask: vec![mask] }])?;

    // Клавиши: по коду клавиши берём символ первой раскладки.
    let (min_key, max_key) = (conn.setup().min_keycode, conn.setup().max_keycode);
    let keymap = conn.get_keyboard_mapping(min_key, max_key - min_key + 1)?.reply()?;
    let keysym = |code: u8| -> u32 { keymap.keysyms.get((code - min_key) as usize * keymap.keysyms_per_keycode as usize).copied().unwrap_or(0) };

    conn.map_window(window)?;
    conn.flush()?;
    println!("window {window:#x}");
    println!("журнал: {}", app.log_path.display());

    let mut win = Win { conn, window, gc, depth, size, net_wm_name, utf8 };
    let started = Instant::now();
    let mut last_title = Instant::now() - Duration::from_secs(1);
    let mut running = true;
    while running {
        while let Some(event) = win.conn.poll_for_event()? {
            match event {
                Event::Expose(_) => app.mark_all_dirty(),
                Event::ConfigureNotify(c) => {
                    if (c.width, c.height) != win.size {
                        win.size = (c.width, c.height);
                        app.mark_all_dirty();
                    }
                }
                Event::ClientMessage(m) if m.data.as_data32()[0] == wm_delete => running = false,
                Event::KeyPress(k) => match keysym(k.detail) {
                    0xff1b => running = false,
                    0x63 => app.clear(),
                    0x7a => app.undo(),
                    0x73 => app.save_png(),
                    0x31 => app.size = 6.0,
                    0x32 => app.size = 12.0,
                    0x33 => app.size = 30.0,
                    _ => {}
                },
                Event::XinputMotion(e) => handle_pointer(&mut app, &mut devices, &e, "motion"),
                Event::XinputButtonPress(e) => handle_pointer(&mut app, &mut devices, &e, "press"),
                Event::XinputButtonRelease(e) => handle_pointer(&mut app, &mut devices, &e, "release"),
                _ => {}
            }
        }
        if let Some(rect) = app.dirty.take() {
            win.present(&app, rect)?;
        }
        if last_title.elapsed() >= Duration::from_millis(250) {
            app.tick();
            win.set_title(&format!("Проверка пера — {}  |  C очистить  Z отменить  S сохранить  1/2/3 размер  Esc выход", app.status_line()))?;
            last_title = Instant::now();
        }
        win.conn.flush()?;
        if autoclose.is_some_and(|sec| started.elapsed() >= Duration::from_secs(sec as u64)) {
            running = false;
        }
        std::thread::sleep(Duration::from_millis(4));
    }
    let total = app.events;
    app.note(&format!("выход, всего событий: {total}"));
    let _ = xproto::destroy_window(&win.conn, win.window);
    let _ = win.conn.flush();
    Ok(())
}
