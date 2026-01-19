mod cmd;
mod events;
mod tax;

use argh::FromArgs;
use cmd::report::ReportCommand;

#[derive(FromArgs, PartialEq, Debug)]
/// UK Tax Calculator for Capital Gains and Income
struct Taxc {
    #[argh(subcommand)]
    cmd: Command,
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
enum Command {
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
    let taxc: Taxc = argh::from_env();
    taxc.cmd.exec()
}
