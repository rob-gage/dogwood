// Copyright Rob Gage 2026

use super::CompiledMaterialReactionProduct;
use super::CompiledMaterialReactionReactant;
use super::MaterialIdentifier;

/// Fixed-stride numeric record. Optional bounds use NaN as an internal GPU-only
/// sentinel; authoring validation rejects all non-finite authored values.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CompiledMaterialReaction {
    pub reactants: [CompiledMaterialReactionReactant; 2],
    pub products: [CompiledMaterialReactionProduct; 2],
    pub minimum_temperature: f32,
    pub maximum_temperature: f32,
    pub minimum_pressure: f32,
    pub maximum_pressure: f32,
    pub minimum_air: f32,
    pub maximum_air: f32,
    pub maximum_extent_per_tick: f32,
    pub thermal_energy: f32,
    pub pressure_output: f32,
    pub priority: i32,
    pub authoring_order: u32,
}

impl CompiledMaterialReaction {
    pub const WORD_COUNT: usize = 27;
    pub const BYTE_SIZE: u64 = (Self::WORD_COUNT * std::mem::size_of::<u32>()) as u64;
}

/// Returns whether a compiled selector slot matches `material`. This mirrors
/// the bounded linear GPU selector scan; authoring tags are never matched by
/// string after registry compilation.
pub fn compiled_selector_matches(
    reactant: CompiledMaterialReactionReactant,
    selector_members: &[MaterialIdentifier],
    material: MaterialIdentifier,
) -> bool {
    reactant.present != 0
        && (reactant.member_offset as usize)
            .checked_add(reactant.member_count as usize)
            .and_then(|end| selector_members.get(reactant.member_offset as usize..end))
            .is_some_and(|members| members.contains(&material))
}
