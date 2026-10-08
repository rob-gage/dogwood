// Copyright Rob Gage 2026

/// An error that can occur while using an `Engine`
#[derive(Debug)]
pub enum EngineError {
    /// An error raised while creating or running the host window
    Window(engine_host::windows::WindowError),

}

impl From<engine_host::windows::WindowError> for EngineError {

    fn from(error: engine_host::windows::WindowError) -> Self { EngineError::Window(error) }

}
