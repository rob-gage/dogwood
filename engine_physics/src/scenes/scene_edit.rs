// Copyright Rob Gage 2026

use crate::{
    materials::MaterialIdentifier,
    tiles::{
        CellCoordinates,
        CellularAppearance,
    },
};

/// A requested material mutation of cells in a `Scene`
pub enum SceneEdit {
    /// Places a material in cells
    PlaceMaterial {
        material_identifier: MaterialIdentifier,
        appearance: CellularAppearance,
        cells: Vec<CellCoordinates>,
    },
    /// Erases material from cells
    Erase {
        cells: Vec<CellCoordinates>,
    },
}
