// Copyright Rob Gage 2026

/// How an `Edit` treats existing matter in its cells
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum EditMode {
    /// Fills empty cells not overlapped by an edit collider; fluid spawns also skip solid cells
    Insert,
    /// Fills every cell, replacing existing matter and ignoring colliders
    Replace,
}
