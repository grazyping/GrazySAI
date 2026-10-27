//! Виртуальный планшет для Linux: устройство ввода через `/dev/uinput` с теми же осями, что у
//! XPPen Artist 24 Pro (драйвер XPPen сам выдаёт события именно так — через uinput). Для системы
//! оно неотличимо от настоящего: его видит композитор (KDE, GNOME), XWayland и наш редактор.
//!
//! ВНИМАНИЕ: устройство подключается к текущему сеансу — перо двигает настоящий курсор и
//! нажимает там, где он окажется. Перед `demo` откройте paint-rs на весь экран.
//!
//!   virtual-tablet probe [секунд]     — создать устройство, не двигая перо, и убрать
//!   virtual-tablet demo [задержка, с] — после задержки провести мазки по экрану
//!   virtual-tablet replay файл.csv [задержка, с] — t_ms,x,y,pressure,phase[,eraser]; x, y — доли
//!                                     экрана 0..1; файл читается после задержки

#[cfg(target_os = "linux")]
mod uinput {
    use std::ffi::CString;
    use std::time::Duration;

    pub const X_MAX: i32 = 52_700; // 527 мм в сотых долях мм
    pub const Y_MAX: i32 = 29_600; // 296 мм
    pub const P_MAX: i32 = 8191;

    const EV_SYN: u16 = 0x00;
    const EV_KEY: u16 = 0x01;
    const EV_ABS: u16 = 0x03;
    const SYN_REPORT: u16 = 0;
    const BTN_TOOL_PEN: u16 = 0x140;
    const BTN_TOOL_RUBBER: u16 = 0x141;
    const BTN_TOUCH: u16 = 0x14a;
    const BTN_STYLUS: u16 = 0x14b;
    const BTN_STYLUS2: u16 = 0x14c;
    const ABS_X: u16 = 0x00;
    const ABS_Y: u16 = 0x01;
    const ABS_PRESSURE: u16 = 0x18;
    const ABS_TILT_X: u16 = 0x1a;
    const ABS_TILT_Y: u16 = 0x1b;
    const INPUT_PROP_DIRECT: libc::c_ulong = 0x01;

    const UI_DEV_CREATE: libc::c_ulong = 0x5501;
    const UI_DEV_DESTROY: libc::c_ulong = 0x5502;
    const UI_SET_EVBIT: libc::c_ulong = 0x4004_5564;
    const UI_SET_KEYBIT: libc::c_ulong = 0x4004_5565;
    const UI_SET_ABSBIT: libc::c_ulong = 0x4004_5567;
    const UI_SET_PROPBIT: libc::c_ulong = 0x4004_556e;

    const UI_DEV_SETUP: libc::c_ulong = 0x405c_5503;
    const UI_ABS_SETUP: libc::c_ulong = 0x401c_5504;

    /// `struct uinput_setup`: код устройства и имя.
    #[repr(C)]
    struct Setup {
        bustype: u16,
        vendor: u16,
        product: u16,
        version: u16,
        name: [u8; 80],
        ff_effects_max: u32,
    }

    /// `struct uinput_abs_setup`: ось и её пределы с разрешением. libinput не принимает планшет
    /// без разрешения осей X и Y (единиц на мм).
    #[repr(C)]
    struct AbsSetup {
        code: u16,
        _pad: u16,
        value: i32,
        min: i32,
        max: i32,
        fuzz: i32,
        flat: i32,
        resolution: i32,
    }

    #[repr(C)]
    struct InputEvent {
        tv_sec: libc::time_t,
        tv_usec: libc::suseconds_t,
        kind: u16,
        code: u16,
        value: i32,
    }

    pub struct Tablet {
        fd: libc::c_int,
        tool: Option<u16>,
    }

    fn check(r: libc::c_int, what: &str) -> std::io::Result<()> {
        if r < 0 {
            let e = std::io::Error::last_os_error();
            return Err(std::io::Error::new(e.kind(), format!("{what}: {e}")));
        }
        Ok(())
    }

