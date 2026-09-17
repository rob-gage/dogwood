// Copyright Rob Gage 2026

use super::{
    accelerator_timing_readback::AcceleratorTimingReadback,
    accelerator_timing_record::AcceleratorTimingRecord,
};
use std::time::Instant;

/// CPU metadata for the current and pending Accelerator timing samples.
pub(crate) struct AcceleratorTimingState {
    pub(crate) active_slot: Option<usize>,
    pub(crate) capacity_exhausted: bool,
    pub(crate) last_sample: Option<Instant>,
    pub(crate) next_sample: u64,
    pub(crate) query_count: u32,
    pub(crate) readbacks: Vec<AcceleratorTimingReadback>,
    pub(crate) records: Vec<AcceleratorTimingRecord>,
}

impl AcceleratorTimingState {
    pub(crate) fn new(readbacks: Vec<AcceleratorTimingReadback>) -> Self {
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
