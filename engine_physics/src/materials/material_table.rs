// Copyright Rob Gage 2026

use super::{CompiledMaterialReaction, MaterialForm, MaterialRegistry, MaterialThermalTransition};
use engine_compute::{Accelerator, AcceleratorBuffer};
use engine_graphics::MaterialGraphics;

/// All immutable derived material properties uploaded to the Accelerator.
pub(crate) struct MaterialTable {
    material_graphics: MaterialGraphics,
    thermal_properties: AcceleratorBuffer,
    thermal_parameters: wgpu::Buffer,
    reaction_records: AcceleratorBuffer,
    reaction_selector_members: AcceleratorBuffer,
}

impl MaterialTable {
    pub(crate) fn new(accelerator: &Accelerator, registry: &MaterialRegistry) -> Self {
        let material_graphics = registry.build_material_graphics(accelerator);
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
        let (reaction_records, reaction_selector_members) =
            Self::create_reaction_buffers(accelerator, registry);
        Self {
            material_graphics,
            thermal_properties: properties,
            thermal_parameters: parameters,
            reaction_records,
            reaction_selector_members,
        }
    }

    pub(crate) const fn properties_buffer(&self) -> &AcceleratorBuffer {
        &self.thermal_properties
    }

    pub(crate) const fn graphics(&self) -> &MaterialGraphics {
        &self.material_graphics
    }
    pub(crate) const fn parameters_buffer(&self) -> &wgpu::Buffer {
        &self.thermal_parameters
    }
}

impl Drop for MaterialTable {
    fn drop(&mut self) {
        self.thermal_properties.free();
        self.thermal_parameters.destroy();
        self.reaction_records.free();
        self.reaction_selector_members.free();
    }
}

impl MaterialTable {
    fn create_reaction_buffers(
        accelerator: &Accelerator,
        materials: &MaterialRegistry,
    ) -> (AcceleratorBuffer, AcceleratorBuffer) {
        let records = accelerator.allocate::<[u32; CompiledMaterialReaction::WORD_COUNT]>(
            materials.reactions().len().max(1),
        );
        let selector_members =
            accelerator.allocate::<u32>(materials.reaction_selector_members().len().max(1));
        let encoded: Vec<[u32; CompiledMaterialReaction::WORD_COUNT]> = materials
            .reactions()
            .iter()
            .map(|rule| {
                let environment_flags = u32::from(rule.minimum_temperature.is_finite())
                    | (u32::from(rule.maximum_temperature.is_finite()) << 1)
                    | (u32::from(rule.minimum_pressure.is_finite()) << 2)
                    | (u32::from(rule.maximum_pressure.is_finite()) << 3)
                    | (u32::from(rule.minimum_air.is_finite()) << 4)
                    | (u32::from(rule.maximum_air.is_finite()) << 5);
                [
                    rule.reactants[0].member_offset,
                    rule.reactants[0].member_count,
                    rule.reactants[0].amount_bits,
                    rule.reactants[0].present,
                    rule.reactants[1].member_offset,
                    rule.reactants[1].member_count,
                    rule.reactants[1].amount_bits,
                    rule.reactants[1].present,
                    rule.products[0].material,
                    rule.products[0].amount_bits,
                    rule.products[0].present,
                    environment_flags,
                    rule.products[1].material,
                    rule.products[1].amount_bits,
                    rule.products[1].present,
                    0,
                    if rule.minimum_temperature.is_nan() {
                        f32::NEG_INFINITY
                    } else {
                        rule.minimum_temperature
                    }
                    .to_bits(),
                    if rule.maximum_temperature.is_nan() {
                        f32::INFINITY
                    } else {
                        rule.maximum_temperature
                    }
                    .to_bits(),
                    if rule.minimum_pressure.is_nan() {
                        f32::NEG_INFINITY
                    } else {
                        rule.minimum_pressure
                    }
                    .to_bits(),
                    if rule.maximum_pressure.is_nan() {
                        f32::INFINITY
                    } else {
                        rule.maximum_pressure
                    }
                    .to_bits(),
                    if rule.minimum_air.is_nan() {
                        f32::NEG_INFINITY
                    } else {
                        rule.minimum_air
                    }
                    .to_bits(),
                    if rule.maximum_air.is_nan() {
                        f32::INFINITY
                    } else {
                        rule.maximum_air
                    }
                    .to_bits(),
                    rule.maximum_extent_per_tick.to_bits(),
                    rule.thermal_energy.to_bits(),
                    rule.pressure_output.to_bits(),
                    rule.priority as u32,
                    rule.authoring_order,
                ]
            })
            .collect();
        if !encoded.is_empty() {
            accelerator.wgpu_queue().write_buffer(
                records.wgpu_buffer(),
                0,
                &encoded
                    .iter()
                    .flat_map(|record| record.iter().flat_map(|word| word.to_le_bytes()))
                    .collect::<Vec<_>>(),
            );
        }
        let members: Vec<u32> = materials
            .reaction_selector_members()
            .iter()
            .map(|id| id.as_u32())
            .collect();
        if !members.is_empty() {
            accelerator.wgpu_queue().write_buffer(
                selector_members.wgpu_buffer(),
                0,
                &members
                    .iter()
                    .flat_map(|word| word.to_le_bytes())
                    .collect::<Vec<_>>(),
            );
        }
        (records, selector_members)
    }
    pub const fn records_buffer(&self) -> &AcceleratorBuffer {
        &self.reaction_records
    }
    pub const fn selector_members_buffer(&self) -> &AcceleratorBuffer {
        &self.reaction_selector_members
    }
}
