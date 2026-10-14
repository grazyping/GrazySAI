//! Режимы наложения слоёв. Формулы общепринятые (как в PSD и в стандарте W3C Compositing).
//! Все 28 режимов образца сверены с работающей программой (тест
//! `blend_modes_match_the_reference_program`). Фирменные Shade, Shine, Shade/Shine, Burn, Dodge и
//! Burn/Dodge — это Linear Burn, Linear Dodge, Linear Light, Color Burn, Color Dodge и Vivid Light,
//! у которых непрозрачность слоя действует как «заливка» в PSD: цвет слоя сначала смешивается с
//! нейтральным для режима цветом (белым, чёрным или серым 50%) и только потом накладывается.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    LinearDodge,
    LinearBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Subtract,
    Divide,
    /// «Сияние» образца: цвет слоя прибавляется к тому, что под ним.
    Shine,
    /// «Тень» образца: обратное «Сиянию» — вычитается недостающее до белого.
    Shade,
    /// Берётся целиком более тёмный из двух цветов.
    DarkerColor,
    /// Берётся целиком более светлый из двух цветов.
    LighterColor,
    /// «Тень/Сияние» образца: Linear Light с непрозрачностью как заливкой.
    ShadeShine,
    /// «Затемнение» образца: Color Burn с непрозрачностью как заливкой.
    Burn,
    /// «Осветление» образца: Color Dodge с непрозрачностью как заливкой.
    Dodge,
    /// «Затемнение/Осветление» образца: Vivid Light с непрозрачностью как заливкой.
    BurnDodge,
    VividLight,
    LinearLight,
    PinLight,
    HardMix,
}

impl BlendMode {
    pub const ALL: [BlendMode; 28] = [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Darken,
        BlendMode::Lighten,
        BlendMode::ColorDodge,
        BlendMode::ColorBurn,
        BlendMode::LinearDodge,
        BlendMode::LinearBurn,
        BlendMode::HardLight,
        BlendMode::SoftLight,
        BlendMode::Difference,
        BlendMode::Exclusion,
        BlendMode::Subtract,
        BlendMode::Divide,
        BlendMode::Shine,
        BlendMode::Shade,
        BlendMode::DarkerColor,
        BlendMode::LighterColor,
        BlendMode::ShadeShine,
        BlendMode::Burn,
        BlendMode::Dodge,
        BlendMode::BurnDodge,
        BlendMode::VividLight,
        BlendMode::LinearLight,
        BlendMode::PinLight,
        BlendMode::HardMix,
    ];

    /// Для режимов «с заливкой»: обычный режим, к которому он сводится, и нейтральный цвет слоя.
    fn fill_variant(self) -> Option<(BlendMode, f32)> {
        use BlendMode::*;
        match self {
            ShadeShine => Some((LinearLight, 0.5)),
            Burn => Some((ColorBurn, 1.0)),
            Dodge => Some((ColorDodge, 0.0)),
            BurnDodge => Some((VividLight, 0.5)),
            _ => None,
        }
    }
}

