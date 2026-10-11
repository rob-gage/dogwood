// Copyright Rob Gage 2026

use super::{
    MaterialForm,
    MaterialPropertiesCellularDynamic,
    MaterialPropertiesCellularStatic,
    MaterialPropertiesFluid,
    MaterialPropertiesGas,
};

/// A material and its form-specific properties
pub enum Material {
    /// A cellular static material
    CellularStatic {
        /// The name of this material
        name: String,
        /// The properties of this material
        properties: MaterialPropertiesCellularStatic,
    },
    /// A cellular dynamic material
    CellularDynamic {
        /// The name of this material
        name: String,
        /// The properties of this material
        properties: MaterialPropertiesCellularDynamic,
    },
    /// A fluid material
    Fluid {
        /// The name of this material
        name: String,
        /// The properties of this material
        properties: MaterialPropertiesFluid,
    },
    /// A gas material
    Gas {
        /// The name of this material
        name: String,
        /// The properties of this material
        properties: MaterialPropertiesGas,
    },
}

impl Material {

    /// Returns the name of this material
    pub fn name(&self) -> &str {
        match self {
            Self::CellularStatic { name, .. }
            | Self::CellularDynamic { name, .. }
            | Self::Fluid { name, .. }
            | Self::Gas { name, .. } => name,
        }
    }

    /// Returns the form of this material
    pub const fn form(&self) -> MaterialForm {
        match self {
            Self::CellularStatic { .. } => MaterialForm::CellularStatic,
            Self::CellularDynamic { .. } => MaterialForm::CellularDynamic,
            Self::Fluid { .. } => MaterialForm::Fluid,
            Self::Gas { .. } => MaterialForm::Gas,
        }
    }

}
