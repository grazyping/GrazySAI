//! Чтение и запись файлов. Сейчас — `.sai2`: структура и пиксели растровых слоёв
//! (см. docs/03-sai2-format.md).

pub mod anim_export;
pub mod brush_tip;
pub mod convert;
pub mod csp;
pub mod descriptor;
pub mod jssf;
pub mod krita;
pub mod linework;
pub mod ora;
pub mod pack;
pub mod palette;
pub mod preview;
pub mod psd;
pub mod record;
pub mod sai2;
pub mod sai2_3d;
pub mod sai2_doc;
pub mod sai2_pixels;
pub mod sai2_text;
pub mod sai2_vector;
pub mod watermark;
pub mod sai2_write;
