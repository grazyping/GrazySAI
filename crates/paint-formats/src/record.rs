//! Запись процесса рисования (`.gsrec`) и её выгрузка в ускоренное видео (анимированный PNG),
//! без выбранных слоёв, с водяным знаком и паролем (zip AES-256).
//!
//! Файл-спутник рисунка дописывается в конец: заголовок, затем записи «вид, длина, данные».
//! Записываются плитки слоёв по номерам слоёв (`Layer::uid`) и после каждого шага — кадр: дерево
//! слоёв со свойствами. Поэтому при выгрузке любой слой можно исключить, даже добавленный позже.
//! **В записи есть все слои, в том числе скрываемые** — передавать нужно только выгрузку.

use paint_core::blend::BlendMode;
use paint_core::document::Change;
use paint_core::pixel::{from_rgba8, to_rgba8};
use paint_core::tile::{ColorMap, Key, Tile, TILE};
use paint_core::{Document, Layer};
use std::collections::{HashMap, HashSet};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;

const MAGIC: &[u8; 8] = b"GSREC\x01\0\0";
const CANVAS: u8 = b'K';
const CLEAR: u8 = b'C';
const TILE_REC: u8 = b'T';
const FRAME: u8 = b'F';
const STOP: u8 = b'S';
/// Одна запись не больше этого (испорченный файл не займёт память без меры).
const MAX_RECORD: usize = 64 << 20;

/// Узел дерева слоёв в кадре.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub uid: u64,
    /// Глубина вложенности (0 — верхний уровень).
    pub depth: u8,
    pub name: String,
    pub visible: bool,
    pub folder: bool,
    pub clipping: bool,
    pub pass_through: bool,
    pub opacity: u8,
    pub mode: u8,
}

