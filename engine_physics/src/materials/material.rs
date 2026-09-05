// Copyright Rob Gage 2026

use super::MaterialAppearance;

/// A material
pub enum Material {
    /// A static cellular material
    CellularStatic {
        /// The name of this `Material`
        name: &'static str,
        /// The appearance of this `Material`
        appearance: MaterialAppearance,
    },
    /// A dynamic cellular material
    CellularDynamic {
        /// The name of this `Material`
        name: &'static str,
        /// The appearance of this `Material`
        appearance: MaterialAppearance,
    },
    /// A fluid material
    Fluid {
        /// The name of this `Material`
        name: &'static str,
        /// The appearance of this `Material`
        appearance: MaterialAppearance,
    }
}

impl Material {

    /// Returns the `MaterialAppearance` of this `Material`
    pub const fn appearance(&self) -> &MaterialAppearance {
        match self {
            Self::CellularStatic { appearance, .. } => appearance,
            Self::CellularDynamic { appearance, .. } => appearance,
            Self::Fluid { appearance, .. } => appearance,
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