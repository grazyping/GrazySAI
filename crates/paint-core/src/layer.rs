//! Слой: цветные плитки плюс свойства.

use crate::blend::BlendMode;
use crate::pixel::{Px, ONE};
use crate::stroke::Stroke;
use crate::tile::{ColorMap, Key, Tile};

/// Прежнее содержимое плиток, затронутых операцией, — этого достаточно для отмены.
pub type TileBackup = Vec<(Key, Option<Tile<Px>>)>;

/// Кривые слоя контура (`liwk`) или фигур (`shap`) — как в файле `.sai2`; пиксели слоя — их вывод.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorData {
    pub kind: [u8; 4],
    pub raw: Vec<u8>,
}

/// Файл модели слоя 3D (glTF, GLB, OBJ с соседними файлами) — общий для копий слоя и шагов
/// истории: при правках позы и камеры не копируется.
#[derive(Debug, PartialEq)]
pub struct Asset3d {
    /// Номер модели (по нему сцена ссылается на неё, и по нему же кэшируется разбор).
    pub id: u64,
    pub name: String,
    /// Главный файл первым, затем соседние (буферы, картинки): имя без пути и содержимое.
    pub files: Vec<(String, Vec<u8>)>,
}

/// Сцена слоя 3D: модели и состояние (положение, поза, камера — текст `paint-3d`). Пиксели
/// слоя — её отрисовка, их обновляет программа после каждой правки.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene3dData {
    pub assets: Vec<std::sync::Arc<Asset3d>>,
    pub state: String,
}

/// Маска слоя, как в SAI 2: сколько слоя скрыто в точке, 0 (виден) ..= 16384 (скрыт).
#[derive(Clone, Debug, PartialEq)]
pub struct LayerMask {
    pub hide: crate::tile::MaskMap,
    /// Четыре байта признаков из файла (смысл не разобран; у образца `01 01 01 00`).
    pub flags: [u8; 4],
}

#[derive(Clone, Debug)]
pub struct Layer {
    pub name: String,
    pub pixels: ColorMap,
    pub opacity: f32,
    pub mode: BlendMode,
    pub visible: bool,
    /// «Сохранять непрозрачность»: красить можно только там, где уже есть краска.
    pub preserve_opacity: bool,
    /// Запрет рисования, запрет перемещения, запрет всего (замки SAI).
    pub lock_paint: bool,
    pub lock_move: bool,
    pub lock_all: bool,
    /// «Источник выбора»: палочка и заливка берут цвет со сведённых слоёв-источников.
    pub selection_source: bool,
    /// Слой контура или фигур: кривые сохраняются в `.sai2` как были; кистью такой слой не
    /// правится (как в SAI), для правки — «Растеризовать».
    pub vector: Option<VectorData>,
    /// Слой 3D: сцена, пиксели слоя — её отрисовка.
    pub scene3d: Option<Box<Scene3dData>>,
    /// Обтравка: слой виден только в пределах слоя, лежащего под ним.
    pub clipping: bool,
    /// Папка: вложенные слои снизу вверх. У обычного слоя — `None`.
    pub children: Option<Vec<Layer>>,
    /// Папка «насквозь»: вложенные слои накладываются прямо на то, что под папкой.
    pub pass_through: bool,
    /// Бумага слоя.
    pub paper: Option<crate::paper::Paper>,
    /// Эффект слоя: кайма или двухцветный.
    pub effect: Option<crate::effects::Effect>,
    /// Маска слоя.
    pub mask: Option<LayerMask>,
    /// Папка отмечена для команд видимости (Ctrl + щелчок по глазу, как в SAI); в файл не пишется.
    pub marked: bool,
    /// Прочие вложенные блоки описания слоя из `.sai2` (эффекты и т. п.) — записываются обратно
    /// как были.
    pub extra: Vec<([u8; 4], Vec<u8>)>,
    /// Номер слоя на время работы программы (в файл не пишется): по нему запись процесса узнаёт
    /// слой после перестановок, отмены и повтора. У копии слоя номер свой (`fresh_uid`).
    pub uid: u64,
}

/// Новый номер слоя.
///
/// Номера не повторяются и между запусками: счёт начинается с «секунды запуска × 2²⁰» (запись
/// процесса дописывается из разных запусков, и номер слоя в ней должен оставаться единственным).
pub fn next_uid() -> u64 {
    uid_counter().fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Следующие номера — больше `uid` (на случай, если часы ушли назад, а в записи есть номера новее).
pub fn uids_above(uid: u64) {
    uid_counter().fetch_max(uid.saturating_add(1), std::sync::atomic::Ordering::Relaxed);
}

fn uid_counter() -> &'static std::sync::atomic::AtomicU64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    if NEXT.load(Ordering::Relaxed) == 0 {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_secs().max(1));
        let _ = NEXT.compare_exchange(0, secs << 20, Ordering::Relaxed, Ordering::Relaxed);
    }
    &NEXT
}

impl Layer {
    /// Дать слою (и вложенным) новые номера — у копии.
    pub fn fresh_uid(&mut self) {
        self.uid = next_uid();
        for c in self.children.iter_mut().flatten() {
            c.fresh_uid();
        }
    }

    pub fn new(name: &str) -> Self {
        Self {
            uid: next_uid(),
            name: name.to_string(),
            pixels: ColorMap::new(),
            opacity: 1.0,
            mode: BlendMode::Normal,
            visible: true,
            preserve_opacity: false,
            lock_paint: false,
            lock_move: false,
            lock_all: false,
            selection_source: false,
            marked: false,
            vector: None,
            scene3d: None,
            clipping: false,
            children: None,
            pass_through: false,
            paper: None,
            effect: None,
            mask: None,
            extra: Vec::new(),
        }
    }

