// Copyright Rob Gage 2026

use engine::{
    Accelerator,
    Processor,
    ProcessorConfiguration,
    games::Game,
    runtime::Engine,
};

struct MyGame;

impl Game for MyGame {}

fn main() -> Result<(), engine::runtime::EngineError> {
    let accelerator: Accelerator = Accelerator::new()?;
    let total_cpu_budget: usize = std::thread::available_parallelism()
        .map(|available_parallelism| available_parallelism.get())
        .unwrap_or(1);
    let reserved_cpu_capacity: usize = usize::from(total_cpu_budget > 1);
    let available_cpu_workers: usize = total_cpu_budget - reserved_cpu_capacity;
    let mut processor_configuration: ProcessorConfiguration =
        ProcessorConfiguration::new(total_cpu_budget, reserved_cpu_capacity);
    if available_cpu_workers >= 2 {
        processor_configuration.add_cpu_bound_pool("simulation", 3);
        processor_configuration.add_cpu_bound_pool("world_generation", 1);
    } else if available_cpu_workers == 1 {
        processor_configuration.add_cpu_bound_pool("simulation", 1);
    }
    processor_configuration.add_dedicated_worker_pool("chunk_io", 2);
    let processor: Processor = Processor::new(processor_configuration)?;
    Engine::new(accelerator, processor).launch(MyGame)
}
