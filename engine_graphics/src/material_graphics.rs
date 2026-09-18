// Copyright Rob Gage 2026

use super::{MaterialAppearance, MaterialOptics};
use crate::engine_compute::{Accelerator, AcceleratorBuffer};
use std::mem::size_of;

/// Accelerator-resident appearance and derived properties for every material form.
///
/// The physics material registry owns the source values; this type owns the packed
/// buffers consumed by rendering and material simulation shaders.
pub struct MaterialGraphics {
    /// The graphics properties for static cellular materials
    pub cellular_statics: AcceleratorBuffer,
    /// The graphics properties for dynamic cellular materials
    pub cellular_dynamics: AcceleratorBuffer,
    /// The graphics properties for fluid materials
    pub fluids: AcceleratorBuffer,
    /// The graphics properties for gas materials
    pub gases: AcceleratorBuffer,
    /// Solver, contact, and physical properties for every registered fluid material
    pub fluid_properties: AcceleratorBuffer,
    /// Density, diffusivity, extinction, dissipation, and compressibility for every gas species
    pub gas_properties: AcceleratorBuffer,
}

macro_rules! material_graphics_form {
    ($name:ident) => {
        #[derive(Copy, Clone)]
        pub struct $name {
            pub appearance: MaterialAppearance,
            pub optics: MaterialOptics,
        }

        impl From<MaterialAppearance> for $name {
            fn from(appearance: MaterialAppearance) -> Self {
                Self {
                    optics: appearance.optics(),
                    appearance,
                }
            }
        }

        impl MaterialGraphicsProperty for $name {
            fn appearance(self) -> MaterialAppearance {
                self.appearance
            }
        }
    };
}

trait MaterialGraphicsProperty: Copy {
    fn appearance(self) -> MaterialAppearance;
}

material_graphics_form!(MaterialGraphicsCellularStatic);
material_graphics_form!(MaterialGraphicsCellularDynamic);
material_graphics_form!(MaterialGraphicsFluid);
material_graphics_form!(MaterialGraphicsGas);

impl MaterialGraphics {
    /// Creates graphics properties for every material form
    pub fn new(
        accelerator: &Accelerator,
        cellular_statics: Vec<MaterialGraphicsCellularStatic>,
        cellular_dynamics: Vec<MaterialGraphicsCellularDynamic>,
        fluids: Vec<MaterialGraphicsFluid>,
        gases: Vec<MaterialGraphicsGas>,
        fluid_properties: Vec<[f32; 8]>,
        gas_properties: Vec<[f32; 8]>,
    ) -> Self {
        Self {
            cellular_statics: Self::create_buffer(accelerator, cellular_statics),
            cellular_dynamics: Self::create_buffer(accelerator, cellular_dynamics),
            fluids: Self::create_buffer(accelerator, fluids),
            gases: Self::create_buffer(accelerator, gases),
            fluid_properties: Self::create_raw_buffer(accelerator, fluid_properties),
            gas_properties: Self::create_raw_buffer(accelerator, gas_properties),
        }
    }

    /// Creates an Accelerator buffer containing graphics properties for one material form
    fn create_buffer(
        accelerator: &Accelerator,
        properties: Vec<impl MaterialGraphicsProperty>,
    ) -> AcceleratorBuffer {
        let data: Vec<u32> = properties
            .into_iter()
            .flat_map(|property| property.appearance().accelerator_data())
            .collect();
        let buffer: AcceleratorBuffer = accelerator.allocate::<u32>(data.len().max(24));
        if !data.is_empty() {
            let mut bytes: Vec<u8> = Vec::with_capacity(data.len() * size_of::<u32>());
            for value in data {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            accelerator
                .wgpu_queue()
                .write_buffer(buffer.wgpu_buffer(), 0, &bytes);
        }
        buffer
    }

    fn create_raw_buffer(
        accelerator: &Accelerator,
        properties: Vec<[f32; 8]>,
    ) -> AcceleratorBuffer {
        let data: Vec<u32> = properties.into_iter().flatten().map(f32::to_bits).collect();
        let buffer: AcceleratorBuffer = accelerator.allocate::<u32>(data.len().max(8));
        if !data.is_empty() {
            let bytes: Vec<u8> = data.into_iter().flat_map(u32::to_le_bytes).collect();
            accelerator
                .wgpu_queue()
                .write_buffer(buffer.wgpu_buffer(), 0, &bytes);
        }
        buffer
    }
}
