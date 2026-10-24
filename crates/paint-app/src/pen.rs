//! Нажим пера. Оконная библиотека (winit 0.30) сама его не отдаёт на Linux, поэтому:
//!
//! - **Linux, X11** — отдельное соединение с X-сервером читает «сырые» события XInput 2 и берёт
//!   ось `Abs Pressure` планшета. Так же устроено в PhotoCraft (crate `photocraft-tablet`,
//!   MIT/Apache-2.0) — подход взят оттуда, код свой. В сеансе Wayland окно открывается через
//!   XWayland: напрямую композитор не даёт перо программам без протокола `zwp_tablet_v2`.
//! - **Windows** — WinTab (`Wintab32.dll` ставит драйвер планшета: Wacom, XP-Pen, Huion,
//!   Gaomon, Veikk и др.): свой поток открывает контекст планшета и забирает пакеты — нажим,
//!   наклон, поворот, обратный конец пера; положение — от курсора системы, как у мыши. Если
//!   WinTab нет (или выключен в настройках) — Windows Ink: winit превращает перо в касание с силой
//!   (`pressure / 1024`), egui отдаёт `Touch { force }`, его читает само окно (`main.rs`).
//!
//! Наклон и поворот идут в динамику кисти (входы «Наклон пера», «Куда наклонено перо»,
//! «Поворот пера»).

use crate::i18n::tr;
#[cfg(xdg)]
use crate::i18n::trf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct PenState {
    pub pressure: f32,
    pub eraser: bool,
    /// Наклон пера 0..1 (1 — 60° и больше) и его направление на экране (радианы, `atan2`,
    /// y вниз); поворот пера вокруг оси — доля оборота. Без датчиков — нули.
    pub tilt: f32,
    pub tilt_heading: f32,
    pub rotation: f32,
    /// Откуда нажим: «XInput 2», «WinTab».
    pub api: &'static str,
    /// Когда пришло последнее событие пера.
    pub at: Option<Instant>,
    pub device: String,
    /// Что удалось найти при запуске — для строки состояния.
    pub info: String,
}

pub type PenFeed = Arc<Mutex<PenState>>;

pub fn new_feed() -> PenFeed {
    Arc::new(Mutex::new(PenState {
        pressure: 1.0,
        eraser: false,
        tilt: 0.0,
        tilt_heading: 0.0,
        rotation: 0.0,
        api: "",
        at: None,
        device: String::new(),
        info: String::from(tr("перо: источник не подключён")),
    }))
}

#[cfg(xdg)]
pub fn spawn(feed: PenFeed, ctx: eframe::egui::Context) {
    std::thread::spawn(move || {
        if let Err(e) = run(&feed, &ctx) {
            if let Ok(mut s) = feed.lock() {
                s.info = trf("перо: чтение XInput 2 не работает ({0})", &[&e]);
            }
        }
    });
}

#[cfg(windows)]
pub fn spawn(feed: PenFeed, ctx: eframe::egui::Context, hwnd: isize, wintab: bool) {
    if let Ok(mut s) = feed.lock() {
        s.info = String::from(tr("перо: Windows Ink (сила касания)"));
    }
    if wintab && hwnd != 0 {
        std::thread::spawn(move || wintab::run(&feed, &ctx, hwnd));
    }
}

#[cfg(not(any(xdg, windows)))]
pub fn spawn(feed: PenFeed, _ctx: eframe::egui::Context) {
    if let Ok(mut s) = feed.lock() {
        s.info = String::from(tr("перо: источник не подключён"));
    }
}

/// Наклон по двум осям (градусы, x вправо, y к себе) — доля 0..1 (60° и больше — 1) и направление.
#[cfg(xdg)]
pub fn tilt_of(tx: f32, ty: f32) -> (f32, f32) {
    ((tx.hypot(ty) / 60.0).clamp(0.0, 1.0), ty.atan2(tx))
}

