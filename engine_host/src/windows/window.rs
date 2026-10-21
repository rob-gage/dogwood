// Copyright Rob Gage 2026

use super::{
    WindowConfiguration,
    WindowError,
};
use crate::{
    acceleration::Accelerator,
    system_events::{
        SystemKeyboardEvent,
        SystemMouseEvent,
        SystemWindowEvent,
    }
};
use engine_user_interface::{
    egui,
    UserInterfaceContext,
    UserInterfaceWindowState,
};
use std::time::{
    Duration,
    Instant,
};
use std::sync::Arc;

/// A native window managed by the host system
pub struct Window<'a> {
    /// Native `winit` window
    window: Option<Arc<winit::window::Window>>,
    /// Shared GPU context used by this window
    accelerator: &'a Accelerator,
    /// User interface composed over every frame drawn into this window
    user_interface_context: &'a UserInterfaceContext,
    /// User interface input translated from this window's events, absent without a window
    user_interface_window_state: Option<UserInterfaceWindowState>,
    /// Presentation surface owned by this window
    surface: Option<wgpu::Surface<'static>>,
    /// Current presentation settings, absent while minimized
    surface_configuration: Option<wgpu::SurfaceConfiguration>,
    /// Error encountered while creating the native window
    error: Option<WindowError>,
    /// Consumer of translated window events
    window_event_handler: &'a mut dyn FnMut(SystemWindowEvent),
    /// Consumer of translated keyboard events
    keyboard_event_handler: &'a mut dyn FnMut(SystemKeyboardEvent),
    /// Consumer of translated mouse events
    mouse_event_handler: &'a mut dyn FnMut(SystemMouseEvent),
    /// Consumer of fixed-step game updates
    update_handler: &'a mut dyn FnMut(Duration),
    /// Consumer of native redraw requests, drawing into the acquired surface texture with the
    /// user interface input received since the previous frame and returning its platform output
    render_handler: &'a mut dyn FnMut(&wgpu::TextureView, egui::RawInput) -> egui::PlatformOutput,
    /// Runtime settings for the native game window
    window_configuration: WindowConfiguration,
    /// Duration between fixed-step updates
    update_interval: Duration,
    /// Maximum elapsed update time retained after a delayed iteration
    maximum_accumulated_time: Duration,
    /// Time of the most recent scheduler iteration
    last_update_time: Instant,
    /// Elapsed time waiting for the next fixed-step update
    update_accumulator: Duration,
    /// Time of the next independent redraw request
    next_render_time: Instant,
}

impl<'a> Window<'a> {

