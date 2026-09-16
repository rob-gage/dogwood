// Copyright Rob Gage 2026

mod material;
mod material_form;
mod material_identifier;
mod material_registry;
mod material_registry_builder;
mod material_thermal_properties;

pub use material::Material;
pub use material_form::MaterialForm;
pub use material_identifier::MaterialIdentifier;
pub use material_registry::MaterialRegistry;
pub use material_registry_builder::MaterialRegistryBuilder;
pub use material_thermal_properties::{MaterialThermalProperties, MaterialThermalTransition};
