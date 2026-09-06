// Copyright Rob Gage 2026

use engine_graphics::MaterialAppearance;

/// A material
pub enum Material {
    /// A static cellular material
    CellularStatic {
        /// The name of this `Material`
        name: &'static str,
        /// The graphics information for this `Material`
        graphics: MaterialAppearance,
    },
    /// A dynamic cellular material
    CellularDynamic {
        /// The name of this `Material`
        name: &'static str,
        /// The graphics information for this `Material`
        graphics: MaterialAppearance,
    },
    /// A fluid material
    Fluid {
        /// The name of this `Material`
        name: &'static str,
        /// The graphics information for this `Material`
        graphics: MaterialAppearance,
    }
}

impl Material {

    /// Returns the graphics information for this `Material`
    pub const fn appearance(&self) -> &MaterialAppearance {
        match self {
            Self::CellularStatic { graphics, .. } => graphics,
            Self::CellularDynamic { graphics, .. } => graphics,
            Self::Fluid { graphics, .. } => graphics,
        }
    }

    /// Returns the name of this `Material`
    pub const fn name(&self) -> &'static str {
        match self {
            Material::CellularStatic { name, .. } => name,
            Material::CellularDynamic { name, .. } => name,
            Material::Fluid { name, .. } => name,
        }
    }

}
