// Copyright Rob Gage 2026

#[derive(clap::Parser)]
#[command(version, about)]
struct Command {
    #[command(subcommand)]
    subcommand: Option<Subcommand>,
}

#[derive(clap::Subcommand)]
enum Subcommand {}

fn main() {
    let _tracing_guard = engine_diagnostics::initialize();
    let _command: Command = clap::Parser::parse();
    tracing::debug!("parsed command line");
}
