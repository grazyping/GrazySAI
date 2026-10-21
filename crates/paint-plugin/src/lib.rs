//! Плагины GrazySAI на Luau (docs/34-brushes-and-plugins.md, docs/36-plugins.md). Пакет
//! `.gsplugin` — zip с описанием `plugin.txt` и кодом `main.lua`. Код выполняется в песочнице
//! Luau: нет файлов, сети, системы и чужих модулей; есть только API программы (холст текущего
//! слоя, цвет, выделение, новый слой, журнал), предел памяти и времени. Общая для рисовалки и
//! мастерской: плагин ведёт себя в обеих одинаково.

use mlua::{Lua, LuaOptions, StdLib, Value, VmState};
use paint_brush::Kv;
use std::cell::RefCell;
use std::path::Path;
use std::time::{Duration, Instant};

/// Версия API для плагинов: плагин с бо́льшей версией не запускается.
pub const API: u32 = 1;

type Res<T> = Result<T, Box<dyn std::error::Error>>;

/// Поле окна параметров команды.
#[derive(Clone, Debug, PartialEq)]
pub enum ParamKind {
    /// Число от `min` до `max` (шаг `step`; целое, если шаг ≥ 1).
    Slider { min: f64, max: f64, step: f64 },
    /// Флажок: 0 или 1.
    Check,
    /// Выбор из списка: номер варианта с 0.
    Choice(Vec<String>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub id: String,
    pub label: String,
    pub kind: ParamKind,
    pub default: f64,
}

/// Команда меню «Плагины».
#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    /// Путь в меню: «Подменю/Команда».
    pub menu: String,
    /// Функция в `main.lua`.
    pub function: String,
    pub params: Vec<Param>,
}

#[derive(Clone, Debug)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub api: u32,
    pub commands: Vec<Command>,
    /// Своя динамика кисти: имя (для списка в настройках кисти) и функция в `main.lua`.
    pub dynamics: Vec<(String, String)>,
    pub source: String,
}

/// Разобрать `plugin.txt` (тот же вид `ключ=значение`, что у файла кисти) и код.
///
/// ```text
/// id=example.invert
/// name=Инверсия
/// version=1.0
/// api=1
/// command.0.menu=Цвет/Инверсия
/// command.0.function=invert
/// command.0.param.0=amount slider 0 100 1 100 Сила
/// ```
/// Параметр: `id вид …`: `slider мин макс шаг по_умолчанию подпись`, `check по_умолчанию подпись`,
/// `choice по_умолчанию вариант|вариант|… подпись`.
pub fn parse(manifest: &str, source: &str) -> Res<Plugin> {
    let kv = Kv::from_text(manifest);
    let get = |k: &str| kv.str(k).unwrap_or("").chars().take(200).collect::<String>();
    let id = get("id");
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) {
        return Err("plugin.txt: нужен id из латинских букв, цифр и «._-»".into());
    }
    let api = kv.f32("api").unwrap_or(1.0) as u32;
    let mut commands = Vec::new();
    for n in 0..64 {
        let (Some(menu), Some(function)) = (kv.str(&format!("command.{n}.menu")), kv.str(&format!("command.{n}.function"))) else { break };
        let mut params = Vec::new();
        for m in 0..32 {
            let Some(line) = kv.str(&format!("command.{n}.param.{m}")) else { break };
            params.push(parse_param(line).ok_or_else(|| format!("plugin.txt: не понят параметр «{line}»"))?);
        }
        commands.push(Command { menu: menu.chars().take(120).collect(), function: function.chars().take(64).collect(), params });
    }
    let mut dynamics = Vec::new();
    for n in 0..16 {
        let (Some(name), Some(function)) = (kv.str(&format!("dynamics.{n}.name")), kv.str(&format!("dynamics.{n}.function"))) else { break };
        dynamics.push((name.chars().take(64).collect(), function.chars().take(64).collect()));
    }
    if commands.is_empty() && dynamics.is_empty() {
        return Err("plugin.txt: нет ни команды (command.0.menu, command.0.function), ни динамики (dynamics.0.name, dynamics.0.function)".into());
    }
    Ok(Plugin { name: if get("name").is_empty() { id.clone() } else { get("name") }, id, version: get("version"), author: get("author"), description: get("description"), api, commands, dynamics, source: source.to_string() })
}

