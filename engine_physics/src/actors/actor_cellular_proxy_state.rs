// Copyright Rob Gage 2026

use super::ActorCollisionShape;

/// CPU-gathered state used to derive one transient GPU actor proxy
pub(crate) struct ActorCellularProxyState {
    pub center: [f32; 2],
    pub velocity: [f32; 2],
    pub drive: [f32; 2],
    pub shape: ActorCollisionShape,
    pub occupancy_kind: u32,
}
