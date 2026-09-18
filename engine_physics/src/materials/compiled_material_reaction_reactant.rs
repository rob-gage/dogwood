// Copyright Rob Gage 2026

/// Numeric selector consumed by Accelerator reaction passes. Exact selectors have one
/// member; tag selectors point into `MaterialRegistry::reaction_selector_members`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompiledMaterialReactionReactant {
    /// Starting index into the registry's compiled selector-member table.
    pub member_offset: u32,
    /// Number of selector members beginning at `member_offset`.
    pub member_count: u32,
    /// IEEE-754 bits for the required amount.
    pub amount_bits: u32,
    /// Nonzero when this slot contains an authored reactant.
    pub present: u32,
}
