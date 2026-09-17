// Copyright Rob Gage 2026

/// A storage buffer allocated from an [`Accelerator`](crate::Accelerator).
pub struct AcceleratorBuffer(pub(crate) wgpu::Buffer);

impl AcceleratorBuffer {
    /// Returns the underlying WGPU buffer for bind groups and copies.
    pub const fn wgpu_buffer(&self) -> &wgpu::Buffer {
        &self.0
    }

    /// Releases the underlying WGPU allocation.
    pub fn free(&self) {
        self.0.destroy();
    }
}
