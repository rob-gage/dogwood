// Copyright Rob Gage 2026

use super::MaterialIdentifier;

/// A reference to a potentially unresolved material
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaterialReference {
    Material(MaterialIdentifier),
    Tag(String),
}
