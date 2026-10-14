// Copyright Rob Gage 2026

/// How an `Edit` treats existing matter in its cells
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum EditMode {
    /// Fills only empty cells not overlapped by a collider in the edit collision group
    Insert,
    /// Fills every cell, replacing existing matter and ignoring colliders
    Replace,
}
