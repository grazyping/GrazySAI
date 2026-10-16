//! Значки пунктов меню: те же векторные знаки, что на панелях (`sai_ui::glyph`), отрисованные
//! один раз в маленькую картинку. Рисуются белым — в меню их красит цвет текста (`tint`), так
//! они бледнеют вместе с недоступным пунктом.

use eframe::egui::{self, Color32, ColorImage, Id, LayerId, Order, Pos2, Rect, TextureHandle, TextureOptions, Vec2};
use eframe::epaint::{ClippedShape, Primitive, TessellationOptions, Tessellator};
use std::collections::HashMap;
use std::sync::Mutex;

/// Сторона значка в точках интерфейса и во сколько раз картинка крупнее (чётче при масштабе).
pub const SIZE: f32 = 14.0;
const SCALE: f32 = 3.0;

static CACHE: Mutex<Option<HashMap<&'static str, TextureHandle>>> = Mutex::new(None);

/// Картинка значка `name` (имя знака из `sai_ui::glyph`).
pub fn texture(ctx: &egui::Context, name: &'static str) -> TextureHandle {
    let mut cache = CACHE.lock().unwrap();
    let map = cache.get_or_insert_with(HashMap::new);
    if let Some(t) = map.get(name) {
        return t.clone();
    }
    let t = ctx.load_texture(format!("icon-{name}"), render(ctx, name), TextureOptions::LINEAR);
    map.insert(name, t.clone());
    t
}

/// Значок для `Button::image_and_text` и `menu_image_text_button`.
pub fn image(ctx: &egui::Context, name: &'static str) -> egui::Image<'static> {
    let t = texture(ctx, name);
    egui::Image::new(egui::load::SizedTexture::new(t.id(), Vec2::splat(SIZE))).tint(ctx.style().visuals.text_color())
}

fn render(ctx: &egui::Context, name: &'static str) -> ColorImage {
    // знак рисуется на скрытом слое и сразу снимается с него — на экран слой не попадает
    let layer = LayerId::new(Order::Background, Id::new(("icon-render", name)));
    let rect = Rect::from_min_size(Pos2::ZERO, Vec2::splat(SIZE));
    let painter = egui::Painter::new(ctx.clone(), layer, rect);
    crate::sai_ui::glyph(&painter, rect.shrink(1.0), Color32::WHITE, name);
    let shapes: Vec<ClippedShape> = ctx.graphics_mut(|g| {
        let list = g.entry(layer);
        let v = list.all_entries().cloned().collect();
        *list = Default::default();
        v
    });
    let font_size = ctx.fonts(|f| f.font_image_size());
    let prims = Tessellator::new(SCALE, TessellationOptions::default(), font_size, Vec::new()).tessellate_shapes(shapes);
    let n = (SIZE * SCALE) as usize;
    let mut px = vec![[0f32; 4]; n * n];
    for p in prims {
        let Primitive::Mesh(mesh) = p.primitive else { continue };
        for tri in mesh.indices.as_chunks::<3>().0 {
            let v = [0, 1, 2].map(|i| mesh.vertices[tri[i] as usize]);
            fill_triangle(&mut px, n, v.map(|v| (v.pos.x * SCALE, v.pos.y * SCALE)), v.map(|v| v.color));
        }
    }
    let rgba: Vec<u8> = px.iter().flat_map(|c| c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)).collect();
    ColorImage::from_rgba_premultiplied([n, n], &rgba)
}

/// Треугольник с цветами вершин (предумноженные) — поверх того, что уже есть.
fn fill_triangle(px: &mut [[f32; 4]], n: usize, p: [(f32, f32); 3], c: [Color32; 3]) {
    let area = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
    if area.abs() < 1e-6 {
        return;
    }
    let (x0, x1) = (p.iter().map(|q| q.0).fold(f32::MAX, f32::min).floor().max(0.0) as usize, p.iter().map(|q| q.0).fold(f32::MIN, f32::max).ceil().min(n as f32) as usize);
    let (y0, y1) = (p.iter().map(|q| q.1).fold(f32::MAX, f32::min).floor().max(0.0) as usize, p.iter().map(|q| q.1).fold(f32::MIN, f32::max).ceil().min(n as f32) as usize);
    let col = c.map(|c| [c.r(), c.g(), c.b(), c.a()].map(|v| v as f32 / 255.0));
    for y in y0..y1 {
        for x in x0..x1 {
            let (sx, sy) = (x as f32 + 0.5, y as f32 + 0.5);
            let w0 = ((p[1].0 - sx) * (p[2].1 - sy) - (p[2].0 - sx) * (p[1].1 - sy)) / area;
            let w1 = ((p[2].0 - sx) * (p[0].1 - sy) - (p[0].0 - sx) * (p[2].1 - sy)) / area;
            let w2 = 1.0 - w0 - w1;
            if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                continue;
            }
            let s: [f32; 4] = std::array::from_fn(|i| col[0][i] * w0 + col[1][i] * w1 + col[2][i] * w2);
            let d = &mut px[y * n + x];
            for i in 0..4 {
                d[i] = s[i] + d[i] * (1.0 - s[3]);
            }
        }
    }
}

/// Подменю со значком. (`menu_image_text_button` egui внутри меню значок не рисует — поэтому
/// перед названием оставляется место, и значок рисуется поверх кнопки.)
pub fn submenu<R>(ui: &mut egui::Ui, icon: &'static str, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<Option<R>> {
    let mut job = egui::text::LayoutJob::default();
    let format = egui::TextFormat { font_id: egui::TextStyle::Button.resolve(ui.style()), color: ui.visuals().text_color(), ..Default::default() };
    job.append(title, SIZE + 4.0, format);
    let r = ui.menu_button(job, add);
    let rect = r.response.rect;
    let at = Rect::from_min_size(Pos2::new(rect.left() + ui.spacing().button_padding.x, rect.center().y - SIZE / 2.0), Vec2::splat(SIZE));
    image(ui.ctx(), icon).paint_at(ui, at);
    r
}
