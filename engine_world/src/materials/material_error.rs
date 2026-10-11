// Copyright Rob Gage 2026

/// An error returned while registering or validating a `Material`
#[derive(Debug)]
pub enum MaterialError {
    /// The form-specific identifier capacity has been exceeded in the `MaterialRegistry`
    CapacityExceeded,
    /// A material name cannot be empty
    EmptyName,
    /// The material index cannot be represented by a `MaterialIdentifier`
    IdentifierIndexOutOfRange,
    /// Graphics resources could not be compiled for a material table
    Compilation(String),
}
