// Copyright Rob Gage 2026

/// Chemical properties shared by every material form
pub struct MaterialPropertiesChemical {
    /// Tags reactions can use to select this material, besides its name
    pub tags: Vec<String>,
}

impl Default for MaterialPropertiesChemical {

    /// Returns default chemical properties
    fn default() -> Self {
        Self { tags: Vec::new() }
    }

}
