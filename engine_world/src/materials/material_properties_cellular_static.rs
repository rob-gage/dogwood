// Copyright Rob Gage 2026

use engine_graphics::world::materials::MaterialGraphicsCellularStatic;
use super::{MaterialPropertiesChemical, MaterialPropertiesThermal};

/// Properties for a cellular static material
pub struct MaterialPropertiesCellularStatic {
    /// Albedo sampled at each cell to give the cells edits and generated chunks create the color
    /// they keep, unless they give another
    pub edit_color: MaterialGraphicsCellularStatic,
    /// Emission and occlusion sampled at each cell using its own repeating sprite frame
    pub edit_radiance: MaterialGraphicsCellularStatic,
    /// Multiplier applied to the linear RGB emission in `edit_radiance`
    pub edit_radiance_intensity: f32,
    /// Mass of one cell, in the units of actor collision masses
    pub mass: f32,
    /// Coulomb friction coefficient of rigid bodies made of this material
    pub friction: f32,
    /// Fraction of speed kept, reversed, when rigid bodies made of this material collide
    pub restitution: f32,
    /// Load in atmospheres the material bears indefinitely: pressure or scripted damage at or below
    /// it never damages its cells, and infinity makes them unbreakable
    pub damage_threshold: f32,
    /// Amount of a cell, in whole cells, lost per second for each atmosphere of load beyond the
    /// threshold; a cell breaks when its amount runs out
    pub damage_coefficient: f32,
    /// Fraction from zero to one of the pressure passing through each cell that the material
    /// absorbs, which damps pressure waves
    pub absorption: f32,
    /// Name of the cellular dynamic material each cell becomes when it breaks off, keeping its
    /// color, or `None` to vanish
    pub debris: Option<String>,
    /// Probability from zero to one that a breaking cell becomes its debris material
    pub debris_chance: f32,
    /// Thermal and phase-transition properties
    pub thermal: MaterialPropertiesThermal,
    /// Chemical reaction and amount-dependent visual properties
    pub chemical: MaterialPropertiesChemical,
}
