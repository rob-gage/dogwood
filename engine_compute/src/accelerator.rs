// Copyright Rob Gage 2026

use super::AcceleratorBuffer;
use std::{
    error::Error,
    mem::size_of,
};

/// A WGPU accelerator shared by graphics and compute workloads.
pub struct Accelerator {
    /// The `Accelerator`'s `wgpu::Instance`
    wgpu_instance: wgpu::Instance,
    /// The `Accelerator`'s `wgpu::Adapter`
    wgpu_adapter: wgpu::Adapter,
    /// The `Accelerator`'s `wgpu::Device`
    wgpu_device: wgpu::Device,
    /// The `Accelerator`'s wgpu::Queue`
    wgpu_queue: wgpu::Queue,
}

impl Accelerator {

    /// Creates an `Accelerator`
    pub async fn new() -> Result<Self, Box<dyn Error>> {
        let instance: wgpu::Instance = wgpu::Instance::default();
        let adapter: wgpu::Adapter = instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: None,
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            },
        ).await?;
        let (device, queue): (wgpu::Device, wgpu::Queue) =
            adapter.request_device(&wgpu::DeviceDescriptor::default()).await?;

        Ok(Self {
            wgpu_instance: instance,
            wgpu_adapter: adapter,
            wgpu_device: device,
            wgpu_queue: queue,
        })
    }

    /// Allocates an `AcceleratorBuffer`
    pub fn allocate<T>(&self, size: usize) -> AcceleratorBuffer {
        AcceleratorBuffer(self.wgpu_device.create_buffer(
            &wgpu::BufferDescriptor {
                label: None,
                size: (size * size_of::<T>()) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST |
                    wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            },
        ))
    }

    /// Polls the accelerator for completed work
    pub fn poll(&self) -> Result<(), wgpu::PollError> {
        self.wgpu_device.poll(wgpu::PollType::Poll)?;
        Ok(())
    }

    /// Returns a reference to the `Accelerator`'s `wgpu::Instance`
    pub const fn wgpu_instance(&self) -> &wgpu::Instance { &self.wgpu_instance }

    /// Returns a reference to the `Accelerator`'s `wgpu::Adapter`
    pub const fn wgpu_adapter(&self) -> &wgpu::Adapter { &self.wgpu_adapter }

    /// Returns a reference the `Accelerator`'s `wgpu::Device`
    pub const fn wgpu_device(&self) -> &wgpu::Device { &self.wgpu_device }

    /// Returns a reference to the `Accelerator`'s `wgpu::Device`
    pub const fn wgpu_queue(&self) -> &wgpu::Queue { &self.wgpu_queue }

}
