// Copyright Rob Gage 2026

use super::{AcceleratorBuffer, accelerator_timing::AcceleratorTiming};
use std::{error::Error, mem::size_of};

/// An Accelerator shared by graphics and compute workloads.
pub struct Accelerator {
    /// The `Accelerator`'s `wgpu::Instance`
    wgpu_instance: wgpu::Instance,
    /// The `Accelerator`'s `wgpu::Adapter`
    wgpu_adapter: wgpu::Adapter,
    /// The `Accelerator`'s `wgpu::Device`
    wgpu_device: wgpu::Device,
    /// The `Accelerator`'s wgpu::Queue`
    wgpu_queue: wgpu::Queue,
    /// Debug-only Accelerator pass timestamp collection
    accelerator_timing: AcceleratorTiming,
}

impl Accelerator {
    /// Creates the shared graphics and compute device used by Dogwood.
    ///
    /// Requests the platform's high-performance adapter, which normally selects
    /// a discrete GPU on hybrid systems. Startup warnings identify CPU/software
    /// adapters and integrated GPUs when the platform does not honor that hint.
    pub fn new() -> Result<Self, Box<dyn Error>> {
        #[cfg(target_os = "windows")]
        let instance: wgpu::Instance = {
            // Prefer the Windows SDK redistributable during development. Release
            // builds may instead ship `dxcompiler.dll` beside the executable.
            let dxc_path: std::path::PathBuf = std::env::var_os("ProgramFiles(x86)")
                .map(std::path::PathBuf::from)
                .map(|directory| {
                    directory
                        .join("Windows Kits")
                        .join("10")
                        .join("Redist")
                        .join("D3D")
                        .join("x64")
                        .join("dxcompiler.dll")
                })
                .filter(|path| path.is_file())
                .unwrap_or_else(|| std::path::PathBuf::from("dxcompiler.dll"));
            let mut descriptor: wgpu::InstanceDescriptor =
                wgpu::InstanceDescriptor::new_without_display_handle();
            descriptor.backends = wgpu::Backends::DX12;
            descriptor.backend_options.dx12.shader_compiler = wgpu::Dx12Compiler::DynamicDxc {
                dxc_path: dxc_path.to_string_lossy().into_owned(),
            };
            wgpu::Instance::new(descriptor)
        };
        #[cfg(not(target_os = "windows"))]
        let instance: wgpu::Instance = wgpu::Instance::default();
        let adapter: wgpu::Adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            }))?;
        let adapter_info: wgpu::AdapterInfo = adapter.get_info();
        match adapter_info.device_type {
            wgpu::DeviceType::Cpu => tracing::warn!(
                target: "dogwood_accelerator",
                adapter = %adapter_info.name,
                backend = ?adapter_info.backend,
                driver = %adapter_info.driver,
                "Accelerator selected a CPU/software renderer; graphics and compute performance will be severely degraded"
            ),
            wgpu::DeviceType::IntegratedGpu => tracing::warn!(
                target: "dogwood_accelerator",
                adapter = %adapter_info.name,
                backend = ?adapter_info.backend,
                driver = %adapter_info.driver,
                "Accelerator selected an integrated GPU despite requesting high performance; verify Windows GPU preferences and driver configuration"
            ),
            _ => {}
        }
        #[cfg(debug_assertions)]
        let mut required_features: wgpu::Features = wgpu::Features::empty();
        #[cfg(not(debug_assertions))]
        let required_features: wgpu::Features = wgpu::Features::empty();
        #[cfg(debug_assertions)]
        let timestamp_query_supported: bool =
            adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        #[cfg(debug_assertions)]
        if timestamp_query_supported {
            required_features.insert(wgpu::Features::TIMESTAMP_QUERY);
        }
        let (device, queue): (wgpu::Device, wgpu::Queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                required_features,
                required_limits: adapter.limits(),
                ..Default::default()
            }))?;
        #[cfg(not(debug_assertions))]
        let timestamp_query_supported: bool = false;
        let accelerator_timing: AcceleratorTiming =
            AcceleratorTiming::new(&device, &queue, timestamp_query_supported);
        #[cfg(debug_assertions)]
        tracing::info!(
            target: "dogwood_accelerator",
            available = timestamp_query_supported,
            "Accelerator timestamp profiling availability"
        );
        tracing::info!(
            adapter = ?adapter_info,
            "initialized graphics accelerator"
        );
        Ok(Self {
            wgpu_instance: instance,
            wgpu_adapter: adapter,
            wgpu_device: device,
            wgpu_queue: queue,
            accelerator_timing,
        })
    }

    /// Allocates storage for `size` values owned by this Accelerator.
    pub fn allocate<T>(&self, size: usize) -> AcceleratorBuffer {
        AcceleratorBuffer(self.wgpu_device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (size * size_of::<T>()) as u64,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        }))
    }

    /// Polls the Accelerator and collects completed debug timing readbacks.
    pub fn poll(&self) -> Result<(), wgpu::PollError> {
        self.wgpu_device.poll(wgpu::PollType::Poll)?;
        self.accelerator_timing.collect();
        Ok(())
    }

    /// Starts a sampled application-frame timing interval when debug tracing requests it.
    #[inline]
    pub fn accelerator_timing_begin_sample(&self) {
        #[cfg(debug_assertions)]
        {
            let _ = self.wgpu_device.poll(wgpu::PollType::Poll);
            self.accelerator_timing.collect();
            self.accelerator_timing.begin_sample();
        }
    }

    /// Begins a named compute pass with optional debug timestamp writes.
    #[inline]
    pub fn begin_compute_pass<'a>(
        &self,
        encoder: &'a mut wgpu::CommandEncoder,
        label: &str,
    ) -> wgpu::ComputePass<'a> {
        encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(label),
            timestamp_writes: self.accelerator_timing.compute_timestamp_writes(label),
        })
    }

    /// Returns optional timestamp writes for a named render pass.
    #[inline]
    pub fn render_pass_timestamp_writes(
        &self,
        label: &str,
    ) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        self.accelerator_timing.render_timestamp_writes(label)
    }

    /// Resolves the current timing sample after all measured passes have been encoded.
    #[inline]
    pub fn accelerator_timing_resolve_sample(&self, encoder: &mut wgpu::CommandEncoder) {
        self.accelerator_timing.resolve_sample(encoder);
    }

    /// Starts asynchronous mapping after the resolve command buffer has been submitted.
    #[inline]
    pub fn accelerator_timing_map_sample(&self) {
        self.accelerator_timing.map_sample();
    }

    /// Returns the underlying WGPU instance.
    pub const fn wgpu_instance(&self) -> &wgpu::Instance {
        &self.wgpu_instance
    }

    /// Returns the adapter selected for this Accelerator.
    pub const fn wgpu_adapter(&self) -> &wgpu::Adapter {
        &self.wgpu_adapter
    }

    /// Returns the underlying device used for graphics and compute work.
    pub const fn wgpu_device(&self) -> &wgpu::Device {
        &self.wgpu_device
    }

    /// Returns the queue used to submit graphics and compute work.
    pub const fn wgpu_queue(&self) -> &wgpu::Queue {
        &self.wgpu_queue
    }
}

