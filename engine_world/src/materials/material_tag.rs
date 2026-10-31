// Copyright Rob Gage 2026

/// A tag reactions can select a material by, with named values its reactions can read
pub struct MaterialTag {
    /// Exact, case-sensitive tag
    pub name: String,
    /// Named values a reaction reads as `tag.parameter`
    pub parameters: Vec<(String, f32)>,
}

impl MaterialTag {

    /// Creates a tag without parameters
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), parameters: Vec::new() }
    }

    /// Sets a named value of the tag, replacing an earlier value of the same name
    pub fn with_parameter(mut self, name: impl Into<String>, value: f32) -> Self {
        let name: String = name.into();
        self.parameters.retain(|(existing, _)| *existing != name);
        self.parameters.push((name, value));
        self
    }

}