fn parse_param(line: &str) -> Option<Param> {
    let mut it = line.split_whitespace();
    let id = it.next()?.to_string();
    let kind = it.next()?;
    let num = |s: Option<&str>| s.and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite());
    let (kind, default) = match kind {
        "slider" => {
            let (min, max, step, def) = (num(it.next())?, num(it.next())?, num(it.next())?, num(it.next())?);
            (ParamKind::Slider { min: min.min(max), max: max.max(min), step: step.max(0.0) }, def.clamp(min.min(max), max.max(min)))
        }
        "check" => (ParamKind::Check, num(it.next())?.clamp(0.0, 1.0)),
        "choice" => {
            let def = num(it.next())?;
            let opts: Vec<String> = it.next()?.split('|').map(str::to_string).collect();
            let d = def.clamp(0.0, (opts.len().max(1) - 1) as f64);
            (ParamKind::Choice(opts), d)
        }
        _ => return None,
    };
    let label: String = it.collect::<Vec<_>>().join(" ");
    Some(Param { label: if label.is_empty() { id.clone() } else { label }, id, kind, default })
}

/// Пакет `.gsplugin` (zip: `plugin.txt`, `main.lua`).
pub fn load(path: &Path) -> Res<Plugin> {
    let files = paint_formats::pack::read(path)?;
    let text = |name: &str| files.iter().find(|(n, _)| n == name).map(|(_, d)| String::from_utf8_lossy(d).into_owned());
    parse(&text("plugin.txt").ok_or("в пакете нет plugin.txt")?, &text("main.lua").ok_or("в пакете нет main.lua")?)
}

/// Собрать пакет из `plugin.txt` и `main.lua`.
pub fn pack(path: &Path, manifest: &str, source: &str) -> Res<()> {
    paint_formats::pack::write(path, &[("plugin.txt".into(), manifest.as_bytes().to_vec()), ("main.lua".into(), source.as_bytes().to_vec())])
}

/// Что плагин может делать с документом. Пиксели — RGBA по 8 бит без предумножения, слой —
/// текущий (или новый после `new_layer`).
pub trait Canvas {
    fn size(&self) -> (u32, u32);
    fn get(&self, x: i32, y: i32) -> [u8; 4];
    fn set(&mut self, x: i32, y: i32, c: [u8; 4]);
    /// Рамка выделения (x0, y0, x1, y1 — правая и нижняя не включены) или `None`.
    fn selection(&self) -> Option<[i32; 4]>;
    /// Насколько точка выделена, 0..1 (без выделения — 1).
    fn selected(&self, x: i32, y: i32) -> f32;
    /// Рамка непустых пикселей слоя.
    fn bounds(&self) -> Option<[i32; 4]>;
    fn color(&self) -> [u8; 3];
    fn set_color(&mut self, c: [u8; 3]);
    /// Новый слой над текущим; дальше `get`/`set` работают с ним.
    fn new_layer(&mut self, name: &str) -> bool;
}

/// Пределы одного запуска.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub memory: usize,
    pub time: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self { memory: 256 << 20, time: Duration::from_secs(60) }
    }
}

/// Итог запуска: журнал (`log` из плагина) и время.
#[derive(Debug, Default)]
pub struct Report {
    pub log: Vec<String>,
    pub elapsed: Duration,
}

