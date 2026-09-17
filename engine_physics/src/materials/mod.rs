// Copyright Rob Gage 2026

//! Material definitions, registries, reactions, and derived tables.

mod compiled_material_reaction;
mod compiled_material_reaction_product;
mod compiled_material_reaction_reactant;
mod material;
mod material_form;
mod material_identifier;
mod material_reaction;
mod material_reaction_product;
mod material_reaction_reactant;
mod material_registry;
mod material_registry_builder;
mod material_selector;
mod material_table;
mod material_thermal_properties;
mod material_thermal_transition;

#[cfg(test)]
pub(crate) mod tests;

pub use compiled_material_reaction::{CompiledMaterialReaction, compiled_selector_matches};
pub use compiled_material_reaction_product::CompiledMaterialReactionProduct;
pub use compiled_material_reaction_reactant::CompiledMaterialReactionReactant;
pub use material::Material;
pub use material_form::MaterialForm;
pub use material_identifier::MaterialIdentifier;
pub use material_reaction::MaterialReaction;
pub use material_reaction_product::MaterialReactionProduct;
pub use material_reaction_reactant::MaterialReactionReactant;
pub use material_registry::MaterialRegistry;
pub use material_registry_builder::MaterialRegistryBuilder;
pub use material_selector::MaterialReference;
pub(crate) use material_table::MaterialTable;
pub use material_thermal_properties::MaterialThermalProperties;
pub use material_thermal_transition::MaterialThermalTransition;
