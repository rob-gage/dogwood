// Copyright Rob Gage 2026

use crate::renders::{
    SceneRenderer,
    UserInterfaceRenderer
};
use super::Game;
use engine_compute::Accelerator;
use engine_graphics::Camera;
use engine_input::{
    InputTranslator,
    KeyboardInputState,
};
use engine_physics::scenes::ScenePosition;
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
    accelerator: Arc<Accelerator>,
    /// The WGPU surface used to render the window
    surface: Option<wgpu::Surface<'static>>,
    /// The current configuration of the window surface
    surface_configuration: Option<wgpu::SurfaceConfiguration>,
    /// The editor content rectangle to contain the scene in physical pixels
    scene_viewport_bounds: Option<[u32; 4]>,
    /// The renderer for the active scene
    scene_renderer: SceneRenderer,
    /// The renderer for the active user interface
    user_interface_renderer: UserInterfaceRenderer,
    /// An error with the `GameApplication`
    error: Option<Box<dyn Error>>,
    /// The current keyboard input state
    keyboard_input_state: KeyboardInputState,
    /// The runtime camera state
    camera: Camera,
    /// The current camera position in world tiles
    camera_position: [f32; 2],
    /// The current camera follow velocity in world tiles per second
    camera_velocity: [f32; 2],
    /// The time at which the previous application update occurred
    update_time: std::time::Instant,
}

impl<G: Game> GameApplication<G> {

    /// Creates a `GameApplication` from a `Game`
    pub fn new(accelerator: Arc<Accelerator>, game: G) -> Self {
        Self::new_with_title(accelerator, game, G::TITLE)
    }

    /// Creates a `GameApplication` from a `Game` with a provided title
    pub fn new_with_title(
        accelerator: Arc<Accelerator>,
        game: G,
        title: impl Into<String>,
    ) -> Self {
        let input_translator: Box<dyn InputTranslator> = game.input_translator();
        let camera: Camera = game.camera();
        Self {
            game,
            input_translator,
            title: title.into(),
            window: None,
            accelerator,
            surface: None,
            surface_configuration: None,
            scene_viewport_bounds: None,
            scene_renderer: SceneRenderer::new(),
            user_interface_renderer: UserInterfaceRenderer::new(),
            error: None,
            keyboard_input_state: KeyboardInputState::new(),
            camera,
            camera_position: [0.5, 0.5],
            camera_velocity: [0.0, 0.0],
            update_time: std::time::Instant::now(),
        }
    }

    /// Returns the error produced while running the application, if any
    pub fn finish(self) -> Result<(), Box<dyn Error>> { self.error.map_or(Ok(()), Err) }

    /// Acquires the next window frame, renders it, and presents it
    fn render(&mut self) {
        use wgpu::CurrentSurfaceTexture::*;
        let frame: wgpu::SurfaceTexture = match self.surface.as_ref()
                .map(wgpu::Surface::get_current_texture) {
                    None => return,
                    Some(Success(frame)) | Some(Suboptimal(frame)) => frame,
                    Some(Outdated) | Some(Lost) => {
                        let (Some(surface), Some(configuration)) = (
                            self.surface.as_ref(),
                            self.surface_configuration.as_ref(),
                        ) else { return; };
                        surface.configure(self.accelerator.wgpu_device(), configuration);
                        return;
                    }
                    Some(Timeout) | Some(Occluded) | Some(Validation) => return,
                };
        let view: wgpu::TextureView =
            frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut command_encoder: wgpu::CommandEncoder = {
            self.accelerator.wgpu_device().create_command_encoder(
                &wgpu::CommandEncoderDescriptor { label: Some("frame") },
            )
        };
        let Some(configuration) = self.surface_configuration.as_ref() else { return; };
        let viewport: [u32; 4] = self.scene_viewport_bounds.map_or_else(
            || [0, 0, configuration.width, configuration.height],
            |bounds| {
                let x: u32 = bounds[0].min(configuration.width - 1);
                let y: u32 = bounds[1].min(configuration.height - 1);
                Self::fit_aspect_ratio([
                    x,
                    y,
                    bounds[2].min(configuration.width - x).max(1),
                    bounds[3].min(configuration.height - y).max(1),
                ], self.camera.width / self.camera.height)
            },
        );
        let mut camera_size: [f32; 2] = [
            self.camera.width * self.camera.zoom,
            self.camera.height * self.camera.zoom,
        ];
        let surface_aspect: f32 = viewport[2] as f32 / viewport[3] as f32;
        let camera_aspect: f32 = camera_size[0] / camera_size[1];
        if surface_aspect > camera_aspect {
            camera_size[0] = camera_size[1] * surface_aspect;
        } else {
            camera_size[1] = camera_size[0] / surface_aspect;
        }
        self.scene_renderer.render(
            &self.accelerator,
            self.game.scene(),
            configuration.format,
            viewport,
            self.camera_position,
            camera_size,
            &mut command_encoder,
            &view,
        );
        self.user_interface_renderer.render(
            self.game.user_interface_context(),
            &self.accelerator,
            configuration.format,
            [configuration.width, configuration.height],
            &mut command_encoder,
            &view,
        );

        self.accelerator.wgpu_queue().submit(Some(command_encoder.finish()));
        self.accelerator.wgpu_queue().present(frame);
    }

