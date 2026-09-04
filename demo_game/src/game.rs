// Copyright Rob Gage 2026

use engine::{
    Game,
    graphics::Camera,
    physics::scenes::Scene,
    user_interface::UserInterfaceContext,
};

/// A minimal game used to exercise the engine.
pub struct DemoGame {
    user_interface_context: UserInterfaceContext,
}

impl DemoGame {

    /// Creates an empty `DemoGame`.
    pub fn new() -> Self {
        Self {
            user_interface_context: UserInterfaceContext::new(),
        }
    }

}

impl Game for DemoGame {

    const TITLE: &'static str = "Demo Game";

    /// Returns the camera configuration used by the demo.
    fn camera(&self) -> Camera {
        Camera {
            width: 16.0,
            height: 9.0,
            zoom: 1.0,
            follow_acceleration: 0.0,
            follow_speed: 0.0,
            follow_distance_maximum: 0.0,
        }
    }

    /// Returns whether the demo simulation is paused.
    fn is_paused(&self) -> bool { true }

    /// Returns no scene because the demo does not load one yet.
    fn scene(&self) -> Option<&Scene> { None }

    /// Returns no scene because the demo does not load one yet.
    fn scene_mutable(&mut self) -> Option<&mut Scene> { None }

    /// Returns the empty user-interface context used by the demo.
    fn user_interface_context(&mut self) -> &mut UserInterfaceContext {
        &mut self.user_interface_context
    }

}
