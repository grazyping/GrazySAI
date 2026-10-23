//! Ядро редактора: пиксели, плитки, слои, наложение, кисть, штрих, документ.
//! От интерфейса и планшета не зависит — всё проверяется тестами и рендером в файл.

pub mod colorize;
pub mod curve;
pub mod dynamics;
pub mod blend;
pub mod brush;
pub mod canvas_ops;
pub mod dab;
pub mod document;
pub mod effects;
pub mod filters;
pub mod gridwarp;
pub mod io;
pub mod layer;
pub mod mix;
pub mod history_disk;
pub mod paper;
pub mod pixel;
pub mod resample;
pub mod scatter;
pub mod selection;
pub mod stabilizer;
pub mod stroke;
pub mod tile;
pub mod vector;
pub mod vectorize;

pub use blend::BlendMode;
pub use brush::BrushParams;
pub use document::{Change, Document, LiveStroke, StepInfo, StepKind};
pub use layer::Layer;
pub use stabilizer::Stabilizer;
pub use stroke::{InputPoint, Mirror, Radial, Stamp, Stroke, Tip};

/// Число потоков для параллельного счёта (ядра машины). Запоминается при первом вызове:
/// `available_parallelism` в Linux читает cgroup при каждом вызове — это десятки микросекунд,
/// а горячие места (каждое движение пера) спрашивают его постоянно.
pub fn threads() -> usize {
    static N: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *N.get_or_init(|| std::thread::available_parallelism().map_or(4, |n| n.get()))
}
