// Copyright Rob Gage 2026

use engine::materials::MaterialIdentifier;

/// The action applied by strokes in the viewport
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum EditorTool {
    /// Fills cells with a registered cellular material
    Material(MaterialIdentifier),
    /// Empties cells, replacing any matter in them
    Eraser,
    /// Adds or removes pressure
    Pressure,
    /// Adds or removes temperature
    Heat,
    /// Places a delayed pressure explosion
    Explosion,
}
