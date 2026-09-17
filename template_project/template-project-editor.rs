// Copyright Rob Gage 2026

use dogwood_editor::EditorGame;
use dogwood_engine::compute::Accelerator;
use std::sync::Arc;
use dogwood_template_project::TemplateProject;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _tracing_guard = dogwood_engine::diagnostics::initialize();
    let accelerator: Arc<Accelerator> = Arc::new(Accelerator::new()?);
    TemplateProject::new(&accelerator)?.launch_in_editor(accelerator)
}
