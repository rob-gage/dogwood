// Copyright Rob Gage 2026

//! Command-line project initialization and development tools for Dogwood.

mod command;
mod subcommand;

use std::{
    error::Error,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

fn main() -> Result<(), Box<dyn Error>> {
    use command::Command;
    use subcommand::Subcommand::*;
    let command: Command = clap::Parser::parse();
    match command.subcommand {
        New { directory, name } => {
            let project_name: String = match name {
                Some(name) => name,
                None => {
                    print!("Project name: ");
                    io::stdout().flush()?;
                    let mut project_name: String = String::new();
                    io::stdin().read_line(&mut project_name)?;
                    project_name.trim().to_owned()
                }
            };
            if project_name.is_empty()
                || !project_name.chars().all(|character| {
                    character.is_ascii_lowercase()
                        || character.is_ascii_digit()
                        || character == '_'
                        || character == '-'
                })
                || project_name.starts_with(|character: char| character.is_ascii_digit())
            {
                return Err("project name must be a non-empty Cargo-compatible name".into());
            }

            let project_directory: PathBuf = if directory == Path::new(".") {
                PathBuf::from(".")
            } else {
                directory
            };
            fs::create_dir_all(&project_directory)?;
            let manifest_path: PathBuf = project_directory.join("Cargo.toml");
            if manifest_path.exists() {
                return Err(format!("{} already exists", manifest_path.display()).into());
            }

            let repository_root: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .ok_or("CLI repository root is unavailable")?
                .to_path_buf();
            let engine_path: PathBuf = repository_root.join("engine");
            let template_project_path: PathBuf = repository_root.join("template_project");
            let manifest: String = format!(
                "# Copyright Rob Gage 2026\n\n[package]\nname = \"{project_name}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[dependencies]\ndogwood_engine = {{ path = \"{}\" }}\ndogwood_template_project = {{ path = \"{}\" }}\n",
                engine_path.display(),
                template_project_path.display(),
            );
            fs::write(manifest_path, manifest)?;
            let source_directory: PathBuf = project_directory.join("src");
            fs::create_dir_all(&source_directory)?;
            fs::write(
                source_directory.join("main.rs"),
                "use dogwood_engine::{Game, compute::Accelerator};\nuse std::sync::Arc;\nuse dogwood_template_project::TemplateProject;\n\nfn main() -> Result<(), Box<dyn std::error::Error>> {\n    let _tracing_guard = dogwood_engine::diagnostics::initialize();\n    let accelerator: Arc<Accelerator> = Arc::new(Accelerator::new()?);\n    TemplateProject::new(&accelerator)?.launch(accelerator)\n}\n",
            )?;
            println!(
                "Created Dogwood project `{project_name}` in {}",
                project_directory.display()
            );
        }
    }
    Ok(())
}
