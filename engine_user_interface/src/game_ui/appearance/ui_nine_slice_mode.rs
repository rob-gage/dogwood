// Copyright Rob Gage 2026

/// Selects stretching or bounded tiling for a nine-slice region
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiNineSliceMode {
    /// Stretch a source region once
    #[default]
    Stretch,
    /// Repeat a source region and crop the final tile to its cell
    Tile,
}
