//! «Пиксельное перо» против живой SAI 2026-07-02b (docs/41): точка щелчком на размерах 1…20 —
//! круг точек; размер округляется до целого (2,3 → 2, 2,6 → 3, 3,5 → 4).

use paint_brush::ToolKind;
use paint_core::{Document, InputPoint};

const SAI: [(f32, &[&str]); 8] = [
    (1.0, &["#"]),
    (1.5, &["##", "##"]),
    (2.3, &["##", "##"]),
    (2.6, &["###", "###", "###"]),
    (3.5, &[".##.", "####", "####", ".##."]),
    (5.0, &[".###.", "#####", "#####", "#####", ".###."]),
    (7.0, &["..###..", ".#####.", "#######", "#######", "#######", ".#####.", "..###.."]),
    (8.0, &["..####..", ".######.", "########", "########", "########", "########", ".######.", "..####.."]),
];

fn dot(size: f32) -> Vec<String> {
    let mut b = ToolKind::PixelPen.new_brush();
    b.params.size = size;
    let mut doc = Document::new(40, 40);
    let mut st = b.stroke(None);
    st.add_point(InputPoint { x: 20.3, y: 20.4, pressure: 0.2 });
    doc.commit_stroke(0, &st, [0, 0, 0]);
    let on = |x: i32, y: i32| paint_core::pixel::to_rgba8(doc.layers[0].pixels.get(x, y))[3] > 0;
    let (mut x0, mut y0, mut x1, mut y1) = (40, 40, -1, -1);
    for y in 0..40 {
        for x in 0..40 {
            if on(x, y) {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
        }
    }
    (y0..=y1).map(|y| (x0..=x1).map(|x| if on(x, y) { '#' } else { '.' }).collect()).collect()
}

#[test]
fn pixel_pen_dots_match_sai() {
    for (size, want) in SAI {
        assert_eq!(dot(size), want.iter().map(|s| s.to_string()).collect::<Vec<_>>(), "размер {size}");
    }
    // крупнее — по числу точек: 9 → 69, 10 → 80, 12 → 112, 14 → 156, 16 → 208, 20 → 316
    for (size, n) in [(9.0, 69), (10.0, 80), (12.0, 112), (14.0, 156), (16.0, 208), (20.0, 316)] {
        let got: usize = dot(size).iter().map(|r| r.matches('#').count()).sum();
        assert_eq!(got, n, "размер {size}");
    }
}
