// Copyright Rob Gage 2026

use crate::renders::{
    RenderContext,
    UserInterfaceRenderer
};
use super::{
    Game,
    render_game,
};
use engine_compute::Accelerator;
use engine_input::{
    ControlState,
    InputTranslator,
    KeyboardInputState
};
use engine_user_interface::Widget;
use std::{
    error::Error,
    sync::Arc,
};

/// A `winit` application used to run a `Game` implementor
pub struct GameApplication<G: Game> {
    /// The game run by this `GameApplication`
    game: G,
    /// The input translator selected by this `Game`
    input_translator: Box<dyn InputTranslator>,
    /// The title of the application window
    title: String,
    /// The `winit` window used by this `GameApplication`
    window: Option<Arc<winit::window::Window>>,
    /// The shared WGPU accelerator used for graphics and compute
    accelerator: Option<Accelerator>,
    /// The graphics-specific state used to render the window
    render_context: Option<RenderContext>,
    /// The renderer for the active user interface
    user_interface_renderer: UserInterfaceRenderer,
    /// An error with the `GameApplication`
    error: Option<Box<dyn Error>>,
    /// The current keyboard input state
    keyboard_input_state: KeyboardInputState,
}

impl<G: Game> GameApplication<G> {

    /// Creates a `GameApplication` from a `Game`
    pub fn new(game: G) -> Self { Self::new_with_title(game, G::TITLE) }

    /// Creates a `GameApplication` from a `Game` with a provided title
    pub fn new_with_title(game: G, title: impl Into<String>) -> Self {
        let input_translator: Box<dyn InputTranslator> = game.input_translator();
        Self {
            game,
            input_translator,
            title: title.into(),
            window: None,
            accelerator: None,
            render_context: None,
            user_interface_renderer: UserInterfaceRenderer::new(),
            error: None,
            keyboard_input_state: KeyboardInputState::new(),
        }
    }

    /// Returns the error produced while running the application, if any
    pub fn finish(self) -> Result<(), Box<dyn Error>> { self.error.map_or(Ok(()), Err) }

    /// Acquires the next window frame, renders it, and presents it
    fn render(&mut self) {
        use wgpu::CurrentSurfaceTexture::*;
        let frame: wgpu::SurfaceTexture = match self.render_context.as_ref()
                .map(|context| context.surface().get_current_texture()) {
                    None => return,
                    Some(Success(frame)) | Some(Suboptimal(frame)) => frame,
                    Some(Outdated) | Some(Lost) => {
                        let (Some(accelerator), Some(render_context)) = (
                            self.accelerator.as_ref(),
                            self.render_context.as_ref()
                        ) else { return; };
                        render_context.configure(accelerator);
                        return;
                    }
                    Some(Timeout) | Some(Occluded) | Some(Validation) => return,
                };
        let view: wgpu::TextureView =
            frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let Some(accelerator) = self.accelerator.as_ref() else { return; };
        let mut command_encoder: wgpu::CommandEncoder = {
            accelerator.wgpu_device().create_command_encoder(
                &wgpu::CommandEncoderDescriptor { label: Some("frame") },
            )
        };
        let Some(render_context) = self.render_context.as_ref() else { return; };
        let configuration: &wgpu::SurfaceConfiguration = render_context.configuration();
        render_game(
            &mut self.game,
            &mut self.user_interface_renderer,
            accelerator,
            configuration.format,
            [configuration.width, configuration.height],
            &mut command_encoder,
            &view,
        );
        let Some(accelerator) = self.accelerator.as_ref() else { return; };
        accelerator.wgpu_queue().submit(Some(command_encoder.finish()));
        accelerator.wgpu_queue().present(frame);
    }

    /// Reconfigures the graphics surface for a new window size
    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 { return; }
        let (Some(accelerator), Some(render_context)) =
            (self.accelerator.as_ref(), self.render_context.as_mut()) else { return; };
        render_context.resize(accelerator, width, height);
    }

    /// Adds a widget to the game's user interface.
    pub fn add_widget(&mut self, widget: &mut impl Widget) {
        let Some(render_context) = self.render_context.as_ref() else { return; };
        let configuration = render_context.configuration();
        self.game.user_interface_context().add_widget(
            widget,
            [configuration.width, configuration.height],
        );
    }

}

impl<G: Game> GameApplication<G> {

    /// Handles window lifecycle, resize, and redraw events
    pub fn handle_window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        event: winit::event::WindowEvent,
    ) {
        use winit::event::WindowEvent::*;
        match event {
            CloseRequested => event_loop.exit(),
            Resized(size) => self.resize(size.width, size.height),
            KeyboardInput { event, .. } => self.keyboard_input_state.process_event(&event),
            RedrawRequested => self.render(),
            _ => {}
        }
    }

    /// Requests another redraw when the event loop is idle
    pub fn redraw(&self) {
        if let Some(window) = self.window.as_ref() { window.request_redraw(); }
    }

    /// Creates the application window and initializes its graphics resources
    pub fn set_up(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.window.is_some() { return; }
        let attributes: winit::window::WindowAttributes =
            winit::window::WindowAttributes::default().with_title(&self.title);
        match event_loop.create_window(attributes) {
            Ok(window) => {
                let window: Arc<winit::window::Window> = Arc::new(window);
                let instance: wgpu::Instance = wgpu::Instance::default();
                let surface: wgpu::Surface = match instance.create_surface(window.clone()) {
                    Ok(surface) => surface,
                    Err(error) => {
                        self.error = Some(Box::new(error));
                        event_loop.exit();
                        return;
                    }
                };
                let size: winit::dpi::PhysicalSize<u32> = window.inner_size();
                let accelerator: Accelerator =
                    match pollster::block_on(Accelerator::new(instance, &surface)) {
                        Ok(accelerator) => accelerator,
                        Err(error) => {
                            self.error = Some(error);
                            event_loop.exit();
                            return;
                        }
                    };
                let render_context: RenderContext = match RenderContext::new(
                    surface,
                    &accelerator,
                    size.width,
                    size.height,
                ) {
                    Ok(render_context) => render_context,
                    Err(error) => {
                        self.error = Some(error);
                        event_loop.exit();
                        return;
                    }
                };
                self.accelerator = Some(accelerator);
                self.render_context = Some(render_context);
                self.window = Some(window);
            }
            Err(error) => {
                self.error = Some(Box::new(error));
                event_loop.exit();
            }
        }
    }

}

impl<G: Game> winit::application::ApplicationHandler for GameApplication<G> {

    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        self.set_up(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        self.handle_window_event(event_loop, event);
    }

    fn about_to_wait(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        let control_state: ControlState = self.input_translator.translate(&self.keyboard_input_state);
        self.game.pass_input(&self.keyboard_input_state, control_state);
        self.redraw();
    }

}
