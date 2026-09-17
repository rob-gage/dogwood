// Copyright Rob Gage 2026

/// Snapshot of one static cellular material state
pub(crate) struct CellularStaticState {
    pub material: u32,
    pub appearance: u32,
    pub integrity: f32,
    pub amount: f32,
    pub temperature: f32,
}
