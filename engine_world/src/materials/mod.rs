// Copyright Rob Gage 2026

mod material;
mod material_error;
mod material_form;
mod material_identifier;
mod material_registry;
mod material_table;
mod material_transition;
mod material_properties_cellular_dynamic;
mod material_properties_cellular_static;
mod material_properties_fluid;
mod material_properties_gas;
mod material_properties_thermal;

pub use material::Material;
pub use material_error::MaterialError;
pub use material_form::MaterialForm;
pub use material_identifier::MaterialIdentifier;
pub use material_registry::MaterialRegistry;
pub use material_table::MaterialTable;
pub use material_transition::MaterialTransition;
pub use material_properties_cellular_dynamic::MaterialPropertiesCellularDynamic;
pub use material_properties_cellular_static::MaterialPropertiesCellularStatic;
pub use material_properties_fluid::MaterialPropertiesFluid;
pub use material_properties_gas::MaterialPropertiesGas;
pub use material_properties_thermal::MaterialPropertiesThermal;
