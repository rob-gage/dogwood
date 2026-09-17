// Copyright Rob Gage 2026

use super::MaterialForm;

/// Identifies a `Material`
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct MaterialIdentifier(u32);

impl MaterialIdentifier {
    /// Used to represent empty cells that contain no material
    pub const NULL: MaterialIdentifier = MaterialIdentifier(0);

    /// The 2-bit tag used to identify `Gas` `MaterialIdentifier`s
    const GAS_TAG: u32 = 0b0000_0000;

    /// The 2-bit tag used to identify `CellularStatic` `MaterialIdentifier`s
    const CELLULAR_STATIC_TAG: u32 = 0b0000_0001;

    /// The 2-bit tag used to identify `CellularDynamic` `MaterialIdentifier`s
    const CELLULAR_DYNAMIC_TAG: u32 = 0b0000_0010;

    /// The 2-bit tag used to identify `Fluid` `MaterialIdentifier`s
    const FLUID_TAG: u32 = 0b0000_0011;

    /// Creates a new `MaterialIdentifier` from a `MaterialForm` and an index
    pub const fn new(form: MaterialForm, index: u32) -> Self {
        let (tag, encoded_index): (u32, u32) = match form {
            MaterialForm::Gas => {
                assert!(index < 0b00111111_11111111_11111111_11111111);
                (Self::GAS_TAG, index + 1)
            }
            MaterialForm::CellularStatic => (Self::CELLULAR_STATIC_TAG, index),
            MaterialForm::CellularDynamic => (Self::CELLULAR_DYNAMIC_TAG, index),
            MaterialForm::Fluid => (Self::FLUID_TAG, index),
        };
        assert!(encoded_index == encoded_index & 0b00111111_11111111_11111111_11111111);
        Self(tag << 30 | encoded_index)
    }

    /// Returns the `MaterialForm` of the `Material` represented by this `MaterialIdentifier`
    pub const fn form(self) -> MaterialForm {
        match self.form_checked() {
            Some(form) => form,
            None => unreachable!(),
        }
    }

    /// Returns the `MaterialForm`, or `None` for an invalid identifier
    pub const fn form_checked(self) -> Option<MaterialForm> {
        if self.0 == 0 {
            return None;
        }
        match self.0 >> 30 {
            Self::GAS_TAG => Some(MaterialForm::Gas),
            Self::CELLULAR_STATIC_TAG => Some(MaterialForm::CellularStatic),
            Self::CELLULAR_DYNAMIC_TAG => Some(MaterialForm::CellularDynamic),
            Self::FLUID_TAG => Some(MaterialForm::Fluid),
            _ => None,
        }
    }

    /// Returns the form-specific index of the `Material` represented by this `MaterialIdentifier`
    pub const fn index(self) -> u32 {
        let index: u32 = self.0 & 0b00111111_11111111_11111111_11111111;
        if self.0 >> 30 == Self::GAS_TAG {
            index.saturating_sub(1)
        } else {
            index
        }
    }

    /// Creates a `MaterialIdentifier` from a `u32`
    pub const fn from_u32(u32: u32) -> MaterialIdentifier {
        Self(u32)
    }

    /// Returns a `MaterialIdentifier` as a `u32`
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}
