mod cmd;
mod events;
mod tax;

use clap::{Parser, Subcommand};
use cmd::events::EventsCommand;
use cmd::html_report::HtmlCommand;
use cmd::summary::SummaryCommand;

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

    /// Generate interactive HTML report
    Html(HtmlCommand),
}

impl Command {
    fn exec(&self) -> color_eyre::Result<()> {
        match self {
            Command::Events(events) => events.exec(),
            Command::Summary(summary) => summary.exec(),
            Command::Html(html) => html.exec(),
        }
    }
}

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    pretty_env_logger::init();
    let cli = Cli::parse();
    cli.command.exec()
}
