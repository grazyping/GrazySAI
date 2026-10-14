//! Ядро редактора: пиксели, плитки, слои, наложение, кисть, штрих, документ.
//! От интерфейса и планшета не зависит — всё проверяется тестами и рендером в файл.

pub mod blend;
pub mod brush;
pub mod dab;
pub mod document;
pub mod io;
pub mod layer;
pub mod pixel;
pub mod resample;
pub mod stabilizer;
pub mod stroke;
pub mod tile;
pub mod vector;

pub use blend::BlendMode;
pub use brush::BrushParams;
pub use document::{Document, LiveStroke};
pub use layer::Layer;
pub use stabilizer::Stabilizer;
pub use stroke::{InputPoint, Stroke};
