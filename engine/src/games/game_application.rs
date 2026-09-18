// Copyright Rob Gage 2026

use super::{Game, GamePointerInput};
use crate::renders::{SceneRenderer, UserInterfaceRenderer};
use engine_compute::Accelerator;
use engine_graphics::Camera;
use engine_input::{InputTranslator, KeyboardInputState};
use engine_physics::scenes::ScenePosition;
use engine_user_interface::{UserInterface, Widget};
use std::{error::Error, sync::Arc};

#[path = "game_application_window.rs"]
mod game_application_window;

fn scene_world_position_from_viewport(
    position: [f32; 2],
    viewport: [u32; 4],
    camera_position: [f32; 2],
    camera_size: [f32; 2],
) -> Option<[f32; 2]> {
    let x: f32 = position[0] - viewport[0] as f32;
    let y: f32 = position[1] - viewport[1] as f32;
    if x < 0.0 || y < 0.0 || x >= viewport[2] as f32 || y >= viewport[3] as f32 {
        return None;
    }
    Some([
        camera_position[0] + (x / viewport[2] as f32 - 0.5) * camera_size[0],
        camera_position[1] + (0.5 - y / viewport[3] as f32) * camera_size[1],
    ])
}

fn accepts_game_pointer_press(
    enabled: bool,
    ui_consumed: bool,
    world_position: Option<[f32; 2]>,
    already_down: bool,
    hosted_scene_viewport: bool,
) -> bool {
    enabled && (!ui_consumed || hosted_scene_viewport) && world_position.is_some() && !already_down
}

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
    /// The shared Accelerator used for graphics and compute
    accelerator: Arc<Accelerator>,
    /// The rendering surface used to render the window
    surface: Option<wgpu::Surface<'static>>,
    /// The current configuration of the window surface
    surface_configuration: Option<wgpu::SurfaceConfiguration>,
    /// The editor content rectangle to contain the scene in physical pixels
    scene_viewport_bounds: Option<[u32; 4]>,
    /// Whether the application host permits ordinary scene simulation
    is_simulation_enabled: bool,
    /// Editor-selected scene visualization mode
    scene_view_mode: u32,
    /// Whether the scene renderer draws tile boundaries
    show_tile_borders: bool,
    /// Whether the scene renderer draws chunk boundaries
    show_chunk_borders: bool,
    /// Whether the host routes primary pointer input to the game.
    game_pointer_input_enabled: bool,
    /// Latest physical cursor position in the window.
    pointer_position: Option<[f32; 2]>,
    /// Whether gameplay currently owns the primary pointer capture.
    primary_pointer_down: bool,
    /// One-frame primary pointer edges.
    primary_pointer_pressed: bool,
    primary_pointer_released: bool,
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
    /// The start of the current editor performance-rate sample
    performance_sample_time: std::time::Instant,
    /// Successfully presented frames in the current performance-rate sample
    rendered_frames: u32,
    /// Fixed simulation ticks completed in the current performance-rate sample
    simulation_ticks: u32,
    /// Successfully presented frames per second in the preceding sample
    frames_per_second: u32,
    /// Fixed simulation ticks per second in the preceding sample
    ticks_per_second: u32,
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
            is_simulation_enabled: true,
            scene_view_mode: 0,
            show_tile_borders: false,
            show_chunk_borders: false,
            game_pointer_input_enabled: true,
            pointer_position: None,
            primary_pointer_down: false,
            primary_pointer_pressed: false,
            primary_pointer_released: false,
            scene_renderer: SceneRenderer::new(),
            user_interface_renderer: UserInterfaceRenderer::new(),
            error: None,
            keyboard_input_state: KeyboardInputState::new(),
            camera,
            camera_position: [0.5, 0.5],
            camera_velocity: [0.0, 0.0],
            update_time: std::time::Instant::now(),
            performance_sample_time: std::time::Instant::now(),
            rendered_frames: 0,
            simulation_ticks: 0,
            frames_per_second: 0,
            ticks_per_second: 0,
        }
    }

    /// Returns the error produced while running the application, if any
    pub fn finish(self) -> Result<(), Box<dyn Error>> {
        self.error.map_or(Ok(()), Err)
    }

    /// Acquires the next window frame, renders it, and presents it
    fn render(&mut self) {
        use wgpu::CurrentSurfaceTexture::*;
        let frame: wgpu::SurfaceTexture = match self
            .surface
            .as_ref()
            .map(wgpu::Surface::get_current_texture)
        {
            None => return,
            Some(Success(frame)) | Some(Suboptimal(frame)) => frame,
            Some(Outdated) | Some(Lost) => {
                tracing::warn!("reconfiguring unavailable rendering surface");
                let (Some(surface), Some(configuration)) =
                    (self.surface.as_ref(), self.surface_configuration.as_ref())
                else {
                    return;
                };
                surface.configure(self.accelerator.wgpu_device(), configuration);
                return;
            }
            Some(Validation) => {
                tracing::error!("rendering surface validation failed");
                return;
            }
            Some(Timeout) | Some(Occluded) => return,
        };
        let view: wgpu::TextureView = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut command_encoder: wgpu::CommandEncoder = {
            self.accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("frame"),
                })
        };
        let Some(configuration) = self.surface_configuration.as_ref() else {
            return;
        };
        let viewport: [u32; 4] = self.scene_viewport(configuration);
        let camera_size: [f32; 2] = self.scene_camera_size(viewport);
        self.scene_renderer.render(
            &self.accelerator,
            self.game.scene(),
            configuration.format,
            viewport,
            self.camera_position,
            camera_size,
            self.scene_viewport_bounds.is_some(),
            self.scene_view_mode,
            self.show_tile_borders,
            self.show_chunk_borders,
            &self.game.scene_overlays(),
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

        self.accelerator
            .accelerator_timing_resolve_sample(&mut command_encoder);
        self.accelerator
            .wgpu_queue()
            .submit(Some(command_encoder.finish()));
        self.accelerator.accelerator_timing_map_sample();
        self.accelerator.wgpu_queue().present(frame);
        self.rendered_frames = self.rendered_frames.saturating_add(1);
    }

    /// Reconfigures the graphics surface for a new window size
    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        let (Some(surface), Some(configuration)) =
            (self.surface.as_ref(), self.surface_configuration.as_mut())
        else {
            return;
        };
        configuration.width = width;
        configuration.height = height;
        surface.configure(self.accelerator.wgpu_device(), configuration);
    }

    /// Adds a widget to the game's user interface.
    pub fn add_widget(&mut self, mut widget: impl Widget + 'static) {
        self.game.user_interface_context().add_contents(move |ui| {
            ui.add_widget(&mut widget);
        });
    }

    /// Queues host-owned UI contents for the shared frame.
    pub fn add_contents(&mut self, add_contents: impl FnOnce(&mut UserInterface) + 'static) {
        self.game
            .user_interface_context()
            .add_contents(add_contents);
    }

    /// Runs all queued game and host UI contents with the window's real input.
    pub fn compose_user_interface(&mut self) {
        let (Some(configuration), Some(window)) =
            (self.surface_configuration.as_ref(), self.window.as_ref())
        else {
            return;
        };
        self.game
            .user_interface_context()
            .compose([configuration.width, configuration.height], window);
    }

    /// Returns the game run by this application
    pub const fn game(&self) -> &G {
        &self.game
    }

    /// Returns mutable access to the game run by this application
    pub const fn game_mutable(&mut self) -> &mut G {
        &mut self.game
    }

    /// Sets the editor content rectangle in which the game scene is rendered
    pub fn set_scene_viewport_bounds(&mut self, bounds: Option<[u32; 4]>) {
        self.scene_viewport_bounds = bounds.filter(|bounds| bounds[2] > 0 && bounds[3] > 0);
    }

    /// Enables or disables ordinary scene simulation for this application host
    pub fn set_simulation_enabled(&mut self, is_enabled: bool) {
        self.is_simulation_enabled = is_enabled;
    }

    /// Enables or disables gameplay-owned pointer input.
    pub fn set_game_pointer_input_enabled(&mut self, is_enabled: bool) {
        self.game_pointer_input_enabled = is_enabled;
        if !is_enabled {
            self.pointer_position = None;
            self.primary_pointer_down = false;
            self.primary_pointer_pressed = false;
            self.primary_pointer_released = false;
        }
    }

    /// Returns the latest sampled rendered-frame and fixed-simulation rates
    pub const fn performance_rates(&self) -> [u32; 2] {
        [self.frames_per_second, self.ticks_per_second]
    }

    /// Configures editor-only scene visualization without changing scene state
    pub fn set_scene_view_settings(
        &mut self,
        mode: u32,
        show_tile_borders: bool,
        show_chunk_borders: bool,
    ) {
        self.scene_view_mode = mode;
        self.show_tile_borders = show_tile_borders;
        self.show_chunk_borders = show_chunk_borders;
    }

    /// Returns the world position rendered at a physical surface pixel
    pub fn scene_world_position(&self, position: [f32; 2]) -> Option<[f32; 2]> {
        let configuration: &wgpu::SurfaceConfiguration = self.surface_configuration.as_ref()?;
        let viewport: [u32; 4] = self.scene_viewport(configuration);
        let camera_size: [f32; 2] = self.scene_camera_size(viewport);
        scene_world_position_from_viewport(position, viewport, self.camera_position, camera_size)
    }

    /// Returns a clipped physical surface rectangle for a world-space rectangle
    pub fn scene_surface_rectangle(&self, rectangle: [f32; 4]) -> Option<[f32; 4]> {
        let configuration: &wgpu::SurfaceConfiguration = self.surface_configuration.as_ref()?;
        let viewport: [u32; 4] = self.scene_viewport(configuration);
        let camera_size: [f32; 2] = self.scene_camera_size(viewport);
        let left: f32 = viewport[0] as f32
            + ((rectangle[0] - self.camera_position[0]) / camera_size[0] + 0.5)
                * viewport[2] as f32;
        let right: f32 = viewport[0] as f32
            + ((rectangle[2] - self.camera_position[0]) / camera_size[0] + 0.5)
                * viewport[2] as f32;
        let top: f32 = viewport[1] as f32
            + (0.5 - (rectangle[3] - self.camera_position[1]) / camera_size[1])
                * viewport[3] as f32;
        let bottom: f32 = viewport[1] as f32
            + (0.5 - (rectangle[1] - self.camera_position[1]) / camera_size[1])
                * viewport[3] as f32;
        let left: f32 = left.max(viewport[0] as f32);
        let top: f32 = top.max(viewport[1] as f32);
        let right: f32 = right.min((viewport[0] + viewport[2]) as f32);
        let bottom: f32 = bottom.min((viewport[1] + viewport[3]) as f32);
        (left < right && top < bottom).then_some([left, top, right, bottom])
    }

    /// Updates the application systems
    fn update(&mut self) -> Result<(), std::io::Error> {
        let update_time: std::time::Instant = std::time::Instant::now();
        let elapsed: std::time::Duration = update_time.duration_since(self.update_time);
        self.update_time = update_time;
        let simulation_active: bool = self.is_simulation_enabled && !self.game.is_paused();
        let mut actor_contacts = Vec::new();
        let mut material_extractions = Vec::new();
        if let Some(scene) = self.game.scene_mutable() {
            self.simulation_ticks = self
                .simulation_ticks
                .saturating_add(scene.update(elapsed, simulation_active)?);
            actor_contacts = scene.drain_actor_contact_events();
            material_extractions = scene.take_material_extraction_results();
        }
        self.game.actor_contacts(&actor_contacts);
        self.game.material_extractions(&material_extractions);
        self.game.update(elapsed);
        self.game.compose_user_interface();
        self.update_performance_rates(update_time);
        self.update_camera(elapsed.as_secs_f32());
        Ok(())
    }

    /// Updates the reported rates after each approximately one-second sample interval
    fn update_performance_rates(&mut self, update_time: std::time::Instant) {
        let elapsed: std::time::Duration = update_time.duration_since(self.performance_sample_time);
        if elapsed < std::time::Duration::from_secs(1) {
            return;
        }
        let seconds: f64 = elapsed.as_secs_f64();
        self.frames_per_second = (f64::from(self.rendered_frames) / seconds).round() as u32;
        self.ticks_per_second = (f64::from(self.simulation_ticks) / seconds).round() as u32;
        self.performance_sample_time = update_time;
        self.rendered_frames = 0;
        self.simulation_ticks = 0;
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
        if self.camera.follow_distance_maximum > 0.0
            && distance > self.camera.follow_distance_maximum
        {
            self.camera_position[0] = target[0] - direction_x * self.camera.follow_distance_maximum;
            self.camera_position[1] = target[1] - direction_y * self.camera.follow_distance_maximum;
            self.camera_velocity = [0.0, 0.0];
            return;
        }
        self.camera_velocity[0] += direction_x * self.camera.follow_acceleration * delta_time;
        self.camera_velocity[1] += direction_y * self.camera.follow_acceleration * delta_time;
        let velocity_squared: f32 = self.camera_velocity[0] * self.camera_velocity[0]
            + self.camera_velocity[1] * self.camera_velocity[1];
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

    pub(super) fn handle_pointer_event(
        &mut self,
        event: &winit::event::WindowEvent,
        ui_consumed: bool,
    ) {
        use winit::event::{ElementState, MouseButton, WindowEvent};
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer_position = Some([position.x as f32, position.y as f32]);
            }
            WindowEvent::CursorLeft { .. } => self.pointer_position = None,
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if accepts_game_pointer_press(
                    self.game_pointer_input_enabled,
                    ui_consumed,
                    self.pointer_position
                        .and_then(|position| self.scene_world_position(position)),
                    self.primary_pointer_down,
                    self.scene_viewport_bounds.is_some(),
                ) {
                    self.primary_pointer_down = true;
                    self.primary_pointer_pressed = true;
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => {
                if self.primary_pointer_down {
                    self.primary_pointer_down = false;
                    self.primary_pointer_released = true;
                }
            }
            WindowEvent::Focused(false) => {
                self.pointer_position = None;
                self.primary_pointer_down = false;
                self.primary_pointer_pressed = false;
                self.primary_pointer_released = true;
            }
            _ => {}
        }
    }

    fn pointer_input(&self) -> GamePointerInput {
        GamePointerInput {
            world_position: self
                .pointer_position
                .and_then(|position| self.scene_world_position(position)),
            primary_down: self.primary_pointer_down,
            primary_pressed: self.primary_pointer_pressed,
            primary_released: self.primary_pointer_released,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{accepts_game_pointer_press, scene_world_position_from_viewport};

    #[test]
    fn pointer_world_conversion_rejects_outside_and_maps_viewport_center() {
        let viewport = [10, 20, 100, 50];
        assert_eq!(
            scene_world_position_from_viewport([60.0, 45.0], viewport, [3.0, 4.0], [10.0, 5.0]),
            Some([3.0, 4.0])
        );
        assert_eq!(
            scene_world_position_from_viewport([9.0, 45.0], viewport, [3.0, 4.0], [10.0, 5.0]),
            None
        );
    }

    #[test]
    fn ui_or_disabled_pointer_press_cannot_capture_gameplay() {
        assert!(!accepts_game_pointer_press(
            true,
            true,
            Some([0.0, 0.0]),
            false,
            false,
        ));
        assert!(!accepts_game_pointer_press(
            false,
            false,
            Some([0.0, 0.0]),
            false,
            false,
        ));
        assert!(!accepts_game_pointer_press(true, false, None, false, false));
        assert!(accepts_game_pointer_press(
            true,
            false,
            Some([0.0, 0.0]),
            false,
            false,
        ));
        assert!(!accepts_game_pointer_press(
            true,
            false,
            Some([0.0, 0.0]),
            true,
            false,
        ));
        assert!(accepts_game_pointer_press(
            true,
            true,
            Some([0.0, 0.0]),
            false,
            true,
        ));
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
        self.handle_window_event(event_loop, &event);
    }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        self.accelerator.accelerator_timing_begin_sample();
        self.game
            .pass_input(&self.keyboard_input_state, self.input_translator.as_ref());
        self.game.pass_pointer_input(&self.pointer_input());
        if let Err(error) = self.update() {
            tracing::error!(%error, "application update failed");
            self.error = Some(Box::new(error));
            event_loop.exit();
            return;
        }
        self.primary_pointer_pressed = false;
        self.primary_pointer_released = false;
        self.compose_user_interface();
        self.redraw();
    }
}
