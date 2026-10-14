//! Векторные блоки `.sai2`: слой контура (`liwk`, объекты `strk`) и слой фигур (`shap`).
//! Устройство — docs/11-linework-shapes.md, раздел 3. Пикселей у таких слоёв в файле нет.

use std::io;

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

/// Читатель по байтам с проверкой границ.
struct Cur<'a> {
    d: &'a [u8],
    o: usize,
}

impl<'a> Cur<'a> {
    fn bytes(&mut self, n: usize) -> io::Result<&'a [u8]> {
        let s = self.d.get(self.o..self.o + n).ok_or_else(|| bad("векторный блок короче ожидаемого"))?;
        self.o += n;
        Ok(s)
    }
    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> io::Result<f32> {
        Ok(f32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }
    fn f64(&mut self) -> io::Result<f64> {
        Ok(f64::from_le_bytes(self.bytes(8)?.try_into().unwrap()))
    }
}

/// Стиль объекта. У контура — цвет и параметры кисти, у фигуры — цвет заливки.
#[derive(Debug, Clone, PartialEq)]
pub enum Style {
    Linework {
        /// `scol`, 8 байт как в файле.
        color: [u8; 8],
        /// `inkd`: толщина, плотность и ещё три числа до 1 (смысл уточняется).
        size: f32,
        density: f32,
        params: [f32; 3],
        /// Ступень жёсткости 0…4 (4 — самая жёсткая).
        hardness: u32,
        /// `inko`: усиление плотности и два флага.
        amplify: f32,
        flags: [bool; 2],
    },
    Fill {
        /// `fcol`, 8 байт как в файле.
        color: [u8; 8],
    },
}

