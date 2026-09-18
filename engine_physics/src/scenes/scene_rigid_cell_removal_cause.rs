// Copyright Rob Gage 2026

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SceneRigidCellRemovalCause {
    Erase,
    Fracture,
    /// A conversion must use the phase-transition product rather than fracture debris.
    PhaseTransition,
    Chemistry,
    /// Intentional gameplay extraction; never create fracture debris.
    Extraction,
}
