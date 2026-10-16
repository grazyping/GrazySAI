//! История отмены на диске: при крошечном бюджете памяти все шаги уходят во временные файлы,
//! а отмена и повтор возвращают картинку точно.

use paint_core::{BrushParams, Document, InputPoint, LiveStroke, Stroke};

#[test]
fn undo_from_disk_restores_every_step() {
    std::env::set_var("PAINT_HISTORY_RAM", "1");
    let dir = std::env::temp_dir().join("SOsai-history").join(std::process::id().to_string());
    let mut doc = Document::new(300, 200);
    let mut shots = vec![doc.flatten()];
    for i in 0..10 {
        let params = BrushParams { size: 20.0 + i as f32 * 3.0, ..Default::default() };
        let mut live = LiveStroke::new(vec![0], Stroke::new(params), [i * 25, 200 - i * 20, 90]);
        for x in (10..290).step_by(5) {
            live.stroke.add_point(InputPoint { x: x as f32, y: 20.0 + i as f32 * 17.0 + (x as f32 * 0.05).sin() * 9.0, pressure: 1.0 });
            doc.live_update(&mut live);
        }
        doc.live_finish(live);
        shots.push(doc.flatten());
    }
    let files = std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0);
    assert!(files >= 9, "на диск ушло шагов: {files}");
    for i in (0..10).rev() {
        doc.undo().unwrap();
        assert!(doc.flatten() == shots[i], "отмена до шага {i} разошлась");
    }
    for (i, shot) in shots.iter().enumerate().skip(1) {
        doc.redo().unwrap();
        assert!(doc.flatten() == *shot, "повтор шага {i} разошёлся");
    }
    drop(doc);
    let left = std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0);
    assert_eq!(left, 0, "файлы истории остались");
}
