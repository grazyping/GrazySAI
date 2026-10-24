//! «Описание действия» Photoshop (Action Descriptor) — дерево ключей и значений, которым
//! Photoshop хранит настройки (у `.abr` — раздел `8BIM desc`). Числа — big-endian, ключ —
//! длина и строка (длина 0 — ровно 4 знака), строки — UTF-16 с длиной в знаках.

use std::collections::BTreeMap;

type Res<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Класс и поля.
    Object(String, BTreeMap<String, Value>),
    List(Vec<Value>),
    Double(f64),
    /// Единица (`#Pxl`, `#Prc`, `#Ang`…) и число.
    Unit(String, f64),
    Bool(bool),
    Long(i32),
    /// Тип и значение перечисления.
    Enum(String, String),
    Text(String),
    Raw(Vec<u8>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(_, m) => m.get(key),
            _ => None,
        }
    }

    pub fn class(&self) -> Option<&str> {
        match self {
            Value::Object(c, _) => Some(c),
            _ => None,
        }
    }

    /// Число любого вида (у единиц — без единицы).
    pub fn num(&self) -> Option<f64> {
        match self {
            Value::Double(v) | Value::Unit(_, v) => Some(*v),
            Value::Long(v) => Some(*v as f64),
            _ => None,
        }
    }

    pub fn bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Value::Text(s) => Some(s),
            _ => None,
        }
    }

    pub fn list(&self) -> &[Value] {
        match self {
            Value::List(v) => v,
            _ => &[],
        }
    }
}

struct R<'a> {
    d: &'a [u8],
    p: usize,
    /// Всего прочитано узлов: испорченный файл не должен разворачиваться в огромное дерево.
    nodes: usize,
}

impl R<'_> {
    fn take(&mut self, n: usize) -> Res<&[u8]> {
        let s = self.d.get(self.p..self.p.checked_add(n).ok_or("описание обрывается")?).ok_or("описание обрывается")?;
        self.p += n;
        Ok(s)
    }
    fn u32(&mut self) -> Res<u32> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn f64(&mut self) -> Res<f64> {
        let b = self.take(8)?;
        Ok(f64::from_be_bytes(b.try_into()?))
    }
    fn key(&mut self) -> Res<String> {
        let n = self.u32()? as usize;
        Ok(String::from_utf8_lossy(self.take(if n == 0 { 4 } else { n })?).into_owned())
    }
    fn ustr(&mut self) -> Res<String> {
        let n = self.u32()? as usize;
        let b = self.take(n.checked_mul(2).ok_or("описание обрывается")?)?;
        let units: Vec<u16> = b.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c)).collect();
        Ok(String::from_utf16_lossy(&units).trim_end_matches('\0').to_string())
    }
    fn object(&mut self, depth: u32) -> Res<Value> {
        self.ustr()?; // имя для показа
        let class = self.key()?;
        let n = self.u32()?;
        let mut m = BTreeMap::new();
        for _ in 0..n {
            let k = self.key()?;
            let v = self.item(depth + 1)?;
            m.insert(k, v);
        }
        Ok(Value::Object(class, m))
    }
    fn item(&mut self, depth: u32) -> Res<Value> {
        self.nodes += 1;
        if depth > 32 || self.nodes > 2_000_000 {
            return Err("описание слишком глубокое".into());
        }
        let t: [u8; 4] = self.take(4)?.try_into()?;
        Ok(match &t {
            b"Objc" | b"GlbO" => self.object(depth)?,
            b"VlLs" => {
                let n = self.u32()?;
                // у каждого элемента не меньше 4 байт: больше не уместится
                if n as usize > self.d.len() / 4 {
                    return Err("описание обрывается".into());
                }
                Value::List((0..n).map(|_| self.item(depth + 1)).collect::<Res<_>>()?)
            }
            b"doub" => Value::Double(self.f64()?),
            b"UntF" => {
                let u = String::from_utf8_lossy(self.take(4)?).into_owned();
                Value::Unit(u, self.f64()?)
            }
            b"bool" => Value::Bool(self.take(1)?[0] != 0),
            b"long" => Value::Long(self.u32()? as i32),
            b"comp" => {
                let b = self.take(8)?;
                Value::Long(i64::from_be_bytes(b.try_into()?).clamp(i32::MIN as i64, i32::MAX as i64) as i32)
            }
            b"enum" => Value::Enum(self.key()?, self.key()?),
            b"TEXT" => Value::Text(self.ustr()?),
            b"tdta" => {
                let n = self.u32()? as usize;
                Value::Raw(self.take(n)?.to_vec())
            }
            b"type" | b"GlbC" => {
                self.ustr()?;
                Value::Text(self.key()?)
            }
            _ => return Err(format!("описание: неизвестный тип {}", String::from_utf8_lossy(&t)).into()),
        })
    }
}

/// Описание с номером версии впереди (16).
pub fn read(d: &[u8]) -> Res<Value> {
    let mut r = R { d, p: 0, nodes: 0 };
    let version = r.u32()?;
    if version != 16 {
        return Err(format!("описание: версия {version}").into());
    }
    r.object(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(out: &mut Vec<u8>, k: &str) {
        if k.len() == 4 {
            out.extend(0u32.to_be_bytes());
        } else {
            out.extend((k.len() as u32).to_be_bytes());
        }
        out.extend(k.as_bytes());
    }

    #[test]
    fn reads_nested_object() {
        let mut d = 16u32.to_be_bytes().to_vec();
        d.extend(1u32.to_be_bytes());
        d.extend([0, 0]); // пустое имя: 1 знак-ноль
        key(&mut d, "null");
        d.extend(2u32.to_be_bytes());
        key(&mut d, "Dmtr");
        d.extend(b"UntF#Pxl");
        d.extend(12.5f64.to_be_bytes());
        key(&mut d, "useScatter");
        d.extend(b"bool");
        d.push(1);
        let v = read(&d).unwrap();
        assert_eq!(v.get("Dmtr").and_then(Value::num), Some(12.5));
        assert_eq!(v.get("useScatter").and_then(Value::bool), Some(true));
        // обрезанный файл — ошибка, не паника
        for n in 0..d.len() {
            let _ = read(&d[..n]);
        }
    }
}
