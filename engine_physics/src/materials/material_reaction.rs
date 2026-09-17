// Copyright Rob Gage 2026

use super::MaterialIdentifier;

/// A material matcher used while authoring a reaction. Tags are resolved when the
/// registry is compiled; shaders never see tag strings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaterialSelector {
    Material(MaterialIdentifier),
    Tag(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialReactionReactant {
    pub selector: MaterialSelector,
    pub amount: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialReactionProduct {
    pub material: MaterialIdentifier,
    pub amount: f32,
}

/// Declarative, form-independent reaction rule. A reaction is deliberately
/// limited to two inputs and two outputs so its compiled GPU representation is
/// fixed stride.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialReaction {
    pub reactants: [Option<MaterialReactionReactant>; 2],
    pub products: [Option<MaterialReactionProduct>; 2],
    pub minimum_temperature: Option<f32>,
    pub maximum_temperature: Option<f32>,
    pub minimum_pressure: Option<f32>,
    pub maximum_pressure: Option<f32>,
    pub minimum_air: Option<f32>,
    pub maximum_air: Option<f32>,
    pub maximum_extent_per_tick: f32,
    pub thermal_energy: f32,
    pub pressure_output: f32,
    pub priority: i32,
}

impl Default for MaterialReaction {
    fn default() -> Self {
        Self {
            reactants: [None, None],
            products: [None, None],
            minimum_temperature: None,
            maximum_temperature: None,
            minimum_pressure: None,
            maximum_pressure: None,
            minimum_air: None,
            maximum_air: None,
            maximum_extent_per_tick: 1.0,
            thermal_energy: 0.0,
            pressure_output: 0.0,
            priority: 0,
        }
    }
}

/// Numeric selector consumed by GPU reaction passes. Exact selectors have one
/// member; tag selectors point into `MaterialRegistry::reaction_selector_members`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompiledMaterialReactionReactant {
    pub member_offset: u32,
    pub member_count: u32,
    pub amount_bits: u32,
    pub present: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CompiledMaterialReactionProduct {
    pub material: u32,
    pub amount_bits: u32,
    pub present: u32,
    pub _padding: u32,
}

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
