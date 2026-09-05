// Copyright Rob Gage 2026

mod games;
mod renders;

pub use games::{
    Game,
    GameApplication,
};

pub mod audio {

}
pub mod compute {
    pub use engine_compute::Accelerator;
}
pub mod input {

}
pub mod graphics {
    pub use engine_graphics::Camera;
}
pub mod physics {
    pub use engine_physics::materials;
    pub use engine_physics::scenes;
}
pub mod user_interface {
    pub use engine_user_interface::UserInterface;
    pub use engine_user_interface::UserInterfaceContext;
    pub use engine_user_interface::Widget;
}
