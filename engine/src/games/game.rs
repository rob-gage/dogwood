// Copyright Rob Gage 2026

use super::game_application::GameApplication;
use engine_compute::Accelerator;
use engine_graphics::Camera;
use engine_input::{
    InputTranslator,
    KeyboardInputState,
};
use engine_physics::actors::ActorControlState;
use engine_physics::{
    scenes::{
        Scene,
        ScenePosition,
    },
    tiles::TileCoordinates,
};
use engine_user_interface::UserInterfaceContext;
use std::{
    error::Error,
    sync::Arc,
};

/// Implementors are games that run on this engine.
pub trait Game {

    /// The title of the game.
    const TITLE: &'static str;

    /// Returns the camera configuration used by this `Game`
    fn camera(&self) -> Camera;

    /// Returns the position followed by this `Game`'s camera
    fn camera_target(&self) -> ScenePosition {
        let static_position: ScenePosition = ScenePosition {
            tile_coordinates: TileCoordinates { x: 0, y: 0 },
            x_offset: 0.5,
            y_offset: 0.5,
        };
        let Some(scene) = self.scene() else { return static_position; };
        let Some(actor) = scene.possessed_actor() else { return static_position; };
        scene.actor_render_position(actor).unwrap_or(static_position)
    }

    /// Returns the input translator used by this `Game`
    fn input_translator(&self) -> Box<dyn InputTranslator> {
        Box::new(engine_input::SimpleInputTranslator::ARROWS_AND_WASD)
    }

    /// Passes input to the `Game`
    fn pass_input(
        &mut self,
        keyboard_input_state: &KeyboardInputState,
        input_translator: &dyn InputTranslator,
    ) {
        let control_state: ActorControlState = ActorControlState(
            input_translator.translate(keyboard_input_state)
        );
        let Some(scene) = self.scene_mutable() else { return; };
        let Some(actor) = scene.possessed_actor() else { return; };
        scene.actor_registry_mutable().set_control_state(actor, control_state);
    }

    /// Launches this `Game`.
    fn launch(self, accelerator: Arc<Accelerator>) -> Result<(), Box<dyn Error>>
    where
        Self: Sized,
    {
        let _application_span = tracing::info_span!(
            "application",
            title = Self::TITLE,
            kind = "game",
        ).entered();
        tracing::info!("launching game");
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
        let event_loop: winit::event_loop::EventLoop<()> = event_loop_builder.build()
            .inspect_err(|error| tracing::error!(%error, "failed to create event loop"))?;
        let mut application: GameApplication<Self> = GameApplication::new(accelerator, self);
        event_loop.run_app(&mut application)?;
        application.finish()
    }

    /// Returns `true` if the simulation of the active `Scene` of this `Game` is paused
    fn is_paused(&self) -> bool;

    /// Returns an immutable reference to the active `Scene` of this `Game`, if one is loaded
    fn scene(&self) -> Option<&Scene>;

    /// Returns a mutable reference to the active `Scene` of this `Game`, if one is loaded
    fn scene_mutable(&mut self) -> Option<&mut Scene>;

    /// Returns a mutable reference to the active `UserInterfaceContext` of this `Game`
    fn user_interface_context(&mut self) -> &mut UserInterfaceContext;

}