/// Выполнить команду: загрузить `main.lua` в новой песочнице и вызвать функцию с таблицей
/// параметров. Ошибка Luau возвращается текстом с номером строки.
pub fn run(plugin: &Plugin, command: &Command, params: &[(String, f64)], canvas: &mut dyn Canvas, limits: Limits) -> Result<Report, String> {
    if plugin.api > API {
        return Err(format!("плагину нужна версия API {}, у программы — {API}", plugin.api));
    }
    let started = Instant::now();
    // только безопасные части стандартной библиотеки Luau: без os, io, debug, package
    let lua = sandbox(limits, started + limits.time).map_err(|e| e.to_string())?;
    let log = RefCell::new(Vec::<String>::new());
    let cell = RefCell::new(canvas);
    let result = lua.scope(|scope| {
        let g = lua.globals();
        let c = lua.create_table()?;
        c.set("width", scope.create_function(|_, ()| Ok(cell.borrow().size().0))?)?;
        c.set("height", scope.create_function(|_, ()| Ok(cell.borrow().size().1))?)?;
        c.set("get", scope.create_function(|_, (x, y): (i32, i32)| {
            let p = cell.borrow().get(x, y);
            Ok((p[0], p[1], p[2], p[3]))
        })?)?;
        c.set(
            "set",
            scope.create_function(|_, (x, y, r, gg, b, a): (i32, i32, f64, f64, f64, Option<f64>)| {
                cell.borrow_mut().set(x, y, [byte(r), byte(gg), byte(b), byte(a.unwrap_or(255.0))]);
                Ok(())
            })?,
        )?;
        c.set("selection", scope.create_function(|_, ()| Ok(rect4(cell.borrow().selection())))?)?;
        c.set("selected", scope.create_function(|_, (x, y): (i32, i32)| Ok(cell.borrow().selected(x, y)))?)?;
        c.set("bounds", scope.create_function(|_, ()| Ok(rect4(cell.borrow().bounds())))?)?;
        c.set("new_layer", scope.create_function(|_, name: Option<String>| Ok(cell.borrow_mut().new_layer(&name.unwrap_or_default().chars().take(64).collect::<String>())))?)?;
        // map(f, [x0, y0, x1, y1]): f(r, g, b, a, x, y) → r, g, b[, a] для каждой точки рамки
        // (по умолчанию — выделение, иначе весь холст); быстрее, чем get/set по одной
        c.set(
            "map",
            scope.create_function(|_, (f, x0, y0, x1, y1): (mlua::Function, Option<i32>, Option<i32>, Option<i32>, Option<i32>)| {
                let (w, h) = cell.borrow().size();
                let sel = cell.borrow().selection().unwrap_or([0, 0, w as i32, h as i32]);
                let r = [x0.unwrap_or(sel[0]).max(0), y0.unwrap_or(sel[1]).max(0), x1.unwrap_or(sel[2]).min(w as i32), y1.unwrap_or(sel[3]).min(h as i32)];
                for y in r[1]..r[3] {
                    for x in r[0]..r[2] {
                        let p = cell.borrow().get(x, y);
                        let (nr, ng, nb, na): (f64, f64, f64, Option<f64>) = f.call((p[0], p[1], p[2], p[3], x, y))?;
                        cell.borrow_mut().set(x, y, [byte(nr), byte(ng), byte(nb), na.map_or(p[3], byte)]);
                    }
                }
                Ok(())
            })?,
        )?;
        g.set("canvas", c)?;
        let col = lua.create_table()?;
        col.set("get", scope.create_function(|_, ()| {
            let c = cell.borrow().color();
            Ok((c[0], c[1], c[2]))
        })?)?;
        col.set("set", scope.create_function(|_, (r, gg, b): (f64, f64, f64)| {
            cell.borrow_mut().set_color([byte(r), byte(gg), byte(b)]);
            Ok(())
        })?)?;
        g.set("color", col)?;
        g.set(
            "log",
            scope.create_function(|_, args: mlua::Variadic<Value>| {
                let mut l = log.borrow_mut();
                if l.len() < 1000 {
                    l.push(args.iter().map(|v| v.to_string().unwrap_or_else(|_| "?".into())).collect::<Vec<_>>().join(" ").chars().take(500).collect());
                }
                Ok(())
            })?,
        )?;
        let p = lua.create_table()?;
        for (k, v) in params {
            p.set(k.as_str(), *v)?;
        }
        // песочница: общие таблицы только для чтения, у скрипта своё окружение
        lua.sandbox(true)?;
        lua.load(plugin.source.as_str()).set_name(format!("={}/main.lua", plugin.id)).exec()?;
        let f: mlua::Function = lua.globals().get(command.function.as_str()).map_err(|_| mlua::Error::runtime(format!("в main.lua нет функции {}", command.function)))?;
        f.call::<()>(p)?;
        Ok(())
    });
    match result {
        Ok(()) => Ok(Report { log: log.into_inner(), elapsed: started.elapsed() }),
        Err(e) => Err(clean_error(&e.to_string())),
    }
}

