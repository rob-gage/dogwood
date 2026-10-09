// Copyright Rob Gage 2026

use super::{
    processor_configuration::ProcessorConfiguration,
    processor_error::ProcessorError,
};
use std::{
    collections::{
        HashMap,
        HashSet,
    },
    sync::Arc,
};

/// Named Rayon pools with worker-count limits and no outstanding-job limit
pub struct Processor {
    /// Configured Rayon pools indexed by their unique names
    thread_pools: HashMap<String, Arc<rayon::ThreadPool>>,
}

impl Processor {

    /// Creates the configured weighted CPU and dedicated worker pools
    pub fn new(configuration: ProcessorConfiguration) -> Result<Self, ProcessorError> {
        if configuration.processor_capacity == 0 {
            return Err(ProcessorError::InvalidConfiguration(
                "CPU budget must be greater than zero".to_owned(),
            ));
        }
        if configuration.processor_capacity_reserved > configuration.processor_capacity {
            return Err(ProcessorError::InvalidConfiguration(format!(
                "Reserved CPU capacity ({}) exceeds budget ({})",
                configuration.processor_capacity_reserved,
                configuration.processor_capacity,
            )));
        }
        // validate names and pool-specific limits before creating any worker threads
        let mut pool_names: HashSet<&str> = HashSet::new();
        let mut cpu_pool_weights: Vec<(String, usize)> = Vec::new();
        for (pool_name, _) in configuration
            .pool_weights
            .iter()
            .chain(configuration.dedicated_worker_pool_counts.iter())
        {
            if pool_name.is_empty() {
                return Err(ProcessorError::InvalidConfiguration(
                    "Thread pool names must not be empty".to_owned(),
                ));
            }
            if !pool_names.insert(pool_name) {
                return Err(ProcessorError::InvalidConfiguration(format!(
                    "Thread pool name '{pool_name}' is configured more than once",
                )));
            }
        }
        for (pool_name, relative_weight) in &configuration.pool_weights {
            if *relative_weight == 0 {
                return Err(ProcessorError::InvalidConfiguration(format!(
                    "Thread pool '{pool_name}' must have a positive weight",
                )));
            }
            cpu_pool_weights.push((pool_name.clone(), *relative_weight));
        }
        for (pool_name, worker_count) in &configuration.dedicated_worker_pool_counts {
            if *worker_count == 0 {
                return Err(ProcessorError::InvalidConfiguration(format!(
                    "Dedicated worker pool '{pool_name}' must have at least one worker",
                )));
            }
        }
        // divide only the unreserved CPU capacity among processor pools
        let available_cpu_workers: usize =
            configuration.processor_capacity - configuration.processor_capacity_reserved;
        let cpu_pool_worker_counts: Vec<(String, usize)> =
            Self::allocate_pool_workers(available_cpu_workers, &cpu_pool_weights)?;
        let mut thread_pools: HashMap<String, Arc<rayon::ThreadPool>> = HashMap::new();
        for (pool_name, worker_count) in cpu_pool_worker_counts
            .into_iter()
            .chain(configuration.dedicated_worker_pool_counts)
        {
            let thread_name_prefix: String = format!("dogwood-{pool_name}");
            let thread_pool_builder: rayon::ThreadPoolBuilder = rayon::ThreadPoolBuilder::new()
                .num_threads(worker_count)
                .thread_name(move |thread_index: usize| {
                    format!("{thread_name_prefix}-{thread_index}")
                });
            let thread_pool: rayon::ThreadPool = thread_pool_builder
                .build()
                .map_err(|source: rayon::ThreadPoolBuildError| ProcessorError::RayonThreadPoolBuild {
                    name: pool_name.clone(),
                    source,
                })?;
            thread_pools.insert(pool_name, Arc::new(thread_pool));
        }
        Ok(Self { thread_pools })
    }

    /// Returns a shared handle to the named pool; worker count does not bound queued jobs
    pub fn thread_pool(&self, thread_pool_name: &str) -> Option<Arc<rayon::ThreadPool>> {
        self.thread_pools.get(thread_pool_name).cloned()
    }

    /// Assigns each thread pool at least one worker, then distributes remaining workers by weight
    fn allocate_pool_workers(
        available_workers: usize,
        pool_weights: &[(String, usize)],
    ) -> Result<Vec<(String, usize)>, ProcessorError> {
        if pool_weights.is_empty() { return Ok(Vec::new()); }
        if pool_weights.len() > available_workers {
            return Err(ProcessorError::InvalidConfiguration(format!(
                "{} thread pools cannot fit in {available_workers} available CPU workers",
                pool_weights.len(),
            )));
        }
        // reserve one worker per pool before applying relative weights
        let remaining_worker_count: usize = available_workers - pool_weights.len();
        let mut total_relative_weight: u128 = 0;
        for (_, relative_weight) in pool_weights {
            total_relative_weight = total_relative_weight
                .checked_add(*relative_weight as u128)
                .ok_or_else(|| ProcessorError::InvalidConfiguration(
                    "Thread pool weights exceed the supported range".to_owned(),
                ))?;
        }
        // use largest remainders so integer rounding still consumes the full CPU budget
        let mut allocated_cpu_workers: Vec<(String, usize)> = Vec::new();
        let mut fractional_remainders: Vec<(usize, u128)> = Vec::new();
        let mut rounded_worker_count: usize = 0;
        for (pool_index, (pool_name, relative_weight)) in pool_weights.iter().enumerate() {
            let weighted_worker_count: u128 =
                (remaining_worker_count as u128) * (*relative_weight as u128);
            let additional_worker_count: usize =
                (weighted_worker_count / total_relative_weight) as usize;
            let fractional_remainder: u128 = weighted_worker_count % total_relative_weight;
            let pool_worker_count: usize = 1 + additional_worker_count;
            rounded_worker_count += pool_worker_count;
            allocated_cpu_workers.push((pool_name.clone(), pool_worker_count));
            fractional_remainders.push((pool_index, fractional_remainder));
        }
        fractional_remainders.sort_by(|left: &(usize, u128), right: &(usize, u128)| {
            right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0))
        });
        let unallocated_worker_count: usize = available_workers - rounded_worker_count;
        for (pool_index, _) in fractional_remainders.iter().take(unallocated_worker_count) {
            allocated_cpu_workers[*pool_index].1 += 1;
        }
        Ok(allocated_cpu_workers)
    }

}