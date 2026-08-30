use crate::cmd::pools::PoolsCommand;
use crate::cmd::report::ReportCommand;
use crate::cmd::schema::SchemaCommand;
use crate::cmd::summary::SummaryCommand;
use clap::{Parser, Subcommand};

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

pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    cli.command.exec()
}
