// Copyright Rob Gage 2026

//! Public facade for the Dogwood engine runtime and its major subsystems.

extern crate dogwood_engine_audio as engine_audio;
extern crate dogwood_engine_compute as engine_compute;
extern crate dogwood_engine_graphics as engine_graphics;
extern crate dogwood_engine_input as engine_input;
extern crate dogwood_engine_physics as engine_physics;
extern crate dogwood_engine_user_interface as engine_user_interface;

mod games;
mod renders;

pub mod diagnostics;

pub use games::{Game, GameApplication, GamePointerInput};

/// Audio subsystem APIs.
pub mod audio {
    pub use engine_audio::*;
}

/// Compute subsystem APIs.
pub mod compute {
    pub use engine_compute::*;
}

/// Input subsystem APIs.
pub mod input {
    pub use engine_input::*;
}

/// Graphics subsystem APIs.
pub mod graphics {
    pub use engine_graphics::*;
}

/// Physics subsystem APIs.
pub mod physics {
    pub use engine_physics::*;
}

/// User-interface subsystem APIs.
pub mod user_interface {
    pub use engine_user_interface::*;
}
