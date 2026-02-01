mod cmd;
mod events;
mod tax;

use clap::{Parser, Subcommand};
use cmd::events::EventsCommand;
use cmd::html_report::ReportCommand;
use cmd::pools::PoolsCommand;
use cmd::summary::SummaryCommand;
use cmd::validate::ValidateCommand;

#[derive(Parser, Debug)]
#[command(name = "taxc")]
#[command(about = "UK Tax Calculator for Capital Gains and Income", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Show all transactions/events in a detailed table
    #[command(alias = "txs")]
    Events(EventsCommand),

    /// Show aggregated tax summary
    Summary(SummaryCommand),

    /// Generate tax report (HTML by default, or JSON with --json)
    Report(ReportCommand),

    /// Show pool balances over time
    Pools(PoolsCommand),

    /// Validate data quality and surface issues
    Validate(ValidateCommand),
}

impl Command {
    fn exec(&self) -> color_eyre::Result<()> {
        match self {
            Command::Events(events) => events.exec(),
            Command::Summary(summary) => summary.exec(),
            Command::Report(report) => report.exec(),
            Command::Pools(pools) => pools.exec(),
            Command::Validate(validate) => validate.exec(),
        }
    }
}

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    pretty_env_logger::init();
    let cli = Cli::parse();
    cli.command.exec()
}
