// Copyright Rob Gage 2026

use super::build;
use std::{
    error::Error,
    io::{
        self,
        Write
    },
    path::PathBuf,
    process::{
        Command,
        exit,
        ExitStatus,
    },
};

/// Implementation of the `dogwood run` subcommand
pub fn run(
    cargo_path: PathBuf,
    path: PathBuf,
    debug: bool,
    verbose: bool,
) -> Result<(), Box<dyn Error>> {
    let build_directory: PathBuf = build(
        cargo_path.clone(),
        path.clone(),
        debug,
        None,
        verbose,
    );
    let mut command: Command = Command::new(cargo_path);
    command.arg("run").arg("--quiet").current_dir(path);
    if !debug { command.arg("--release"); }
    command.arg("--target-dir").arg(build_directory);
    if verbose {
        println!("Running command: {command:?}");
        let _ = io::stdout().flush();
    }
    let status: ExitStatus = command.status()?;
    if !status.success() { exit(status.code().unwrap_or(1)); }
    Ok(())
}