    /// Opens a native window and dispatches its host events
    pub fn open(
        window_event_handler: &'a mut dyn FnMut(SystemWindowEvent),
        keyboard_event_handler: &'a mut dyn FnMut(SystemKeyboardEvent),
        mouse_event_handler: &'a mut dyn FnMut(SystemMouseEvent),
        update_handler: &'a mut dyn FnMut(Duration),
        render_handler: &'a mut dyn FnMut(
            &wgpu::TextureView,
            egui::RawInput,
        ) -> egui::PlatformOutput,
        accelerator: &'a Accelerator,
        user_interface_context: &'a UserInterfaceContext,
        window_configuration: WindowConfiguration,
        update_interval: Duration,
        maximum_accumulated_time: Duration,
    ) -> Result<(), WindowError> {
        let mut event_loop_builder = winit::event_loop::EventLoop::builder();
        // GNOME supplies system decorations for X11 windows, including through XWayland.
        #[cfg(target_os = "linux")]
        {
            use winit::platform::x11::EventLoopBuilderExtX11;

            let is_gnome = std::env::var("XDG_CURRENT_DESKTOP")
                .is_ok_and(|desktop| desktop.split(':').any(|name| name.eq_ignore_ascii_case("GNOME")));
            let has_x11 = std::env::var_os("DISPLAY").is_some_and(|display| !display.is_empty());
            if is_gnome && has_x11 {
                event_loop_builder.with_x11();
            }
        }
        let event_loop: winit::event_loop::EventLoop<()> =
            event_loop_builder.build().map_err(WindowError::from)?;
        let mut window: Self = Self {
            window: None,
            accelerator,
            user_interface_context,
            user_interface_window_state: None,
            surface: None,
            surface_configuration: None,
            error: None,
            window_event_handler,
            keyboard_event_handler,
            mouse_event_handler,
            update_handler,
            render_handler,
            window_configuration,
            update_interval,
            maximum_accumulated_time,
            last_update_time: Instant::now(),
            update_accumulator: Duration::ZERO,
            next_render_time: Instant::now(),
        };
        event_loop.run_app(&mut window).map_err(WindowError::from)?;
        window.error.map_or(Ok(()), Err)
    }

    /// Resizes the presentation surface or suspends it for a zero-sized window
    fn resize_surface(&mut self, width: u32, height: u32) -> Result<(), WindowError> {
        if width == 0 || height == 0 {
            self.surface_configuration = None;
            return Ok(());
        }
        let surface: &wgpu::Surface<'static> = self.surface.as_ref().ok_or(WindowError::UnsupportedSurface)?;
        let accelerator: &Accelerator = self.accelerator;
        let mut configuration: wgpu::SurfaceConfiguration = match self.surface_configuration.clone() {
            Some(configuration) => configuration,
            None => {
                let mut configuration: wgpu::SurfaceConfiguration = surface
                    .get_default_config(accelerator.adapter(), width, height)
                    .ok_or(WindowError::UnsupportedSurface)?;
                // scene rendering writes linear color that the surface encodes as sRGB
                let formats: Vec<wgpu::TextureFormat> =
                    surface.get_capabilities(accelerator.adapter()).formats;
                if let Some(format) = formats.into_iter().find(|format| format.is_srgb()) {
                    configuration.format = format;
                }
                configuration
            }
        };
        configuration.width = width;
        configuration.height = height;
        surface.configure(accelerator.device(), &configuration);
        self.surface_configuration = Some(configuration);
        Ok(())
    }

    /// Renders and presents one frame, recovering a lost or outdated surface
    fn present(&mut self) -> Result<(), WindowError> {
        if self.surface_configuration.is_none() { return Ok(()); }
        let current_texture: wgpu::CurrentSurfaceTexture = self.surface
            .as_ref()
            .ok_or(WindowError::UnsupportedSurface)?
            .get_current_texture();
        let (frame, suboptimal): (wgpu::SurfaceTexture, bool) = match current_texture {
            wgpu::CurrentSurfaceTexture::Success(frame) => (frame, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => (frame, true),
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return Ok(()),
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.reconfigure_surface();
                return self.present_after_reconfigure();
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                let window: Arc<winit::window::Window> = self.window
                    .as_ref()
                    .ok_or(WindowError::UnsupportedSurface)?
                    .clone();
                self.surface = Some(self.accelerator
                    .instance()
                    .create_surface(window)?);
                self.reconfigure_surface();
                return self.present_after_reconfigure();
            }
            wgpu::CurrentSurfaceTexture::Validation => return Err(WindowError::SurfaceValidation),
        };
        self.render_and_present(frame);
        if suboptimal { self.reconfigure_surface(); }
        Ok(())
    }

    fn present_after_reconfigure(&mut self) -> Result<(), WindowError> {
        let current_texture: wgpu::CurrentSurfaceTexture = self.surface
            .as_ref()
            .ok_or(WindowError::UnsupportedSurface)?
            .get_current_texture();
        let (frame, suboptimal): (wgpu::SurfaceTexture, bool) = match current_texture {
            wgpu::CurrentSurfaceTexture::Success(frame) => (frame, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => (frame, true),
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => return Ok(()),
            wgpu::CurrentSurfaceTexture::Validation => return Err(WindowError::SurfaceValidation),
        };
        self.render_and_present(frame);
        if suboptimal { self.reconfigure_surface(); }
        Ok(())
    }

    fn reconfigure_surface(&self) {
        if let (Some(surface), Some(configuration)) = (
            self.surface.as_ref(),
            self.surface_configuration.as_ref(),
        ) {
            surface.configure(self.accelerator.device(), configuration);
        }
    }

    /// Lets the render handler draw into the frame with the user interface input, then presents it
    fn render_and_present(&mut self, frame: wgpu::SurfaceTexture) {
        let view: wgpu::TextureView = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let input: egui::RawInput = match (
            self.user_interface_window_state.as_mut(),
            self.window.as_ref(),
        ) {
            (Some(user_interface_window_state), Some(window)) =>
                user_interface_window_state.take_input(window),
            _ => egui::RawInput::default(),
        };
        let platform_output: egui::PlatformOutput = (self.render_handler)(&view, input);
        // apply the clipboard, cursor, and text input requested by the composed user interface
        if let (Some(user_interface_window_state), Some(window)) = (
            self.user_interface_window_state.as_mut(),
            self.window.as_ref(),
        ) { user_interface_window_state.handle_platform_output(window, platform_output); }
        self.accelerator.queue().present(frame);
    }

}

