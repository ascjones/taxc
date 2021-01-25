#![recursion_limit = "128"]

mod assets;
mod cmd;
mod trades;

use argh::FromArgs;
use assets::currencies;
use cmd::{
    import::ImportTradesCommand,
    report::ReportCommand,
};

use rusty_money;

type Money<'a> = rusty_money::Money<'a, currencies::Currency>;

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
    Import(ImportTradesCommand),
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
    let cccgt: Cccgt = argh::from_env();

    cccgt.cmd.exec()
}
