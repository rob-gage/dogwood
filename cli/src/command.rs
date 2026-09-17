// Copyright Rob Gage 2026

use crate::subcommand::Subcommand;

#[derive(clap::Parser)]
#[command(version, about)]
pub(super) struct Command {
    #[command(subcommand)]
    pub(super) subcommand: Subcommand,
}
