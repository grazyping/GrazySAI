//! Разбор контейнера `.sai2`: заголовок, оглавление, описания слоёв.
//! Пиксели (`dpcm`) пока не декодируются.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

pub const MAGIC: &[u8; 16] = b"SAI-CANVAS-TYPE0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub tag: [u8; 4],
    pub id: u32,
    pub offset: u64,
    pub len: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayerInfo {
    pub id: u32,
    pub order: u32,
    pub is_folder: bool,
    /// Границы содержимого в пикселях: лево, верх, право, низ.
    pub bounds: [i32; 4],
    /// Область слоя в блоках по 32 px, как в файле: лево, верх, ширина, высота.
    pub block_rect: [i32; 4],
    /// Обтравка («Прикрепление»): слой виден только в пределах слоя под ним.
    pub clipping: bool,
    /// Глубина вложенности: 0 — верхний уровень, 1 — внутри папки и так далее.
    pub depth: u8,
    /// Замки: сохранять непрозрачность, запрет рисования, запрет перемещения, запрет всего.
    pub locks: [bool; 4],
    /// Папка раскрыта в списке слоёв.
    pub folder_open: bool,
    /// Вид слоя, как в файле: `norm`, `fold`, `liwk`, `shap`, `text`, `pers`, `grid`.
    pub kind: String,
    pub blend: String,
    pub opacity: u32,
    pub visible: bool,
    /// «Источник выбора» (байт 54, бит 7; по записи образца — вероятно).
    pub selection_source: bool,
    pub name: String,
    /// Бумага слоя (`ptex`): имя текстуры, сила, масштаб, флаг.
    pub paper: Option<(String, u16, u16, u16)>,
    /// Маска (`lmsk`): номер блока `mpix`, область в блоках 32 точки (лево, верх, ширина, высота),
    /// четыре байта признаков.
    pub mask: Option<(u32, [i32; 4], [u8; 4])>,
    /// Прочие вложенные блоки (кроме `name`, `ptex`, `lmsk`) — как в файле.
    pub extra: Vec<([u8; 4], Vec<u8>)>,
    pub has_pixels: bool,
}

#[derive(Debug, Clone)]
pub struct Sai2Info {
    pub width: u32,
    pub height: u32,
    /// Разрешение печати, точек на дюйм.
    pub dpi: f64,
    /// Фон холста (байт 0x11), его цвет (0x38) и «Layer Effect Color» (0x3C).
    pub bg_code: u8,
    pub bg_color: [u8; 3],
    pub effect_color: [u8; 4],
    /// Разрешение как в файле: точек на сантиметр × 65536.
    pub resolution: u32,
    pub entries: Vec<Entry>,
    /// В порядке оглавления файла.
    pub layers: Vec<LayerInfo>,
}

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

