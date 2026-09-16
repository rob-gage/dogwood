// Copyright Rob Gage 2026

use super::{Material, MaterialIdentifier, MaterialRegistry, MaterialThermalProperties};
use std::collections::BTreeMap;

/// Declarative authoring path for compiled material metadata.
pub struct MaterialRegistryBuilder {
    registry: MaterialRegistry,
    thermal: BTreeMap<MaterialIdentifier, MaterialThermalProperties>,
    tags: BTreeMap<String, Vec<MaterialIdentifier>>,
}

impl MaterialRegistryBuilder {
    pub const fn new() -> Self {
        Self {
            registry: MaterialRegistry::new(),
            thermal: BTreeMap::new(),
            tags: BTreeMap::new(),
        }
    }
    pub fn register(&mut self, material: Material) -> MaterialIdentifier {
        self.registry.register(material)
    }
    pub fn set_thermal(
        &mut self,
        identifier: MaterialIdentifier,
        properties: MaterialThermalProperties,
    ) -> Result<(), String> {
        if self.registry.get(identifier).is_none() {
            return Err("Thermal metadata references an unregistered material".into());
        }
        self.thermal.insert(identifier, properties);
        Ok(())
    }
    pub fn tag(
        &mut self,
        identifier: MaterialIdentifier,
        tag: impl Into<String>,
    ) -> Result<(), String> {
        if self.registry.get(identifier).is_none() {
            return Err("Tag references an unregistered material".into());
        }
        let members = self.tags.entry(tag.into()).or_default();
        if !members.contains(&identifier) {
            members.push(identifier);
        }
        Ok(())
    }
    pub fn compile(mut self) -> Result<MaterialRegistry, String> {
        self.registry
            .set_compiled_metadata(self.thermal, self.tags)?;
        Ok(self.registry)
    }
}
