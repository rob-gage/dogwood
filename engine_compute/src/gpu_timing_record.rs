// Copyright Rob Gage 2026

/// One pass recorded in a GPU timing sample.
pub(crate) struct GpuTimingRecord {
    pub(crate) kind: &'static str,
    pub(crate) label: String,
    pub(crate) occurrence: u32,
}