fn tree(layers: &[Layer], depth: u8, out: &mut Vec<Node>) {
    for l in layers {
        out.push(Node {
            uid: l.uid,
            depth,
            name: l.name.chars().take(200).collect(),
            visible: l.visible,
            folder: l.children.is_some(),
            clipping: l.clipping,
            pass_through: l.pass_through,
            opacity: (l.opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
            mode: BlendMode::ALL.iter().position(|m| *m == l.mode).unwrap_or(0) as u8,
        });
        if let Some(ch) = &l.children {
            tree(ch, depth.saturating_add(1), out);
        }
    }
}

/// Слоям с одинаковым номером (копии, вставки) — новые номера; возвращает переименованные.
fn fix_duplicates(layers: &mut [Layer], seen: &mut HashSet<u64>, fixed: &mut Vec<u64>) {
    for l in layers {
        if !seen.insert(l.uid) {
            l.uid = paint_core::layer::next_uid();
            seen.insert(l.uid);
            fixed.push(l.uid);
        }
        if let Some(ch) = l.children.as_mut() {
            fix_duplicates(ch, seen, fixed);
        }
    }
}

fn find(layers: &[Layer], uid: u64) -> Option<&Layer> {
    for l in layers {
        if l.uid == uid {
            return Some(l);
        }
        if let Some(f) = l.children.as_deref().and_then(|c| find(c, uid)) {
            return Some(f);
        }
    }
    None
}

enum Job {
    Canvas { w: u32, h: u32, bg: [u8; 4] },
    Clear(u64),
    Tile { uid: u64, key: Key, px: Option<Box<[paint_core::pixel::Px]>> },
    Frame { ms: u64, bg: [u8; 4], nodes: Vec<Node> },
    Stop,
}

/// Идущая запись: изменения документа уходят в поток записи (сжатие — там).
pub struct Recorder {
    pub path: PathBuf,
    tx: Option<Sender<Job>>,
    writer: Option<std::thread::JoinHandle<io::Result<()>>>,
    bytes: Arc<AtomicU64>,
    last: Vec<Node>,
    pub frames: u64,
}

fn bg_of(doc: &Document) -> [u8; 4] {
    match doc.background {
        Some(c) => [c[0], c[1], c[2], 255],
        None => [0, 0, 0, 0],
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

impl Recorder {
    /// Начать или продолжить запись в `path` и включить журнал документа. С каждого запуска —
    /// полный кадр (номера слоёв живут только до закрытия программы).
    pub fn start(path: &Path, doc: &mut Document) -> io::Result<Self> {
        let fresh = !path.exists() || std::fs::metadata(path)?.len() < MAGIC.len() as u64;
        if !fresh {
            let mut m = [0u8; 8];
            std::fs::File::open(path)?.read_exact(&mut m)?;
            if &m != MAGIC {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "это не запись процесса GrazySAI"));
            }
        }
        if !fresh {
            // продолжение: номера слоёв — как в последнем кадре записи (тот же слой остаётся
            // одной строкой в списке «Не показывать»), новые — больше всех, что уже есть
            let old = info(path)?;
            paint_core::layer::uids_above(old.max_uid);
            let mut cur = Vec::new();
            tree(&doc.layers, 0, &mut cur);
            if cur.iter().any(|n| n.uid <= old.max_uid) {
                for l in &mut doc.layers {
                    l.fresh_uid();
                }
            }
            let mut used = vec![false; old.last_nodes.len()];
            fn adopt(layers: &mut [Layer], depth: u8, old: &[Node], used: &mut [bool]) {
                for l in layers {
                    let folder = l.children.is_some();
                    let name: String = l.name.chars().take(200).collect();
                    if let Some(k) = (0..old.len()).find(|&k| !used[k] && old[k].depth == depth && old[k].folder == folder && old[k].name == name) {
                        used[k] = true;
                        l.uid = old[k].uid;
                    }
                    if let Some(ch) = l.children.as_mut() {
                        adopt(ch, depth.saturating_add(1), old, used);
                    }
                }
            }
            adopt(&mut doc.layers, 0, &old.last_nodes, &mut used);
        }
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
        if fresh {
            f.write_all(MAGIC)?;
        }
        let bytes = Arc::new(AtomicU64::new(f.metadata()?.len()));
        let (tx, rx) = channel::<Job>();
        let b = bytes.clone();
        let writer = std::thread::spawn(move || -> io::Result<()> {
            let mut w = BufWriter::new(f);
            let put = |w: &mut BufWriter<std::fs::File>, kind: u8, body: &[u8]| -> io::Result<()> {
                w.write_all(&[kind])?;
                w.write_all(&(body.len() as u32).to_le_bytes())?;
                w.write_all(body)?;
                b.fetch_add(5 + body.len() as u64, Ordering::Relaxed);
                Ok(())
            };
            for job in rx {
                match job {
                    Job::Canvas { w: cw, h: ch, bg } => {
                        let mut v = cw.to_le_bytes().to_vec();
                        v.extend(ch.to_le_bytes());
                        v.extend(bg);
                        put(&mut w, CANVAS, &v)?;
                    }
                    Job::Clear(uid) => put(&mut w, CLEAR, &uid.to_le_bytes())?,
                    Job::Stop => put(&mut w, STOP, &[])?,
                    Job::Tile { uid, key, px } => {
                        let mut v = uid.to_le_bytes().to_vec();
                        v.extend(key.0.to_le_bytes());
                        v.extend(key.1.to_le_bytes());
                        match px {
                            Some(p) => {
                                let rgba: Vec<u8> = p.iter().flat_map(|&q| to_rgba8(q)).collect();
                                v.push(1);
                                v.extend(miniz_oxide::deflate::compress_to_vec(&rgba, 3));
                            }
                            None => v.push(0),
                        }
                        put(&mut w, TILE_REC, &v)?;
                    }
                    Job::Frame { ms, bg, nodes } => {
                        let mut v = ms.to_le_bytes().to_vec();
                        v.extend(bg);
                        v.extend((nodes.len() as u32).to_le_bytes());
                        for n in nodes {
                            v.extend(n.uid.to_le_bytes());
                            v.push(n.depth);
                            v.push(n.visible as u8 | (n.folder as u8) << 1 | (n.clipping as u8) << 2 | (n.pass_through as u8) << 3);
                            v.push(n.opacity);
                            v.push(n.mode);
                            let name = n.name.as_bytes();
                            v.extend((name.len() as u16).to_le_bytes());
                            v.extend(name);
                        }
                        put(&mut w, FRAME, &v)?;
                        // кадр — конец шага: на диск, чтобы сбой программы не терял запись
                        w.flush()?;
                    }
                }
            }
            w.flush()
        });
        let mut r = Self { path: path.to_path_buf(), tx: Some(tx), writer: Some(writer), bytes, last: Vec::new(), frames: 0 };
        doc.journal = Some(Vec::new());
        r.keyframe(doc);
        Ok(r)
    }

    /// Размер файла записи, байты.
    pub fn bytes(&self) -> u64 {
        self.bytes.load(Ordering::Relaxed)
    }

    fn send(&self, j: Job) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(j);
        }
    }

    fn layer_full(&self, l: &Layer) {
        self.send(Job::Clear(l.uid));
        for (k, t) in &l.pixels.tiles {
            self.send(Job::Tile { uid: l.uid, key: *k, px: Some(t.0.clone()) });
        }
    }

    /// Полный кадр: холст и все слои целиком.
    fn keyframe(&mut self, doc: &mut Document) {
        let mut fixed = Vec::new();
        fix_duplicates(&mut doc.layers, &mut HashSet::new(), &mut fixed);
        self.send(Job::Canvas { w: doc.width, h: doc.height, bg: bg_of(doc) });
        fn all(r: &Recorder, layers: &[Layer]) {
            for l in layers {
                if l.children.is_none() {
                    r.layer_full(l);
                }
                if let Some(ch) = &l.children {
                    all(r, ch);
                }
            }
        }
        all(self, &doc.layers);
        self.frame(doc, true);
    }

    fn frame(&mut self, doc: &Document, force: bool) {
        let mut nodes = Vec::new();
        tree(&doc.layers, 0, &mut nodes);
        if !force && nodes == self.last {
            return;
        }
        self.last = nodes.clone();
        self.send(Job::Frame { ms: now_ms(), bg: bg_of(doc), nodes });
        self.frames += 1;
    }

    /// Записать то, что изменилось с прошлого раза (вызывать после шагов, например раз в кадр
    /// окна). Кадр пишется, если было изменение пикселей или свойств слоёв.
    pub fn capture(&mut self, doc: &mut Document) {
        // документ подменили без журнала — включить снова и записать всё целиком
        if doc.journal.is_none() {
            doc.journal = Some(Vec::new());
            self.keyframe(doc);
            return;
        }
        let changes = doc.journal.as_mut().map(std::mem::take).unwrap_or_default();
        if changes.contains(&Change::Canvas) {
            self.keyframe(doc);
            return;
        }
        // слои-копии получают свои номера и записываются целиком
        let mut fixed = Vec::new();
        fix_duplicates(&mut doc.layers, &mut HashSet::new(), &mut fixed);
        let mut full: HashSet<u64> = fixed.into_iter().collect();
        let mut tiles: HashMap<u64, HashSet<Key>> = HashMap::new();
        for c in &changes {
            match c {
                Change::Layer(uid) => {
                    full.insert(*uid);
                }
                Change::Tiles { uid, keys } => tiles.entry(*uid).or_default().extend(keys.iter().copied()),
                Change::Canvas => {}
            }
        }
        for uid in &full {
            if let Some(l) = find(&doc.layers, *uid) {
                self.layer_full(l);
            }
        }
        for (uid, keys) in tiles {
            if full.contains(&uid) {
                continue;
            }
            let Some(l) = find(&doc.layers, uid) else { continue };
            for k in keys {
                self.send(Job::Tile { uid, key: k, px: l.pixels.tiles.get(&k).map(|t| t.0.clone()) });
            }
        }
        self.frame(doc, !changes.is_empty());
    }

    /// Остановить запись (журнал документа выключается, файл дописывается). `by_user` — остановил
    /// пользователь: при открытии рисунка запись сама не продолжится.
    pub fn stop(mut self, doc: &mut Document, by_user: bool) -> io::Result<()> {
        self.capture(doc);
        if by_user {
            self.send(Job::Stop);
        }
        doc.journal = None;
        drop(self.tx.take());
        match self.writer.take().map(|h| h.join()) {
            Some(Ok(r)) => r,
            _ => Err(io::Error::other("сбой потока записи")),
        }
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        drop(self.tx.take());
        if let Some(h) = self.writer.take() {
            let _ = h.join();
        }
    }
}

