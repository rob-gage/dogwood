// Copyright Rob Gage 2026

use dogwood_editor::EditorGame;
use dogwood_engine::compute::Accelerator;
use dogwood_template_project::TemplateProject;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _tracing_guard = dogwood_engine::diagnostics::initialize();
    let accelerator: Arc<Accelerator> = Arc::new(Accelerator::new()?);
    TemplateProject::new(&accelerator)?.launch_in_editor(accelerator)
}
