use crate::materials::{MaterialForm, MaterialRegistry, MaterialThermalTransition};
use engine_compute::{Accelerator, AcceleratorBuffer};

/// Immutable GPU copy of compiled material thermal metadata.
pub(crate) struct ThermalMaterialTable {
    properties: AcceleratorBuffer,
    parameters: wgpu::Buffer,
}

impl ThermalMaterialTable {
    pub(crate) fn new(accelerator: &Accelerator, registry: &MaterialRegistry) -> Self {
        // Four vec4-compatible groups (conductivity/heat capacity, cold transition,
        // hot transition, and padding) keep the WGSL record naturally 16-byte aligned.
        let records: Vec<[u32; 16]> = registry
            .iter()
            .map(|(id, _)| {
                let properties = registry.thermal_properties(id).cloned().unwrap_or_default();
                let transition = |value: Option<&MaterialThermalTransition>| {
                    value.map_or([0, 0, 0, 0], |t| {
                        [
                            t.threshold_temperature.to_bits(),
                            t.target.as_u32(),
                            t.yield_rate.to_bits(),
                            t.latent_energy.to_bits(),
                        ]
                    })
                };
                let cold = transition(properties.cold_transition.as_ref());
                let hot = transition(properties.hot_transition.as_ref());
                [
                    properties.conductivity.to_bits(),
                    properties.specific_heat_capacity.to_bits(),
                    cold[0],
                    cold[1],
                    cold[2],
                    cold[3],
                    u32::from(properties.cold_transition.is_some()),
                    0,
                    hot[0],
                    hot[1],
                    hot[2],
                    hot[3],
                    u32::from(properties.hot_transition.is_some()),
                    0,
                    0,
                    0,
                ]
            })
            .collect();
        let properties = accelerator.allocate::<[u32; 16]>(records.len().max(1));
        if !records.is_empty() {
            let bytes: Vec<u8> = records
                .iter()
                .flat_map(|record| record.iter().flat_map(|v| v.to_le_bytes()))
                .collect();
            accelerator
                .wgpu_queue()
                .write_buffer(properties.wgpu_buffer(), 0, &bytes);
        }
        let static_count = registry
            .iter()
            .filter(|(id, _)| id.form() == MaterialForm::CellularStatic)
            .count() as u32;
        let dynamic_count = registry
            .iter()
            .filter(|(id, _)| id.form() == MaterialForm::CellularDynamic)
            .count() as u32;
        let fluid_count = registry
            .iter()
            .filter(|(id, _)| id.form() == MaterialForm::Fluid)
            .count() as u32;
        let gas_count = registry
            .iter()
            .filter(|(id, _)| id.form() == MaterialForm::Gas)
            .count() as u32;
        let offsets = [
            static_count + dynamic_count + fluid_count,
            0,
            static_count,
            static_count + dynamic_count,
            gas_count,
            static_count,
            dynamic_count,
            fluid_count,
        ];
        let parameters = accelerator
            .wgpu_device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("thermal material table parameters"),
                size: 32,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        accelerator.wgpu_queue().write_buffer(
            &parameters,
            0,
            &offsets
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        Self {
            properties,
            parameters,
        }
    }

    pub(crate) const fn properties_buffer(&self) -> &AcceleratorBuffer {
        &self.properties
    }
    pub(crate) const fn parameters_buffer(&self) -> &wgpu::Buffer {
        &self.parameters
    }
}

impl Drop for ThermalMaterialTable {
    fn drop(&mut self) {
        self.properties.free();
        self.parameters.destroy();
    }
}