    impl Tablet {
        pub fn create() -> std::io::Result<Tablet> {
            let path = CString::new("/dev/uinput").unwrap();
            // SAFETY: путь — строка C с нулём в конце, живёт до конца вызова.
            let fd = unsafe { libc::open(path.as_ptr(), libc::O_WRONLY | libc::O_NONBLOCK) };
            check(fd, "открыть /dev/uinput")?;
            // SAFETY: fd открыт выше и проверен; эти ioctl принимают число, не указатель.
            unsafe {
                for ev in [EV_SYN, EV_KEY, EV_ABS] {
                    check(libc::ioctl(fd, UI_SET_EVBIT, ev as libc::c_int), "EV")?;
                }
                for key in [BTN_TOOL_PEN, BTN_TOOL_RUBBER, BTN_TOUCH, BTN_STYLUS, BTN_STYLUS2] {
                    check(libc::ioctl(fd, UI_SET_KEYBIT, key as libc::c_int), "KEY")?;
                }
                for abs in [ABS_X, ABS_Y, ABS_PRESSURE, ABS_TILT_X, ABS_TILT_Y] {
                    check(libc::ioctl(fd, UI_SET_ABSBIT, abs as libc::c_int), "ABS")?;
                }
                // экранный планшет: перо указывает прямо в точку экрана
                check(libc::ioctl(fd, UI_SET_PROPBIT, INPUT_PROP_DIRECT as libc::c_int), "PROP")?;
            }
            // оси: координаты в сотых долях мм (100 на мм), нажим 0..8191, наклон ±60° (57 на радиан)
            for (code, min, max, resolution) in [(ABS_X, 0, X_MAX, 100), (ABS_Y, 0, Y_MAX, 100), (ABS_PRESSURE, 0, P_MAX, 0), (ABS_TILT_X, -60, 60, 57), (ABS_TILT_Y, -60, 60, 57)] {
                let abs = AbsSetup { code, _pad: 0, value: 0, min, max, fuzz: 0, flat: 0, resolution };
                // SAFETY: fd открыт; указатель на локальную #[repr(C)] структуру, живую на время вызова.
                unsafe { check(libc::ioctl(fd, UI_ABS_SETUP, &abs as *const AbsSetup), "ось")? };
            }
            let mut setup = Setup { bustype: 0x03, vendor: 0x28bd, product: 0x092d, version: 1, name: [0; 80], ff_effects_max: 0 };
            let name = b"XP-Pen Artist 24 Pro (virtual) Pen";
            setup.name[..name.len()].copy_from_slice(name);
            // SAFETY: fd открыт; указатель на локальную #[repr(C)] структуру, живую на время вызова.
            unsafe { check(libc::ioctl(fd, UI_DEV_SETUP, &setup as *const Setup), "описание устройства")? };
            // SAFETY: fd открыт; у UI_DEV_CREATE нет аргумента.
            unsafe { check(libc::ioctl(fd, UI_DEV_CREATE), "создать устройство")? };
            // дать системе подхватить устройство
            std::thread::sleep(Duration::from_millis(800));
            Ok(Tablet { fd, tool: None })
        }

        fn emit(&self, kind: u16, code: u16, value: i32) {
            let ev = InputEvent { tv_sec: 0, tv_usec: 0, kind, code, value };
            // SAFETY: fd открыт, пока жив Tablet; пишем ровно size_of байт локальной #[repr(C)] структуры.
            unsafe {
                libc::write(self.fd, &ev as *const InputEvent as *const libc::c_void, std::mem::size_of::<InputEvent>());
            }
        }

        /// Одно состояние пера: место (доли экрана), нажим 0..1, касается ли, ластик ли.
        pub fn report(&mut self, x: f32, y: f32, pressure: f32, touching: bool, eraser: bool) {
            let tool = if eraser { BTN_TOOL_RUBBER } else { BTN_TOOL_PEN };
            if self.tool != Some(tool) {
                if let Some(old) = self.tool {
                    self.emit(EV_KEY, old, 0);
                }
                self.emit(EV_KEY, tool, 1);
                self.tool = Some(tool);
            }
            self.emit(EV_ABS, ABS_X, (x.clamp(0.0, 1.0) * X_MAX as f32) as i32);
            self.emit(EV_ABS, ABS_Y, (y.clamp(0.0, 1.0) * Y_MAX as f32) as i32);
            self.emit(EV_ABS, ABS_PRESSURE, if touching { (pressure.clamp(0.0, 1.0) * P_MAX as f32) as i32 } else { 0 });
            self.emit(EV_KEY, BTN_TOUCH, touching as i32);
            self.emit(EV_SYN, SYN_REPORT, 0);
        }

        /// Увести перо из зоны досягаемости.
        pub fn leave(&mut self) {
            if let Some(old) = self.tool.take() {
                self.emit(EV_KEY, BTN_TOUCH, 0);
                self.emit(EV_KEY, old, 0);
                self.emit(EV_SYN, SYN_REPORT, 0);
            }
        }
    }

