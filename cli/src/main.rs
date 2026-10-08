// Copyright Rob Gage 2026

mod commands;
mod template;
mod utility;

use utility::locate_cargo;
use std::{
    path::PathBuf,
    process::exit
};

#[derive(Debug, clap::Parser)]
#[command(version, about)]
/// Parsed top-level CLI arguments
struct Command {
    #[command(subcommand)]
    /// Selected command and its arguments
    subcommand: Subcommand,
}

#[derive(Debug, clap::Subcommand)]
/// Supported Dogwood CLI operations
enum Subcommand {
    /// Creates a new project that uses the engine
    New {
        /// The name of the project
        #[arg(value_name = "NAME")]
        name: String,
        /// The directory to create the new project in (defaults to `./<name>`)
        #[arg(long, value_name = "PATH")]
        path: Option<PathBuf>,
    },
    /// Builds a project that uses the engine
    Build {
        /// Builds the project in debug mode if set
        #[arg(long)]
        debug: bool,
        /// Prints the commands and build output
        #[arg(long)]
        verbose: bool,
        /// Path to the project to build
        #[arg(long, value_name = "PATH", default_value = ".")]
        path: PathBuf,
        /// Writes Cargo build files to this target directory
        #[arg(short, long, value_name = "PATH")]
        output: Option<PathBuf>,
    },
    /// Runs a project that uses the engine
    Run {
        /// Runs the project in debug mode if set
        #[arg(long)]
        debug: bool,
        /// Prints the commands and build output
        #[arg(long)]
        verbose: bool,
        /// Path to the project to run
        #[arg(long, value_name = "PATH", default_value = ".")]
        path: PathBuf,
    },
    /// Runs a project that uses the engine in the engine editor
    Edit {
        /// Prints the commands and build output
        #[arg(long)]
        verbose: bool,
        /// Path to the project to edit
        #[arg(long, value_name = "PATH", default_value = ".")]
        path: PathBuf,
    },
    /// Runs the canonical Dogwood template project from a temporary directory
    RunTemplate {
        /// Runs the project in debug mode if set
        #[arg(long)]
        debug: bool,
        /// Prints the commands and build output
        #[arg(long)]
        verbose: bool,
    },
    /// Opens the canonical Dogwood template project in the Dogwood editor
    EditTemplate {
        /// Prints the commands and build output
        #[arg(long)]
        verbose: bool,
    },
}

/// Parses arguments and dispatches the selected CLI operation
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let command: Command = clap::Parser::parse();
    match command.subcommand {
        Subcommand::New { name, path } => {
            let destination: PathBuf = path.unwrap_or_else(|| PathBuf::from(&name));
            commands::new(name, destination)
        }
        Subcommand::Build {
            debug,
            verbose,
            path,
            output,
        } => {
            let cargo_path: PathBuf = locate_cargo().unwrap_or_else(|()| exit(1));
            let _build_directory: PathBuf =
                commands::build(cargo_path, path, debug, output, verbose);
            Ok(())
        }
        Subcommand::Run {
            debug,
            verbose,
            path,
        } => {
            let cargo_path: PathBuf = locate_cargo().unwrap_or_else(|()| exit(1));
            commands::run(cargo_path, path, debug, verbose)
        }
        Subcommand::Edit { verbose, path } => {
            let cargo_path: PathBuf = locate_cargo().unwrap_or_else(|()| exit(1));
            commands::edit(cargo_path, path, verbose)
        }
        Subcommand::RunTemplate { debug, verbose } => {
            let cargo_path: PathBuf = locate_cargo().unwrap_or_else(|()| exit(1));
            commands::run_template(cargo_path, debug, verbose)
        }
        Subcommand::EditTemplate { verbose } => {
            let cargo_path: PathBuf = locate_cargo().unwrap_or_else(|()| exit(1));
            commands::edit_template(cargo_path, verbose)
        }
    }
}
