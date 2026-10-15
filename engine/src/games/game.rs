// Copyright Rob Gage 2026

use engine_host::system_events::{
    SystemKeyboardEvent,
    SystemMouseEvent,
    SystemWindowEvent,
};
use engine_host::windows::WindowConfiguration;
use engine_user_interface::UserInterface;
use engine_world::{
    actors::Actor,
    scenes::Scene,
};
use std::time::Duration;

/// A game that runs on the `Engine`
pub trait Game {

    /// Returns runtime settings for the native game window
    fn window_configuration(&self) -> WindowConfiguration { WindowConfiguration::default() }

    /// Advances game state by one fixed update step
    fn update(&mut self, _elapsed_time: Duration) {}

    /// Returns the scene streamed and drawn by the engine with its camera actor, or `None`
    fn scene(&mut self) -> Option<(&mut Scene, Actor)> { None }

    /// Composes the user interface drawn over the scene in every rendered frame
    fn user_interface(&mut self, _user_interface: &mut UserInterface) {}

    /// Handles a host window event
    fn window_event(&mut self, _event: SystemWindowEvent) {}

    /// Handles a host keyboard event
    fn keyboard_event(&mut self, _event: SystemKeyboardEvent) {}

    /// Handles a host mouse event
    fn mouse_event(&mut self, _event: SystemMouseEvent) {}

}
