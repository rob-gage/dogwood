// Copyright Rob Gage 2026

use std::fmt;

/// An error encountered while initializing an Accelerator
#[derive(Debug)]
pub enum AcceleratorError {
    /// No suitable adapter could be selected
    RequestAdapter(wgpu::RequestAdapterError),
    /// The selected adapter could not create a device and queue
    RequestDevice(wgpu::RequestDeviceError),
    /// No WGPU backend is configured for the current operating system
    UnsupportedPlatform,
}

impl fmt::Display for AcceleratorError {

    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RequestAdapter(error) => write!(formatter, "No suitable WGPU adapter: {error}"),
            Self::RequestDevice(error) => write!(formatter, "WGPU device request failed: {error}"),
            Self::UnsupportedPlatform =>
                write!(formatter, "No graphics backend is configured for this operating system."),
        }
    }

}

impl std::error::Error for AcceleratorError {

    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::RequestAdapter(error) => Some(error),
            Self::RequestDevice(error) => Some(error),
            Self::UnsupportedPlatform => None,
        }
    }

}
