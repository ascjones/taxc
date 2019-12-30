#![recursion_limit = "128"]

use std::{error::Error, fs::File, io};

use clap::{App, Arg, SubCommand};

mod cmd;
mod coins;
mod trades;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();
    let matches = App::new("cgt")
        .version("0.1")
        .author("Andrew Jones <ascjones@gmail.com>")
        .about("Calculate UK Capital Gains Tax (CGT)")
        .subcommand(
            SubCommand::with_name("import")
                .about("Import trades")
                .arg(
                    Arg::with_name("file")
                        .help("exchange csv file")
                        .takes_value(true)
                        .short("f")
                        .long("file"),
                )
                .arg(
                    Arg::with_name("source")
                        .help("source of the csv file")
                        .takes_value(true)
                        .short("s")
                        .long("source"),
                )
                .arg(
                    Arg::with_name("group-by-day")
                        .help("groups trades by day")
                        .takes_value(false)
                        .short("g")
                        .long("group-by-day"),
                ),
        )
        .subcommand(
            SubCommand::with_name("report")
                .about("Calculate and display Tax Report")
                .arg(
                    Arg::with_name("file")
                        .help("transaction csv file")
                        .takes_value(true)
                        .short("f")
                        .long("file"),
                )
                .arg(
                    Arg::with_name("prices")
                        .help("prices csv file")
                        .takes_value(true)
                        .short("p")
                        .long("prices"),
                )
                .arg(
                    Arg::with_name("year")
                        .help("tax year")
                        .takes_value(true)
                        .short("y")
                        .long("year"),
                ),
        )
        .subcommand(
            SubCommand::with_name("prices")
                .about("Import prices")
                .arg(
                    Arg::with_name("gbp")
                        .help("gbp/usd prices")
                        .takes_value(true)
                        .long("gbp"),
                )
                .arg(
                    Arg::with_name("btc")
                        .help("btc/usd prices")
                        .takes_value(true)
                        .long("btc"),
                )
                .arg(
                    Arg::with_name("eth")
                        .help("eth/usd prices")
                        .takes_value(true)
                        .long("eth"),
                ),
        )
        .get_matches();

    match matches.subcommand() {
        ("import", Some(m)) => {
            let file = m.value_of("file").unwrap();
            let source = m.value_of("source").unwrap(); // todo: handle args not present
            let group_by_day = m.is_present("group-by-day");
            cmd::import::import_csv(file, source, group_by_day)
        }
        ("report", Some(m)) => {
            let file = m.value_of("file").unwrap();
            let prices = m.value_of("prices").expect("expected prices");
            let year = m.value_of("year");
            cmd::report::generate_report(file, prices, year)
        }
        ("prices", Some(m)) => {
            let btc = m.value_of("btc").expect("btc");
            let eth = m.value_of("eth").expect("eth");
            let gbp = m.value_of("gbp").expect("gbp");

            prices(btc, eth, gbp)
        }
        _ => Ok(()),
    }
}

fn prices(btc: &str, eth: &str, gbp: &str) -> Result<(), Box<dyn Error>> {
    let gbp_usd_file = File::open(gbp)?;
    let btc_usd_file = File::open(btc)?;
    let eth_usd_file = File::open(eth)?;
    let prices = cmd::prices::import_prices(gbp_usd_file, btc_usd_file, eth_usd_file)?;
    prices.write_csv(io::stdout())
}
