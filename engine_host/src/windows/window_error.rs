// Copyright Rob Gage 2026

/// An error that can occur with a `HostWindow`
#[derive(Debug)]
pub enum WindowError {
    /// An error that occurs with the `winit` event loop
    WinitEventLoop(winit::error::EventLoopError),
    /// An error that occurs when the OS cannot or will not perform a requested operation
    WinitOs(winit::error::OsError),
    /// WGPU could not create a surface for the native window
    SurfaceCreation(wgpu::CreateSurfaceError),
    /// The selected adapter does not support the native window surface
    UnsupportedSurface,
    /// WGPU reported a validation failure while acquiring a frame
    SurfaceValidation,
}

impl From<winit::error::EventLoopError> for WindowError {

    fn from(error: winit::error::EventLoopError) -> Self { WindowError::WinitEventLoop(error) }

}

impl From<winit::error::OsError> for WindowError {

    fn from(error: winit::error::OsError) -> Self { WindowError::WinitOs(error) }
    
}

impl From<wgpu::CreateSurfaceError> for WindowError {

    fn from(error: wgpu::CreateSurfaceError) -> Self { WindowError::SurfaceCreation(error) }

}
