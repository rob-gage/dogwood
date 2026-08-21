// Copyright Rob Gage 2026

use std::error::Error;

use super::game_application::GameApplication;

/// Implementors are games that run on this engine.
pub trait Game {

    /// The title of the game.
    const TITLE: &'static str;

    /// Launches this `Game`.
    fn launch(self) -> Result<(), Box<dyn Error>>
    where
        Self: Sized,
    {
        let event_loop: winit::event_loop::EventLoop<()> = winit::event_loop::EventLoop::new()?;
        let mut application: GameApplication<Self> = GameApplication::new(self);
        event_loop.run_app(&mut application)?;
        application.finish()
    }

}
