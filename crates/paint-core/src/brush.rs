//! Параметры кисти и форма одного отпечатка.

#[derive(Clone, Copy, Debug)]
pub struct BrushParams {
    /// Диаметр в пикселях при полном давлении.
    pub size: f32,
    /// Доля диаметра при нулевом давлении (0..=1).
    pub min_size: f32,
    /// Плотность краски при полном давлении (0..=1).
    pub density: f32,
    /// Доля плотности при нулевом давлении (0..=1).
    pub min_density: f32,
    /// Жёсткость края: 1 — резкий край, 0 — самый мягкий. Ступени образца — [`HARDNESS_STEPS`].
    pub hardness: f32,
    /// Шаг между отпечатками в долях диаметра; 0 — по правилу образца (зависит от жёсткости).
    pub spacing: f32,
    pub size_by_pressure: bool,
    pub density_by_pressure: bool,
    /// Ластик: штрих не красит, а стирает.
    pub erase: bool,
    /// Штрих слоя контура: отпечатки строго с шагом образца, без поправки «не реже 0,85 px», и без
    /// отдельной ветки самой жёсткой ступени — её образец включает только у обычных инструментов
    /// (проверено по `пинг5.sai2`: на концах толстых штрихов видны отдельные отпечатки).
    pub linework: bool,
    /// Sharpness из раздела Miscellaneous, 0..=1 (у карандаша по умолчанию 0): от неё зависит
    /// ширина края тонкой кисти.
    pub sharpness: f32,
    /// «Ver1 Prs Spec.»: нажим на плотность по правилу версии 1 (`DabInput::ver1`).
    pub ver1_density: bool,
    /// «Amplify Density», 0..=1 (`DabInput::amplify`).
    pub amplify: f32,
    /// Смешивание с холстом (Кисть, Акварель, Маркер); `None` — краска не смешивается.
    pub mix: Option<crate::mix::MixParams>,
}

impl Default for BrushParams {
    fn default() -> Self {
        Self {
            size: 12.0,
            min_size: 0.0,
            density: 1.0,
            min_density: 0.0,
            hardness: 1.0,
            spacing: 0.0,
            size_by_pressure: true,
            density_by_pressure: false,
            erase: false,
            linework: false,
            sharpness: 0.0,
            ver1_density: false,
            amplify: 0.0,
            mix: None,
        }
    }
}

impl BrushParams {
    pub fn diameter_at(&self, pressure: f32) -> f32 {
        if !self.size_by_pressure {
            return self.size;
        }
        let p = pressure.clamp(0.0, 1.0);
        self.size * (self.min_size + (1.0 - self.min_size) * p)
    }

    pub fn density_at(&self, pressure: f32) -> f32 {
        if !self.density_by_pressure {
            return self.density;
        }
        let p = pressure.clamp(0.0, 1.0);
        self.density * (self.min_density + (1.0 - self.min_density) * p)
    }
}

/// Пять ступеней жёсткости образца, от самой мягкой к самой жёсткой: значения `hardness`.
pub const HARDNESS_STEPS: [f32; 5] = [0.0, 0.5, 0.75, 0.9, 1.0];

