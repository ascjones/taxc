mod cmd;
mod events;
mod tax;

use clap::{Parser, Subcommand};
use cmd::report::ReportCommand;

#[derive(Parser, Debug)]
#[command(name = "taxc")]
#[command(about = "UK Tax Calculator for Capital Gains and Income", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Calculate UK taxes from taxable events CSV
    Report(ReportCommand),
}

impl Command {
    fn exec(&self) -> color_eyre::Result<()> {
        match self {
            Command::Report(report) => report.exec(),
        }
    }
}

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    pretty_env_logger::init();
    let cli = Cli::parse();
    cli.command.exec()
}
