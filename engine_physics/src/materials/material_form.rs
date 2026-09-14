// Copyright Rob Gage 2026

/// The form of a `Material`
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum MaterialForm {
    /// A static cellular `Material`
    CellularStatic,
    /// A dynamic cellular `Material`
    CellularDynamic,
    /// A fluid `Material`
    Fluid,
    /// A gaseous species transported through the shared gas mixture
    Gas,
}
