// Copyright Rob Gage 2026

/// The mutually exclusive visualization used by the editor viewport
#[derive(Copy, Clone, Eq, PartialEq)]
pub(crate) enum EditorViewMode {
    /// Draws registered material appearances
    Normal,
    /// Draws a distinct color for each material form
    MaterialForm,
    /// Draws retained cellular pressure
    Pressure,
    /// Reserved for the future temperature field
    Temperature,
    /// Draws total Eulerian gas concentration
    Gas,
}

impl EditorViewMode {
    /// Returns the value consumed by the scene shader
    pub const fn shader_value(self) -> u32 {
        match self {
            Self::Normal => 0,
            Self::MaterialForm => 1,
            Self::Pressure => 2,
            Self::Temperature => 3,
            Self::Gas => 4,
        }
    }
}
