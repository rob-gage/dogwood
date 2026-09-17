// Copyright Rob Gage 2026

use super::{
    Material, MaterialIdentifier, MaterialReaction, MaterialRegistry, MaterialThermalProperties,
};
use std::collections::BTreeMap;

/// Declarative authoring path for compiled material metadata.
pub struct MaterialRegistryBuilder {
    registry: MaterialRegistry,
    thermal: BTreeMap<MaterialIdentifier, MaterialThermalProperties>,
    tags: BTreeMap<String, Vec<MaterialIdentifier>>,
    reactions: Vec<MaterialReaction>,
}

impl Default for MaterialRegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl MaterialRegistryBuilder {
    pub const fn new() -> Self {
        Self {
            registry: MaterialRegistry::new(),
            thermal: BTreeMap::new(),
            tags: BTreeMap::new(),
            reactions: Vec::new(),
        }
    }
    /// Registers a generic reaction. Identifiers and tags are checked only once
    /// all material/tag declarations have been supplied, at `compile` time.
    pub fn register_reaction(&mut self, reaction: MaterialReaction) {
        self.reactions.push(reaction);
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
            .set_compiled_metadata(self.thermal, self.tags, self.reactions)?;
        Ok(self.registry)
    }
}