/// Событие записи. Плитка — ещё сжатая (распаковывается, только когда нужна).
enum Event {
    Canvas(u32, u32),
    Clear(u64),
    Tile(u64, Key, Option<Vec<u8>>),
    Frame(u64, [u8; 4], Vec<Node>),
    /// Запись остановлена пользователем (при открытии рисунка не продолжается сама).
    Stop,
}

/// Чтение записи по одному событию: в памяти — только текущая запись файла.
struct Reader {
    f: std::io::BufReader<std::fs::File>,
    pos: u64,
    total: u64,
    /// Пропускать плитки, не читая (для сведений и подсчёта шагов).
    skip_tiles: bool,
}

impl Reader {
    fn open(path: &Path, skip_tiles: bool) -> io::Result<Self> {
        let mut f = std::io::BufReader::with_capacity(1 << 20, std::fs::File::open(path)?);
        let mut m = [0u8; 8];
        f.read_exact(&mut m)?;
        if &m != MAGIC {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "это не запись процесса GrazySAI"));
        }
        let total = f.get_ref().metadata()?.len();
        Ok(Self { f, pos: 8, total, skip_tiles })
    }

    /// Следующее событие; `None` — конец (оборванный хвост после сбоя — тоже конец).
    fn next_event(&mut self) -> Option<Event> {
        loop {
            let mut head = [0u8; 5];
            self.f.read_exact(&mut head).ok()?;
            let len = u32::from_le_bytes([head[1], head[2], head[3], head[4]]) as usize;
            self.pos += 5;
            if len > MAX_RECORD || self.pos + len as u64 > self.total {
                return None;
            }
            self.pos += len as u64;
            if head[0] == TILE_REC && self.skip_tiles {
                self.f.seek_relative(len as i64).ok()?;
                continue;
            }
            let mut b = vec![0u8; len];
            self.f.read_exact(&mut b).ok()?;
            if let Some(e) = parse(head[0], b) {
                return Some(e);
            }
        }
    }
}