#[cfg(windows)]
mod wintab {
    //! WinTab: контекст на окно программы, пакеты забираются раз в 3 мс (`WTPacketsGet`).
    //! Поля пакета и порядок — как в проверке пера (`crates/pen-test/src/win.rs`, сверено на
    //! планшете).
    use super::{PenFeed, Instant};
    use crate::i18n::{tr, trf};
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryW(name: *const u16) -> *mut c_void;
        fn GetProcAddress(lib: *mut c_void, name: *const u8) -> Option<unsafe extern "system" fn() -> isize>;
    }

    const WTI_DEFSYSCTX: u32 = 4;
    const WTI_DEVICES: u32 = 100;
    const DVC_NAME: u32 = 1;
    const DVC_NPRESSURE: u32 = 15;
    const PK_TIME: u32 = 0x0004;
    const PK_CURSOR: u32 = 0x0020;
    const PK_BUTTONS: u32 = 0x0040;
    const PK_X: u32 = 0x0080;
    const PK_Y: u32 = 0x0100;
    const PK_NORMAL_PRESSURE: u32 = 0x0400;
    const PK_ORIENTATION: u32 = 0x1000;
    const PACKET_DATA: u32 = PK_TIME | PK_CURSOR | PK_BUTTONS | PK_X | PK_Y | PK_NORMAL_PRESSURE | PK_ORIENTATION;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct LogContext {
        name: [u16; 40],
        options: u32,
        status: u32,
        locks: u32,
        msg_base: u32,
        device: u32,
        pkt_rate: u32,
        pkt_data: u32,
        pkt_mode: u32,
        move_mask: u32,
        btn_dn_mask: u32,
        btn_up_mask: u32,
        in_org: [i32; 3],
        in_ext: [i32; 3],
        out_org: [i32; 3],
        out_ext: [i32; 3],
        sens: [u32; 3],
        sys_mode: i32,
        sys_org: [i32; 2],
        sys_ext: [i32; 2],
        sys_sens: [u32; 2],
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Packet {
        time: u32,
        cursor: u32,
        buttons: u32,
        x: i32,
        y: i32,
        pressure: u32,
        /// Азимут (десятые градуса по часовой от «вверх»), высота (900 — перо стоит прямо),
        /// поворот (десятые градуса).
        orientation: [i32; 3],
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Axis {
        min: i32,
        max: i32,
        units: u32,
        resolution: u32,
    }

    pub fn run(feed: &PenFeed, ctx: &eframe::egui::Context, hwnd: isize) {
        let note = |text: String| {
            if let Ok(mut s) = feed.lock() {
                s.info = text;
            }
        };
        // SAFETY: функции берутся из Wintab32.dll по именам и вызываются с их подписями из
        // описания WinTab; структуры повторяют раскладку WinTab (проверена pen-test)
        unsafe {
            let name: Vec<u16> = "Wintab32.dll\0".encode_utf16().collect();
            let lib = LoadLibraryW(name.as_ptr());
            if lib.is_null() {
                return; // драйвер без WinTab — остаётся Windows Ink
            }
            macro_rules! sym {
                ($name:literal, $ty:ty) => {
                    match GetProcAddress(lib, concat!($name, "\0").as_ptr()) {
                        Some(f) => std::mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(f),
                        None => return,
                    }
                };
            }
            let info = sym!("WTInfoW", unsafe extern "system" fn(u32, u32, *mut c_void) -> u32);
            let open = sym!("WTOpenW", unsafe extern "system" fn(isize, *mut LogContext, i32) -> *mut c_void);
            let packets = sym!("WTPacketsGet", unsafe extern "system" fn(*mut c_void, i32, *mut c_void) -> i32);
            let queue = sym!("WTQueueSizeSet", unsafe extern "system" fn(*mut c_void, i32) -> i32);
            let mut lc: LogContext = std::mem::zeroed();
            if info(WTI_DEFSYSCTX, 0, &mut lc as *mut _ as *mut c_void) == 0 {
                return; // служба есть, планшета нет
            }
            let mut dev = [0u16; 256];
            info(WTI_DEVICES, DVC_NAME, dev.as_mut_ptr() as *mut c_void);
            let dev = String::from_utf16_lossy(&dev[..dev.iter().position(|&c| c == 0).unwrap_or(0)]);
            let mut axis = Axis::default();
            info(WTI_DEVICES, DVC_NPRESSURE, &mut axis as *mut _ as *mut c_void);
            lc.pkt_data = PACKET_DATA;
            lc.pkt_mode = 0;
            lc.move_mask = PACKET_DATA;
            let wt = open(hwnd, &mut lc, 1);
            if wt.is_null() {
                note(String::from(tr("перо: WinTab не открылся, Windows Ink")));
                return;
            }
            queue(wt, 128);
            note(trf("перо: WinTab, {0}", &[&dev]));
            let max = axis.max.max(1) as f32;
            let mut buf = [Packet::default(); 128];
            loop {
                let n = packets(wt, buf.len() as i32, buf.as_mut_ptr() as *mut c_void);
                if n > 0 {
                    let p = buf[(n as usize - 1).min(buf.len() - 1)];
                    let [az, alt, twist] = p.orientation;
                    // высота 900 — прямо; наклон — от вертикали; азимут от «вверх» по часовой
                    let lean = (90.0 - alt.abs() as f32 / 10.0).clamp(0.0, 90.0);
                    let heading = (az as f32 / 10.0 - 90.0).to_radians();
                    if let Ok(mut s) = feed.lock() {
                        s.pressure = (p.pressure as f32 / max).clamp(0.0, 1.0);
                        // у большинства драйверов курсор №2 (и 5, 8…) — обратный конец пера
                        s.eraser = p.cursor % 3 == 2;
                        s.tilt = if alt == 0 { 0.0 } else { (lean / 60.0).min(1.0) };
                        s.tilt_heading = heading;
                        s.rotation = (twist as f32 / 3600.0).rem_euclid(1.0);
                        s.api = "WinTab";
                        s.at = Some(Instant::now());
                        if s.device != dev {
                            s.device = dev.clone();
                        }
                    }
                    ctx.request_repaint();
                }
                std::thread::sleep(std::time::Duration::from_millis(3));
            }
        }
    }
}

#[cfg(xdg)]
fn run(feed: &PenFeed, ctx: &eframe::egui::Context) -> Result<(), Box<dyn std::error::Error>> {
    use std::collections::HashMap;
    use x11rb::connection::Connection;
    use x11rb::protocol::xinput::{self, ConnectionExt as _, DeviceClassData, XIEventMask};
    use x11rb::protocol::xproto::ConnectionExt as _;
    use x11rb::protocol::Event;

    struct Pen {
        name: String,
        axis: u16,
        min: f64,
        max: f64,
        eraser: bool,
        /// Оси наклона и поворота: номер, наименьшее и наибольшее.
        tilt_x: Option<(u16, f64, f64)>,
        tilt_y: Option<(u16, f64, f64)>,
        turn: Option<(u16, f64, f64)>,
    }
    let fp = |v: xinput::Fp3232| v.integral as f64 + v.frac as f64 / 4294967296.0;

    let (conn, screen) = x11rb::connect(None)?;
    let root = conn.setup().roots[screen].root;
    conn.xinput_xi_query_version(2, 2)?.reply()?;

    let query = |conn: &x11rb::rust_connection::RustConnection| -> Result<HashMap<u16, Pen>, Box<dyn std::error::Error>> {
        let mut pens = HashMap::new();
        for info in conn.xinput_xi_query_device(xinput::Device::ALL)?.reply()?.infos {
            let name = String::from_utf8_lossy(&info.name).to_string();
            let mut axes: HashMap<Vec<u8>, (u16, f64, f64)> = HashMap::new();
            for class in &info.classes {
                if let DeviceClassData::Valuator(v) = &class.data {
                    if v.label != 0 {
                        axes.insert(conn.get_atom_name(v.label)?.reply()?.name, (v.number, fp(v.min), fp(v.max)));
                    }
                }
            }
            if let Some(&(axis, min, max)) = axes.get(&b"Abs Pressure"[..]) {
                let turn = axes.get(&b"Abs Wheel"[..]).or(axes.get(&b"Abs Rotation"[..])).or(axes.get(&b"Abs Rotation Z"[..])).copied();
                pens.insert(info.deviceid, Pen { eraser: name.to_lowercase().contains("eraser"), name: name.clone(), axis, min, max, tilt_x: axes.get(&b"Abs Tilt X"[..]).copied(), tilt_y: axes.get(&b"Abs Tilt Y"[..]).copied(), turn });
            }
        }
        Ok(pens)
    };
    let describe = |pens: &HashMap<u16, Pen>| {
        if pens.is_empty() {
            String::from(tr("перо: планшет в X11 не найден, рисование мышью"))
        } else {
            trf(
                "перо: XInput 2, {0}",
                &[&pens.values().map(|p| p.name.as_str()).collect::<Vec<_>>().join("; ")],
            )
        }
    };
    let mut pens = query(&conn)?;
    if let Ok(mut s) = feed.lock() {
        s.info = describe(&pens);
    }
    let mask =
        XIEventMask::RAW_MOTION | XIEventMask::RAW_BUTTON_PRESS | XIEventMask::RAW_BUTTON_RELEASE;
    conn.xinput_xi_select_events(
        root,
        &[xinput::EventMask {
            deviceid: xinput::Device::ALL.into(),
            mask: vec![mask | XIEventMask::HIERARCHY],
        }],
    )?;
    conn.flush()?;

    loop {
        let raw = match conn.wait_for_event()? {
            Event::XinputRawMotion(e)
            | Event::XinputRawButtonPress(e)
            | Event::XinputRawButtonRelease(e) => e,
            Event::XinputHierarchy(_) => {
                // Планшет подключили или отключили.
                pens = query(&conn)?;
                if let Ok(mut s) = feed.lock() {
                    s.info = describe(&pens);
                }
                continue;
            }
            _ => continue,
        };
        let Some(pen) = pens.get(&raw.deviceid).or_else(|| pens.get(&raw.sourceid)) else {
            continue;
        };
        // Значения идут подряд только для осей, отмеченных в маске.
        let mut index = 0;
        let mut pressure = None;
        let (mut tx, mut ty, mut turn) = (None, None, None);
        for (word, bits) in raw.valuator_mask.iter().enumerate() {
            for bit in 0..32u16 {
                if bits & (1 << bit) != 0 {
                    let n = word as u16 * 32 + bit;
                    let v = raw.axisvalues.get(index).map(|v| fp(*v));
                    if n == pen.axis {
                        pressure = v;
                    }
                    if pen.tilt_x.is_some_and(|a| a.0 == n) {
                        tx = v;
                    }
                    if pen.tilt_y.is_some_and(|a| a.0 == n) {
                        ty = v;
                    }
                    if pen.turn.is_some_and(|a| a.0 == n) {
                        turn = v;
                    }
                    index += 1;
                }
            }
        }
        if let Ok(mut s) = feed.lock() {
            if let Some(v) = pressure {
                s.pressure = if pen.max > pen.min {
                    ((v - pen.min) / (pen.max - pen.min)).clamp(0.0, 1.0) as f32
                } else {
                    1.0
                };
            }
            s.eraser = pen.eraser;
            // наклон — в градусах (у Wacom и libinput оси −64…63 или −90…90)
            let deg = |v: Option<f64>, a: Option<(u16, f64, f64)>| match (v, a) {
                (Some(v), Some((_, lo, hi))) if hi > lo => Some(if hi - lo <= 181.0 { v } else { (v - (lo + hi) / 2.0) / (hi - lo) * 128.0 }),
                _ => None,
            };
            if let (Some(x), Some(y)) = (deg(tx, pen.tilt_x), deg(ty, pen.tilt_y)) {
                (s.tilt, s.tilt_heading) = tilt_of(x as f32, y as f32);
            }
            if let (Some(v), Some((_, lo, hi))) = (turn, pen.turn) {
                if hi > lo {
                    s.rotation = ((v - lo) / (hi - lo)) as f32;
                }
            }
            s.api = "XInput 2";
            s.at = Some(Instant::now());
            if s.device != pen.name {
                s.device = pen.name.clone();
            }
        }
        ctx.request_repaint();
    }
}
