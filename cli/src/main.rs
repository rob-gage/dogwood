// Copyright Rob Gage 2026

use std::path::PathBuf;

#[derive(Debug, clap::Parser)]
#[command(version, about)]
struct Command {
    #[command(subcommand)]
    subcommand: Subcommand,
}

#[derive(Debug, clap::Subcommand)]
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
    },
    /// Runs a project that uses the engine
    Run {
        /// Runs the project in debug mode if set
        #[arg(long)]
        debug: bool,
    },
    /// Runs a project that uses the engine in the engine editor
    Edit {

    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let command: Command = clap::Parser::parse();
    match command.subcommand {
        Subcommand::New { name, path } => {
            let path = path.unwrap_or_else(|| PathBuf::from(name));
            if path.try_exists()? {
                eprintln!("A directory already exists at {}.", path.display());
                return Ok(());
            }
            todo!()
        },
        Subcommand::Build { debug } => {
            todo!()
        },
        Subcommand::Run { debug } => {
            todo!()
        },
        Subcommand::Edit { } => {
            todo!()
        }
    }
}