fn parse(kind: u8, b: Vec<u8>) -> Option<Event> {
    let len = b.len();
    let u64_at = |o: usize| b.get(o..o + 8).map(|s| u64::from_le_bytes(s.try_into().unwrap()));
    let u32_at = |o: usize| b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()));
    Some(match kind {
        CANVAS if len >= 12 => Event::Canvas(u32_at(0)?.clamp(1, 100_000), u32_at(4)?.clamp(1, 100_000)),
        CLEAR if len >= 8 => Event::Clear(u64_at(0)?),
        STOP => Event::Stop,
        TILE_REC if len >= 17 => {
            let (uid, tx, ty) = (u64_at(0)?, u32_at(8)? as i32, u32_at(12)? as i32);
            let px = (b[16] == 1).then(|| b[17..].to_vec());
            Event::Tile(uid, (tx, ty), px)
        }
        FRAME if len >= 16 => {
            let ms = u64_at(0)?;
            let bg = [b[8], b[9], b[10], b[11]];
            let n = u32_at(12)?.min(10_000) as usize;
            let mut nodes = Vec::with_capacity(n);
            let mut p = 16;
            for _ in 0..n {
                let Some(hdr) = b.get(p..p + 14) else { break };
                let uid = u64::from_le_bytes(hdr[0..8].try_into().unwrap());
                let (depth, flags, opacity, mode) = (hdr[8], hdr[9], hdr[10], hdr[11]);
                let nl = u16::from_le_bytes([hdr[12], hdr[13]]) as usize;
                let Some(name) = b.get(p + 14..p + 14 + nl) else { break };
                nodes.push(Node {
                    uid,
                    depth,
                    name: String::from_utf8_lossy(name).into_owned(),
                    visible: flags & 1 != 0,
                    folder: flags & 2 != 0,
                    clipping: flags & 4 != 0,
                    pass_through: flags & 8 != 0,
                    opacity,
                    mode,
                });
                p += 14 + nl;
            }
            Event::Frame(ms, bg, nodes)
        }
        _ => return None,
    })
}

/// Распаковать плитку RGBA8 256×256.
fn inflate_tile(z: &[u8]) -> Option<Vec<u8>> {
    let n = (TILE * TILE * 4) as usize;
    miniz_oxide::inflate::decompress_to_vec_with_limit(z, n).ok().filter(|r| r.len() == n)
}

/// Сведения о записи: число шагов, время, все слои (номер и последнее имя, по порядку появления).
#[derive(Clone, Debug, Default)]
pub struct Info {
    pub frames: usize,
    pub first_ms: u64,
    pub last_ms: u64,
    pub layers: Vec<(u64, String)>,
    pub bytes: u64,
    /// Наибольший номер слоя в записи.
    pub max_uid: u64,
    /// Узлы последнего кадра (чтобы продолжить запись с теми же номерами слоёв).
    pub last_nodes: Vec<Node>,
    /// Последнее событие — остановка пользователем.
    pub stopped: bool,
}

pub fn info(path: &Path) -> io::Result<Info> {
    let mut i = Info { bytes: std::fs::metadata(path)?.len(), ..Default::default() };
    let mut seen: HashMap<u64, usize> = HashMap::new();
    let mut r = Reader::open(path, true)?;
    while let Some(e) = r.next_event() {
        if let Event::Stop = e {
            i.stopped = true;
            continue;
        }
        i.stopped = false;
        if let Event::Frame(ms, _, nodes) = e {
            if i.frames == 0 {
                i.first_ms = ms;
            }
            i.last_ms = ms;
            i.frames += 1;
            i.max_uid = nodes.iter().map(|n| n.uid).fold(i.max_uid, u64::max);
            i.last_nodes = nodes.clone();
            for n in nodes.into_iter().filter(|n| !n.folder) {
                match seen.get(&n.uid) {
                    Some(&k) => i.layers[k].1 = n.name,
                    None => {
                        seen.insert(n.uid, i.layers.len());
                        i.layers.push((n.uid, n.name));
                    }
                }
            }
        }
    }
    Ok(i)
}

/// Как выгружать.
#[derive(Clone, Debug)]
pub struct Export {
    /// Номера слоёв, которых не должно быть в видео.
    pub exclude: HashSet<u64>,
    /// Большая сторона кадра, px.
    pub max_side: u32,
    /// Длительность видео без последнего кадра, с; кадров не больше, чем шагов.
    pub seconds: f32,
    pub fps: u32,
    /// Сколько секунд держать последний кадр.
    pub hold: f32,
    pub watermark: Option<crate::watermark::Watermark>,
    /// Пароль: выгрузка — zip AES-256 с видео внутри.
    pub password: Option<String>,
}

impl Default for Export {
    fn default() -> Self {
        Self { exclude: HashSet::new(), max_side: 1280, seconds: 30.0, fps: 20, hold: 3.0, watermark: None, password: None }
    }
}

