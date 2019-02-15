#![recursion_limit="128"]

#[macro_use] extern crate prettytable;

use std::error::Error;
use std::{fs::File, io};

use clap::{Arg, App, SubCommand};
use steel_cent::{currency::{self, GBP}, Money};

use crate::exchanges::uphold;
use crate::exchanges::poloniex;
use crate::exchanges::bittrex;
use crate::exchanges::binance;

use crate::transaction::*;
use crate::journal::Journal;
use crate::prices::*;

mod cgt;
mod exchanges;
mod trades;
mod transaction;
mod journal;
mod balance;
mod etherscan;
mod prices;

fn main() -> Result<(), Box<Error>> {
    // binance::trade_history("ETHBTC");
    // bittrex::get_transactions();

    let matches = App::new("cgt")
        .version("0.1")
        .author("Andrew Jones <ascjones@gmail.com>")
        .about("Calculate UK Capital Gains Tax (CGT)")
        .subcommand(SubCommand::with_name("balance")
            .about("Display balances")
            .arg(Arg::with_name("file")
                .help("transaction csv file")
                .takes_value(true)
                .short("f")
                .long("file")))
        .subcommand(SubCommand::with_name("report")
            .about("Calculate and display Tax Report")
            .arg(Arg::with_name("file")
                .help("transaction csv file")
                .takes_value(true)
                .short("f")
                .long("file"))
            .arg(Arg::with_name("prices")
                .help("prices csv file")
                .takes_value(true)
                .short("p")
                .long("prices"))
            .arg(Arg::with_name("year")
                .help("tax year")
                .takes_value(true)
                .short("y")
                .long("year")))
        .subcommand(SubCommand::with_name("trades")
            .about("Export trades")
            .arg(Arg::with_name("file")
                .help("transaction csv file")
                .takes_value(true)
                .short("f")
                .long("file"))
            .arg(Arg::with_name("prices")
                .help("prices csv file")
                .takes_value(true)
                .short("p")
                .long("prices"))
            .arg(Arg::with_name("year")
                .help("tax year")
                .takes_value(true)
                .short("y")
                .long("year"))
            .arg(Arg::with_name("group-by-day")
                .help("groups trades by day")
                .takes_value(false)
                .short("g")
                .long("group-by-day")))
        .subcommand(SubCommand::with_name("import")
            .about("Import trades")
            .arg(Arg::with_name("file")
                .help("exchange csv file")
                .takes_value(true)
                .short("f")
                .long("file"))
            .arg(Arg::with_name("source")
                .help("source of the csv file")
                .takes_value(true)
                .short("s")
                .long("source"))
            .arg(Arg::with_name("group-by-day")
                .help("groups trades by day")
                .takes_value(false)
                .short("g")
                .long("group-by-day")))
        .subcommand(SubCommand::with_name("merge")
            .about("Merge transactions")
            .arg(Arg::with_name("ours")
                .help("existing csv txs")
                .takes_value(true)
                .long("ours"))
            .arg(Arg::with_name("theirs")
                .help("new csv txs")
                .takes_value(true)
                .long("theirs")))
        .subcommand(SubCommand::with_name("prices")
            .about("Import prices")
            .arg(Arg::with_name("gbp")
                .help("gbp/usd prices")
                .takes_value(true)
                .long("gbp"))
            .arg(Arg::with_name("btc")
                .help("btc/usd prices")
                .takes_value(true)
                .long("btc"))
            .arg(Arg::with_name("eth")
                .help("eth/usd prices")
                .takes_value(true)
                .long("eth")))
        .get_matches();

    match matches.subcommand() {
        ("import", Some(m)) => {
            let file = m.value_of("file").unwrap();
            let source = m.value_of("source").unwrap(); // todo: handle args not present
            let group_by_day = m.is_present("group-by-day");
            import_csv(file, source, group_by_day)
        },
        ("merge", Some(m)) => {
            let ours = m.value_of("ours").expect("ours");
            let theirs = m.value_of("theirs").expect("theirs"); // todo: handle args not present
            merge_csv(ours, theirs)
        },
        ("report", Some(m)) => {
            let file = m.value_of("file").unwrap();
            let prices = m.value_of("prices").expect("expected prices");
            let year = m.value_of("year");
            report(file, prices, year)
        },
        ("trades", Some(m)) => {
            let file = m.value_of("file").unwrap();
            let prices = m.value_of("prices").expect("expected prices");
            let year = m.value_of("year");
            let group_by_day = m.is_present("group-by-day");
            trades(file, prices, year, group_by_day)
        },
        ("balance", Some(m)) => {
            let file = m.value_of("file").unwrap();
            balance(file)
        },
        ("prices", Some(m)) => {
            let btc = m.value_of("btc").expect("btc");
            let eth = m.value_of("eth").expect("eth");
            let gbp = m.value_of("gbp").expect("gbp");

            prices(btc, eth, gbp)
        },
        _ => Ok(())
    }
}

