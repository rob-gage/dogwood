// Copyright Rob Gage 2026

use super::EngineError;
use crate::games::{
    Game,
    GameInstance,
};
use engine_host::system_events::{
    SystemKeyboardEvent,
    SystemMouseEvent,
    SystemWindowEvent,
};
use engine_host::windows::Window;
use std::cell::RefCell;
use std::time::Duration;

/// An engine runtime to be used by `Game`s
pub struct Engine {

}

impl Engine {

    /// Launches a game using this `Engine`
    pub fn launch<G: Game>(self, game: G) -> Result<(), EngineError> {
        let mut game_instance: GameInstance<G> = GameInstance::new(self, game);
        let shared_game_instance: RefCell<&mut GameInstance<G>> = RefCell::new(&mut game_instance);
        let mut window_event_handler: Box<dyn FnMut(SystemWindowEvent)> =
            Box::new(|event: SystemWindowEvent| {
                shared_game_instance.borrow_mut().game.window_event(event);
            });
        let mut keyboard_event_handler: Box<dyn FnMut(SystemKeyboardEvent)> = Box::new(|event: SystemKeyboardEvent| {
            shared_game_instance.borrow_mut().game.keyboard_event(event);
        });
        let mut mouse_event_handler: Box<dyn FnMut(SystemMouseEvent)> = Box::new(|event: SystemMouseEvent| {
            shared_game_instance.borrow_mut().game.mouse_event(event);
        });
        let mut update_handler: Box<dyn FnMut(Duration)> = Box::new(|elapsed_time: Duration| {
            shared_game_instance.borrow_mut().game.update(elapsed_time);
        });
        let mut render_handler: Box<dyn FnMut()> = Box::new(|| {
            shared_game_instance.borrow_mut().game.render();
        });
        Window::open(
            &mut window_event_handler,
            &mut keyboard_event_handler,
            &mut mouse_event_handler,
            &mut update_handler,
            &mut render_handler,
        ).map_err(EngineError::from)
    }

}
