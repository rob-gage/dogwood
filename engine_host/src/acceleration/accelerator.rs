// Copyright Rob Gage 2026

use super::AcceleratorError;

/// Shared host hardware context for graphics and compute workloads
#[derive(Clone)]
pub struct Accelerator {
    /// WGPU instance that owns the available backends
    instance: wgpu::Instance,
    /// Adapter selected for this context
    adapter: wgpu::Adapter,
    /// Device used to create GPU resources
    device: wgpu::Device,
    /// Queue used to submit GPU work
    queue: wgpu::Queue,
}

impl Accelerator {

    /// Creates a WGPU instance for the current platform
    fn create_instance() -> Result<wgpu::Instance, AcceleratorError> {
        #[cfg(target_os = "windows")]
        let backends: wgpu::Backends = wgpu::Backends::VULKAN | wgpu::Backends::DX12;
        #[cfg(target_os = "linux")]
        let backends: wgpu::Backends = wgpu::Backends::VULKAN;
        #[cfg(target_os = "macos")]
        let backends: wgpu::Backends = wgpu::Backends::METAL;
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        return Err(AcceleratorError::UnsupportedPlatform);

        let mut instance_descriptor: wgpu::InstanceDescriptor =
            wgpu::InstanceDescriptor::new_without_display_handle();
        instance_descriptor.backends = backends;
        Ok(wgpu::Instance::new(instance_descriptor))
    }

    /// Initializes the adapter, device, and queue
    async fn initialize(
        instance: &wgpu::Instance,
    ) -> Result<(wgpu::Adapter, wgpu::Device, wgpu::Queue), AcceleratorError> {
        let adapter: wgpu::Adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .map_err(AcceleratorError::RequestAdapter)?;
        // dynamic cell kinematics binds more storage buffers than the default eight, a limit
        // every supported desktop and integrated GPU exceeds
        let (device, queue): (wgpu::Device, wgpu::Queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_limits: wgpu::Limits {
                    max_storage_buffers_per_shader_stage: 16,
                    ..wgpu::Limits::default()
                },
                ..wgpu::DeviceDescriptor::default()
            })
            .await
            .map_err(AcceleratorError::RequestDevice)?;
        Ok((adapter, device, queue))
    }

    /// Returns the WGPU instance used by this context
    pub const fn instance(&self) -> &wgpu::Instance { &self.instance }

    /// Returns the selected WGPU adapter
    pub const fn adapter(&self) -> &wgpu::Adapter { &self.adapter }

    /// Returns the device used to create graphics and compute resources
    pub const fn device(&self) -> &wgpu::Device { &self.device }

    /// Returns the queue used to submit graphics and compute work
    pub const fn queue(&self) -> &wgpu::Queue { &self.queue }

}

impl Default for Accelerator {

    /// Creates a shared graphics and compute device context, panicking if no device is available
    fn default() -> Self {
        let instance: wgpu::Instance = Self::create_instance()
            .unwrap_or_else(|error: AcceleratorError| panic!("{error}"));
        let (adapter, device, queue): (wgpu::Adapter, wgpu::Device, wgpu::Queue) =
            pollster::block_on(Self::initialize(&instance))
                .unwrap_or_else(|error: AcceleratorError| panic!("{error}"));
        Self { instance, adapter, device, queue }
    }

}
