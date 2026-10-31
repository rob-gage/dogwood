// Copyright Rob Gage 2026

use super::{
    MaterialAmountColor,
    MaterialTag,
};

/// Chemical properties shared by every material form
pub struct MaterialPropertiesChemical {
    /// Tags reactions can use to select this material, besides its name, and read values of
    pub tags: Vec<MaterialTag>,
    /// Visual-only color and radiance transformations selected by the amount in a cell or fluid
    /// particle; unused by gases
    pub amount_colors: Vec<MaterialAmountColor>,
}

impl Default for MaterialPropertiesChemical {

    /// Returns default chemical properties
    fn default() -> Self {
        Self { tags: Vec::new(), amount_colors: Vec::new() }
    }

}
