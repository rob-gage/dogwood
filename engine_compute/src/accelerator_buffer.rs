// Copyright Rob Gage 2026

/// A buffer of data stored on an `Accelerator`
pub struct AcceleratorBuffer(pub(crate) wgpu::Buffer);

impl AcceleratorBuffer {
    /// Returns the `AcceleratorBuffer` as a `&wgpu::Buffer`
    pub const fn wgpu_buffer(&self) -> &wgpu::Buffer {
        &self.0
    }

    /// Frees the buffer
    pub fn free(&self) {
        self.0.destroy();
    }
}
