// Copyright Rob Gage 2026

use engine::{
    Game,
    GameApplication
};
use std::error::Error;

/// A windowed editor application for a `Game`
pub struct EditorApplication<G: Game> {
    application: GameApplication<G>,
}

impl<G: Game> EditorApplication<G> {

    fn new(game: G) -> Self {
        Self {
            application: GameApplication::new_with_title(game, "Editor"),
        }
    }

    pub(crate) fn launch(game: G) -> Result<(), Box<dyn Error>> {
        let event_loop: winit::event_loop::EventLoop<()> =
            winit::event_loop::EventLoop::builder().build()?;
        let mut application: EditorApplication<G> = Self::new(game);
        event_loop.run_app(&mut application)?;
        application.application.finish()
    }

}

impl<G: Game> winit::application::ApplicationHandler for EditorApplication<G> {

    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        self.application.set_up(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        self.application.handle_window_event(event_loop, event);
    }

    fn about_to_wait(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        self.application.redraw();
    }

}
