// Copyright Rob Gage 2026

use super::MaterialIdentifier;
use engine_graphics::MaterialAppearance;

/// A material
pub enum Material {
    /// A static cellular material
    CellularStatic {
        /// The name of this `Material`
        name: String,
        /// The graphics information for this `Material`
        graphics: MaterialAppearance,
        /// Load below which this material takes no structural damage
        pressure_ignore_threshold: f32,
        /// Structural integrity assigned to newly placed/generated cells
        default_integrity: f32,
        /// Optional dynamic material produced when this cell fractures
        debris_material: Option<MaterialIdentifier>,
        /// Fraction of failed matter that becomes debris
        debris_yield_rate: f32,
    },
    /// A dynamic cellular material
    CellularDynamic {
        /// The name of this `Material`
        name: String,
        /// The graphics information for this `Material`
        graphics: MaterialAppearance,
    },
    /// A fluid material
    Fluid {
        /// The name of this `Material`
        name: String,
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
    pub fn name(&self) -> &str {
        match self {
            Material::CellularStatic { name, .. } => name,
            Material::CellularDynamic { name, .. } => name,
            Material::Fluid { name, .. } => name,
        }
    }

}
