//! Скорость марионетки на большом слое (круг 1400 точек): `cargo test --release -p paint-core --test puppet_speed -- --ignored --nocapture`.
#[test]
#[ignore]
fn puppet_speed() {
    use paint_core::puppet::*;
    let mut src = paint_core::tile::ColorMap::new();
    for y in 100..1600 {
        for x in 100..1600 {
            if ((x - 850) * (x - 850) + (y - 850) * (y - 850)) < 700 * 700 {
                src.set(x, y, paint_core::pixel::from_rgba8([200, 50, 50, 255]));
            }
        }
    }
    let mut p = Puppet::new(src);
    let mut a = Pin::new(PinKind::Position, [300.0, 850.0]);
    a.keys.insert(0, PinPose { pos: [250.0, 900.0], rot: 0.0, scale: 1.0 });
    p.pins = vec![a, Pin::new(PinKind::Position, [1400.0, 850.0]), Pin::new(PinKind::Position, [850.0, 300.0])];
    let t = std::time::Instant::now();
    let mesh = p.mesh();
    let t1 = t.elapsed();
    let pos = p.deform(&mesh, 0, None);
    let t2 = t.elapsed();
    let _img = p.render(&mesh, &pos);
    let t3 = t.elapsed();
    eprintln!("verts {} mesh {:?} deform {:?} render {:?}", mesh.rest.len(), t1, t2 - t1, t3 - t2);
}
#[test]
#[ignore]
fn puppet_speed_frames() {
    use paint_core::puppet::*;
    let mut src = paint_core::tile::ColorMap::new();
    for y in 100..1600 {
        for x in 100..1600 {
            if ((x - 850) * (x - 850) + (y - 850) * (y - 850)) < 700 * 700 {
                src.set(x, y, paint_core::pixel::from_rgba8([200, 50, 50, 255]));
            }
        }
    }
    let mut p = Puppet::new(src);
    let mut a = Pin::new(PinKind::Position, [300.0, 850.0]);
    a.keys.insert(0, PinPose { pos: [250.0, 900.0], rot: 0.0, scale: 1.0 });
    a.keys.insert(10, PinPose { pos: [200.0, 1000.0], rot: 0.0, scale: 1.0 });
    p.pins = vec![a, Pin::new(PinKind::Position, [1400.0, 850.0]), Pin::new(PinKind::Position, [850.0, 300.0])];
    for f in 0..4 {
        let t = std::time::Instant::now();
        let _ = p.frame(f);
        eprintln!("frame {f}: {:?}", t.elapsed());
    }
}
#[test]
#[ignore]
fn puppet_speed_parts() {
    use paint_core::puppet::*;
    let mut src = paint_core::tile::ColorMap::new();
    for y in 100..1600 {
        for x in 100..1600 {
            if ((x - 850) * (x - 850) + (y - 850) * (y - 850)) < 700 * 700 {
                src.set(x, y, paint_core::pixel::from_rgba8([200, 50, 50, 255]));
            }
        }
    }
    let mut p = Puppet::new(src);
    let mut a = Pin::new(PinKind::Position, [300.0, 850.0]);
    a.keys.insert(0, PinPose { pos: [250.0, 900.0], rot: 0.0, scale: 1.0 });
    p.pins = vec![a, Pin::new(PinKind::Position, [1400.0, 850.0]), Pin::new(PinKind::Position, [850.0, 300.0])];
    let mesh = p.mesh();
    let w = p.deform(&mesh, 0, None);
    let t = std::time::Instant::now();
    let pos = p.deform(&mesh, 0, Some(&w));
    eprintln!("warm deform {:?}", t.elapsed());
    let t = std::time::Instant::now();
    let _ = p.render(&mesh, &pos);
    eprintln!("slow render {:?}", t.elapsed());
}
