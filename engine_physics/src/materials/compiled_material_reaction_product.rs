// Copyright Rob Gage 2026

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CompiledMaterialReactionProduct {
    /// Packed material identifier produced by the reaction.
    pub material: u32,
    /// IEEE-754 bits for the produced amount.
    pub amount_bits: u32,
    /// Nonzero when this slot contains an authored product.
    pub present: u32,
    /// Alignment padding retained by the Accelerator record layout.
    pub _padding: u32,
}
