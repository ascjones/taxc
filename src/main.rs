#![recursion_limit = "128"]

mod cmd;
mod coins;
mod trades;

use std::{path::PathBuf};
use argh::FromArgs;
use cmd::report::ReportCommand;

#[derive(FromArgs, PartialEq, Debug)]
/// Top-level command.
struct Cccgt {
    #[argh(subcommand)]
    cmd: Command,
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
/// Calculate UK Capital Gains Tax (CGT)
enum Command {
    Import(ImportCommand),
    Report(ReportCommand),
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "import")]
/// Import trades
pub struct ImportCommand {
    #[argh(subcommand)]
    sub: ImportSubcommand
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
pub enum ImportSubcommand {
    Trades(ImportTradesCommand),
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "trades")]
/// Import trades from a csv file
pub struct ImportTradesCommand {
    /// the csv file containing trades to import
    #[argh(positional)]
    file: PathBuf,
    /// the source of the csv file, e.g. which exchange
    #[argh(option)]
    source: String,
    /// combines trades on the same pair on the same day into a single trade
    #[argh(switch, short = 'g')]
    group_by_day: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();
    let cccgt: Cccgt = argh::from_env();

    match cccgt.cmd {
        Command::Import(import) => {
            match import.sub {
                ImportSubcommand::Trades(trades) => {
                    cmd::import::import_csv(trades.file, &trades.source, trades.group_by_day)
                }
            }
        },
        Command::Report(report) => {
           report.exec()
        }
    }
}