/// Функция смешивания одного канала: `b` — то, что снизу, `s` — слой сверху; оба 0..=1.
pub fn channel(mode: BlendMode, b: f32, s: f32) -> f32 {
    use BlendMode::*;
    match mode {
        Normal => s,
        Multiply => b * s,
        Screen => b + s - b * s,
        Overlay => channel(HardLight, s, b),
        Darken => b.min(s),
        Lighten => b.max(s),
        ColorDodge => {
            if b <= 0.0 {
                0.0
            } else if s >= 1.0 {
                1.0
            } else {
                (b / (1.0 - s)).min(1.0)
            }
        }
        ColorBurn => {
            if b >= 1.0 {
                1.0
            } else if s <= 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - b) / s).min(1.0)
            }
        }
        LinearDodge => (b + s).min(1.0),
        LinearBurn => (b + s - 1.0).max(0.0),
        HardLight => {
            if s <= 0.5 {
                b * 2.0 * s
            } else {
                let s2 = 2.0 * s - 1.0;
                b + s2 - b * s2
            }
        }
        SoftLight => {
            if s <= 0.5 {
                b - (1.0 - 2.0 * s) * b * (1.0 - b)
            } else {
                let d = if b <= 0.25 { ((16.0 * b - 12.0) * b + 4.0) * b } else { b.sqrt() };
                b + (2.0 * s - 1.0) * (d - b)
            }
        }
        Difference => (b - s).abs(),
        Exclusion => b + s - 2.0 * b * s,
        Subtract => (b - s).max(0.0),
        Divide => {
            if s <= 0.0 {
                1.0
            } else {
                (b / s).min(1.0)
            }
        }
        Shine => (b + s).min(1.0),
        Shade => (b + s - 1.0).max(0.0),
        VividLight | BurnDodge => {
            if s <= 0.5 {
                channel(ColorBurn, b, 2.0 * s)
            } else {
                channel(ColorDodge, b, 2.0 * s - 1.0)
            }
        }
        LinearLight | ShadeShine => (b + 2.0 * s - 1.0).clamp(0.0, 1.0),
        PinLight => {
            if s <= 0.5 {
                b.min(2.0 * s)
            } else {
                b.max(2.0 * s - 1.0)
            }
        }
        HardMix => {
            if b + s >= 1.0 {
                1.0
            } else {
                0.0
            }
        }
        Burn => channel(ColorBurn, b, s),
        Dodge => channel(ColorDodge, b, s),
        // Для этих двух выбор делается по цвету целиком, см. `composite`.
        DarkerColor => b.min(s),
        LighterColor => b.max(s),
    }
}

/// Накладывает `src` на `dst` (оба предумноженные RGBA 0..=1) с непрозрачностью слоя `opacity`.
pub fn composite(mode: BlendMode, dst: [f32; 4], src: [f32; 4], opacity: f32) -> [f32; 4] {
    let sa = src[3] * opacity;
    if sa <= 0.0 {
        return dst;
    }
    let da = dst[3];
    let out_a = sa + da - sa * da;
    let mut out = [0.0, 0.0, 0.0, out_a];
    match mode {
        // Прибавление и вычитание идут по предумноженному цвету: чем прозрачнее слой, тем слабее.
        BlendMode::Shine => {
            for i in 0..3 {
                out[i] = (dst[i] + src[i] * opacity).min(out_a);
            }
            return out;
        }
        BlendMode::Shade if da > 0.0 => {
            for i in 0..3 {
                let lack = sa - src[i] * opacity; // сколько слою не хватает до белого
                out[i] = (dst[i] - lack * da).max(0.0) + src[i] * opacity * (1.0 - da);
            }
            return out;
        }
        m if m.fill_variant().is_some() && da > 0.0 => {
            // Слой сначала смешивается с нейтральным цветом, потом накладывается без ослабления.
            let (base, neutral) = m.fill_variant().unwrap_or((BlendMode::Normal, 0.0));
            for i in 0..3 {
                let s = neutral + (src[i] / src[3] - neutral) * sa;
                let b = dst[i] / da;
                out[i] = sa * (1.0 - da) * (src[i] / src[3]) + da * channel(base, b, s);
            }
            return out;
        }
        BlendMode::DarkerColor | BlendMode::LighterColor if da > 0.0 => {
            let lum = |c: [f32; 3]| 0.30 * c[0] + 0.59 * c[1] + 0.11 * c[2];
            let b = [dst[0] / da, dst[1] / da, dst[2] / da];
            let s = [src[0] / src[3], src[1] / src[3], src[2] / src[3]];
            let take_src = (lum(s) < lum(b)) == (mode == BlendMode::DarkerColor);
            let mixed = if take_src { s } else { b };
            for i in 0..3 {
                out[i] = sa * (1.0 - da) * s[i] + da * (1.0 - sa) * b[i] + sa * da * mixed[i];
            }
            return out;
        }
        _ => {}
    }
    if mode == BlendMode::Normal || da <= 0.0 {
        for i in 0..3 {
            out[i] = src[i] * opacity + dst[i] * (1.0 - sa);
        }
        return out;
    }
    for i in 0..3 {
        let s = src[i] / src[3]; // непредумноженный цвет слоя
        let b = dst[i] / da;
        let mixed = channel(mode, b, s);
        out[i] = sa * (1.0 - da) * s + da * (1.0 - sa) * b + sa * da * mixed;
    }
    out
}
