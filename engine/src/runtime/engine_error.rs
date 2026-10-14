// Copyright Rob Gage 2026

/// An error that can occur while using an `Engine`
#[derive(Debug)]
pub enum EngineError {
    /// An error encountered while creating the shared GPU context
    Accelerator(engine_host::acceleration::AcceleratorError),
    /// Configuration or pool creation failure while creating the CPU processor
    Processor(engine_host::processors::ProcessorError),
    /// An error raised while creating or running the host window
    Window(engine_host::windows::WindowError),
    /// An error raised while creating a sprite sheet
    SpriteSheet(engine_graphics::sprites::SpriteSheetError),
    /// An error raised while registering or compiling materials
    Material(engine_world::materials::MaterialError),
    /// An error raised while managing actors
    Actor(engine_world::actors::ActorError),
}

impl From<engine_host::acceleration::AcceleratorError> for EngineError {

    fn from(error: engine_host::acceleration::AcceleratorError) -> Self {
        EngineError::Accelerator(error)
    }

}

impl From<engine_host::processors::ProcessorError> for EngineError {

    fn from(error: engine_host::processors::ProcessorError) -> Self {
        EngineError::Processor(error)
    }

}

impl From<engine_host::windows::WindowError> for EngineError {

    fn from(error: engine_host::windows::WindowError) -> Self { EngineError::Window(error) }

}

impl From<engine_graphics::sprites::SpriteSheetError> for EngineError {

    fn from(error: engine_graphics::sprites::SpriteSheetError) -> Self {
        EngineError::SpriteSheet(error)
    }

}

impl From<engine_world::materials::MaterialError> for EngineError {

    fn from(error: engine_world::materials::MaterialError) -> Self { EngineError::Material(error) }

}

impl From<engine_world::actors::ActorError> for EngineError {

    fn from(error: engine_world::actors::ActorError) -> Self { EngineError::Actor(error) }

}
