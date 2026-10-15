//! Нажим пера. Оконная библиотека (winit 0.30) сама его не отдаёт на Linux, поэтому:
//!
//! - **Linux, X11** — отдельное соединение с X-сервером читает «сырые» события XInput 2 и берёт
//!   ось `Abs Pressure` планшета. Так же устроено в PhotoCraft (crate `photocraft-tablet`,
//!   MIT/Apache-2.0) — подход взят оттуда, код свой. В сеансе Wayland окно открывается через
//!   XWayland: напрямую композитор не даёт перо программам без протокола `zwp_tablet_v2`.
//! - **Windows** — winit превращает перо Windows Ink в касание с силой (`pressure / 1024`),
//!   egui отдаёт это событием `Touch { force }`; его читает само окно, см. `main.rs`.
//!   В драйвере планшета должен быть включён режим Windows Ink.

use crate::i18n::tr;
#[cfg(target_os = "linux")]
use crate::i18n::trf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct PenState {
    pub pressure: f32,
    pub eraser: bool,
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
        at: None,
        device: String::new(),
        info: String::from(tr("перо: источник не подключён")),
    }))
}

#[cfg(target_os = "linux")]
pub fn spawn(feed: PenFeed, ctx: eframe::egui::Context) {
    std::thread::spawn(move || {
        if let Err(e) = run(&feed, &ctx) {
            if let Ok(mut s) = feed.lock() {
                s.info = trf("перо: чтение XInput 2 не работает ({0})", &[&e]);
            }
        }
    });
}

#[cfg(not(target_os = "linux"))]
pub fn spawn(feed: PenFeed, _ctx: eframe::egui::Context) {
    if let Ok(mut s) = feed.lock() {
        s.info = String::from(tr("перо: Windows Ink (сила касания)"));
    }
}

#[cfg(target_os = "linux")]
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
    }
    let fp = |v: xinput::Fp3232| v.integral as f64 + v.frac as f64 / 4294967296.0;

    let (conn, screen) = x11rb::connect(None)?;
    let root = conn.setup().roots[screen].root;
    conn.xinput_xi_query_version(2, 2)?.reply()?;

    let query = |conn: &x11rb::rust_connection::RustConnection| -> Result<HashMap<u16, Pen>, Box<dyn std::error::Error>> {
        let mut pens = HashMap::new();
        for info in conn.xinput_xi_query_device(xinput::Device::ALL)?.reply()?.infos {
            let name = String::from_utf8_lossy(&info.name).to_string();
            for class in &info.classes {
                if let DeviceClassData::Valuator(v) = &class.data {
                    if v.label != 0 && conn.get_atom_name(v.label)?.reply()?.name == b"Abs Pressure" {
                        pens.insert(info.deviceid, Pen { eraser: name.to_lowercase().contains("eraser"), name: name.clone(), axis: v.number, min: fp(v.min), max: fp(v.max) });
                    }
                }
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
        for (word, bits) in raw.valuator_mask.iter().enumerate() {
            for bit in 0..32u16 {
                if bits & (1 << bit) != 0 {
                    if word as u16 * 32 + bit == pen.axis {
                        pressure = raw.axisvalues.get(index).map(|v| fp(*v));
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
            s.at = Some(Instant::now());
            if s.device != pen.name {
                s.device = pen.name.clone();
            }
        }
        ctx.request_repaint();
    }
}
