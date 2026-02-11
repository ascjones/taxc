mod cmd;
mod core;

use clap::{Parser, Subcommand};
use cmd::pools::PoolsCommand;
use cmd::report::ReportCommand;
use cmd::schema::SchemaCommand;
use cmd::summary::SummaryCommand;

#[derive(Parser, Debug)]
#[command(name = "taxc", version)]
#[command(about = "UK Tax Calculator for Capital Gains and Income", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Show aggregated tax summary
    Summary(SummaryCommand),

    /// Generate tax report (HTML by default, or JSON with --json)
    Report(ReportCommand),

    /// Show pool balances over time
    Pools(PoolsCommand),

    /// Print expected JSON input schema
    Schema(SchemaCommand),
}

impl Command {
    fn exec(&self) -> anyhow::Result<()> {
        match self {
            Command::Summary(summary) => summary.exec(),
            Command::Report(report) => report.exec(),
            Command::Pools(pools) => pools.exec(),
            Command::Schema(schema) => schema.exec(),
        }
    }
}

fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();
    let cli = Cli::parse();
    cli.command.exec()
}
