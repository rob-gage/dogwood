// Copyright Rob Gage 2026

/// Resolution used to calculate radiance cascade lighting
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LightingQuality {
    /// Uses one lighting texel for each four by four cells
    Low,
    /// Uses one lighting texel for each two by two cells
    Medium,
    /// Uses one lighting texel for each cell
    #[default]
    High,
}

impl LightingQuality {

    /// Returns the number of cells along each side of one lighting texel
    pub fn texel_size(self) -> u32 {
        match self {
            Self::Low => 4,
            Self::Medium => 2,
            Self::High => 1,
        }
    }

}
