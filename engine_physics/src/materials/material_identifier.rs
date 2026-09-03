// Copyright Rob Gage 2026

/// Identifies a `Material`
#[derive(Copy, Clone)]
pub struct MaterialIdentifier(pub(super) u32);

impl MaterialIdentifier {

    /// Used to represent empty cells that contain no material
    pub const NULL: MaterialIdentifier = MaterialIdentifier(0);

    pub(crate) const fn from_u32(value: u32) -> Self { Self(value) }

    pub(crate) const fn as_u32(self) -> u32 { self.0 }

}
