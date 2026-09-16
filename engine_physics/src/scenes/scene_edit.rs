// Copyright Rob Gage 2026

use super::SceneEditCellPlacement;
use crate::tiles::CellCoordinates;

/// A requested material mutation of cells in a `Scene`
pub enum SceneEdit {
    /// Places explicit state in cells
    PlaceCells { cells: Vec<SceneEditCellPlacement> },
    /// Creates one authoritative body-local rigid cellular body.
    PlaceRigidBody { cells: Vec<SceneEditCellPlacement> },
    /// Erases material from cells
    Erase { cells: Vec<CellCoordinates> },
    /// Destroys every representation occupying the requested world cells.
    DestroyCells { cells: Vec<CellCoordinates> },
}