/// Собрать документ кадра: слои по дереву узлов, пиксели — из состояния.
fn build(nodes: &[Node], state: &mut HashMap<u64, ColorMap>, exclude: &HashSet<u64>) -> Vec<Layer> {
    fn level(nodes: &[Node], i: &mut usize, depth: u8, state: &mut HashMap<u64, ColorMap>, exclude: &HashSet<u64>) -> Vec<Layer> {
        let mut out = Vec::new();
        while *i < nodes.len() && nodes[*i].depth >= depth {
            let n = nodes[*i].clone();
            *i += 1;
            let mut l = Layer::new(&n.name);
            l.uid = n.uid;
            l.visible = n.visible && !exclude.contains(&n.uid);
            l.opacity = n.opacity as f32 / 255.0;
            l.mode = BlendMode::ALL.get(n.mode as usize).copied().unwrap_or(BlendMode::Normal);
            l.clipping = n.clipping;
            l.pass_through = n.pass_through;
            if n.folder {
                l.children = Some(level(nodes, i, n.depth.saturating_add(1), state, exclude));
            } else if let Some(p) = state.get_mut(&n.uid) {
                l.pixels = std::mem::take(p);
            }
            out.push(l);
        }
        out
    }
    let mut i = 0;
    level(nodes, &mut i, 0, state, exclude)
}

/// Вернуть пиксели слоёв в состояние после показа кадра.
fn give_back(layers: Vec<Layer>, state: &mut HashMap<u64, ColorMap>) {
    for l in layers {
        if let Some(ch) = l.children {
            give_back(ch, state);
        } else {
            state.insert(l.uid, l.pixels);
        }
    }
}

/// Уменьшить RGBA (среднее по клеткам) так, чтобы большая сторона была не больше `max`, а стороны —
/// чётными (так видео открывают все проигрыватели).
fn shrink(rgba: &[u8], w: u32, h: u32, max: u32) -> (u32, u32, Vec<u8>) {
    let k = (max as f32 / w.max(h) as f32).min(1.0);
    let nw = (((w as f32 * k) as u32).max(2)) & !1;
    let nh = (((h as f32 * k) as u32).max(2)) & !1;
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    for y in 0..nh {
        let (y0, y1) = (y * h / nh, ((y + 1) * h / nh).max(y * h / nh + 1).min(h));
        for x in 0..nw {
            let (x0, x1) = (x * w / nw, ((x + 1) * w / nw).max(x * w / nw + 1).min(w));
            let mut s = [0u32; 4];
            for yy in y0..y1 {
                for xx in x0..x1 {
                    let i = ((yy * w + xx) * 4) as usize;
                    for c in 0..4 {
                        s[c] += rgba[i + c] as u32;
                    }
                }
            }
            let n = ((y1 - y0) * (x1 - x0)).max(1);
            let o = ((y * nw + x) * 4) as usize;
            for c in 0..4 {
                out[o + c] = (s[c] / n) as u8;
            }
        }
    }
    (nw, nh, out)
}

/// Выгрузить запись в анимированный PNG `out` (или zip с паролем, если он задан). `progress` —
/// доля 0..1. Возвращает число кадров.
pub fn export(rec: &Path, out: &Path, opts: &Export, progress: &dyn Fn(f32)) -> Result<usize, Box<dyn std::error::Error>> {
    let frames_total = info(rec)?.frames;
    if frames_total == 0 {
        return Err("в записи нет ни одного шага".into());
    }
    let fps = opts.fps.clamp(1, 60);
    let want = ((opts.seconds.max(1.0) * fps as f32) as usize).clamp(1, 20_000).min(frames_total);
    // какие шаги станут кадрами: равномерно, последний — всегда
    let pick: HashSet<usize> = (0..want).map(|i| if want == 1 { frames_total - 1 } else { i * (frames_total - 1) / (want - 1) }).collect();
    let hold_frames = (opts.hold.max(0.0) * fps as f32) as u32;
    let mut reader = Reader::open(rec, false)?;
    // видео пишется сразу на диск (рядом с выгрузкой), в памяти — только текущий кадр
    let tmp = {
        let mut t = out.as_os_str().to_owned();
        t.push(".part");
        PathBuf::from(t)
    };
    let png_file = Buf(std::rc::Rc::new(std::cell::RefCell::new(BufWriter::new(std::fs::File::create(&tmp)?))));
    let r = encode(&mut reader, &pick, hold_frames, fps, opts, &png_file, progress);
    let flushed = png_file.0.borrow_mut().flush();
    drop(png_file);
    let written = match r.and_then(|n| flushed.map(|_| n).map_err(Into::into)) {
        Ok(n) => n,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
    };
    let done: Result<(), Box<dyn std::error::Error>> = match &opts.password {
        Some(p) if !p.is_empty() => zip_into(&tmp, out, p),
        _ => std::fs::rename(&tmp, out).map_err(Into::into),
    };
    let _ = std::fs::remove_file(&tmp);
    done?;
    Ok(written)
}

/// Положить видео `src` в zip `out` с паролем (AES-256), не читая его целиком в память.
fn zip_into(src: &Path, out: &Path, password: &str) -> Result<(), Box<dyn std::error::Error>> {
    let name = out.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "process".into());
    let f = std::fs::File::create(out)?;
    let mut z = zip::ZipWriter::new(BufWriter::new(f));
    let o = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .with_aes_encryption(zip::AesMode::Aes256, password)
        .large_file(std::fs::metadata(src)?.len() >= u32::MAX as u64);
    z.start_file(format!("{name}.png"), o)?;
    io::copy(&mut std::io::BufReader::new(std::fs::File::open(src)?), &mut z)?;
    z.finish()?.flush()?;
    Ok(())
}

