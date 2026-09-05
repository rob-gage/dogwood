// Copyright Rob Gage 2026

use engine::{
    Game,
    GameApplication
};
use engine_user_interface::widgets::{
    Spacer,
    StackHorizontal,
    StackVertical,
};
use engine_graphics::Color;
use std::{
    error::Error,
    sync::Arc,
};

/// A windowed editor application for a `Game`
pub struct EditorApplication<G: Game> {
    application: GameApplication<G>,
    layout: StackVertical,
}

impl<G: Game> EditorApplication<G> {

    fn new(accelerator: Arc<engine::compute::Accelerator>, game: G) -> Self {
        let background: Color = Color::new_rgba(47, 47, 47, 255);
        let background_dark: Color = Color::new_rgba(37, 37, 37, 255);
        let stack_horizontal: StackHorizontal = StackHorizontal::new()
            .with_child(Spacer::new(128.0).with_background_color(&background))
            .with_child(Spacer::new_flexible())
            .with_child(Spacer::new(32.0).with_background_color(&background));
        let stack_vertical: StackVertical = StackVertical::new()
            .with_child(Spacer::new(32.0).with_background_color(&background_dark))
            .with_child(stack_horizontal)
            .with_child(Spacer::new(16.0).with_background_color(&background_dark));
        Self {
            application: GameApplication::new_with_title(
                accelerator,
                game,
                format!("Engine Editor: {}", G::TITLE)
            ),
            layout: stack_vertical,
        }
    }

    pub fn launch(
        accelerator: Arc<engine::compute::Accelerator>,
        game: G,
    ) -> Result<(), Box<dyn Error>> {
        let event_loop: winit::event_loop::EventLoop<()> =
            winit::event_loop::EventLoop::builder().build()?;
        let mut application: EditorApplication<G> = Self::new(accelerator, game);
        event_loop.run_app(&mut application)?;
        application.application.finish()
    }

}

impl<G: Game> winit::application::ApplicationHandler for EditorApplication<G> {

    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop)
    { self.application.window_initialize(event_loop); }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) { self.application.handle_window_event(event_loop, event); }

    fn about_to_wait(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        self.application.add_widget(&mut self.layout);
        self.application.redraw();
    }

}
