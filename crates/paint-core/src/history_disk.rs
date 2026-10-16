//! История отмены на диске: старые шаги с плитками уходят во временные файлы, чтобы на очень
//! больших холстах держать все 100 шагов (как в SAI, который пишет историю на диск).
//!
//! Файл на шаг — подряд плитки 64×64 по 8 байт на точку (u16 LE ×4). Папка —
//! `<временная>/GrazySAI-history/<номер процесса>`; файл удаляется вместе с шагом, папки прошлых
//! запусков старше суток убираются при первой выгрузке. `PAINT_HISTORY_RAM` — сколько байт
//! истории держать в памяти (по умолчанию 1 ГБ).

use crate::pixel::Px;
use crate::tile::{Key, Tile, TILE_LEN};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// Сколько памяти держит история, прежде чем выгружать на диск.
pub fn ram_budget() -> usize {
    std::env::var("PAINT_HISTORY_RAM").ok().and_then(|v| v.parse().ok()).unwrap_or(1 << 30)
}

/// Предел истории на диске.
pub const DISK_BYTES: u64 = 16 << 30;

static DISK_USED: AtomicU64 = AtomicU64::new(0);

/// Плитки шага в файле; файл удаляется вместе со значением.
pub struct Spill {
    file: PathBuf,
    /// Номер плитки и была ли она (пустые плитки в файл не пишутся).
    pub keys: Vec<(Key, bool)>,
    bytes: u64,
}

impl Drop for Spill {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.file);
        DISK_USED.fetch_sub(self.bytes, Ordering::Relaxed);
    }
}

fn dir() -> std::io::Result<PathBuf> {
    let root = std::env::temp_dir().join("GrazySAI-history");
    let d = root.join(std::process::id().to_string());
    if !d.exists() {
        // папки прошлых запусков (программа могла закрыться аварийно)
        if let Ok(rd) = std::fs::read_dir(&root) {
            for e in rd.flatten() {
                let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok());
                if old.is_some_and(|age| age.as_secs() > 24 * 3600) {
                    let _ = std::fs::remove_dir_all(e.path());
                }
            }
        }
        std::fs::create_dir_all(&d)?;
    }
    Ok(d)
}

/// Записать плитки в файл. `None` — места или права нет (шаг останется в памяти или отбросится).
pub fn spill(tiles: &[(Key, Option<Tile<Px>>)]) -> Option<Spill> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let bytes = (tiles.iter().filter(|(_, t)| t.is_some()).count() * TILE_LEN * 8) as u64;
    if DISK_USED.load(Ordering::Relaxed) + bytes > DISK_BYTES {
        return None;
    }
    let file = dir().ok()?.join(format!("{}.tiles", NEXT.fetch_add(1, Ordering::Relaxed)));
    let mut out = std::io::BufWriter::new(std::fs::File::create(&file).ok()?);
    let mut buf = Vec::with_capacity(TILE_LEN * 8);
    for (_, t) in tiles {
        if let Some(t) = t {
            buf.clear();
            for px in t.0.iter() {
                for c in px {
                    buf.extend_from_slice(&c.to_le_bytes());
                }
            }
            if out.write_all(&buf).is_err() {
                let _ = std::fs::remove_file(&file);
                return None;
            }
        }
    }
    if out.flush().is_err() {
        let _ = std::fs::remove_file(&file);
        return None;
    }
    DISK_USED.fetch_add(bytes, Ordering::Relaxed);
    Some(Spill { file, keys: tiles.iter().map(|(k, t)| (*k, t.is_some())).collect(), bytes })
}

/// Прочитать плитки обратно.
pub fn load(s: &Spill) -> Option<Vec<(Key, Option<Tile<Px>>)>> {
    let mut f = std::io::BufReader::new(std::fs::File::open(&s.file).ok()?);
    let mut buf = vec![0u8; TILE_LEN * 8];
    let mut out = Vec::with_capacity(s.keys.len());
    for (k, present) in &s.keys {
        if !present {
            out.push((*k, None));
            continue;
        }
        f.read_exact(&mut buf).ok()?;
        let mut t = Tile::filled([0u16; 4]);
        for (i, px) in t.0.iter_mut().enumerate() {
            for (c, v) in px.iter_mut().enumerate() {
                let o = (i * 4 + c) * 2;
                *v = u16::from_le_bytes([buf[o], buf[o + 1]]);
            }
        }
        out.push((*k, Some(t)));
    }
    Some(out)
}

/// Убрать папку истории этого запуска (при выходе из программы).
pub fn cleanup() {
    let _ = std::fs::remove_dir_all(std::env::temp_dir().join("GrazySAI-history").join(std::process::id().to_string()));
}
