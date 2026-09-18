// Copyright Rob Gage 2026

use super::CompiledMaterialReactionProduct;
use super::CompiledMaterialReactionReactant;
use super::MaterialIdentifier;

/// Fixed-stride numeric record. Optional bounds use NaN as an internal Accelerator-only
/// sentinel; authoring validation rejects all non-finite authored values.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CompiledMaterialReaction {
    /// Reactants consumed by this reaction, including unused slots.
    pub reactants: [CompiledMaterialReactionReactant; 2],
    /// Products emitted by this reaction, including unused slots.
    pub products: [CompiledMaterialReactionProduct; 2],
    /// Inclusive lower temperature bound, or the internal unbounded sentinel.
    pub minimum_temperature: f32,
    /// Inclusive upper temperature bound, or the internal unbounded sentinel.
    pub maximum_temperature: f32,
    /// Inclusive lower pressure bound, or the internal unbounded sentinel.
    pub minimum_pressure: f32,
    /// Inclusive upper pressure bound, or the internal unbounded sentinel.
    pub maximum_pressure: f32,
    /// Inclusive lower air bound, or the internal unbounded sentinel.
    pub minimum_air: f32,
    /// Inclusive upper air bound, or the internal unbounded sentinel.
    pub maximum_air: f32,
    /// Maximum reaction extent permitted during one fixed tick.
    pub maximum_extent_per_tick: f32,
    /// Thermal energy emitted per unit of reaction extent.
    pub thermal_energy: f32,
    /// Pressure output emitted per unit of reaction extent.
    pub pressure_output: f32,
    /// Authoring priority used during reaction arbitration.
    pub priority: i32,
    /// Stable authoring order used to break equal-priority ties.
    pub authoring_order: u32,
}

impl CompiledMaterialReaction {
    /// Number of 32-bit words in the fixed-stride Accelerator record.
    pub const WORD_COUNT: usize = 27;
    /// Size in bytes of the fixed-stride Accelerator record.
    pub const BYTE_SIZE: u64 = (Self::WORD_COUNT * std::mem::size_of::<u32>()) as u64;
}

/// Returns whether a compiled selector slot matches `material`. This mirrors
/// the bounded linear Accelerator selector scan; authoring tags are never matched by
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