/// Рамка как четыре значения Luau (x0, y0, x1, y1) или `nil`.
fn rect4(r: Option<[i32; 4]>) -> mlua::Variadic<i32> {
    mlua::Variadic::from_iter(r.into_iter().flatten())
}

/// Песочница Luau без API холста: безопасная часть стандартной библиотеки, без загрузчиков кода,
/// с пределом памяти и временем до `deadline`.
fn sandbox(limits: Limits, deadline: Instant) -> mlua::Result<Lua> {
    let lua = Lua::new_with(StdLib::MATH | StdLib::STRING | StdLib::TABLE | StdLib::UTF8 | StdLib::BIT, LuaOptions::default())?;
    lua.set_memory_limit(limits.memory)?;
    lua.set_interrupt(move |_| if Instant::now() > deadline { Err(mlua::Error::runtime("плагин работает слишком долго — остановлен")) } else { Ok(VmState::Continue) });
    // загрузка кода из файлов (require в mlua умеет читать модули с диска) — убрать
    for name in ["require", "dofile", "loadfile", "load"] {
        lua.globals().raw_set(name, Value::Nil)?;
    }
    Ok(lua)
}

/// Ошибка своей динамики кисти (последняя), общая для мазка и программы.
pub type DynError = std::rc::Rc<RefCell<Option<String>>>;

/// Своя динамика кисти из плагина: функция `name` из `dynamics.N` вызывается на каждый отпечаток
/// с таблицей входов (`pressure`, `speed`, `direction` 0..1, `distance` px, `random`, `random2`) и
/// возвращает таблицу (`size`, `density`, `ratio` — множители, `angle` — градусы, `scatter` — доля
/// диаметра поперёк мазка; чего нет — без изменений). Песочница — на один мазок, до 10 с на мазок;
/// после ошибки динамика выключается до конца мазка, текст ошибки — в `DynError`.
pub fn dynamics_hook(plugin: &Plugin, name: &str) -> Result<(paint_core::dynamics::DynHook, DynError), String> {
    if plugin.api > API {
        return Err(format!("плагину нужна версия API {}, у программы — {API}", plugin.api));
    }
    let (_, function) = plugin.dynamics.iter().find(|(n, _)| n == name).ok_or_else(|| format!("в плагине нет динамики «{name}»"))?;
    let limits = Limits { memory: 64 << 20, time: Duration::from_secs(10) };
    let lua = sandbox(limits, Instant::now() + limits.time).map_err(|e| e.to_string())?;
    let log_free = lua.create_function(|_, _: mlua::Variadic<Value>| Ok(())).map_err(|e| e.to_string())?;
    lua.globals().set("log", log_free).map_err(|e| e.to_string())?;
    lua.sandbox(true).map_err(|e| e.to_string())?;
    lua.load(plugin.source.as_str()).set_name(format!("={}/main.lua", plugin.id)).exec().map_err(|e| clean_error(&e.to_string()))?;
    let f: mlua::Function = lua.globals().get(function.as_str()).map_err(|_| format!("в main.lua нет функции {function}"))?;
    let error: DynError = Default::default();
    let err = error.clone();
    let hook = move |i: &paint_core::dynamics::DynInputs| -> paint_core::dynamics::DynEffect {
        let none = paint_core::dynamics::DynEffect::default();
        if err.borrow().is_some() {
            return none;
        }
        let r = (|| -> mlua::Result<paint_core::dynamics::DynEffect> {
            let t = lua.create_table()?;
            t.set("pressure", i.pressure)?;
            t.set("speed", i.speed)?;
            t.set("direction", (i.heading / std::f32::consts::TAU).rem_euclid(1.0))?;
            t.set("distance", i.distance)?;
            t.set("random", i.random[0])?;
            t.set("random2", i.random[1])?;
            let out: mlua::Table = f.call(t)?;
            let num = |k: &str, d: f32| -> f32 { out.get::<Option<f64>>(k).ok().flatten().map(|v| v as f32).filter(|v| v.is_finite()).unwrap_or(d) };
            Ok(paint_core::dynamics::DynEffect {
                size: num("size", 1.0).clamp(0.0, 10.0),
                density: num("density", 1.0).clamp(0.0, 1.0),
                angle: num("angle", 0.0).clamp(-3600.0, 3600.0).to_radians(),
                ratio: num("ratio", 1.0).clamp(0.01, 1.0),
                scatter: num("scatter", 0.0).clamp(-10.0, 10.0),
            })
        })();
        match r {
            Ok(e) => e,
            Err(e) => {
                *err.borrow_mut() = Some(clean_error(&e.to_string()));
                none
            }
        }
    };
    Ok((Box::new(hook), error))
}