impl<'a> winit::application::ApplicationHandler for Window<'a> {

    /// Creates the native window when the event loop resumes
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.window.is_none() {
            let current_time: Instant = Instant::now();
            self.last_update_time = current_time;
            self.next_render_time = current_time;
            self.update_accumulator = Duration::ZERO;
            match event_loop.create_window(
                winit::window::Window::default_attributes().with_transparent(false),
            ) {
                Ok(window) => {
                    let window: Arc<winit::window::Window> = Arc::new(window);
                    let surface: wgpu::Surface<'static> = match self.accelerator.instance().create_surface(window.clone()) {
                        Ok(surface) => surface,
                        Err(error) => {
                            self.error = Some(WindowError::from(error));
                            event_loop.exit();
                            return;
                        }
                    };
                    let size: winit::dpi::PhysicalSize<u32> = window.inner_size();
                    let surface_configuration: Option<wgpu::SurfaceConfiguration> =
                        if size.width > 0 && size.height > 0 {
                            match surface.get_default_config(self.accelerator.adapter(), size.width, size.height) {
                                Some(mut configuration) => {
                                    let formats: Vec<wgpu::TextureFormat> = surface
                                        .get_capabilities(self.accelerator.adapter())
                                        .formats;
                                    if let Some(format) =
                                        formats.into_iter().find(|format| format.is_srgb())
                                    { configuration.format = format; }
                                    surface.configure(self.accelerator.device(), &configuration);
                                    Some(configuration)
                                }
                                None => {
                                    self.error = Some(WindowError::UnsupportedSurface);
                                    event_loop.exit();
                                    return;
                                }
                            }
                        } else {
                            None
                    };
                    self.surface = Some(surface);
                    self.surface_configuration = surface_configuration;
                    self.user_interface_window_state = Some(UserInterfaceWindowState::new(
                        self.user_interface_context,
                        &window,
                        Some(self.accelerator.device().limits().max_texture_dimension_2d as usize),
                    ));
                    window.request_redraw();
                    self.window = Some(window);
                }
                Err(error) => {
                    self.error = Some(WindowError::from(error));
                    event_loop.exit();
                }
            }
        }
    }

    /// Translates and dispatches events for the managed window
    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let is_managed_window: bool = self
            .window
            .as_ref()
            .is_some_and(|window| window.id() == window_id);
        if !is_managed_window {
            return;
        }
        // the user interface sees every event, and its consumption never stops game delivery;
        // its repaint requests are ignored because frames are already redrawn continuously
        if let (Some(user_interface_window_state), Some(window)) = (
            self.user_interface_window_state.as_mut(),
            self.window.as_ref(),
        ) { let _ = user_interface_window_state.process_window_event(window, &event); }
        match event {
            winit::event::WindowEvent::CloseRequested => {
                let host_event: SystemWindowEvent = SystemWindowEvent::CloseRequested;
                (self.window_event_handler)(host_event);
                event_loop.exit();
            }
            winit::event::WindowEvent::Resized(size) => {
                let host_event: SystemWindowEvent = SystemWindowEvent::Resized {
                    width: size.width,
                    height: size.height,
                };
                if let Err(error) = self.resize_surface(size.width, size.height) {
                    self.error = Some(error);
                    event_loop.exit();
                    return;
                }
                (self.window_event_handler)(host_event);
            }
            winit::event::WindowEvent::Focused(focused) => {
                let host_event: SystemWindowEvent = if focused { SystemWindowEvent::FocusAcquired }
                else { SystemWindowEvent::FocusReleased };
                (self.window_event_handler)(host_event);
            }
            winit::event::WindowEvent::KeyboardInput { event, .. } => {
                let host_event: SystemKeyboardEvent = SystemKeyboardEvent {
                    physical_key: event.physical_key,
                    pressed: event.state == winit::event::ElementState::Pressed,
                    repeat: event.repeat,
                };
                (self.keyboard_event_handler)(host_event);
            }
            winit::event::WindowEvent::MouseInput { state, button, .. } => {
                let host_event: SystemMouseEvent = SystemMouseEvent::Button {
                    button,
                    pressed: state == winit::event::ElementState::Pressed,
                };
                (self.mouse_event_handler)(host_event);
            }
            winit::event::WindowEvent::CursorMoved { position, .. } => {
                let host_event: SystemMouseEvent = SystemMouseEvent::CursorMoved {
                    x: position.x,
                    y: position.y,
                };
                (self.mouse_event_handler)(host_event);
            }
            winit::event::WindowEvent::CursorEntered { .. } => {
                let host_event: SystemMouseEvent = SystemMouseEvent::CursorEntered;
                (self.mouse_event_handler)(host_event);
            }
            winit::event::WindowEvent::CursorLeft { .. } => {
                let host_event: SystemMouseEvent = SystemMouseEvent::CursorLeft;
                (self.mouse_event_handler)(host_event);
            }
            winit::event::WindowEvent::MouseWheel { delta, .. } => match delta {
                winit::event::MouseScrollDelta::LineDelta(horizontal, vertical) => {
                    let host_event: SystemMouseEvent = SystemMouseEvent::WheelLines {
                        horizontal,
                        vertical,
                    };
                    (self.mouse_event_handler)(host_event);
                }
                winit::event::MouseScrollDelta::PixelDelta(position) => {
                    let host_event: SystemMouseEvent = SystemMouseEvent::WheelPixels {
                        horizontal: position.x,
                        vertical: position.y,
                    };
                    (self.mouse_event_handler)(host_event);
                }
            },
            winit::event::WindowEvent::RedrawRequested => {
                let is_minimized: bool = self.surface_configuration.is_none();
                if !is_minimized {
                    if let Err(error) = self.present() {
                        self.error = Some(error);
                        event_loop.exit();
                    }
                }
            }
            _ => {}
        }
    }

    /// Releases the native window while the event loop is suspended
    fn suspended(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        self.surface = None;
        self.surface_configuration = None;
        self.user_interface_window_state = None;
        self.window = None;
        self.update_accumulator = Duration::ZERO;
    }

    /// Runs bounded fixed-step updates and schedules the next event-loop wakeup
    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.window.is_none() {
            event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);
            return;
        }
        let current_time: Instant = Instant::now();
        let elapsed_time: Duration = current_time.duration_since(self.last_update_time);
        self.last_update_time = current_time;
        self.update_accumulator =
            self.update_accumulator.saturating_add(elapsed_time).min(self.maximum_accumulated_time);
        while self.update_accumulator >= self.update_interval {
            (self.update_handler)(self.update_interval);
            self.update_accumulator -= self.update_interval;
        }
        let time_until_update: Duration = self.update_interval - self.update_accumulator;
        let next_update_time: Instant = current_time + time_until_update;
        if self.surface_configuration.is_none() {
            event_loop
                .set_control_flow(winit::event_loop::ControlFlow::WaitUntil(next_update_time));
            return;
        }
        if self.window_configuration.fps_limit < u32::MAX {
            let fps_limit: u32 = self.window_configuration.fps_limit.max(1);
            if current_time >= self.next_render_time {
                if let Some(window) = self.window.as_ref() { window.request_redraw(); }
                let render_interval_nanoseconds: u64 =
                    (1_000_000_000_u64 / u64::from(fps_limit)).max(1);
                let render_interval: Duration = Duration::from_nanos(render_interval_nanoseconds);
                self.next_render_time = current_time + render_interval;
            }
            let next_wakeup_time: Instant = next_update_time.min(self.next_render_time);
            event_loop
                .set_control_flow(winit::event_loop::ControlFlow::WaitUntil(next_wakeup_time));
        } else {
            if let Some(window) = self.window.as_ref() { window.request_redraw(); }
            event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
        }
    }

}
