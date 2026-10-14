// Copyright Rob Gage 2026

use crate::{
    materials::MaterialIdentifier,
    scenes::cellular_tiles::CellularTileCoordinates,
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
    /// A material of the edit has a form that edits cannot place
    MaterialFormUnsupported {
        /// Identifier of the unsupported material
        material_identifier: MaterialIdentifier,
    },
    /// The edit would exceed the cellular tiles requested before the next stream
    CapacityReached,
}
