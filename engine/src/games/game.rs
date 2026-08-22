// Copyright Rob Gage 2026

use super::game_application::GameApplication;
use std::error::Error;
use engine_physics::scenes::Scene;
use engine_user_interface::UserInterfaceContext;

/// Implementors are games that run on this engine.
pub trait Game {

    /// The title of the game.
    const TITLE: &'static str;

    /// Launches this `Game`.
    fn launch(self) -> Result<(), Box<dyn Error>>
    where
        Self: Sized,
    {
        let mut event_loop_builder: winit::event_loop::EventLoopBuilder<()> =
            winit::event_loop::EventLoop::builder();
        #[cfg(all(
            unix,
            not(target_os = "android"),
            not(target_os = "emscripten"),
            not(target_os = "ios"),
            not(target_os = "macos"),
            not(target_os = "redox"),
            feature = "x11",
            not(feature = "wayland"),
        ))]
        winit::platform::x11::EventLoopBuilderExtX11::with_x11(
            &mut event_loop_builder
        );
        #[cfg(all(
            unix,
            not(target_os = "android"),
            not(target_os = "emscripten"),
            not(target_os = "ios"),
            not(target_os = "macos"),
            not(target_os = "redox"),
            feature = "wayland",
            not(feature = "x11"),
        ))]
        winit::platform::wayland::EventLoopBuilderExtWayland::with_wayland(
            &mut event_loop_builder,
        );
        let event_loop: winit::event_loop::EventLoop<()> = event_loop_builder.build()?;
        let mut application: GameApplication<Self> = GameApplication::new(self);
        event_loop.run_app(&mut application)?;
        application.finish()
    }

    /// Returns `true` if the simulation of the active `Scene` of this `Game` is paused
    fn is_paused(&self) -> bool;

    /// Returns an immutable reference to the active `Scene` of this `Game`
    fn scene(&self) -> &Scene;

    /// Returns a mutable reference to the active `UserInterfaceContext` of this `Game`
    fn user_interface_context(&mut self) -> &mut UserInterfaceContext;

}
