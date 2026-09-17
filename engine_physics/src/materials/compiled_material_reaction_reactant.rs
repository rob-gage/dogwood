// Copyright Rob Gage 2026

/// Numeric selector consumed by Accelerator reaction passes. Exact selectors have one
/// member; tag selectors point into `MaterialRegistry::reaction_selector_members`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompiledMaterialReactionReactant {
    pub member_offset: u32,
    pub member_count: u32,
    pub amount_bits: u32,
    pub present: u32,
}
