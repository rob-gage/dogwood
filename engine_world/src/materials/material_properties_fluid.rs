// Copyright Rob Gage 2026

use super::MaterialPropertiesThermal;

/// Properties for a fluid material
pub struct MaterialPropertiesFluid {
    /// Mass of one fluid cell in cellular mass units; must be finite and positive
    pub density: f32,
    /// Nonlinear XSPH viscosity control in the inclusive range from zero to one; one is the
    /// strongest relaxation toward the neighbor-weighted mean velocity
    pub viscosity: f32,
    /// Surface cohesion strength; must be finite and nonnegative
    pub cohesion: f32,
    /// PBF artificial-pressure strength; must be finite and nonnegative
    pub artificial_pressure: f32,
    /// Physical fluid viscosity for body drag and swimming; must be finite and nonnegative
    pub drag: f32,
    /// Coulomb friction for dynamic-cell contacts and fluid-solid tangential damping; finite and
    /// nonnegative, with solid-boundary damping clamped to one
    pub friction: f32,
    /// Restitution for direct fluid-cell contacts, in the inclusive range from zero to one
    pub restitution: f32,
    /// Packed sRGB RGBA color of the particles edits create, unless they give another; alpha is
    /// the opacity of a full cell of the fluid
    pub edit_color: u32,
    /// Packed linear HDR emission and occlusion of the particles edits create, unless they give
    /// another
    pub edit_radiance: u32,
    /// Thermal and phase-transition properties
    pub thermal: MaterialPropertiesThermal,
}