fn u32_at(b: &[u8], o: usize) -> io::Result<u32> {
    let s = b.get(o..o + 4).ok_or_else(|| bad("блок короче ожидаемого"))?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn tag_text(t: &[u8]) -> String {
    String::from_utf8_lossy(t).trim_end().to_string()
}

fn parse_layer(d: &[u8], has_pixels: bool) -> io::Result<LayerInfo> {
    if d.len() < 56 || &d[0..4] != b"layr" {
        return Err(bad("повреждено описание слоя"));
    }
    let mut block_rect = [0i32; 4];
    for (i, b) in block_rect.iter_mut().enumerate() {
        *b = u32_at(d, 28 + i * 4)? as i32;
    }
    let [l, t, w, h] = block_rect.map(|b| b.clamp(-100_000, 100_000));
    let bounds = [l * 32, t * 32, (l + w) * 32, (t + h) * 32];
    let mut name = String::new();
    let mut paper = None;
    let mut mask = None;
    let mut extra = Vec::new();
    let mut o = 56;
    while o + 8 <= d.len() {
        let len = u32_at(d, o + 4)? as usize;
        let body = d.get(o + 8..o + 8 + len).ok_or_else(|| bad("вложенный блок выходит за слой"))?;
        if &d[o..o + 4] == b"name" && body.len() >= 2 {
            let n = u16::from_le_bytes([body[0], body[1]]) as usize;
            let units: Vec<u16> =
                body[2..].as_chunks::<2>().0.iter().take(n).map(|c| u16::from_le_bytes(*c)).collect();
            name = String::from_utf16_lossy(&units);
        } else if &d[o..o + 4] == b"ptex" && body.len() >= 8 {
            let v = |i: usize| u16::from_le_bytes([body[i], body[i + 1]]);
            let n = v(6) as usize;
            let units: Vec<u16> =
                body[8..].as_chunks::<2>().0.iter().take(n).map(|c| u16::from_le_bytes(*c)).collect();
            let tex = String::from_utf16_lossy(&units).trim_end_matches('\0').to_string();
            paper = Some((tex, v(0), v(2), v(4)));
        } else if &d[o..o + 4] == b"lmsk" && body.len() >= 24 {
            let v = |i: usize| u32::from_le_bytes([body[i], body[i + 1], body[i + 2], body[i + 3]]);
            let rect = [v(4), v(8), v(12), v(16)].map(|x| (x as i32).clamp(-100_000, 100_000));
            mask = Some((v(0), rect, [body[20], body[21], body[22], body[23]]));
        } else if len > 0 || &d[o..o + 4] != b"\0\0\0\0" {
            extra.push((d[o..o + 4].try_into().unwrap(), body.to_vec()));
        }
        o += 8 + len;
    }
    Ok(LayerInfo {
        id: u32_at(d, 4)?,
        order: u32_at(d, 12)? & 0x7fff_ffff,
        is_folder: &d[16..20] == b"fold",
        bounds,
        block_rect,
        clipping: d[55] & 0x01 != 0,
        depth: d[52],
        locks: [d[53] & 1 != 0, d[53] & 2 != 0, d[53] & 4 != 0, d[53] & 0x80 != 0],
        folder_open: d[55] & 0x40 != 0,
        kind: tag_text(&d[16..20]),
        blend: tag_text(&d[44..48]),
        opacity: u32_at(d, 48)?,
        visible: d[54] & 1 != 0,
        selection_source: d[54] & 0x80 != 0,
        name,
        paper,
        mask,
        extra,
        has_pixels,
    })
}

pub fn read_info(path: &Path) -> io::Result<Sai2Info> {
    let mut f = File::open(path)?;
    let size = f.metadata()?.len();
    let mut head = [0u8; 64];
    f.read_exact(&mut head)?;
    if &head[0..16] != MAGIC {
        return Err(bad("это не файл SAI 2"));
    }
    let count = u32_at(&head, 0x20)? as usize;
    if count > 100_000 || 64 + count as u64 * 16 > size {
        return Err(bad("оглавление повреждено"));
    }
    let mut table = vec![0u8; count * 16];
    f.read_exact(&mut table)?;
    let mut entries: Vec<Entry> = table
        .as_chunks::<16>()
        .0
        .iter()
        .map(|c| Entry {
            tag: [c[0], c[1], c[2], c[3]],
            id: u32::from_le_bytes([c[4], c[5], c[6], c[7]]),
            offset: u64::from_le_bytes([c[8], c[9], c[10], c[11], c[12], c[13], c[14], c[15]]),
            len: 0,
        })
        .collect();
    let mut offsets: Vec<u64> = entries.iter().map(|e| e.offset).collect();
    offsets.push(size);
    offsets.sort_unstable();
    for e in &mut entries {
        if e.offset >= size {
            return Err(bad("блок за пределами файла"));
        }
        let next = offsets.iter().copied().find(|&o| o > e.offset).unwrap_or(size);
        e.len = next - e.offset;
    }
    let mut layers = Vec::new();
    for e in entries.iter().filter(|e| &e.tag == b"layr") {
        if e.len > 1 << 20 {
            return Err(bad("описание слоя слишком велико"));
        }
        let mut d = vec![0u8; e.len as usize];
        f.seek(SeekFrom::Start(e.offset))?;
        f.read_exact(&mut d)?;
        let has_pixels = entries.iter().any(|p| &p.tag == b"lpix" && p.id == e.id);
        layers.push(parse_layer(&d, has_pixels)?);
    }
    Ok(Sai2Info {
        width: u32_at(&head, 0x14)?,
        height: u32_at(&head, 0x18)?,
        dpi: u32_at(&head, 0x1c)? as f64 / 65536.0 * 2.54,
        bg_code: head.get(0x11).copied().unwrap_or(1),
        bg_color: head.get(0x38..0x3b).and_then(|c| c.try_into().ok()).unwrap_or([0x80; 3]),
        effect_color: head.get(0x3c..0x40).and_then(|c| c.try_into().ok()).unwrap_or(*b"norm"),
        resolution: u32_at(&head, 0x1c)?,
        entries,
        layers,
    })
}
