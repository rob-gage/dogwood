// Copyright Rob Gage 2026

use crate::materials::{CompiledMaterialReaction, MaterialRegistry};
use engine_compute::{Accelerator, AcceleratorBuffer};

/// Immutable reaction metadata uploaded once with the material registry. This
/// is intentionally separate from mutable simulation buffers: a chemistry tick
/// always discovers from one stable logical snapshot.
pub(crate) struct ReactionMaterialTable {
    records: AcceleratorBuffer,
    selector_members: AcceleratorBuffer,
}

impl ReactionMaterialTable {
    pub fn new(accelerator: &Accelerator, materials: &MaterialRegistry) -> Self {
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
        Self {
            records,
            selector_members,
        }
    }
    pub const fn records_buffer(&self) -> &AcceleratorBuffer {
        &self.records
    }
    pub const fn selector_members_buffer(&self) -> &AcceleratorBuffer {
        &self.selector_members
    }
}
