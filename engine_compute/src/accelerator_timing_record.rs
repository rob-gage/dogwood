// Copyright Rob Gage 2026

/// One pass recorded in a Accelerator timing sample.
pub(crate) struct AcceleratorTimingRecord {
    pub(crate) kind: &'static str,
    pub(crate) label: String,
    pub(crate) occurrence: u32,
}
