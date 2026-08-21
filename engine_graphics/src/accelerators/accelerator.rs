// Copyright Rob Gage 2026

/// A graphics accelerator
pub struct Accelerator {
    wgpu_adapter: wgpu::Adapter,
}

impl Accelerator {

    /// Returns the `Accelerator` as a `&wgpu::Adapter`
    pub const fn as_wgpu_adapter(&self) -> &wgpu::Adapter { &self.wgpu_adapter }

}
