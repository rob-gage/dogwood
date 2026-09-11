// Copyright Rob Gage 2026

/// Selects the movement behavior currently active for a pawn
#[derive(Copy, Clone)]
pub enum ActorPawnMovement {
    /// Uses configured walking movement
    Walking,
    /// Uses configured flying movement
    Flying,
    /// Uses configured swimming movement
    Swimming,
    /// Uses configured unrestricted noclip movement
    Noclip,
}
