// Copyright Rob Gage 2026

/// A number of a reaction: a constant, or a parameter of its reactants' material tags
pub enum MaterialValue {
    /// The same number for every material the reaction selects
    Constant(f32),
    /// A `tag.parameter` name, read from the first reactant whose material's tag sets it
    Parameter(String),
}

impl MaterialValue {

    /// Creates a value read from the reactants' material tags by its `tag.parameter` name
    pub fn parameter(name: impl Into<String>) -> Self {
        Self::Parameter(name.into())
    }

}

impl From<f32> for MaterialValue {

    /// Creates a constant value
    fn from(value: f32) -> Self {
        Self::Constant(value)
    }

}