    impl Drop for Tablet {
        fn drop(&mut self) {
            self.leave();
            // SAFETY: fd принадлежит Tablet и закрывается один раз, здесь.
            unsafe {
                libc::ioctl(self.fd, UI_DEV_DESTROY);
                libc::close(self.fd);
            }
        }
    }
}

/// Точка сценария: t, мс; x, y — доли экрана; нажим; касается ли; ластик ли.
#[cfg(target_os = "linux")]
type Sample = (f64, f32, f32, f32, bool, bool);

#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::time::{Duration, Instant};
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("");
    let num = |i: usize, d: f64| args.get(i).and_then(|s| s.parse::<f64>().ok()).unwrap_or(d);
    let samples: Vec<Sample> = match mode {
        "probe" => {
            let _t = uinput::Tablet::create()?;
            println!("устройство «XP-Pen Artist 24 Pro (virtual) Pen» создано; проверка: libinput list-devices / xinput list");
            std::thread::sleep(Duration::from_secs_f64(num(2, 10.0)));
            println!("убрано");
            return Ok(());
        }
        "demo" => demo(),
        // файл читается после задержки: можно создать устройство заранее (Wine видит планшет,
        // только если он был при запуске программы), а сценарий дописать потом
        "replay" => Vec::new(),
        _ => {
            eprintln!("использование: virtual-tablet probe [с] | demo [задержка, с] | replay файл.csv");
            std::process::exit(2);
        }
    };
    let mut tab = uinput::Tablet::create()?;
    let delay = if mode == "demo" { num(2, 5.0) } else { num(3, 3.0) };
    println!("виртуальный планшет готов; мазки через {delay} с — переключитесь в окно редактора");
    std::thread::sleep(Duration::from_secs_f64(delay));
    let samples = if mode == "replay" { replay(args.get(2).ok_or("нужен файл")?)? } else { samples };
    let start = Instant::now();
    for (t, x, y, p, touching, eraser) in samples {
        let at = Duration::from_secs_f64(t / 1000.0);
        if let Some(wait) = at.checked_sub(start.elapsed()) {
            std::thread::sleep(wait);
        }
        tab.report(x, y, p, touching, eraser);
    }
    tab.leave();
    println!("готово");
    Ok(())
}

/// Те же мазки, что встроенный сценарий редактора (`virtual_pen.rs`), в долях экрана.
#[cfg(target_os = "linux")]
fn demo() -> Vec<Sample> {
    let mut out = Vec::new();
    let mut t = 0.0;
    let mut stroke = |out: &mut Vec<_>, n: usize, f: &dyn Fn(f32) -> (f32, f32, f32), eraser: bool| {
        let (x0, y0, _) = f(0.0);
        out.push((t, x0, y0, 0.0, false, eraser)); // перо подлетает
        t += 30.0;
        for i in 0..=n {
            let u = i as f32 / n as f32;
            let (x, y, p) = f(u);
            out.push((t, x, y, p, true, eraser));
            t += 8.0;
        }
        let (x1, y1, _) = f(1.0);
        out.push((t, x1, y1, 0.0, false, eraser));
        t += 250.0;
    };
    let sx = |x: f32| 0.25 + x / 1600.0;
    let sy = |y: f32| 0.25 + y / 1000.0;
    stroke(&mut out, 160, &|u| (sx(60.0 + u * 520.0), sy(120.0 + (u * 9.0).sin() * 30.0), (u * std::f32::consts::PI).sin()), false);
    stroke(&mut out, 80, &|u| (sx(60.0 + u * 520.0), sy(220.0), 0.15), false);
    stroke(&mut out, 80, &|u| (sx(60.0 + u * 520.0), sy(270.0), 1.0), false);
    stroke(&mut out, 60, &|u| (sx(320.0), sy(60.0 + u * 140.0), 0.8), true);
    out
}

#[cfg(target_os = "linux")]
fn replay(path: &str) -> Result<Vec<Sample>, Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    for line in std::fs::read_to_string(path)?.lines() {
        let f: Vec<&str> = line.split(',').map(str::trim).collect();
        if f.len() < 5 || line.starts_with('#') || line.starts_with("t_ms") {
            continue;
        }
        let touching = f[4] != "up";
        out.push((f[0].parse()?, f[1].parse()?, f[2].parse()?, f[3].parse()?, touching, f.get(5).is_some_and(|v| *v == "1" || *v == "eraser")));
    }
    Ok(out)
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("virtual-tablet работает только в Linux (uinput). Для Windows — InjectSyntheticPointerInput, см. docs/14-virtual-pen.md");
}
