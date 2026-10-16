use paint_core::vectorize::{vectorize, StrokePoint};
use paint_formats::linework::{append, empty, LineStyle};

#[test]
#[ignore]
fn linework_render_speed() {
    let mut raw = empty().raw;
    for k in 0..2 {
        let pts: Vec<StrokePoint> = (0..=500).map(|i| StrokePoint { at: [100.0 + i as f64 * 1.3, 200.0 + k as f64 * 300.0 + (i as f64 / 60.0).sin() * 40.0], pressure: 1.0 }).collect();
        let (nodes, closed) = vectorize(&pts, 1.0).unwrap();
        raw = append(&raw, &nodes, closed, &LineStyle { rgb: [20, 20, 20], size: 3.0, density: 1.0, hardness: 4 }).unwrap();
    }
    let shapes = paint_formats::sai2_vector::parse(&raw, true).unwrap();
    let t = std::time::Instant::now();
    let px = paint_formats::linework::render_shapes(&shapes);
    eprintln!("render_shapes {:.1} ms, tiles {}", t.elapsed().as_secs_f32() * 1e3, px.tiles.len());
}

#[test]
#[ignore]
fn linework_render_parts() {
    use paint_core::vector::{sample_path, Node};
    let pts: Vec<StrokePoint> = (0..=500).map(|i| StrokePoint { at: [100.0 + i as f64 * 1.3, 200.0 + (i as f64 / 60.0).sin() * 40.0], pressure: 1.0 }).collect();
    let (nodes, closed) = vectorize(&pts, 1.0).unwrap();
    let raw = append(&empty().raw, &nodes, closed, &LineStyle { rgb: [20, 20, 20], size: 3.0, density: 1.0, hardness: 4 }).unwrap();
    let shapes = paint_formats::sai2_vector::parse(&raw, true).unwrap();
    let p = &shapes[0].paths[0];
    let nodes: Vec<Node> = p.points.iter().map(|c| Node { point: c.point, handle_in: c.handle_in, handle_out: c.handle_out }).collect();
    let w: Vec<f64> = p.points.iter().map(|c| c.weight as f64).collect();
    let pps: Vec<Vec<(f64, f64)>> = p.points.iter().map(|c| c.pressure.iter().map(|q| (q.pos, q.weight as f64)).collect()).collect();
    let t = std::time::Instant::now();
    let s = sample_path(&nodes, p.closed, &w, &pps, 0.25);
    eprintln!("nodes {} sample_path {:.1} ms -> {} points", nodes.len(), t.elapsed().as_secs_f32() * 1e3, s.len());
    let params = paint_core::BrushParams { size: 3.0, linework: true, size_by_pressure: true, ..Default::default() };
    let t = std::time::Instant::now();
    let mut st = paint_core::Stroke::new(params);
    for (q, pr) in &s {
        st.add_point(paint_core::InputPoint { x: q[0] as f32, y: q[1] as f32, pressure: *pr as f32 });
    }
    eprintln!("add_point {:.1} ms", t.elapsed().as_secs_f32() * 1e3);
    let t = std::time::Instant::now();
    let mut l = paint_core::layer::Layer::new("");
    l.apply_stroke(&st, [0, 0, 0], &Default::default());
    eprintln!("apply {:.1} ms", t.elapsed().as_secs_f32() * 1e3);
}
