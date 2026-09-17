// Copyright Rob Gage 2026

/// One asynchronously completed rigid/granular reaction batch
pub(crate) struct RigidGranularReactionBatch {
    pub(crate) sequence: u64,
    pub(crate) topology_revision: u64,
    pub(crate) body_count: usize,
    /// One-shot equal/opposite transfers; consume every compatible sequence exactly once.
    pub(crate) reactions: Box<[[f32; 3]]>,
    pub(crate) contact_counts: Box<[u32]>,
    pub(crate) static_contact_counts: Box<[u32]>,
    pub(crate) granular_contact_counts: Box<[u32]>,
    pub(crate) moving_contact_counts: Box<[u32]>,
    pub(crate) energy_budgets: Box<[f32]>,
    /// Newest state wins. XYZ is generalized impulse; W is its own energy allowance.
    pub(crate) constraints: Box<[[f32; 4]]>,
    /// Confirmed gravity impulse for each actual fixed step; zero clears support.
    pub(crate) supports: Box<[[f32; 4]]>,
    /// Bounded integration bias, removed from physical velocity after the step.
    pub(crate) recovery: Box<[[f32; 4]]>,
    /// Submitted linear/angular motion and whether the target includes actor kinematics.
    pub(crate) source_motion: Box<[[f32; 4]]>,
    /// Stable rigid-cell slots fractured by pressure in this topology revision.
    pub(crate) fractured_slots: Box<[u32]>,
}
