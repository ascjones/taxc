#![recursion_limit = "128"]

mod cmd;
mod assets;
mod trades;

use assets::currencies;
use argh::FromArgs;
use cmd::{
    import::ImportTradesCommand,
    report::ReportCommand,
};

type Money = rusty_money::Money<'static, currencies::Currency>;

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
