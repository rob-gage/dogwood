// Copyright Rob Gage 2026

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CompiledMaterialReactionProduct {
    pub material: u32,
    pub amount_bits: u32,
    pub present: u32,
    pub _padding: u32,
}
