// Copyright Rob Gage 2026

/// An error that can occur with a `HostWindow`
pub enum HostWindowError {
    /// An error that occurs with the `winit` event loop
    WinitEventLoop(winit::error::EventLoopError),
    /// An error that occurs when the OS cannot or will not perform a requested operation
    WinitOs(winit::error::OsError),
}

impl From<winit::error::EventLoopError> for HostWindowError {

    fn from(error: winit::error::EventLoopError) -> Self { HostWindowError::WinitEventLoop(error) }

}

impl From<winit::error::OsError> for HostWindowError {

    fn from(error: winit::error::OsError) -> Self { HostWindowError::WinitOs(error) }
    
}