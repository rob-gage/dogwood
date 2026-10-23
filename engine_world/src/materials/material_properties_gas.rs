// Copyright Rob Gage 2026

use engine_graphics::world::materials::MaterialGraphicsGas;

/// Properties for a gas material
pub struct MaterialPropertiesGas {
    /// Mass of one atmosphere of the gas in one cell, compared with the scene's ambient gas mass:
    /// lighter gases rise and heavier gases sink
    pub mass: f32,
    /// Fraction of the gas exchanged with each neighboring cell per substep, from zero to 0.1
    pub diffusivity: f32,
    /// Atmospheres of the gas a cell must hold before the gas spreads into a cell without it,
    /// from zero to one, so thinning clouds keep their edges
    pub cohesion: f32,
    /// Graphics for this gas material
    pub graphics: MaterialGraphicsGas,
}
