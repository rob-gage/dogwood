// Copyright Rob Gage 2026

use super::{gpu_timing_readback::GpuTimingReadback, gpu_timing_record::GpuTimingRecord};
use std::time::Instant;

/// CPU metadata for the current and pending GPU timing samples.
pub(crate) struct GpuTimingState {
    pub(crate) active_slot: Option<usize>,
    pub(crate) capacity_exhausted: bool,
    pub(crate) last_sample: Option<Instant>,
    pub(crate) next_sample: u64,
    pub(crate) query_count: u32,
    pub(crate) readbacks: Vec<GpuTimingReadback>,
    pub(crate) records: Vec<GpuTimingRecord>,
}

impl GpuTimingState {
    pub(crate) fn new(readbacks: Vec<GpuTimingReadback>) -> Self {
        Self {
            active_slot: None,
            capacity_exhausted: false,
            last_sample: None,
            next_sample: 0,
            query_count: 0,
            readbacks,
            records: Vec::new(),
        }
    }
}
