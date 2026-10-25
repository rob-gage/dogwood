// Copyright Rob Gage 2026

use std::fmt;

/// An error encountered while reading data back from an accelerator
#[derive(Debug)]
pub enum AcceleratorReadbackError {
    /// The accelerator could not be polled
    Poll(wgpu::PollError),
    /// A readback buffer could not be mapped
    BufferAsync(wgpu::BufferAsyncError),
}

impl fmt::Display for AcceleratorReadbackError {

    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Poll(error) => write!(formatter, "WGPU polling failed: {error}"),
            Self::BufferAsync(error) => write!(formatter, "WGPU buffer mapping failed: {error}"),
        }
    }

}

impl std::error::Error for AcceleratorReadbackError {

    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Poll(error) => Some(error),
            Self::BufferAsync(error) => Some(error),
        }
    }

}
