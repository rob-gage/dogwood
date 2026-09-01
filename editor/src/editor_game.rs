// Copyright Rob Gage 2026

use crate::editor_application::EditorApplication;
use engine::games::Game;
use std::error::Error;

/// Extends a `Game` with editor launch support
pub trait EditorGame: Game {

    /// Launches this `Game` inside the editor
    fn launch_in_editor(self) -> Result<(), Box<dyn Error>>
    where
        Self: Sized,
    { EditorApplication::launch(self) }

}

impl<G: Game> EditorGame for G {}