/// Собрать кадры APNG из событий записи.
fn encode(reader: &mut Reader, pick: &HashSet<usize>, hold_frames: u32, fps: u32, opts: &Export, png_file: &Buf, progress: &dyn Fn(f32)) -> Result<usize, Box<dyn std::error::Error>> {
    let mut state: HashMap<u64, ColorMap> = HashMap::new();
    let (mut cw, mut ch) = (1u32, 1u32);
    let mut size: Option<(u32, u32)> = None;
    let mut enc: Option<png::Writer<Buf>> = None;
    let mut prev: Option<Vec<u8>> = None;
    let mut written = 0usize;
    let mut step = 0usize;
    let count = pick.len() as u32 + hold_frames.min(1);
    while let Some(e) = reader.next_event() {
        match e {
            Event::Stop => {}
            Event::Canvas(w, h) => {
                (cw, ch) = (w, h);
                state.clear();
            }
            Event::Clear(uid) => {
                state.insert(uid, ColorMap::new());
            }
            Event::Tile(uid, key, px) => {
                let m = state.entry(uid).or_default();
                match px {
                    Some(z) => {
                        if let Some(raw) = inflate_tile(&z) {
                            let t: Vec<paint_core::pixel::Px> = raw.as_chunks::<4>().0.iter().map(|c| from_rgba8(*c)).collect();
                            m.tiles.insert(key, Tile(t.into_boxed_slice()));
                        }
                    }
                    None => {
                        m.tiles.remove(&key);
                    }
                }
            }
            Event::Frame(_, bg, nodes) => {
                // удалённые слои больше не нужны (вернутся отменой — запишутся заново целиком)
                let alive: HashSet<u64> = nodes.iter().map(|n| n.uid).collect();
                state.retain(|uid, _| alive.contains(uid));
                let this = step;
                step += 1;
                if !pick.contains(&this) {
                    continue;
                }
                let mut doc = Document::new(cw, ch);
                doc.background = (bg[3] == 255).then_some([bg[0], bg[1], bg[2]]);
                doc.layers = build(&nodes, &mut state, &opts.exclude);
                let full = doc.flatten();
                give_back(std::mem::take(&mut doc.layers), &mut state);
                let (fw, fh, mut img) = shrink(&full, cw, ch, opts.max_side.clamp(64, 8192));
                if let Some(wm) = &opts.watermark {
                    crate::watermark::apply(&mut img, fw, fh, wm);
                }
                // размер видео — по первому кадру; холст мог поменяться — кадр вписывается
                let (vw, vh) = *size.get_or_insert((fw, fh));
                if (fw, fh) != (vw, vh) {
                    img = fit(&img, fw, fh, vw, vh);
                }
                if enc.is_none() {
                    let mut e = png::Encoder::new(png_file.clone(), vw, vh);
                    e.set_color(png::ColorType::Rgba);
                    e.set_depth(png::BitDepth::Eight);
                    e.set_animated(count.max(1), 0)?;
                    e.set_frame_delay(1, fps as u16)?;
                    e.set_compression(png::Compression::Fast);
                    enc = Some(e.write_header()?);
                }
                let w = enc.as_mut().expect("кодировщик готов");
                write_frame(w, &img, prev.as_deref(), vw, vh, 1, fps as u16)?;
                prev = Some(img);
                written += 1;
                progress(written as f32 / pick.len() as f32);
            }
        }
    }
    // последний кадр держится: одна копия с долгой задержкой
    if hold_frames > 0 {
        if let (Some(w), Some(img)) = (enc.as_mut(), prev.as_ref()) {
            let (vw, vh) = size.unwrap_or((1, 1));
            write_frame(w, img, Some(img), vw, vh, hold_frames.min(u16::MAX as u32) as u16, fps as u16)?;
        }
    }
    if let Some(w) = enc {
        w.finish()?;
    }
    Ok(written)
}

/// Файл для кодировщика PNG, общий с выгрузкой (дописать и закрыть его после `finish`).
#[derive(Clone)]
struct Buf(std::rc::Rc<std::cell::RefCell<BufWriter<std::fs::File>>>);

impl Write for Buf {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().write(b)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.borrow_mut().flush()
    }
}

/// Вписать кадр другого размера в размер видео (посередине, поля прозрачные).
fn fit(img: &[u8], w: u32, h: u32, vw: u32, vh: u32) -> Vec<u8> {
    let k = (vw as f32 / w as f32).min(vh as f32 / h as f32);
    let (nw, nh) = (((w as f32 * k) as u32).clamp(1, vw), ((h as f32 * k) as u32).clamp(1, vh));
    let mut out = vec![0u8; (vw * vh * 4) as usize];
    let (ox, oy) = ((vw - nw) / 2, (vh - nh) / 2);
    for y in 0..nh {
        for x in 0..nw {
            let (sx, sy) = ((x * w / nw).min(w - 1), (y * h / nh).min(h - 1));
            let s = ((sy * w + sx) * 4) as usize;
            let d = (((y + oy) * vw + x + ox) * 4) as usize;
            out[d..d + 4].copy_from_slice(&img[s..s + 4]);
        }
    }
    out
}

