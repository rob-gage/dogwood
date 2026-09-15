// Copyright Rob Gage 2026

use crate::{
    materials::MaterialIdentifier,
    tiles::{CellCoordinates, CellularAppearance},
};

/// Explicit material state to place in one `Scene` cell
pub struct SceneEditCellPlacement {
    /// The world cell receiving this placement
    pub coordinates: CellCoordinates,
    /// The material to place in this cell
    pub material_identifier: MaterialIdentifier,
    /// The persistent cell appearance to place with the material
    pub appearance: CellularAppearance,
}
