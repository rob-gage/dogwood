// Copyright Rob Gage 2026

use super::AcceleratorBuffer;
use std::mem::size_of;

/// A graphics accelerator
pub struct Accelerator {
    /// The `Accelerator`'s `wgpu::Adapter`
    wgpu_adapter: wgpu::Adapter,
    /// The `Accelerator`'s `wgpu::Device`
    wgpu_device: wgpu::Device,
    /// The `Accelerator`'s wgpu::Queue`
    wgpu_queue: wgpu::Queue,
}

impl Accelerator {

    /// Allocates an `AcceleratorBuffer`
    pub fn allocate<T>(&self, size: usize) -> AcceleratorBuffer {
        AcceleratorBuffer(self.wgpu_device.create_buffer(
            &wgpu::BufferDescriptor {
                label: None,
                size: (size as u64).into(),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        ))
    }

    /// Returns a reference to the `Accelerator`'s `wgpu::Adapter`
    pub const fn wgpu_adapter(&self) -> &wgpu::Adapter { &self.wgpu_adapter }

    /// Returns a reference the `Accelerator`'s `wgpu::Device`
    pub const fn wgpu_device(&self) -> &wgpu::Device { &self.wgpu_device }

    /// Returns a reference to the `Accelerator`'s `wgpu::Device`
    pub const fn wgpu_queue(&self) -> &wgpu::Queue { &self.wgpu_queue }

}
