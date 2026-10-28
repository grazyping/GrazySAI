#![no_main]
//! Модели 3D из произвольных байтов: glTF/GLB, OBJ, FBX (ufbx), PMX, COLLADA — тем же путём, что
//! «Новый слой 3D» и конвертер. Ошибка — можно, падение, зависание, гигабайты — нет.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    for name in ["m.glb", "m.gltf", "m.obj", "m.fbx", "m.pmx", "m.dae"] {
        let files = vec![(name.to_string(), data.to_vec())];
        if let Ok(m) = paint_3d::model::load(&files) {
            // прочитанное должно и выводиться без падений
            let _ = paint_3d::export::glb(&m);
            let _ = paint_3d::export::obj(&m, "m");
        }
    }
});
