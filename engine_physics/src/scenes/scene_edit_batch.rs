// Copyright Rob Gage 2026

use super::SceneEdit;
use crate::{
    materials::MaterialIdentifier,
    tiles::{
        CellCoordinates,
        CellularAppearance,
    },
};

/// Pending requested material mutations of a `Scene`
pub struct SceneEditBatch {
    edits: Vec<SceneEdit>,
}

impl SceneEditBatch {

    /// Creates an empty `SceneEditBatch`
    pub const fn new() -> Self { Self { edits: Vec::new() } }

    /// Adds a request to place a material in cells
    pub fn place_material(
        &mut self,
        material_identifier: MaterialIdentifier,
        appearance: CellularAppearance,
        cells: Vec<CellCoordinates>,
    ) {
        self.edits.push(SceneEdit::PlaceMaterial { material_identifier, appearance, cells });
    }

    /// Adds a request to erase material from cells
    pub fn erase(&mut self, cells: Vec<CellCoordinates>) {
        self.edits.push(SceneEdit::Erase { cells });
    }

    /// Drains the pending edits
    pub fn drain(&mut self) -> impl Iterator<Item = SceneEdit> + '_ {
        self.edits.drain(..)
    }

}