/// Кадр APNG: только изменившийся прямоугольник (у первого кадра — весь).
fn write_frame(w: &mut png::Writer<Buf>, img: &[u8], prev: Option<&[u8]>, vw: u32, vh: u32, delay: u16, fps: u16) -> Result<(), png::EncodingError> {
    let (mut x0, mut y0, mut x1, mut y1) = (0u32, 0u32, vw, vh);
    if let Some(p) = prev {
        let (mut a, mut b, mut c, mut d) = (vw, vh, 0u32, 0u32);
        for y in 0..vh {
            for x in 0..vw {
                let i = ((y * vw + x) * 4) as usize;
                if img[i..i + 4] != p[i..i + 4] {
                    (a, b, c, d) = (a.min(x), b.min(y), c.max(x + 1), d.max(y + 1));
                }
            }
        }
        // ничего не изменилось — один пиксель (кадр всё равно нужен для времени)
        if c == 0 {
            (a, b, c, d) = (0, 0, 1, 1);
        }
        (x0, y0, x1, y1) = (a, b, c, d);
    }
    let (fw, fh) = (x1 - x0, y1 - y0);
    let mut sub = Vec::with_capacity((fw * fh * 4) as usize);
    for y in y0..y1 {
        let i = ((y * vw + x0) * 4) as usize;
        sub.extend_from_slice(&img[i..i + (fw * 4) as usize]);
    }
    // сдвиг проверяется вместе с размером: сначала сбросить сдвиг прошлого кадра
    w.set_frame_position(0, 0)?;
    w.set_frame_dimension(fw, fh)?;
    w.set_frame_position(x0, y0)?;
    w.set_dispose_op(png::DisposeOp::None)?;
    w.set_blend_op(png::BlendOp::Source)?;
    w.set_frame_delay(delay, fps)?;
    w.write_image_data(&sub)
}

#[cfg(test)]
mod tests {
    use super::*;
    use paint_core::{BrushParams, InputPoint, Stroke};

    fn stroke(doc: &mut Document, layer: usize, y: f32) {
        let mut s = Stroke::new(BrushParams { size: 12.0, size_by_pressure: false, ..Default::default() });
        s.add_point(InputPoint { x: 10.0, y, pressure: 1.0 });
        s.add_point(InputPoint { x: 150.0, y, pressure: 1.0 });
        doc.commit_stroke(layer, &s, [0, 0, 0]);
    }