#[cfg(test)]
mod tests {

    use super::Accelerator;

    #[cfg(debug_assertions)]
    use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

    #[cfg(debug_assertions)]
    fn initialize_test_tracing() -> Option<tracing_appender::non_blocking::WorkerGuard> {
        let filter: EnvFilter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
        let (writer, guard) = tracing_appender::non_blocking(std::io::stderr());
        let console = tracing_subscriber::fmt::layer()
            .compact()
            .with_target(true)
            .with_writer(writer);
        tracing_subscriber::registry()
            .with(filter)
            .with(console)
            .try_init()
            .ok()
            .map(|()| guard)
    }

    #[cfg(not(debug_assertions))]
    const fn initialize_test_tracing() -> Option<tracing_appender::non_blocking::WorkerGuard> {
        None
    }

    #[test]
    fn test_accelerator_timing_readback_completes_with_nonblocking_polls() {
        let _tracing_guard: Option<tracing_appender::non_blocking::WorkerGuard> =
            initialize_test_tracing();
        let accelerator: Accelerator = Accelerator::new().unwrap();
        if !accelerator.accelerator_timing.is_available() {
            return;
        }
        accelerator.accelerator_timing_begin_sample();
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Accelerator timing smoke test"),
                });
        {
            let _pass: wgpu::ComputePass<'_> =
                accelerator.begin_compute_pass(&mut encoder, "empty compute pass");
        }
        accelerator.accelerator_timing_resolve_sample(&mut encoder);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        accelerator.accelerator_timing_map_sample();
        for _ in 0..100 {
            accelerator.poll().unwrap();
            if accelerator.accelerator_timing.is_idle() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("Accelerator timing readback did not complete");
    }
}