    /// Рисовать на слое можно (нет запрета рисования или «всего»).
    pub fn paintable(&self) -> bool {
        !self.lock_paint && !self.lock_all && self.vector.is_none() && self.scene3d.is_none()
    }

    /// Двигать и трансформировать слой можно.
    pub fn movable(&self) -> bool {
        !self.lock_move && !self.lock_all && self.vector.is_none() && self.scene3d.is_none()
    }

    pub fn folder(name: &str, children: Vec<Layer>) -> Self {
        Self { children: Some(children), ..Self::new(name) }
    }

    /// Накладывает покрытие штриха на одну плитку слоя.
    /// `sel` — плитка выделения: покрытие штриха умножается на неё (`None` — выделения нет).
    pub fn apply_mask_tile(&mut self, key: Key, mask: &Tile<u16>, erase: bool, rgb: [u8; 3], sel: Option<&Tile<u16>>, effect: Option<crate::blend::BlendMode>) {
        let preserve = self.preserve_opacity;
        mask_into_tile(self.pixels.tile_mut(key), mask, erase, rgb, preserve, sel, effect);
    }

    /// Переносит готовый штрих на слой цветом `rgb`. Возвращает данные для отмены.
    pub fn apply_stroke(&mut self, stroke: &Stroke, rgb: [u8; 3], sel: &crate::selection::Selection) -> TileBackup {
        let mut backup = Vec::new();
        for (key, mask) in &stroke.mask.tiles {
            if mask.0.iter().all(|&m| m == 0) {
                continue;
            }
            backup.push((*key, self.pixels.tiles.get(key).cloned()));
            self.apply_mask_tile(*key, mask, stroke.params.erase, rgb, sel.tile(*key).as_ref(), stroke.params.effect);
        }
        backup
    }

    /// Возвращает плитки из `backup`; отдаёт данные для повтора.
    pub fn restore(&mut self, backup: TileBackup) -> TileBackup {
        let mut redo = Vec::with_capacity(backup.len());
        for (key, old) in backup {
            let current = match old {
                Some(t) => self.pixels.tiles.insert(key, t),
                None => self.pixels.tiles.remove(&key),
            };
            redo.push((key, current));
        }
        redo
    }
}

/// Перо эффектов: покрытие `mask` — непрозрачность краски `rgb`, наложенной режимом `mode`.
fn effect_into_tile(tile: &mut Tile<Px>, mask: &Tile<u16>, rgb: [u8; 3], preserve: bool, sel: Option<&Tile<u16>>, mode: crate::blend::BlendMode) {
    let k = ONE as f32;
    let src = [rgb[0] as f32 / 255.0, rgb[1] as f32 / 255.0, rgb[2] as f32 / 255.0, 1.0];
    for (i, (px, &m)) in tile.0.iter_mut().zip(mask.0.iter()).enumerate() {
        let m = match sel {
            Some(s) => crate::selection::limit(m as u32, s.0[i]),
            None => m as u32,
        };
        if m == 0 || (preserve && px[3] == 0) {
            continue;
        }
        let dst = px.map(|c| c as f32 / k);
        let mut out = crate::blend::composite(mode, dst, src, m as f32 / k);
        if preserve {
            // непрозрачность прежняя, меняется только цвет
            let s = if out[3] > 1e-9 { dst[3] / out[3] } else { 0.0 };
            for c in out.iter_mut().take(3) {
                *c *= s;
            }
            out[3] = dst[3];
        }
        let a = (out[3].clamp(0.0, 1.0) * k + 0.5) as u16;
        *px = [0, 1, 2, 3].map(|c| if c == 3 { a } else { ((out[c].clamp(0.0, 1.0) * k + 0.5) as u16).min(a) });
    }
}

/// Покрытие штриха `mask` (с учётом выделения `sel`) — в плитку цветом `rgb` или ластиком;
/// `preserve` — сохранять непрозрачность; `effect` — перо эффектов: краска ложится этим режимом
/// наложения (сверено с SAI 2026: на прозрачном — как обычная краска, на краске — формула режима).
/// Плитки независимы — их можно класть параллельно.
pub fn mask_into_tile(tile: &mut Tile<Px>, mask: &Tile<u16>, erase: bool, rgb: [u8; 3], preserve: bool, sel: Option<&Tile<u16>>, effect: Option<crate::blend::BlendMode>) {
    if let Some(mode) = effect.filter(|_| !erase) {
        return effect_into_tile(tile, mask, rgb, preserve, sel, mode);
    }
    let color = rgb.map(|c| (c as u32 * ONE + 127) / 255);
    for (i, (px, &m)) in tile.0.iter_mut().zip(mask.0.iter()).enumerate() {
        let m = match sel {
            Some(s) => crate::selection::limit(m as u32, s.0[i]),
            None => m as u32,
        };
        if m == 0 {
            continue;
        }
        let inv = ONE - m;
        if erase {
            for c in px.iter_mut() {
                *c = ((*c as u32 * inv + ONE / 2) / ONE) as u16;
            }
        } else if preserve {
            let a = px[3] as u32;
            for i in 0..3 {
                let src = color[i] * a / ONE; // краска с альфой самого пикселя
                px[i] = ((src * m + px[i] as u32 * inv + ONE / 2) / ONE) as u16;
            }
        } else {
            for i in 0..3 {
                px[i] = ((color[i] * m + px[i] as u32 * inv + ONE / 2) / ONE) as u16;
            }
            px[3] = (m + (px[3] as u32 * inv + ONE / 2) / ONE).min(ONE) as u16;
        }
    }
}