/// Значение канала из числа плагина: 0..255, дробное округляется.
fn byte(v: f64) -> u8 {
    if v.is_finite() { v.round().clamp(0.0, 255.0) as u8 } else { 0 }
}

/// Ошибка Luau без хвоста стека Rust: первая строка и строки с `main.lua`.
fn clean_error(s: &str) -> String {
    let lines: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    let mut out: Vec<&str> = lines.iter().take(1).copied().collect();
    out.extend(lines.iter().skip(1).filter(|l| l.contains("main.lua")).take(4));
    out.join("\n")
}

/// Холст в памяти: для проверок плагина в мастерской и в тестах.
pub struct MemoryCanvas {
    pub width: u32,
    pub height: u32,
    /// Слои снизу вверх, RGBA8; рисуется в последний.
    pub layers: Vec<Vec<u8>>,
    pub names: Vec<String>,
    pub color: [u8; 3],
    pub selection: Option<[i32; 4]>,
}

impl MemoryCanvas {
    pub fn new(width: u32, height: u32, fill: [u8; 4]) -> Self {
        Self { width, height, layers: vec![fill.repeat((width * height) as usize)], names: vec!["Слой 1".into()], color: [0, 0, 0], selection: None }
    }

    /// Все слои, наложенные обычным режимом на белое.
    pub fn flatten(&self) -> Vec<u8> {
        let mut out = vec![255u8; (self.width * self.height * 4) as usize];
        for l in &self.layers {
            for (o, p) in out.as_chunks_mut::<4>().0.iter_mut().zip(l.as_chunks::<4>().0) {
                let a = p[3] as u32;
                for c in 0..3 {
                    o[c] = ((p[c] as u32 * a + o[c] as u32 * (255 - a) + 127) / 255) as u8;
                }
            }
        }
        out
    }
}

