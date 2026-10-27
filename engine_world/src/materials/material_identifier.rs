// Copyright Rob Gage 2026

use super::{
    MaterialError,
    MaterialForm
};

/// Identifies a `Material`
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct MaterialIdentifier(u32);

impl MaterialIdentifier {

    /// Represents an empty cell as cellular static index zero
    pub const EMPTY: MaterialIdentifier = MaterialIdentifier(0);

    /// The 2-bit tag used to identify `CellularStatic` `MaterialIdentifier`s
    const CELLULAR_STATIC_TAG: u32 = 0b0000_0000;

    /// The 2-bit tag used to identify `CellularDynamic` `MaterialIdentifier`s
    const CELLULAR_DYNAMIC_TAG: u32 = 0b0000_0001;

    /// The 2-bit tag used to identify `Fluid` `MaterialIdentifier`s
    const FLUID_TAG: u32 = 0b0000_0010;

    /// The 2-bit tag used to identify `Gas` `MaterialIdentifier`s
    const GAS_TAG: u32 = 0b0000_0011;

    /// Bitmask used to apply 2-bit tags
    const INDEX_MASK: u32 = 0b00111111_11111111_11111111_11111111;

    /// Creates a new `MaterialIdentifier` from a `MaterialForm` and an index
    pub const fn new(form: MaterialForm, index: u32) -> Result<Self, MaterialError> {
        if index > Self::INDEX_MASK { return Err(MaterialError::IdentifierIndexOutOfRange); }
        let tag: u32 = match form {
            MaterialForm::CellularStatic => Self::CELLULAR_STATIC_TAG,
            MaterialForm::CellularDynamic => Self::CELLULAR_DYNAMIC_TAG,
            MaterialForm::Fluid => Self::FLUID_TAG,
            MaterialForm::Gas => Self::GAS_TAG,
        };
        Ok(Self(tag << 30 | index))
    }

    /// Returns a `MaterialIdentifier` as a `u32`
    pub const fn as_u32(self) -> u32 { self.0 }

    /// Creates a `MaterialIdentifier` from a `u32`
    pub const fn from_u32(value: u32) -> Self { Self(value) }

    /// Returns the `MaterialForm` of the `Material` represented by this `MaterialIdentifier`
    pub const fn form(self) -> MaterialForm {
        match self.0 >> 30 {
            Self::CELLULAR_STATIC_TAG => MaterialForm::CellularStatic,
            Self::CELLULAR_DYNAMIC_TAG => MaterialForm::CellularDynamic,
            Self::FLUID_TAG => MaterialForm::Fluid,
            Self::GAS_TAG => MaterialForm::Gas,
            _ => unreachable!(),
        }
    }

    /// Returns the form-specific index of the `Material` represented by this `MaterialIdentifier`
    pub const fn index(self) -> u32 { self.0 & Self::INDEX_MASK }

}
