// Copyright Rob Gage 2026

use super::MaterialForm;

/// Identifies a `Material`
#[derive(Copy, Clone)]
pub struct MaterialIdentifier(u32);

impl MaterialIdentifier {

    /// Used to represent empty cells that contain no material
    pub const NULL: MaterialIdentifier = MaterialIdentifier(0);

    /// The 2-bit tag used to identify `CellularStatic` `MaterialIdentifier`s
    const CELLULAR_STATIC_TAG: u32 = 0b000000_01;

    /// The 2-bit tag used to identify `CellularDynamic` `MaterialIdentifier`s
    const CELLULAR_DYNAMIC_TAG: u32 = 0b000000_10;

    /// The 2-bit tag used to identify `Fluid` `MaterialIdentifier`s
    const FLUID_TAG: u32 = 0b000000_11;

    /// Creates a new `MaterialIdentifier` from a `MaterialForm` and an index
    pub const fn new(form: MaterialForm, index: u32) -> Self {
        assert!(index == index & 0b00111111_11111111_11111111_11111111);
        let tag: u32 = match form {
            MaterialForm::CellularStatic    => Self::CELLULAR_STATIC_TAG,
            MaterialForm::CellularDynamic   => Self::CELLULAR_DYNAMIC_TAG,
            MaterialForm::Fluid             => Self::FLUID_TAG,
        };
        Self(tag << 30 | index)
    }

    /// Returns the `MaterialForm` of the `Material` represented by this `MaterialIdentifier`
    pub const fn form(self) -> MaterialForm {
        match self.form_checked() { Some(form) => form, None => unreachable!(), }
    }

    /// Returns the `MaterialForm`, or `None` for an invalid identifier
    pub const fn form_checked(self) -> Option<MaterialForm> {
        match self.0 >> 30 {
            Self::CELLULAR_STATIC_TAG => Some(MaterialForm::CellularStatic),
            Self::CELLULAR_DYNAMIC_TAG => Some(MaterialForm::CellularDynamic),
            Self::FLUID_TAG => Some(MaterialForm::Fluid),
            _ => None,
        }
    }

    /// Returns the form-specific index of the `Material` represented by this `MaterialIdentifier`
    pub const fn index(self) -> u32 { self.0 & 0b00111111_11111111_11111111_11111111 }

    /// Creates a `MaterialIdentifier` from a `u32`
    pub const fn from_u32(u32: u32) -> MaterialIdentifier { Self(u32) }

    /// Returns a `MaterialIdentifier` as a `u32`
    pub const fn as_u32(self) -> u32 { self.0 }

}
