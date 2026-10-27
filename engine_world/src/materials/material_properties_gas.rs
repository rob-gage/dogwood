// Copyright Rob Gage 2026

use engine_graphics::world::materials::MaterialGraphicsGas;
use super::MaterialPropertiesThermal;

/// Properties for a gas material
pub struct MaterialPropertiesGas {
    /// Mass of one atmosphere of the gas in one cell, compared with the scene's ambient gas mass:
    /// lighter gases rise and heavier gases sink
    pub mass: f32,
    /// Fraction of the gas exchanged with each neighboring cell per tick, from zero to 0.4
    pub diffusivity: f32,
    /// Atmospheres of the gas a cell must hold before the gas spreads into a cell without it,
    /// from zero to one, so thinning clouds keep their edges
    pub cohesion: f32,
    /// Graphics for this gas material
    pub graphics: MaterialGraphicsGas,
    /// Thermal and phase-transition properties; heat capacity is per atmosphere
    pub thermal: MaterialPropertiesThermal,
}