impl Canvas for MemoryCanvas {
    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    fn get(&self, x: i32, y: i32) -> [u8; 4] {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return [0; 4];
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        let l = self.layers.last().expect("есть слой");
        [l[i], l[i + 1], l[i + 2], l[i + 3]]
    }
    fn set(&mut self, x: i32, y: i32, c: [u8; 4]) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        self.layers.last_mut().expect("есть слой")[i..i + 4].copy_from_slice(&c);
    }
    fn selection(&self) -> Option<[i32; 4]> {
        self.selection
    }
    fn selected(&self, x: i32, y: i32) -> f32 {
        match self.selection {
            Some(r) => (x >= r[0] && y >= r[1] && x < r[2] && y < r[3]) as u8 as f32,
            None => 1.0,
        }
    }
    fn bounds(&self) -> Option<[i32; 4]> {
        Some([0, 0, self.width as i32, self.height as i32])
    }
    fn color(&self) -> [u8; 3] {
        self.color
    }
    fn set_color(&mut self, c: [u8; 3]) {
        self.color = c;
    }
    fn new_layer(&mut self, name: &str) -> bool {
        if self.layers.len() >= 64 {
            return false;
        }
        self.layers.push(vec![0; (self.width * self.height * 4) as usize]);
        self.names.push(name.to_string());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "id=test.invert\nname=Инверсия\napi=1\ncommand.0.menu=Цвет/Инверсия\ncommand.0.function=invert\ncommand.0.param.0=amount slider 0 100 1 100 Сила\n";

    fn plugin(src: &str) -> Plugin {
        parse(MANIFEST, src).unwrap()
    }

    #[test]
    fn manifest_and_params() {
        let p = plugin("");
        assert_eq!((p.id.as_str(), p.name.as_str(), p.commands.len()), ("test.invert", "Инверсия", 1));
        let c = &p.commands[0];
        assert_eq!(c.params[0], Param { id: "amount".into(), label: "Сила".into(), kind: ParamKind::Slider { min: 0.0, max: 100.0, step: 1.0 }, default: 100.0 });
        assert!(parse("id=../x\ncommand.0.menu=a\ncommand.0.function=f", "").is_err(), "id с путём");
        assert!(parse("id=x", "").is_err(), "без команд");
        assert_eq!(parse_param("mode choice 1 Мягко|Жёстко Режим").unwrap().kind, ParamKind::Choice(vec!["Мягко".into(), "Жёстко".into()]));
    }

    #[test]
    fn invert_with_map_and_params() {
        let src = "function invert(p)\n  local k = p.amount / 100\n  canvas.map(function(r, g, b, a)\n    return r + (255 - 2 * r) * k, g + (255 - 2 * g) * k, b + (255 - 2 * b) * k\n  end)\n  log('готово', canvas.width())\nend\n";
        let p = plugin(src);
        let mut c = MemoryCanvas::new(4, 2, [10, 100, 200, 255]);
        let r = run(&p, &p.commands[0], &[("amount".into(), 100.0)], &mut c, Limits::default()).unwrap();
        assert_eq!(c.get(1, 1), [245, 155, 55, 255]);
        assert_eq!(r.log, vec!["готово 4".to_string()]);
    }

    #[test]
    fn sandbox_has_no_files_os_or_require() {
        for code in ["io.open('x')", "os.execute('ls')", "require('x')", "debug.getinfo(1)", "dofile('x')", "load('return 1')()", "string.dump(print)"] {
            let src = format!("function invert(p) {code} end");
            let p = plugin(&src);
            let mut c = MemoryCanvas::new(2, 2, [0; 4]);
            assert!(run(&p, &p.commands[0], &[], &mut c, Limits::default()).is_err(), "должно не выйти: {code}");
        }
        // загрузчиков кода нет вовсе
        let p = plugin("function invert(p) assert(require == nil and dofile == nil and loadfile == nil and load == nil and io == nil and os == nil and debug == nil) end");
        let mut c = MemoryCanvas::new(2, 2, [0; 4]);
        run(&p, &p.commands[0], &[], &mut c, Limits::default()).unwrap();
        // общие таблицы менять нельзя
        let p = plugin("function invert(p) math.floor = nil end");
        let mut c = MemoryCanvas::new(2, 2, [0; 4]);
        assert!(run(&p, &p.commands[0], &[], &mut c, Limits::default()).is_err());
    }

    #[test]
    fn endless_loop_and_memory_are_stopped() {
        let p = plugin("function invert(p) while true do end end");
        let mut c = MemoryCanvas::new(2, 2, [0; 4]);
        let e = run(&p, &p.commands[0], &[], &mut c, Limits { time: Duration::from_millis(200), ..Default::default() }).unwrap_err();
        assert!(e.contains("долго"), "{e}");
        let p = plugin("function invert(p) local t = {} for i = 1, 1e9 do t[i] = string.rep('x', 1000) end end");
        let e = run(&p, &p.commands[0], &[], &mut c, Limits { memory: 16 << 20, ..Default::default() }).unwrap_err();
        assert!(e.to_lowercase().contains("memory") || e.contains("памят"), "{e}");
    }

    #[test]
    fn errors_point_to_the_line() {
        let p = plugin("function invert(p)\n  local x = nil\n  return x.y\nend");
        let mut c = MemoryCanvas::new(2, 2, [0; 4]);
        let e = run(&p, &p.commands[0], &[], &mut c, Limits::default()).unwrap_err();
        assert!(e.contains("main.lua:3"), "{e}");
    }

    #[test]
    fn new_layer_and_color() {
        let p = plugin("function invert(p) color.set(1, 2, 3) local r, g, b = color.get() canvas.new_layer('Тень') canvas.set(0, 0, r + 8, g + 7, b + 6, 128) local a1, a2, a3, a4 = canvas.get(0, 0) assert(a4 == 128) end");
        let mut c = MemoryCanvas::new(2, 2, [0; 4]);
        run(&p, &p.commands[0], &[], &mut c, Limits::default()).unwrap();
        assert_eq!((c.layers.len(), c.color, c.get(0, 0)), (2, [1, 2, 3], [9, 9, 9, 128]));
    }

    #[test]
    fn brush_dynamics_from_plugin() {
        let m = "id=test.dyn\ndynamics.0.name=Дрожь\ndynamics.0.function=jitter\n";
        let p = parse(m, "function jitter(i) return { size = 0.5 + i.pressure * 0.5, angle = i.random * 90, scatter = 0.25 } end").unwrap();
        let (mut h, err) = dynamics_hook(&p, "Дрожь").unwrap();
        let e = h(&paint_core::dynamics::DynInputs { pressure: 1.0, random: [0.5, 0.0], ..Default::default() });
        assert!((e.size - 1.0).abs() < 1e-6 && (e.angle - 45f32.to_radians()).abs() < 1e-4 && (e.scatter - 0.25).abs() < 1e-6);
        assert!(err.borrow().is_none());
        // ошибка — динамика выключается, текст сохраняется
        let p = parse(m, "function jitter(i) return nil + 1 end").unwrap();
        let (mut h, err) = dynamics_hook(&p, "Дрожь").unwrap();
        assert_eq!(h(&Default::default()), paint_core::dynamics::DynEffect::default());
        assert!(err.borrow().as_deref().is_some_and(|e| e.contains("main.lua:1")));
        assert!(dynamics_hook(&p, "нет").is_err());
    }

    #[test]
    fn package_round_trip() {
        let dir = std::env::temp_dir().join(format!("grazysai-plugin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("инверсия.gsplugin");
        pack(&path, MANIFEST, "function invert(p) end").unwrap();
        let p = load(&path).unwrap();
        assert_eq!(p.commands[0].function, "invert");
        std::fs::remove_dir_all(dir).ok();
    }
}

#[cfg(test)]
mod examples {
    /// Примеры из `plugins/examples/` разбираются и выполняются на маленьком холсте.
    #[test]
    fn bundled_examples_run() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../plugins/examples");
        let mut n = 0;
        for e in std::fs::read_dir(root).unwrap().flatten() {
            let d = e.path();
            let p = super::parse(&std::fs::read_to_string(d.join("plugin.txt")).unwrap(), &std::fs::read_to_string(d.join("main.lua")).unwrap()).unwrap();
            for c in &p.commands {
                let values: Vec<(String, f64)> = c.params.iter().map(|q| (q.id.clone(), q.default)).collect();
                let mut cv = super::MemoryCanvas::new(40, 30, [120, 60, 30, 255]);
                super::run(&p, c, &values, &mut cv, super::Limits::default()).unwrap_or_else(|e| panic!("{}: {e}", p.id));
            }
            for (name, _) in &p.dynamics {
                let (mut h, err) = super::dynamics_hook(&p, name).unwrap();
                for k in 0..20 {
                    h(&paint_core::dynamics::DynInputs { pressure: k as f32 / 20.0, distance: k as f32 * 40.0, random: [0.3, 0.7], ..Default::default() });
                }
                assert!(err.borrow().is_none(), "{}: {:?}", p.id, err.borrow());
            }
            n += 1;
        }
        assert!(n >= 3);
    }
}
