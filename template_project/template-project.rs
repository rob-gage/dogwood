// Copyright Rob Gage 2026

use engine::{Game, compute::Accelerator};
use std::sync::Arc;
use template_project::TemplateProject;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _tracing_guard = engine::diagnostics::initialize();
    let accelerator: Arc<Accelerator> = Arc::new(Accelerator::new()?);
    TemplateProject::new(&accelerator)?.launch(accelerator)
}
