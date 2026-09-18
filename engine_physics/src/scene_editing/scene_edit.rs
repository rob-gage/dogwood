// Copyright Rob Gage 2026

use super::SceneEditCellPlacement;
use crate::tiles::CellCoordinates;

/// A requested material mutation of cells in a `Scene`
pub enum SceneEdit {
    /// Places explicit state in cells
    PlaceCells {
        /// Explicit material and state for each target cell.
        cells: Vec<SceneEditCellPlacement>,
    },
    /// Creates one authoritative body-local rigid cellular body.
    PlaceRigidBody {
        /// Body-local material and state for each rigid cell.
        cells: Vec<SceneEditCellPlacement>,
    },
    /// Erases material from cells
    Erase {
        /// World cells whose material representation is erased.
        cells: Vec<CellCoordinates>,
    },
    /// Destroys every representation occupying the requested world cells.
    DestroyCells {
        /// World cells whose material representations are destroyed.
        cells: Vec<CellCoordinates>,
    },
    /// Adds a signed temperature delta to all representations at these cells.
    Thermal {
        /// World cells receiving the temperature delta.
        cells: Vec<CellCoordinates>,
        /// Signed temperature change applied to each representation.
        delta_temperature: f32,
    },
}
