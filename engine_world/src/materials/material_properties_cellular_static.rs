// Copyright Rob Gage 2026

use engine_graphics::world::materials::MaterialGraphicsCellularStatic;
use super::MaterialPropertiesThermal;

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
    /// Pressure difference in atmospheres the material holds indefinitely: loads below it never
    /// damage its cells, and infinity makes them unbreakable
    pub strength: f32,
    /// Fraction from zero to one of the pressure passing through each cell that the material
    /// absorbs, which damps pressure waves and, above its strength, damages the cell
    pub absorption: f32,
    /// Name of the cellular dynamic material each cell becomes when it breaks off, keeping its
    /// color, or `None` to vanish
    pub debris: Option<String>,
    /// Thermal and phase-transition properties
    pub thermal: MaterialPropertiesThermal,
}
