// Copyright Rob Gage 2026

use crate::{
    materials::MaterialIdentifier,
    scenes::cellular_tiles::CellularTileCoordinates,
};
use std::{
    error::Error,
    fmt::{
        Display,
        Formatter,
        Result,
    },
};

/// An error returned while requesting an `Edit`
#[derive(Clone, Debug)]
pub enum EditError {
    /// A cellular tile of the edit is outside the resident active region
    CellularTileInactive {
        /// Coordinates of the inactive cellular tile
        coordinates: CellularTileCoordinates,
    },
    /// A material of the edit is not registered in the scene's material table
    MaterialUnregistered {
        /// Identifier of the unregistered material
        material_identifier: MaterialIdentifier,
    },
    /// A pressure is negative or not finite
    InvalidPressure,
    /// The edit would exceed a segment, queued tile, or fluid-particle capacity
    CapacityReached,
}

impl Display for EditError {

    /// Describes the error
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
        match self {
            Self::CellularTileInactive { coordinates } =>
                write!(formatter, "Cellular tile {coordinates:?} is not active"),
            Self::MaterialUnregistered { material_identifier } =>
                write!(formatter, "Material {material_identifier:?} is not registered"),
            Self::InvalidPressure => write!(formatter, "Pressure must be finite and nonnegative"),
            Self::CapacityReached => write!(formatter, "Edit capacity is reached"),
        }
    }

}

impl Error for EditError {}