fn import_csv(file: &str, source: &str, group_by_day: bool) -> Result<(), Box<Error>> {
    let csv_file = File::open(file)?;
    let trades =
        match source {
            "uphold" => uphold::import_trades(csv_file),
//            "etherscan" => etherscan::read_csv(csv_file),
            "poloniex" => poloniex::import_trades(csv_file),
            "bittrex" => bittrex::import_trades(csv_file),
            "binance" => binance::import_trades(csv_file),
            x => panic!("Unknown file source {}", x), // yes I know should be an error
        }?;
    let mut trades =
        if group_by_day {
            trades::group_trades_by_day(&trades)
        } else {
            trades
        };

    trades.sort_by(|t1, t2| { t1.date_time.cmp(&t2.date_time)});
    trades::write_csv(trades, io::stdout())
}

fn merge_csv(ours: &str, theirs: &str) -> Result<(), Box<Error>> {
    let mut our_journal = Journal::read_csv(File::open(ours)?)?;
    let their_journal = Journal::read_csv(File::open(theirs)?)?;

    our_journal.merge(&their_journal);
    our_journal.write_csv(io::stdout())
}

fn report(file: &str, prices: &str, year: Option<&str>) -> Result<(), Box<Error>> {
    let trades = trades::read_csv(File::open(file)?)?;
    let prices = Prices::read_csv(File::open(prices)?)?;
    let report = cgt::calculate(trades, &prices)?;
    let year = year.map(|y| y.parse::<i32>().expect("valid year"));
    let mut gains = year
        .and_then(|y| report.years.get(&y).map(|ty| ty.gains.clone()))
        .unwrap_or(report.years.iter().flat_map(|(_, y)| y.gains.clone()).collect::<Vec<_>>());
    gains.sort_by(|g1, g2| g1.trade.date_time.cmp(&g2.trade.date_time));

    let (total_proceeds, total_allowable_costs, total_gains) = gains
        .iter()
        .fold((Money::zero(GBP), Money::zero(GBP),  Money::zero(GBP)),
              |(p, ac, gain), g| {
            (p + g.proceeds(), ac + g.allowable_costs(), gain + g.gain())
        });

    let estimated_liability = (total_gains - Money::of_major(GBP, 11_300)) * 0.2;

    println!("Disposals: {} Proceeds {}, Allowable Costs {}, Gains {}, Estimated Liability {}",
             gains.len(), total_proceeds, total_allowable_costs, total_gains, estimated_liability);

    cgt::Gain::write_csv(&gains, io::stdout())
}

// todo: common with report
fn trades(file: &str, prices: &str, year: Option<&str>, group_by_day: bool) -> Result<(), Box<Error>> {
    let journal = Journal::read_csv(File::open(file)?)?;
    let prices = Prices::read_csv(File::open(prices)?)?;
    let year = year.map(|y| y.parse::<i32>().expect("valid year"));
    let trades = Vec::new(); // trades::trades(currency::GBP, &journal, &prices, year, group_by_day);
    trades::write_csv(trades, io::stdout())?;
    Ok(())
}


fn balance(file: &str) -> Result<(), Box<Error>> {
    let csv_file = File::open(file)?;
    let journal = Journal::read_csv(csv_file)?;
    balance::display_balances(&journal)
}

fn prices(btc: &str, eth: &str, gbp: &str) -> Result<(), Box<Error>> {
    let gbp_usd_file = File::open(gbp)?;
    let btc_usd_file = File::open(btc)?;
    let eth_usd_file = File::open(eth)?;
    let prices = prices::import_prices(gbp_usd_file, btc_usd_file, eth_usd_file)?;
    prices.write_csv(io::stdout())
}