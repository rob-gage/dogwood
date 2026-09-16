// Copyright Rob Gage 2026

use engine::physics::materials::MaterialIdentifier;

/// The editor action selected for the primary scene interaction.
#[derive(Copy, Clone)]
pub(crate) enum EditorTool {
    /// Removes cellular matter
    Eraser,
    /// Places one registered cellular material
    Material(MaterialIdentifier),
    /// Applies a radial mechanical impulse
    Impulse,
    /// Adds or removes temperature from all matter under the brush.
    Thermal,
}
