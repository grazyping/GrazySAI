use crate::app::{exe_dir, App, PenEvent, CANVAS_H, CANVAS_W};
use std::cell::RefCell;
use std::ffi::c_void;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

// ---- Windows Ink (WM_POINTER): объявления, которых нет в подключённых разделах windows-sys ----

const WM_POINTERUPDATE: u32 = 0x0245;
const WM_POINTERDOWN: u32 = 0x0246;
const WM_POINTERUP: u32 = 0x0247;
const PT_PEN: u32 = 3;
const PEN_FLAG_BARREL: u32 = 1;
const PEN_FLAG_INVERTED: u32 = 2;
const PEN_FLAG_ERASER: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy)]
struct PointerInfo {
    pointer_type: u32,
    pointer_id: u32,
    frame_id: u32,
    pointer_flags: u32,
    source_device: *mut c_void,
    hwnd_target: HWND,
    pixel: POINT,
    himetric: POINT,
    pixel_raw: POINT,
    himetric_raw: POINT,
    time: u32,
    history_count: u32,
    input_data: i32,
    key_states: u32,
    performance_count: u64,
    button_change: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct PointerPenInfo {
    info: PointerInfo,
    pen_flags: u32,
    pen_mask: u32,
    pressure: u32,
    rotation: u32,
    tilt_x: i32,
    tilt_y: i32,
}

#[repr(C)]
struct PointerDeviceInfo {
    display_orientation: u32,
    device: *mut c_void,
    device_type: u32,
    monitor: *mut c_void,
    starting_cursor_id: u32,
    max_active_contacts: u16,
    product: [u16; 520],
}

#[link(name = "user32")]
extern "system" {
    fn GetPointerType(id: u32, kind: *mut u32) -> i32;
    fn GetPointerPenInfoHistory(id: u32, count: *mut u32, info: *mut PointerPenInfo) -> i32;
    fn GetPointerDeviceRects(device: *mut c_void, device_rect: *mut RECT, display_rect: *mut RECT) -> i32;
    fn GetPointerDevices(count: *mut u32, devices: *mut PointerDeviceInfo) -> i32;
    fn SetProcessDpiAwarenessContext(ctx: isize) -> i32;
    fn GetMessageExtraInfo() -> isize;
    fn GetDpiForWindow(hwnd: HWND) -> u32;
}

// ---- WinTab ----

const WT_PACKET: u32 = 0x7FF0;
const WTI_DEFSYSCTX: u32 = 4;
const WTI_DEVICES: u32 = 100;
const DVC_NAME: u32 = 1;
const DVC_NPRESSURE: u32 = 15;
const CXO_MESSAGES: u32 = 0x0004;
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
struct WtPacket {
    time: u32,
    cursor: u32,
    buttons: u32,
    x: i32,
    y: i32,
    pressure: u32,
    orientation: [i32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct WtAxis {
    min: i32,
    max: i32,
    units: u32,
    resolution: u32,
}

struct WinTab {
    ctx: *mut c_void,
    packet: unsafe extern "system" fn(*mut c_void, u32, *mut c_void) -> i32,
    close: unsafe extern "system" fn(*mut c_void) -> i32,
    max_pressure: f32,
    out_height: i32,
    was_down: bool,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(w: &[u16]) -> String {
    let n = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..n])
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    static WINTAB: RefCell<Option<WinTab>> = const { RefCell::new(None) };
}

unsafe fn log_devices(app: &mut App, hwnd: HWND) {
    app.note(&format!("dpi окна: {}", GetDpiForWindow(hwnd)));
    app.note(&format!("экран: {}x{}", GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)));
    let mut count = 0u32;
    if GetPointerDevices(&mut count, std::ptr::null_mut()) != 0 && count > 0 {
        let mut devices: Vec<PointerDeviceInfo> = (0..count).map(|_| std::mem::zeroed()).collect();
        if GetPointerDevices(&mut count, devices.as_mut_ptr()) != 0 {
            for d in devices.iter().take(count as usize) {
                app.note(&format!("устройство указателя: тип={} (1 встроенное перо, 2 внешнее перо, 3 касание) контактов={} имя={:?}", d.device_type, d.max_active_contacts, from_wide(&d.product)));
            }
        }
    } else {
        app.note("устройств указателя Windows Ink не найдено");
    }
}

unsafe fn open_wintab(app: &mut App, hwnd: HWND) {
    let lib = LoadLibraryW(wide("Wintab32.dll").as_ptr());
    if lib.is_null() {
        app.note("WinTab: Wintab32.dll не найдена — драйвер планшета не даёт WinTab");
        return;
    }
    macro_rules! sym {
        ($name:literal, $ty:ty) => {
            match GetProcAddress(lib, concat!($name, "\0").as_ptr()) {
                Some(f) => std::mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(f),
                None => {
                    app.note(concat!("WinTab: в библиотеке нет функции ", $name));
                    return;
                }
            }
        };
    }
    let info = sym!("WTInfoW", unsafe extern "system" fn(u32, u32, *mut c_void) -> u32);
    let open = sym!("WTOpenW", unsafe extern "system" fn(HWND, *mut LogContext, i32) -> *mut c_void);
    let packet = sym!("WTPacket", unsafe extern "system" fn(*mut c_void, u32, *mut c_void) -> i32);
    let close = sym!("WTClose", unsafe extern "system" fn(*mut c_void) -> i32);

    let mut lc: LogContext = std::mem::zeroed();
    if info(WTI_DEFSYSCTX, 0, &mut lc as *mut _ as *mut c_void) == 0 {
        app.note("WinTab: служба есть, но планшет не найден (WTInfo вернула 0)");
        return;
    }
    let mut name = [0u16; 256];
    info(WTI_DEVICES, DVC_NAME, name.as_mut_ptr() as *mut c_void);
    let mut axis = WtAxis::default();
    info(WTI_DEVICES, DVC_NPRESSURE, &mut axis as *mut _ as *mut c_void);
    app.note(&format!("WinTab: устройство {:?}, давление {}..{}, вывод {}x{}", from_wide(&name), axis.min, axis.max, lc.out_ext[0], lc.out_ext[1]));

    lc.options |= CXO_MESSAGES;
    lc.pkt_data = PACKET_DATA;
    lc.pkt_mode = 0;
    lc.move_mask = PACKET_DATA;
    let out_height = lc.out_ext[1].abs();
    let ctx = open(hwnd, &mut lc, 1);
    if ctx.is_null() {
        app.note("WinTab: не удалось открыть контекст (WTOpen вернула 0)");
        return;
    }
    app.note("WinTab: контекст открыт");
    WINTAB.with(|w| *w.borrow_mut() = Some(WinTab { ctx, packet, close, max_pressure: axis.max.max(1) as f32, out_height, was_down: false }));
}

unsafe fn client_origin(hwnd: HWND) -> POINT {
    let mut p = POINT { x: 0, y: 0 };
    ClientToScreen(hwnd, &mut p);
    p
}

unsafe fn handle_pointer(hwnd: HWND, msg: u32, wparam: WPARAM) -> bool {
    let id = (wparam & 0xFFFF) as u32;
    let mut kind = 0u32;
    if GetPointerType(id, &mut kind) == 0 || kind != PT_PEN {
        return false;
    }
    let mut count = 64u32;
    let mut history: [PointerPenInfo; 64] = std::mem::zeroed();
    if GetPointerPenInfoHistory(id, &mut count, history.as_mut_ptr()) == 0 {
        return false;
    }
    let count = (count as usize).min(history.len());
    let origin = client_origin(hwnd);
    let (mut dev, mut disp): (RECT, RECT) = (std::mem::zeroed(), std::mem::zeroed());
    let precise = count > 0 && GetPointerDeviceRects(history[0].info.source_device, &mut dev, &mut disp) != 0 && dev.right > dev.left && dev.bottom > dev.top;

    APP.with(|cell| {
        let mut guard = cell.borrow_mut();
        let Some(app) = guard.as_mut() else { return };
        if app.mode == "wintab" {
            return; // в режиме WinTab рисуем только по пакетам WinTab
        }
        // История идёт от новых к старым — обходим в обратном порядке.
        for (n, pen) in history[..count].iter().rev().enumerate() {
            let (x, y) = if precise {
                let fx = disp.left as f32 + (pen.info.himetric_raw.x - dev.left) as f32 * (disp.right - disp.left) as f32 / (dev.right - dev.left) as f32;
                let fy = disp.top as f32 + (pen.info.himetric_raw.y - dev.top) as f32 * (disp.bottom - disp.top) as f32 / (dev.bottom - dev.top) as f32;
                (fx - origin.x as f32, fy - origin.y as f32)
            } else {
                ((pen.info.pixel.x - origin.x) as f32, (pen.info.pixel.y - origin.y) as f32)
            };
            let pressure = pen.pressure as f32 / 1024.0;
            let eraser = pen.pen_flags & (PEN_FLAG_ERASER | PEN_FLAG_INVERTED) != 0;
            let last_in_batch = n + 1 == count;
            let event = match msg {
                WM_POINTERDOWN if n == 0 => "down",
                WM_POINTERUP if last_in_batch => "up",
                _ => "move",
            };
            app.record(&PenEvent { source: "ink", event, x, y, pressure_raw: pen.pressure, pressure, tilt: (pen.tilt_x, pen.tilt_y), rotation: pen.rotation as i32, flags: pen.pen_flags, buttons: pen.pen_flags & PEN_FLAG_BARREL, dev_time: pen.info.performance_count });
            match event {
                "down" => {
                    app.pen_down(eraser);
                    app.pen_move(x, y, pressure);
                }
                "up" => {
                    app.pen_move(x, y, pressure);
                    app.pen_up();
                }
                _ => app.pen_move(x, y, pressure),
            }
        }
    });
    InvalidateRect(hwnd, std::ptr::null(), 0);
    true
}

unsafe fn handle_wintab(hwnd: HWND, serial: u32) {
    let origin = client_origin(hwnd);
    let got = WINTAB.with(|w| {
        let mut guard = w.borrow_mut();
        let wt = guard.as_mut()?;
        let mut pkt = WtPacket::default();
        if (wt.packet)(wt.ctx, serial, &mut pkt as *mut _ as *mut c_void) == 0 {
            return None;
        }
        let down = pkt.pressure > 0;
        let event = match (wt.was_down, down) {
            (false, true) => "down",
            (true, false) => "up",
            _ => "move",
        };
        wt.was_down = down;
        Some((pkt, event, down, wt.max_pressure, wt.out_height))
    });
    let Some((pkt, event, down, max_pressure, out_height)) = got else { return };
    APP.with(|cell| {
        let mut guard = cell.borrow_mut();
        let Some(app) = guard.as_mut() else { return };
        let pressure = pkt.pressure as f32 / max_pressure;
        // В WinTab ось Y по умолчанию направлена вверх.
        let x = (pkt.x - origin.x) as f32;
        let y = (out_height - 1 - pkt.y - origin.y) as f32;
        let eraser = pkt.cursor % 3 == 2; // у большинства драйверов курсор №2 — ластик
        app.record(&PenEvent { source: "wintab", event, x, y, pressure_raw: pkt.pressure, pressure, tilt: (pkt.orientation[0], pkt.orientation[1]), rotation: pkt.orientation[2], flags: pkt.cursor, buttons: pkt.buttons, dev_time: pkt.time as u64 });
        match event {
            "down" => {
                app.pen_down(eraser);
                app.pen_move(x, y, pressure);
            }
            "up" => app.pen_up(),
            _ if down => app.pen_move(x, y, pressure),
            _ => {}
        }
    });
    InvalidateRect(hwnd, std::ptr::null(), 0);
}

unsafe fn handle_mouse(hwnd: HWND, msg: u32, lparam: LPARAM) {
    // События мыши, порождённые пером или касанием, помечены подписью — их пропускаем.
    if (GetMessageExtraInfo() as usize & 0xFFFF_FF00) == 0xFF51_5700 {
        return;
    }
    let x = (lparam & 0xFFFF) as i16 as f32;
    let y = ((lparam >> 16) & 0xFFFF) as i16 as f32;
    APP.with(|cell| {
        let mut guard = cell.borrow_mut();
        let Some(app) = guard.as_mut() else { return };
        match msg {
            WM_LBUTTONDOWN => {
                app.record(&PenEvent { source: "mouse", event: "down", x, y, pressure_raw: 0, pressure: 1.0, tilt: (0, 0), rotation: 0, flags: 0, buttons: 1, dev_time: 0 });
                app.pen_down(false);
                app.pen_move(x, y, 0.6);
            }
            WM_MOUSEMOVE if app.stroke.is_some() => {
                app.record(&PenEvent { source: "mouse", event: "move", x, y, pressure_raw: 0, pressure: 1.0, tilt: (0, 0), rotation: 0, flags: 0, buttons: 1, dev_time: 0 });
                app.pen_move(x, y, 0.6);
            }
            WM_LBUTTONUP if app.stroke.is_some() => {
                app.record(&PenEvent { source: "mouse", event: "up", x, y, pressure_raw: 0, pressure: 0.0, tilt: (0, 0), rotation: 0, flags: 0, buttons: 0, dev_time: 0 });
                app.pen_up();
            }
            _ => {}
        }
    });
    InvalidateRect(hwnd, std::ptr::null(), 0);
}

unsafe fn paint(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    APP.with(|cell| {
        let guard = cell.borrow();
        let Some(app) = guard.as_ref() else { return };
        let img = app.frame();
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = CANVAS_W as i32;
        bmi.bmiHeader.biHeight = -(CANVAS_H as i32); // сверху вниз
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        SetDIBitsToDevice(hdc, 0, 0, CANVAS_W, CANVAS_H, 0, 0, 0, CANVAS_H, img.as_ptr() as *const c_void, &bmi, DIB_RGB_COLORS);
        SetBkMode(hdc, 2); // OPAQUE
        let lines = [
            format!("Режим: {}   {}", if app.mode == "wintab" { "WinTab" } else { "Windows Ink" }, app.status_line()),
            app.last.clone(),
            format!("Журнал: {}", app.log_path.display()),
            "C — очистить   Z — отменить   S — сохранить PNG   1/2/3 — размер   Esc — выход".to_string(),
        ];
        for (i, line) in lines.iter().enumerate() {
            let w: Vec<u16> = line.encode_utf16().collect();
            TextOutW(hdc, 8, 8 + i as i32 * 20, w.as_ptr(), w.len() as i32);
        }
    });
    EndPaint(hwnd, &ps);
}

unsafe fn key(hwnd: HWND, code: u32) {
    APP.with(|cell| {
        let mut guard = cell.borrow_mut();
        let Some(app) = guard.as_mut() else { return };
        match code {
            0x1B => PostQuitMessage(0),
            0x43 => app.clear(),
            0x5A => app.undo(),
            0x53 => app.save_png(),
            0x31 => app.size = 6.0,
            0x32 => app.size = 12.0,
            0x33 => app.size = 30.0,
            _ => {}
        }
    });
    InvalidateRect(hwnd, std::ptr::null(), 0);
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_POINTERDOWN | WM_POINTERUPDATE | WM_POINTERUP => {
            if handle_pointer(hwnd, msg, wparam) {
                return 0;
            }
        }
        WT_PACKET => {
            handle_wintab(hwnd, wparam as u32);
            return 0;
        }
        WM_LBUTTONDOWN | WM_MOUSEMOVE | WM_LBUTTONUP => {
            handle_mouse(hwnd, msg, lparam);
            return 0;
        }
        WM_KEYDOWN => {
            key(hwnd, wparam as u32);
            return 0;
        }
        WM_TIMER => {
            if wparam == 2 {
                PostQuitMessage(0); // --autoclose
            }
            APP.with(|cell| {
                if let Some(app) = cell.borrow_mut().as_mut() {
                    app.tick();
                }
            });
            InvalidateRect(hwnd, std::ptr::null(), 0);
            return 0;
        }
        WM_ERASEBKGND => return 1,
        WM_PAINT => {
            paint(hwnd);
            return 0;
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            return 0;
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

pub fn run(use_wintab: bool, autoclose: Option<u32>) {
    std::panic::set_hook(Box::new(|info| {
        let _ = std::fs::write(exe_dir().join("pen-test-crash.txt"), format!("{info}\n"));
    }));

    // SAFETY: вызовы Win32 с подписями из описания API; строки — с нулём в конце, живут до конца блока,
    // окно и его данные используются только в этом потоке.
    unsafe {
        SetProcessDpiAwarenessContext(-4); // по монитору, версия 2: координаты в настоящих пикселях
        let class = wide("PaintRsPenTest");
        let hinstance = GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_CROSS),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class.as_ptr(),
        };
        RegisterClassW(&wc);
        let title = wide(if use_wintab { "Проверка пера — WinTab" } else { "Проверка пера — Windows Ink" });
        let hwnd = CreateWindowExW(0, class.as_ptr(), title.as_ptr(), WS_OVERLAPPEDWINDOW | WS_VISIBLE, CW_USEDEFAULT, CW_USEDEFAULT, 1280, 800, std::ptr::null_mut(), std::ptr::null_mut(), hinstance, std::ptr::null());
        if hwnd.is_null() {
            let _ = std::fs::write(exe_dir().join("pen-test-crash.txt"), "окно не создано\n");
            return;
        }
        match App::new(if use_wintab { "wintab" } else { "ink" }) {
            Ok(mut app) => {
                log_devices(&mut app, hwnd);
                if use_wintab {
                    open_wintab(&mut app, hwnd);
                }
                APP.with(|cell| *cell.borrow_mut() = Some(app));
            }
            Err(e) => {
                let _ = std::fs::write(exe_dir().join("pen-test-crash.txt"), format!("журнал не создан: {e}\n"));
                return;
            }
        }
        SetTimer(hwnd, 1, 500, None);
        if let Some(sec) = autoclose {
            SetTimer(hwnd, 2, sec * 1000, None);
        }
        ShowWindow(hwnd, SW_MAXIMIZE);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        APP.with(|cell| {
            if let Some(wt) = WINTAB.with(|w| w.borrow_mut().take()) {
                (wt.close)(wt.ctx);
            }
            if let Some(mut app) = cell.borrow_mut().take() {
                let total = app.events;
                app.note(&format!("выход, всего событий: {total}"));
            }
        });
    }
}
