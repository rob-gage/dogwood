// Copyright Rob Gage 2026

use engine_graphics::world::materials::MaterialGraphicsFluid;

/// Properties for a fluid material
pub struct MaterialPropertiesFluid {
    /// Mass of one fluid cell in cellular mass units; must be finite and positive
    pub density: f32,
    /// XSPH velocity smoothing fraction in the inclusive range from zero to one
    pub viscosity: f32,
    /// Surface cohesion strength; must be finite and nonnegative
    pub cohesion: f32,
    /// PBF artificial-pressure strength; must be finite and nonnegative
    pub artificial_pressure: f32,
    /// Physical fluid viscosity for body drag and swimming; must be finite and nonnegative
    pub drag: f32,
    /// Coulomb friction for direct fluid-cell contacts; must be finite and nonnegative
    pub friction: f32,
    /// Restitution for direct fluid-cell contacts, in the inclusive range from zero to one
    pub restitution: f32,
    /// Graphics for this fluid material
    pub graphics: MaterialGraphicsFluid,
}
