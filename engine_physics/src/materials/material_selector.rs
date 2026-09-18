// Copyright Rob Gage 2026

use super::MaterialIdentifier;

/// A reference to a potentially unresolved material
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaterialReference {
    /// Matches one exact material identifier.
    Material(MaterialIdentifier),
    /// Matches every material registered under this tag.
    Tag(String),
}
