//! Векторные блоки `.sai2`: запись и чтение по разобранному формату (docs/11-linework-shapes.md).
//! Файла из образца пока нет — проверяется согласованность разбора и записи и раскладка байт.

use paint_formats::sai2_vector::*;

fn stroke() -> Shape {
    let cp = |id, x: f64, pointed| ControlPoint {
        id,
        point: [x, 10.0],
        handle_in: [x - 1.0, 10.0],
        handle_out: [x + 1.0, 10.0],
        weight: 0.5,
        second: 1.0,
        pointed,
        rest: [0; 3],
        pressure: Vec::new(),
    };
    let mut a = cp(1, 0.0, false);
    a.pressure = vec![
        PressurePoint { id: 2, pos: 0.75, weight: 0.2, second: 1.0, rest: [0; 4] },
        PressurePoint { id: 1, pos: 0.25, weight: 0.9, second: 1.0, rest: [0; 4] },
    ];
    Shape {
        id: 7,
        next: 0,
        style: Style::Linework {
            color: [1, 2, 3, 4, 5, 6, 7, 8],
            size: 4.0,
            density: 1.0,
            params: [0.0, 0.5, 0.0],
            hardness: 4,
            amplify: 0.0,
            flags: [true, false],
        },
        paths: vec![Path {
            id: 3,
            next_in_layer: 0,
            offset: [100.0, 200.0],
            curve: CurveType::Normal,
            closed: false,
            rest: [0; 2],
            points: vec![a, cp(2, 50.0, true)],
        }],
    }
}

#[test]
fn linework_block_round_trips() {
    let s = stroke();
    let bytes = write(std::slice::from_ref(&s), true);
    let back = parse(&bytes, true).unwrap();
    // Точки нажима при чтении упорядочены по положению — как у образца.
    let mut want = s;
    want.paths[0].points[0].pressure.reverse();
    assert_eq!(back, vec![want]);
}

#[test]
fn linework_block_layout_matches_the_format() {
    let bytes = write(&[stroke()], true);
    assert_eq!(&bytes[0..4], b"strk");
    let len = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    assert_eq!(bytes.len(), 8 + len + 4, "один объект и завершающий ноль");
    let b = &bytes[8..];
    // номер, следующий, затем свойства стиля
    assert_eq!(u32::from_le_bytes(b[0..4].try_into().unwrap()), 7);
    assert_eq!(&b[8..12], b"scol");
    assert_eq!(&b[24..28], b"inkd");
    assert_eq!(u32::from_le_bytes(b[28..32].try_into().unwrap()), 24);
    assert_eq!(&b[56..60], b"inko");
    // после ноля конца стиля (байты 72–75) — путь: номер, владелец, два «следующих», сдвиг, вид, замкнутость
    let p = &b[76..];
    assert_eq!(u32::from_le_bytes(p[0..4].try_into().unwrap()), 3);
    assert_eq!(u32::from_le_bytes(p[4..8].try_into().unwrap()), 7);
    assert_eq!(f64::from_le_bytes(p[16..24].try_into().unwrap()), 100.0);
    // узел: номер + 60 байт, флаг острого угла — байт 57 записи
    let cp2 = &p[36 + 64..];
    assert_eq!(u32::from_le_bytes(cp2[0..4].try_into().unwrap()), 2);
    assert_eq!(cp2[4 + 57], 1);
}

#[test]
fn shape_block_reads_fill_color_and_skips_unknown_properties() {
    let mut b = Vec::new();
    b.extend(5u32.to_le_bytes());
    b.extend(0u32.to_le_bytes());
    b.extend(b"zzzz");
    b.extend(3u32.to_le_bytes());
    b.extend([9, 9, 9]);
    b.extend(b"fcol");
    b.extend(8u32.to_le_bytes());
    b.extend([10, 0, 20, 0, 30, 0, 0, 64]);
    b.extend([0; 4]); // конец стиля
    b.extend([0; 4]); // путей нет
    let mut block = b"shap".to_vec();
    block.extend((b.len() as u32).to_le_bytes());
    block.extend(b);
    block.extend([0; 4]);
    let shapes = parse(&block, false).unwrap();
    assert_eq!(shapes.len(), 1);
    assert_eq!(shapes[0].style, Style::Fill { color: [10, 0, 20, 0, 30, 0, 0, 64] });
    assert!(parse(&block, true).is_err(), "у слоя контура объекты помечены `strk`");
}
