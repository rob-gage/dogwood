// Copyright Rob Gage 2026

use super::EditorApplication;
use engine::{Game, physics::tiles::CellCoordinates};

impl<G: Game> EditorApplication<G> {
    /// Returns the cell currently beneath the cursor in the rendered Scene viewport
    pub(super) fn hovered_cell(&self) -> Option<CellCoordinates> {
        self.application
            .scene_world_position(self.cursor_position?)
            .map(CellCoordinates::from_world_position)
    }
}
