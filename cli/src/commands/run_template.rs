// Copyright Rob Gage 2026

use super::run::run;
use crate::template::{
    TemplateCache,
    TemplateParameters,
    TemplateSource,
};
use std::{
    error::Error,
    path::PathBuf,
};

/// Runs the canonical template project from its stable temporary directory
pub fn run_template(
    cargo_path: PathBuf,
    debug: bool,
    verbose: bool,
) -> Result<(), Box<dyn Error>> {
    let source: TemplateSource = TemplateSource::canonical();
    let parameters: TemplateParameters =
        TemplateParameters::new("dogwood_template".to_owned())?;
    let cache: TemplateCache = TemplateCache::new(TemplateCache::default_root()?);
    let project_path: PathBuf = cache.select(&source, &parameters)?;
    run(cargo_path, project_path, debug, verbose)
}
