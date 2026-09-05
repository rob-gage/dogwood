// Copyright Rob Gage 2026

use super::MaterialGraphicsProperties;

/// Graphics properties for every material form
pub struct MaterialGraphics {
    /// The graphics properties for static cellular materials
    pub cellular_statics: Vec<MaterialGraphicsProperties>,
    /// The graphics properties for dynamic cellular materials
    pub cellular_dynamics: Vec<MaterialGraphicsProperties>,
    /// The graphics properties for fluid materials
    pub fluids: Vec<MaterialGraphicsProperties>,
}

impl MaterialGraphics {

    /// Creates graphics properties for every material form
    pub const fn new(
        cellular_statics: Vec<MaterialGraphicsProperties>,
        cellular_dynamics: Vec<MaterialGraphicsProperties>,
        fluids: Vec<MaterialGraphicsProperties>,
    ) -> Self { Self { cellular_statics, cellular_dynamics, fluids } }

}
