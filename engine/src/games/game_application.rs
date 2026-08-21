// Copyright Rob Gage 2026

use std::error::Error;

use super::Game;

/// A `winit` application used to run a `Game` implementor
pub struct GameApplication<G: Game> {
    /// The game run by this `GameApplication`
    game: G,
    /// The `winit` window used by this `GameApplication`
    window: Option<winit::window::Window>,
    /// An error with the `GameApplication`
    error: Option<Box<dyn Error>>,
}

impl<G: Game> GameApplication<G> {

    pub fn new(game: G) -> Self {
        Self {
            game: game,
            window: None,
            error: None,
        }
    }

    pub fn finish(self) -> Result<(), Box<dyn Error>> { self.error.map_or(Ok(()), Err) }

}

impl<G: Game> winit::application::ApplicationHandler for GameApplication<G> {

    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.window.is_some() { return; }
        let attributes: winit::window::WindowAttributes =
            winit::window::WindowAttributes::default().with_title(G::TITLE);
        match event_loop.create_window(attributes) {
            Ok(window) => self.window = Some(window),
            Err(error) => {
                self.error = Some(Box::new(error));
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        use winit::event::WindowEvent::*;
        match event {
            CloseRequested => event_loop.exit(),
            _ => {}
        }
    }

}
