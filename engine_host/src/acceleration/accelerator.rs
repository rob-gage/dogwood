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
    _pipelines: std::sync::Arc<super::pipelines::PipelineStore>,
}

impl Accelerator {

    /// Creates a WGPU instance for the current platform
    fn create_instance() -> Result<wgpu::Instance, AcceleratorError> {
        #[cfg(target_os = "windows")]
        let backends: wgpu::Backends = wgpu::Backends::DX12;
        #[cfg(target_os = "linux")]
        let backends: wgpu::Backends = wgpu::Backends::VULKAN;
        #[cfg(target_os = "macos")]
        let backends: wgpu::Backends = wgpu::Backends::METAL;
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        let backends: wgpu::Backends = wgpu::Backends::empty();
        if backends.is_empty() { return Err(AcceleratorError::UnsupportedPlatform); }

        #[cfg(not(target_os = "windows"))]
        let instance_descriptor = {
            let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
            descriptor.backends = backends;
            descriptor
        };
        #[cfg(target_os = "windows")]
        let instance_descriptor = {
            let path = std::env::current_exe()
                .map_err(|e| AcceleratorError::ShaderCompiler(e.to_string()))?
                .with_file_name("dxcompiler.dll");
            if !path.is_file() {
                return Err(AcceleratorError::ShaderCompiler(format!(
                    "Missing bundled DXC: {}. Extract the complete Windows package.", path.display()
                )));
            }
            eprintln!("Dogwood GPU: DirectX 12, bundled DXC, shader optimizations enabled");
            windows_descriptor(path.to_string_lossy().into_owned())
        };
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
        eprintln!("Dogwood GPU: {} ({:?})",adapter.get_info().name,adapter.get_info().backend);
        // Metal shares buffer slots between storage, uniforms, and vertex buffers, so the storage
        // budget stays well below its 31 slots: compute stages bind at most a few uniforms beside
        // their storage buffers.
        let required_limits = wgpu::Limits {
            max_storage_buffers_per_shader_stage: 24,
            // the pressure step keeps the halo of a block of 2x2 cellular tiles per workgroup
            max_compute_workgroup_storage_size: 32768,
            ..wgpu::Limits::default()
        };
        let mut unsupported_limit = None;
        required_limits.check_limits_with_fail_fn(&adapter.limits(), true, |name, required, supported| {
            unsupported_limit = Some(AcceleratorError::UnsupportedLimit {
                adapter: format!("{} ({:?})", adapter.get_info().name, adapter.get_info().backend),
                name,
                required,
                supported,
            });
        });
        if let Some(error) = unsupported_limit { return Err(error); }
        // the cellular tile and fluid particle stages bind more than twenty storage buffers
        let (device, queue): (wgpu::Device, wgpu::Queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_limits,
                ..wgpu::DeviceDescriptor::default()
            })
            .await
            .map_err(AcceleratorError::RequestDevice)?;
        Ok((adapter, device, queue))
    }

    /// Creates a GPU context, returning backend, adapter, or device failures
    pub fn new() -> Result<Self, AcceleratorError> {
        super::performance::configure();
        let instance = Self::create_instance()?;
        let (adapter, device, queue) = pollster::block_on(Self::initialize(&instance))?;
        let pipelines = std::sync::Arc::new(super::pipelines::PipelineStore::default());
        super::pipelines::register(&device, &pipelines);
        Ok(Self { instance, adapter, device, queue, _pipelines: pipelines })
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
        Self::new().unwrap_or_else(|error| panic!("{error}"))
    }

}

/// DXC must be explicit: Auto can silently fall back to FXC. DEBUG makes wgpu pass
/// -Od to DXC even when the Rust executable itself was compiled with optimizations.
#[cfg(any(target_os = "windows", test))]
fn windows_descriptor(dxc_path: String) -> wgpu::InstanceDescriptor {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::DX12;
    descriptor.backend_options.dx12.shader_compiler = wgpu::Dx12Compiler::DynamicDxc { dxc_path };
    descriptor.flags.remove(wgpu::InstanceFlags::DEBUG);
    if !cfg!(debug_assertions) {
        descriptor.flags = wgpu::InstanceFlags::empty();
    }
    descriptor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_requires_dx12_and_bundled_dxc_with_shader_optimization() {
        let descriptor = windows_descriptor("C:/game/dxcompiler.dll".into());
        assert_eq!(descriptor.backends, wgpu::Backends::DX12);
        assert!(!descriptor.flags.contains(wgpu::InstanceFlags::DEBUG));
        match descriptor.backend_options.dx12.shader_compiler {
            wgpu::Dx12Compiler::DynamicDxc { dxc_path } => assert_eq!(dxc_path, "C:/game/dxcompiler.dll"),
            other => panic!("DXC must never silently fall back to FXC: {other:?}"),
        }
    }
}
