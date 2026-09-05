// Copyright Rob Gage 2026

use crate::editor_application::EditorApplication;
use engine::Game;
use std::{
    error::Error,
    sync::Arc,
};

/// Extends a `Game` with editor launch support
pub trait EditorGame: Game {

    /// Launches this `Game` inside the editor
    fn launch_in_editor(
        self,
        accelerator: Arc<engine::compute::Accelerator>,
    ) -> Result<(), Box<dyn Error>>
    where
        Self: Sized,
    { EditorApplication::launch(accelerator, self) }

}

impl<G: Game> EditorGame for G {}
