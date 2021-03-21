#![recursion_limit = "128"]

mod cmd;
mod money;
mod trades;
mod utils;
mod coingecko;

use argh::FromArgs;
use cmd::{import::ImportCommand, report::ReportCommand};
use money::{currencies, Money};

#[derive(FromArgs, PartialEq, Debug)]
/// Top-level command.
struct Taxc {
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

impl Command {
    fn exec(&self) -> color_eyre::Result<()> {
        match self {
            Command::Import(import) => import.exec(),
            Command::Report(report) => report.exec(),
        }
    }
}

fn main() -> color_eyre::Result<()> {
    pretty_env_logger::init();
    let taxc: Taxc = argh::from_env();

    taxc.cmd.exec()
}
