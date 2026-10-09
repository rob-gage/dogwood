// Copyright Rob Gage 2026

use std::thread;

/// Configuration for named processor execution pools
pub struct ProcessorConfiguration {
    /// Total processor capacity used to divide CPU-bound workers
    pub processor_capacity: usize,
    /// Processor capacity reserved for the main, window, or tick thread
    pub processor_capacity_reserved: usize,
    /// CPU-bound pool names and relative weights
    pub pool_weights: Vec<(String, usize)>,
    /// Dedicated worker pool names and worker counts
    pub dedicated_worker_pool_counts: Vec<(String, usize)>,
}

impl ProcessorConfiguration {

    /// Creates a configuration with an explicit CPU budget and main-thread reservation
    pub const fn new(processor_capacity: usize, processor_capacity_reserved: usize) -> Self {
        Self {
            processor_capacity,
            processor_capacity_reserved,
            pool_weights: Vec::new(),
            dedicated_worker_pool_counts: Vec::new(),
        }
    }

    /// Adds a CPU-bound pool allocated by its relative weight
    pub fn add_cpu_bound_pool(&mut self, name: impl Into<String>, relative_weight: usize) {
        self.pool_weights.push((name.into(), relative_weight));
    }

    /// Adds a dedicated worker pool with an explicit worker count
    pub fn add_dedicated_worker_pool(&mut self, name: impl Into<String>, worker_count: usize) {
        self.dedicated_worker_pool_counts.push((name.into(), worker_count));
    }

}

impl Default for ProcessorConfiguration {

    fn default() -> Self {
        let processor_capacity: usize = thread::available_parallelism()
            .map(|available_parallelism| available_parallelism.get())
            .unwrap_or(1);
        let reserved_cpu_capacity: usize = usize::from(processor_capacity > 1);
        Self::new(processor_capacity, reserved_cpu_capacity)
    }

}
