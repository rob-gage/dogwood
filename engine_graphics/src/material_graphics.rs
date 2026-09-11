// Copyright Rob Gage 2026

use super::MaterialAppearance;
use engine_compute::{
    Accelerator,
    AcceleratorBuffer,
};
use std::mem::size_of;

/// Graphics properties for every material form
pub struct MaterialGraphics {
    /// The graphics properties for static cellular materials
    pub cellular_statics: AcceleratorBuffer,
    /// The graphics properties for dynamic cellular materials
    pub cellular_dynamics: AcceleratorBuffer,
    /// The graphics properties for fluid materials
    pub fluids: AcceleratorBuffer,
}

impl MaterialGraphics {

    /// Creates graphics properties for every material form
    pub fn new(
        accelerator: &Accelerator,
        cellular_statics: Vec<MaterialAppearance>,
        cellular_dynamics: Vec<MaterialAppearance>,
        fluids: Vec<MaterialAppearance>,
    ) -> Self {
        Self {
            cellular_statics: Self::create_buffer(accelerator, cellular_statics),
            cellular_dynamics: Self::create_buffer(accelerator, cellular_dynamics),
            fluids: Self::create_buffer(accelerator, fluids),
        }
    }

    /// Creates a GPU buffer containing graphics properties for one material form
    fn create_buffer(
        accelerator: &Accelerator,
        properties: Vec<MaterialAppearance>,
    ) -> AcceleratorBuffer {
        let data: Vec<u32> = properties.into_iter().flat_map(
            MaterialAppearance::accelerator_data
        ).collect();
        // Keep empty material-form buffers large enough for one WGSL element.
        let buffer: AcceleratorBuffer = accelerator.allocate::<u32>(data.len().max(16));
        if !data.is_empty() {
            let mut bytes: Vec<u8> = Vec::with_capacity(data.len() * size_of::<u32>());
            for value in data { bytes.extend_from_slice(&value.to_le_bytes()); }
            accelerator.wgpu_queue().write_buffer(buffer.wgpu_buffer(), 0, &bytes);
        }
        buffer
    }

}
