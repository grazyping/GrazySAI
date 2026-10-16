//! Миниатюры `jssf` для проверки глазами (env-gated): GRAZYSAI_JSSF_IN — файл `.sai2` (копия),
//! GRAZYSAI_JSSF_OUT — папка: `decoded.png` — его миниатюра нашим разбором; `ours.sai2` — тот же
//! рисунок, сохранённый GrazySAI (с нашей миниатюрой).
#[test]
fn jssf_to_png() {
    let (Some(src), Some(dir)) = (std::env::var_os("GRAZYSAI_JSSF_IN"), std::env::var_os("GRAZYSAI_JSSF_OUT")) else { return };
    let dir = std::path::PathBuf::from(dir);
    let data = std::fs::read(&src).unwrap();
    let n = u32::from_le_bytes(data[0x20..0x24].try_into().unwrap()) as usize;
    let mut offs: Vec<u64> = (0..n).map(|i| u64::from_le_bytes(data[64 + 16 * i + 8..64 + 16 * i + 16].try_into().unwrap())).collect();
    offs.sort();
    offs.push(data.len() as u64);
    for i in 0..n {
        if &data[64 + 16 * i..64 + 16 * i + 4] == b"thum" {
            let o = u64::from_le_bytes(data[64 + 16 * i + 8..64 + 16 * i + 16].try_into().unwrap());
            let end = offs[offs.iter().position(|&x| x == o).unwrap() + 1];
            let (w, h, rgb) = paint_formats::jssf::decode(&data[o as usize + 12..end as usize]).expect("разбор миниатюры SAI");
            let rgba: Vec<u8> = rgb.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect();
            paint_core::io::save_png(&dir.join("decoded.png"), w, h, &rgba).unwrap();
        }
    }
    let doc = paint_formats::sai2_doc::load(std::path::Path::new(&src)).unwrap();
    paint_formats::sai2_write::save(&doc.document, &dir.join("ours.sai2")).unwrap();
}
