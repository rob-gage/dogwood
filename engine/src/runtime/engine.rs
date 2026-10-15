// Copyright Rob Gage 2026

use super::EngineError;
use crate::games::{
    Game,
    GameInstance,
};
use engine_host::{
    acceleration::Accelerator,
    processors::Processor,
    system_events::{
        SystemKeyboardEvent,
        SystemMouseEvent,
        SystemWindowEvent,
    },
    windows::{
        Window,
        WindowConfiguration,
    },
};
use engine_user_interface::{
    egui,
    UserInterfaceContext,
};
use std::{
    cell::RefCell,
    time::Duration,
};

/// Fixed update frequency in updates per second
const UPDATES_PER_SECOND: u32 = 60;

/// Maximum number of fixed updates retained after a delayed event-loop iteration
const MAXIMUM_ACCUMULATED_UPDATES: u32 = 6;

/// An engine runtime to be used by `Game`s
pub struct Engine {
    /// Shared graphics and compute context used by this engine
    accelerator: Accelerator,
    /// Named CPU execution pools used by this engine
    processor: Processor,
}

impl Engine {

    /// Creates an engine from initialized graphics and CPU processing resources
    pub const fn new(accelerator: Accelerator, processor: Processor) -> Self {
        Self { accelerator, processor }
    }

    /// Returns the shared graphics and compute context
    pub const fn accelerator(&self) -> &Accelerator { &self.accelerator }

    /// Returns the CPU execution pool manager
    pub const fn processor(&self) -> &Processor { &self.processor }

    /// Launches a game using this `Engine`
    pub fn launch<G: Game>(self, game: G) -> Result<(), EngineError> {
        let window_configuration: WindowConfiguration = game.window_configuration();
        let game_instance: RefCell<GameInstance<G>> =
            RefCell::new(GameInstance::new(self.accelerator(), game));
        let user_interface_context: UserInterfaceContext =
            game_instance.borrow().user_interface_context().clone();
        let mut window_event_handler: Box<dyn FnMut(SystemWindowEvent)> =
            Box::new(|event: SystemWindowEvent| game_instance.borrow_mut().window_event(event));
        let mut keyboard_event_handler: Box<dyn FnMut(SystemKeyboardEvent)> =
            Box::new(|event: SystemKeyboardEvent| {
                game_instance.borrow_mut().keyboard_event(event);
            });
        let mut mouse_event_handler: Box<dyn FnMut(SystemMouseEvent)> =
            Box::new(|event: SystemMouseEvent| game_instance.borrow_mut().mouse_event(event));
        let mut update_handler: Box<dyn FnMut(Duration)> = Box::new(|elapsed_time: Duration| {
            game_instance.borrow_mut().update(self.accelerator(), self.processor(), elapsed_time);
        });
        let mut render_handler: Box<
            dyn FnMut(&wgpu::TextureView, egui::RawInput) -> egui::PlatformOutput,
        > = Box::new(|texture_view: &wgpu::TextureView, input: egui::RawInput| {
            game_instance.borrow_mut().render(self.accelerator(), texture_view, input)
        });
        let update_interval: Duration = Duration::from_nanos(
            1_000_000_000 / u64::from(UPDATES_PER_SECOND),
        );
        let maximum_accumulated_time: Duration = update_interval * MAXIMUM_ACCUMULATED_UPDATES;
        Window::open(
            &mut window_event_handler,
            &mut keyboard_event_handler,
            &mut mouse_event_handler,
            &mut update_handler,
            &mut render_handler,
            self.accelerator(),
            &user_interface_context,
            window_configuration,
            update_interval,
            maximum_accumulated_time,
        ).map_err(EngineError::from)
    }

}

impl Default for Engine {

    /// Creates an engine with the default accelerator and processor, panicking if either fails
    fn default() -> Self { Self::new(Accelerator::default(), Processor::default()) }

}
