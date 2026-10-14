// Copyright Rob Gage 2026

/// Motion of the rigid body carrying a set of colliders
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CollisionBody {
    /// Never moves, while other bodies collide against it
    Fixed,
    /// Moves only where its owner's position is set, pushing dynamic bodies aside
    Kinematic,
    /// Moves under simulated forces and collisions
    Dynamic,
}