    /// Запись → выгрузка: кадров столько, сколько шагов; исключённый слой в кадрах не виден.
    #[test]
    fn record_and_export_without_a_layer() {
        let dir = std::env::temp_dir().join(format!("grazysai-rec-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rec = dir.join("рисунок.sai2.gsrec");
        let _ = std::fs::remove_file(&rec);
        let mut doc = Document::new(160, 100);
        let mut r = Recorder::start(&rec, &mut doc).unwrap();
        // слой-опора сверху: его потом исключим
        doc.insert_layer(&[1], Layer::new("Опора"));
        r.capture(&mut doc);
        stroke(&mut doc, 1, 20.0);
        r.capture(&mut doc);
        stroke(&mut doc, 0, 70.0);
        r.capture(&mut doc);
        // ничего не изменилось — кадра нет
        r.capture(&mut doc);
        r.stop(&mut doc, false).unwrap();
        assert!(doc.journal.is_none());
        let i = info(&rec).unwrap();
        assert_eq!(i.frames, 4, "начало + 3 шага");
        let support = i.layers.iter().find(|(_, n)| n == "Опора").unwrap().0;
        let out = dir.join("процесс.png");
        let opts = Export { exclude: [support].into(), max_side: 160, hold: 0.0, ..Default::default() };
        let n = export(&rec, &out, &opts, &|_| {}).unwrap();
        assert_eq!(n, 4);
        // последний кадр APNG = финальная картинка без опоры: читаем и смотрим строки 20 и 70
        let dec = png::Decoder::new(std::io::Cursor::new(std::fs::read(&out).unwrap()));
        let mut rd = dec.read_info().unwrap();
        assert_eq!(rd.info().animation_control.map(|a| a.num_frames), Some(4));
        let mut canvas = vec![0u8; 160 * 100 * 4];
        let mut buf = vec![0u8; rd.output_buffer_size()];
        while let Ok(fi) = rd.next_frame(&mut buf) {
            let fc = rd.info().frame_control.unwrap();
            for y in 0..fc.height as usize {
                for x in 0..fc.width as usize {
                    let s = (y * fc.width as usize + x) * 4;
                    let d = ((y + fc.y_offset as usize) * 160 + x + fc.x_offset as usize) * 4;
                    canvas[d..d + 4].copy_from_slice(&buf[s..s + 4]);
                }
            }
            let _ = fi;
        }
        let at = |x: usize, y: usize| canvas[(y * 160 + x) * 4];
        assert!(at(80, 70) < 80, "мазок на обычном слое есть");
        assert!(at(80, 20) > 200, "мазка на исключённой опоре нет");
        std::fs::remove_dir_all(dir).ok();
    }

    /// Продолжение записи в другом запуске: номера слоёв совпали случайно — слой «Опора» всё равно
    /// одна строка и исключается во всех частях записи; остановка пользователем запоминается.
    #[test]
    fn resume_keeps_layer_identity() {
        let dir = std::env::temp_dir().join(format!("grazysai-recr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rec = dir.join("r.gsrec");
        let _ = std::fs::remove_file(&rec);
        let mut doc = Document::new(160, 100);
        doc.insert_layer(&[1], Layer::new("Опора"));
        let mut r = Recorder::start(&rec, &mut doc).unwrap();
        stroke(&mut doc, 1, 20.0);
        r.capture(&mut doc);
        r.stop(&mut doc, false).unwrap();
        let first = info(&rec).unwrap();
        assert!(!first.stopped);
        // «новый запуск»: рисунок открыт заново, номера слоёв перепутаны с прошлыми
        let (a, b) = (doc.layers[0].uid, doc.layers[1].uid);
        doc.layers[0].uid = b;
        doc.layers[1].uid = a;
        let mut r = Recorder::start(&rec, &mut doc).unwrap();
        stroke(&mut doc, 1, 50.0);
        stroke(&mut doc, 0, 80.0);
        r.capture(&mut doc);
        r.stop(&mut doc, true).unwrap();
        let i = info(&rec).unwrap();
        assert!(i.stopped);
        let names: Vec<&str> = i.layers.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(names.iter().filter(|n| **n == "Опора").count(), 1, "{names:?}");
        let support = i.layers.iter().find(|(_, n)| n == "Опора").unwrap().0;
        let out = dir.join("r.png");
        export(&rec, &out, &Export { exclude: [support].into(), max_side: 160, hold: 0.0, ..Default::default() }, &|_| {}).unwrap();
        let mut rd = png::Decoder::new(std::io::Cursor::new(std::fs::read(&out).unwrap())).read_info().unwrap();
        let mut canvas = vec![0u8; 160 * 100 * 4];
        let mut buf = vec![0u8; rd.output_buffer_size()];
        while rd.next_frame(&mut buf).is_ok() {
            let fc = rd.info().frame_control.unwrap();
            for y in 0..fc.height as usize {
                for x in 0..fc.width as usize {
                    let s = (y * fc.width as usize + x) * 4;
                    let d = ((y + fc.y_offset as usize) * 160 + x + fc.x_offset as usize) * 4;
                    canvas[d..d + 4].copy_from_slice(&buf[s..s + 4]);
                }
            }
        }
        let at = |y: usize| canvas[(y * 160 + 80) * 4];
        assert!(at(80) < 80, "обычный слой виден");
        assert!(at(20) > 200 && at(50) > 200, "опоры нет ни из первой, ни из второй части");
        std::fs::remove_dir_all(dir).ok();
    }

    /// С паролем — zip AES-256, внутри — тот же PNG.
    #[test]
    fn password_export_is_aes_zip() {
        let dir = std::env::temp_dir().join(format!("grazysai-recz-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rec = dir.join("a.gsrec");
        let _ = std::fs::remove_file(&rec);
        let mut doc = Document::new(64, 64);
        let mut r = Recorder::start(&rec, &mut doc).unwrap();
        stroke(&mut doc, 0, 30.0);
        r.capture(&mut doc);
        r.stop(&mut doc, false).unwrap();
        let out = dir.join("a.zip");
        export(&rec, &out, &Export { password: Some("секрет".into()), hold: 0.0, ..Default::default() }, &|_| {}).unwrap();
        let mut z = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
        assert!(z.by_index_decrypt(0, b"wrong").is_err(), "без пароля не открыть");
        let mut f = z.by_index_decrypt(0, "секрет".as_bytes()).unwrap();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        assert!(data.starts_with(b"\x89PNG"));
        std::fs::remove_dir_all(dir).ok();
    }

    /// Испорченный или оборванный хвост файла не мешает читать то, что успело записаться.
    #[test]
    fn torn_tail_is_ignored() {
        let dir = std::env::temp_dir().join(format!("grazysai-rect-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rec = dir.join("t.gsrec");
        let _ = std::fs::remove_file(&rec);
        let mut doc = Document::new(64, 64);
        let mut r = Recorder::start(&rec, &mut doc).unwrap();
        stroke(&mut doc, 0, 30.0);
        r.capture(&mut doc);
        r.stop(&mut doc, false).unwrap();
        let mut f = std::fs::OpenOptions::new().append(true).open(&rec).unwrap();
        f.write_all(&[b'T', 255, 255, 255, 0, 1, 2]).unwrap();
        assert_eq!(info(&rec).unwrap().frames, 2);
        std::fs::remove_dir_all(dir).ok();
    }
}