    /// Reconfigures the graphics surface for a new window size
    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 { return; }
        let (Some(surface), Some(configuration)) = (
            self.surface.as_ref(),
            self.surface_configuration.as_mut(),
        ) else { return; };
        configuration.width = width;
        configuration.height = height;
        surface.configure(self.accelerator.wgpu_device(), configuration);
    }

    /// Adds a widget to the game's user interface.
    pub fn add_widget(&mut self, widget: &mut impl Widget) {
        let (Some(configuration), Some(window)) = (
            self.surface_configuration.as_ref(),
            self.window.as_ref(),
        ) else { return; };
        self.game.user_interface_context().add_widget(
            widget,
            [configuration.width, configuration.height],
            window,
        );
    }

    /// Returns the game run by this application
    pub const fn game(&self) -> &G { &self.game }

    /// Returns mutable access to the game run by this application
    pub const fn game_mutable(&mut self) -> &mut G { &mut self.game }

    /// Sets the editor content rectangle in which the game scene is rendered
    pub fn set_scene_viewport_bounds(&mut self, bounds: Option<[u32; 4]>) {
        self.scene_viewport_bounds = bounds.filter(|bounds| bounds[2] > 0 && bounds[3] > 0);
    }

    /// Updates the application systems
    fn update(&mut self) -> Result<(), std::io::Error> {
        let update_time: std::time::Instant = std::time::Instant::now();
        let elapsed: std::time::Duration = update_time.duration_since(self.update_time);
        self.update_time = update_time;
        let simulation_active: bool = !self.game.is_paused();
        if let Some(scene) = self.game.scene_mutable() {
            scene.update(elapsed, simulation_active)?;
        }
        self.update_camera(elapsed.as_secs_f32());
        Ok(())
    }

    /// Updates the camera position to follow the game's camera target
    fn update_camera(&mut self, delta_time: f32) {
        let target: ScenePosition = self.game.camera_target();
        let target: [f32; 2] = [
            target.tile_coordinates.x as f32 + target.x_offset,
            target.tile_coordinates.y as f32 + target.y_offset,
        ];
        if delta_time <= 0.0 {
            self.camera_position = target;
            self.camera_velocity = [0.0, 0.0];
            return;
        }
        let difference_x: f32 = target[0] - self.camera_position[0];
        let difference_y: f32 = target[1] - self.camera_position[1];
        let distance_squared: f32 = difference_x * difference_x + difference_y * difference_y;
        if distance_squared <= f32::EPSILON {
            self.camera_position = target;
            self.camera_velocity = [0.0, 0.0];
            return;
        }
        if self.camera.follow_acceleration <= 0.0 || self.camera.follow_speed <= 0.0 {
            self.camera_position = target;
            self.camera_velocity = [0.0, 0.0];
            return;
        }
        let distance: f32 = distance_squared.sqrt();
        let direction_x: f32 = difference_x / distance;
        let direction_y: f32 = difference_y / distance;
        if self.camera.follow_distance_maximum > 0.0 &&
                distance > self.camera.follow_distance_maximum {
            self.camera_position[0] = target[0] - direction_x * self.camera.follow_distance_maximum;
            self.camera_position[1] = target[1] - direction_y * self.camera.follow_distance_maximum;
            self.camera_velocity = [0.0, 0.0];
            return;
        }
        self.camera_velocity[0] += direction_x * self.camera.follow_acceleration * delta_time;
        self.camera_velocity[1] += direction_y * self.camera.follow_acceleration * delta_time;
        let velocity_squared: f32 = self.camera_velocity[0] * self.camera_velocity[0] +
            self.camera_velocity[1] * self.camera_velocity[1];
        let speed: f32 = velocity_squared.sqrt();
        if speed > self.camera.follow_speed {
            self.camera_velocity[0] = self.camera_velocity[0] / speed * self.camera.follow_speed;
            self.camera_velocity[1] = self.camera_velocity[1] / speed * self.camera.follow_speed;
        }
        self.camera_position[0] += self.camera_velocity[0] * delta_time;
        self.camera_position[1] += self.camera_velocity[1] * delta_time;
        let remaining_x: f32 = target[0] - self.camera_position[0];
        let remaining_y: f32 = target[1] - self.camera_position[1];
        if remaining_x * difference_x + remaining_y * difference_y < 0.0 {
            self.camera_position = target;
            self.camera_velocity = [0.0, 0.0];
        }
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
        let ui_consumed: bool = self.window.as_ref().is_some_and(|window| {
            self.game.user_interface_context().process_window_event(window, &event)
        });
        match event {
            CloseRequested => event_loop.exit(),
            Resized(size) => self.resize(size.width, size.height),
            KeyboardInput { event, .. } if !ui_consumed =>
                self.keyboard_input_state.process_event(&event),
            RedrawRequested => self.render(),
            _ => {}
        }
    }

    /// Requests another redraw when the event loop is idle
    pub fn redraw(&self) {
        if let Some(window) = self.window.as_ref() { window.request_redraw(); }
    }

    /// Creates the application window and initializes its graphics resources
    pub fn window_initialize(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.window.is_some() { return; }
        let attributes: winit::window::WindowAttributes =
            winit::window::WindowAttributes::default().with_title(&self.title);
        match event_loop.create_window(attributes) {
            Ok(window) => {
                let window: Arc<winit::window::Window> = Arc::new(window);
                let surface: wgpu::Surface = match self.accelerator.wgpu_instance().create_surface(
                    window.clone()
                ) {
                    Ok(surface) => surface,
                    Err(error) => {
                        self.error = Some(Box::new(error));
                        event_loop.exit();
                        return;
                    }
                };
                let size: winit::dpi::PhysicalSize<u32> = window.inner_size();
                let Some(configuration) = surface.get_default_config(
                    self.accelerator.wgpu_adapter(),
                    size.width.max(1),
                    size.height.max(1),
                ) else {
                    self.error = Some("The surface has no supported configuration".into());
                    event_loop.exit();
                    return;
                };
                surface.configure(self.accelerator.wgpu_device(), &configuration);
                self.game.user_interface_context().initialize_window(
                    window.as_ref(),
                    self.accelerator.wgpu_device().limits().max_texture_dimension_2d as usize,
                );
                self.surface = Some(surface);
                self.surface_configuration = Some(configuration);
                self.update_camera(0.0);
                self.window = Some(window);
            }
            Err(error) => {
                self.error = Some(Box::new(error));
                event_loop.exit();
            }
        }
    }

    /// Centers an aspect-constrained viewport within editor content bounds
    fn fit_aspect_ratio(bounds: [u32; 4], aspect_ratio: f32) -> [u32; 4] {
        let [x, y, width, height] = bounds;
        if width as f32 / height as f32 > aspect_ratio {
            let viewport_width: u32 = (height as f32 * aspect_ratio).round() as u32;
            [x + (width - viewport_width) / 2, y, viewport_width, height]
        } else {
            let viewport_height: u32 = (width as f32 / aspect_ratio).round() as u32;
            [x, y + (height - viewport_height) / 2, width, viewport_height]
        }
    }

}

impl<G: Game> winit::application::ApplicationHandler for GameApplication<G> {

    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        self.window_initialize(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        self.handle_window_event(event_loop, event);
    }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        self.game.pass_input(
            &self.keyboard_input_state,
            self.input_translator.as_ref(),
        );
        if let Err(error) = self.update() {
            self.error = Some(Box::new(error));
            event_loop.exit();
            return;
        }
        self.redraw();
    }

}
