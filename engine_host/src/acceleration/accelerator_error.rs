// Copyright Rob Gage 2026

use std::fmt;

/// An error encountered while initializing an Accelerator
#[derive(Debug)]
pub enum AcceleratorError {
    /// Vulkan could not provide a suitable adapter
    VulkanRequestAdapter(wgpu::RequestAdapterError),
    /// Vulkan could not create a device and queue
    VulkanRequestDevice(wgpu::RequestDeviceError),
    /// DirectX 12 could not provide a suitable adapter
    Dx12RequestAdapter(wgpu::RequestAdapterError),
    /// DirectX 12 could not create a device and queue
    Dx12RequestDevice(wgpu::RequestDeviceError),
    /// Metal could not provide a suitable adapter
    MetalRequestAdapter(wgpu::RequestAdapterError),
    /// Metal could not create a device and queue
    MetalRequestDevice(wgpu::RequestDeviceError),
    /// No WGPU backend is configured for the current operating system
    UnsupportedPlatform,
}

impl fmt::Display for AcceleratorError {

    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::VulkanRequestAdapter(error) =>
                write!(formatter, "No suitable Vulkan adapter: {error}"),
            Self::VulkanRequestDevice(error) =>
                write!(formatter, "Vulkan device request failed: {error}"),
            Self::Dx12RequestAdapter(error) =>
                write!(formatter, "No suitable DirectX 12 adapter: {error}"),
            Self::Dx12RequestDevice(error) =>
                write!(formatter, "DirectX 12 device request failed: {error}"),
            Self::MetalRequestAdapter(error) =>
                write!(formatter, "No suitable Metal adapter: {error}"),
            Self::MetalRequestDevice(error) =>
                write!(formatter, "Metal device request failed: {error}"),
            Self::UnsupportedPlatform =>
                write!(formatter, "No graphics backend is configured for this operating system."),
        }
    }

}

impl std::error::Error for AcceleratorError {

    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::VulkanRequestAdapter(error)
            | Self::Dx12RequestAdapter(error)
            | Self::MetalRequestAdapter(error) => Some(error),
            Self::VulkanRequestDevice(error)
            | Self::Dx12RequestDevice(error)
            | Self::MetalRequestDevice(error) => Some(error),
            Self::UnsupportedPlatform => None,
        }
    }

}