/// Вид кривой пути.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveType {
    Normal,
    Bezier,
    Ver1,
    Other(u8),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PressurePoint {
    pub id: u32,
    /// Доля длины дуги отрезка от узла до следующего, 0…1.
    pub pos: f64,
    pub weight: f32,
    pub second: f32,
    pub rest: [u8; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct ControlPoint {
    pub id: u32,
    /// Точка и ручки — относительно сдвига пути.
    pub point: [f64; 2],
    pub handle_in: [f64; 2],
    pub handle_out: [f64; 2],
    pub weight: f32,
    pub second: f32,
    /// Острый угол: ручки не сглаживаются.
    pub pointed: bool,
    /// Байты 56, 58, 59 записи узла — смысл не выяснен.
    pub rest: [u8; 3],
    pub pressure: Vec<PressurePoint>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    pub id: u32,
    /// Номер следующего пути в общем порядке наложения слоя (0 — последний).
    pub next_in_layer: u32,
    pub offset: [f64; 2],
    pub curve: CurveType,
    pub closed: bool,
    pub rest: [u8; 2],
    pub points: Vec<ControlPoint>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub id: u32,
    /// Номер следующего объекта в порядке наложения (0 — последний).
    pub next: u32,
    pub style: Style,
    pub paths: Vec<Path>,
}

fn read_style(c: &mut Cur, linework: bool) -> io::Result<Style> {
    let (mut color, mut size, mut density, mut params, mut hardness) = ([0u8; 8], 1.0, 1.0, [0.0; 3], 4);
    let (mut amplify, mut flags) = (0.0, [true, false]);
    loop {
        let tag = c.bytes(4)?;
        if tag == [0; 4] {
            break;
        }
        let len = c.u32()? as usize;
        let body = c.bytes(len)?;
        let mut b = Cur { d: body, o: 0 };
        match (tag, len) {
            (b"scol", 8) | (b"fcol", 8) => color.copy_from_slice(body),
            (b"inkd", 24) => {
                size = b.f32()?;
                density = b.f32()?;
                for p in &mut params {
                    *p = b.f32()?;
                }
                hardness = b.u32()?;
            }
            (b"inko", 8) => {
                amplify = b.f32()?;
                let f = b.bytes(2)?;
                flags = [f[0] != 0, f[1] != 0];
            }
            _ => {}
        }
    }
    Ok(if linework {
        Style::Linework { color, size, density, params, hardness, amplify, flags }
    } else {
        Style::Fill { color }
    })
}

fn read_point(c: &mut Cur, id: u32) -> io::Result<ControlPoint> {
    let mut v = [0.0; 6];
    for x in &mut v {
        *x = c.f64()?;
    }
    let (weight, second) = (c.f32()?, c.f32()?);
    let b = c.bytes(4)?;
    Ok(ControlPoint {
        id,
        point: [v[0], v[1]],
        handle_in: [v[2], v[3]],
        handle_out: [v[4], v[5]],
        weight,
        second,
        pointed: b[1] != 0,
        rest: [b[0], b[2], b[3]],
        pressure: Vec::new(),
    })
}

/// Путь; `None`, если вместо номера стоит 0 (конец списка путей объекта).
fn read_path(c: &mut Cur) -> io::Result<Option<Path>> {
    let id = c.u32()?;
    if id == 0 {
        return Ok(None);
    }
    let _owner = c.u32()?;
    let next_in_layer = c.u32()?;
    let _next_in_shape = c.u32()?;
    let offset = [c.f64()?, c.f64()?];
    let h = c.bytes(4)?;
    let curve = match h[0] {
        0 => CurveType::Normal,
        1 => CurveType::Bezier,
        2 => CurveType::Ver1,
        n => CurveType::Other(n),
    };
    let mut points = Vec::new();
    loop {
        let cp = c.u32()?;
        if cp == 0 {
            break;
        }
        if points.len() >= 10_000 {
            return Err(bad("слишком много узлов в пути"));
        }
        points.push(read_point(c, cp)?);
    }
    loop {
        let cp = c.u32()?;
        if cp == 0 {
            break;
        }
        let mut list = Vec::new();
        loop {
            let id = c.u32()?;
            if id == 0 {
                break;
            }
            let pos = c.f64()?;
            let (weight, second) = (c.f32()?, c.f32()?);
            let rest = c.bytes(4)?.try_into().unwrap();
            list.push(PressurePoint { id, pos, weight, second, rest });
        }
        list.sort_by(|a, b| a.pos.total_cmp(&b.pos));
        // Узла с таким номером нет: образец на этом прекращает чтение пути, мы — пропускаем.
        if let Some(p) = points.iter_mut().find(|p| p.id == cp) {
            p.pressure = list;
        }
    }
    Ok(Some(Path { id, next_in_layer, offset, curve, closed: h[1] != 0, rest: [h[2], h[3]], points }))
}

/// Разбор векторного блока одного слоя (данные блока оглавления `liwk` или `shap` целиком).
/// `linework` — слой контура (объекты `strk`), иначе слой фигур (объекты `shap`).
/// Объекты возвращаются в порядке файла; порядок наложения задают поля `next`.
pub fn parse(data: &[u8], linework: bool) -> io::Result<Vec<Shape>> {
    let want: &[u8; 4] = if linework { b"strk" } else { b"shap" };
    let mut c = Cur { d: data, o: 0 };
    let mut shapes = Vec::new();
    loop {
        let tag = c.bytes(4)?;
        if tag == [0; 4] {
            break;
        }
        if tag != want {
            return Err(bad("неожиданная метка в векторном блоке"));
        }
        let len = c.u32()? as usize;
        let mut s = Cur { d: c.bytes(len)?, o: 0 };
        let id = s.u32()?;
        if id == 0 {
            continue;
        }
        let next = s.u32()?;
        let style = read_style(&mut s, linework)?;
        let mut paths = Vec::new();
        while let Some(p) = read_path(&mut s)? {
            paths.push(p);
        }
        shapes.push(Shape { id, next, style, paths });
    }
    Ok(shapes)
}

/// Запись в том же виде, в каком пишет образец (без выравнивания в конце: его добавляет
/// тот, кто пишет файл). Порядок путей внутри объекта — порядок в `paths`.
pub fn write(shapes: &[Shape], linework: bool) -> Vec<u8> {
    let mut out = Vec::new();
    for sh in shapes {
        let mut b = Vec::new();
        b.extend(sh.id.to_le_bytes());
        b.extend(sh.next.to_le_bytes());
        let mut prop = |tag: &[u8; 4], body: &[u8]| {
            b.extend(tag);
            b.extend((body.len() as u32).to_le_bytes());
            b.extend(body);
        };
        match &sh.style {
            Style::Linework { color, size, density, params, hardness, amplify, flags } => {
                prop(b"scol", color);
                let mut ink = Vec::new();
                for v in [*size, *density, params[0], params[1], params[2]] {
                    ink.extend(v.to_le_bytes());
                }
                ink.extend(hardness.to_le_bytes());
                prop(b"inkd", &ink);
                let mut o = amplify.to_le_bytes().to_vec();
                o.extend([flags[0] as u8, flags[1] as u8, 0, 0]);
                prop(b"inko", &o);
            }
            Style::Fill { color } => prop(b"fcol", color),
        }
        b.extend([0; 4]);
        for (i, p) in sh.paths.iter().enumerate() {
            let next_in_shape = sh.paths.get(i + 1).map_or(0, |n| n.id);
            for v in [p.id, sh.id, p.next_in_layer, next_in_shape] {
                b.extend(v.to_le_bytes());
            }
            b.extend(p.offset[0].to_le_bytes());
            b.extend(p.offset[1].to_le_bytes());
            let curve = match p.curve {
                CurveType::Normal => 0,
                CurveType::Bezier => 1,
                CurveType::Ver1 => 2,
                CurveType::Other(n) => n,
            };
            b.extend([curve, p.closed as u8, p.rest[0], p.rest[1]]);
            for cp in &p.points {
                b.extend(cp.id.to_le_bytes());
                for v in cp.point.iter().chain(&cp.handle_in).chain(&cp.handle_out) {
                    b.extend(v.to_le_bytes());
                }
                b.extend(cp.weight.to_le_bytes());
                b.extend(cp.second.to_le_bytes());
                b.extend([cp.rest[0], cp.pointed as u8, cp.rest[1], cp.rest[2]]);
            }
            b.extend([0; 4]);
            for cp in p.points.iter().filter(|cp| !cp.pressure.is_empty()) {
                b.extend(cp.id.to_le_bytes());
                for pp in &cp.pressure {
                    b.extend(pp.id.to_le_bytes());
                    b.extend(pp.pos.to_le_bytes());
                    b.extend(pp.weight.to_le_bytes());
                    b.extend(pp.second.to_le_bytes());
                    b.extend(pp.rest);
                }
                b.extend([0; 4]);
            }
            b.extend([0; 4]);
        }
        b.extend([0; 4]);
        out.extend(if linework { b"strk" } else { b"shap" });
        out.extend((b.len() as u32).to_le_bytes());
        out.extend(b);
    }
    out.extend([0; 4]);
    out
}
