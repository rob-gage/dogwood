// Copyright Rob Gage 2026

use engine_graphics::world::materials::MaterialGraphicsCellularDynamic;
use super::MaterialPropertiesThermal;

/// Properties for a cellular dynamic material
pub struct MaterialPropertiesCellularDynamic {
    /// Albedo sampled at each cell to give the cells edits and generated chunks create the color
    /// they keep, unless they give another
    pub edit_color: MaterialGraphicsCellularDynamic,
    /// Mass of one cell, in the units of actor collision masses
    pub mass: f32,
    /// Coulomb friction coefficient, the tangent of the steepest slope a resting cell holds
    pub friction: f32,
    /// Fraction of a cell's speed kept, reversed, when it hits something it cannot enter
    pub restitution: f32,
    /// Thermal and phase-transition properties
    pub thermal: MaterialPropertiesThermal,
}
