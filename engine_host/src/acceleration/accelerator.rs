// Copyright Rob Gage 2026

use super::AcceleratorError;

/// Shared host hardware context for graphics and compute workloads
pub struct Accelerator {
    /// WGPU instance that owns the adapter
    instance: wgpu::Instance,
    /// Adapter selected for this context
    adapter: wgpu::Adapter,
    /// Device used to create GPU resources
    device: wgpu::Device,
    /// Queue used to submit GPU work
    queue: wgpu::Queue,
}

impl Accelerator {

    /// Creates a high performance hardware context synchronously
    pub fn new() -> Result<Self, AcceleratorError> {
        #[cfg(target_os = "windows")]
        {
            return match pollster::block_on(Self::initialize(wgpu::Backends::VULKAN)) {
                Ok(accelerator) => Ok(accelerator),
                Err(_) => pollster::block_on(Self::initialize(wgpu::Backends::DX12)),
            };
        }
        #[cfg(target_os = "linux")]
        let backends: wgpu::Backends = wgpu::Backends::VULKAN;
        #[cfg(target_os = "macos")]
        let backends: wgpu::Backends = wgpu::Backends::METAL;
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        return Err(AcceleratorError::UnsupportedPlatform);
        pollster::block_on(Self::initialize(backends))
    }

    /// Initializes WGPU resources for the desired backend
    async fn initialize(backends: wgpu::Backends) -> Result<Self, AcceleratorError> {
        let mut instance_descriptor: wgpu::InstanceDescriptor =
            wgpu::InstanceDescriptor::new_without_display_handle();
        instance_descriptor.backends = backends;
        let instance: wgpu::Instance = wgpu::Instance::new(instance_descriptor);
        let adapter: wgpu::Adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            }).await.map_err(|error| match backends {
                wgpu::Backends::DX12 => AcceleratorError::Dx12RequestAdapter(error),
                wgpu::Backends::METAL => AcceleratorError::MetalRequestAdapter(error),
                _ => AcceleratorError::VulkanRequestAdapter(error),
            })?;
        let (device, queue): (wgpu::Device, wgpu::Queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default()).await
            .map_err(|error| match backends {
                wgpu::Backends::DX12 => AcceleratorError::Dx12RequestDevice(error),
                wgpu::Backends::METAL => AcceleratorError::MetalRequestDevice(error),
                _ => AcceleratorError::VulkanRequestDevice(error),
            })?;
        Ok(Self { instance, adapter, device, queue })
    }

    /// Returns the WGPU instance that owns this context
    pub const fn instance(&self) -> &wgpu::Instance { &self.instance }

    /// Returns the selected WGPU adapter
    pub const fn adapter(&self) -> &wgpu::Adapter { &self.adapter }

    /// Returns the device used to create graphics and compute resources
    pub const fn device(&self) -> &wgpu::Device { &self.device }

    /// Returns the queue used to submit graphics and compute work
    pub const fn queue(&self) -> &wgpu::Queue { &self.queue }

}
