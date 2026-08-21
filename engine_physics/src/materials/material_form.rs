// Copyright Rob Gage 2026

/// The form of a `Material`
pub enum MaterialForm {
    /// A static `Material`
    Static,
    /// A cellular dynamic `Material`
    Cellular,
    /// A fluid dynamic `Material`
    Fluid,
}