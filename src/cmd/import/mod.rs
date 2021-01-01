use crate::trades::Trade;
use argh::FromArgs;
use serde::de::DeserializeOwned;
use std::{
    convert::TryInto,
    fs::File,
    io::{
        self,
        Read,
    },
    path::PathBuf,
};

mod exchanges;

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "import")]
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

impl ImportTradesCommand {
    pub fn exec(&self) -> color_eyre::Result<()> {
        let csv_file = File::open(&self.file)?;
        let trades = match self.source.as_str() {
            "uphold" => Self::csv_to_trades::<exchanges::uphold::Record, _, _>(csv_file),
            "poloniex" => {
                Self::csv_to_trades::<exchanges::poloniex::Record, _, _>(csv_file)
            }
            "bittrex" => {
                Self::csv_to_trades::<exchanges::bittrex::Record, _, _>(csv_file)
            }
            "binance" => {
                Self::csv_to_trades::<exchanges::binance::Record, _, _>(csv_file)
            }
            "coinbase" => {
                Self::csv_to_trades::<exchanges::coinbase::Record, _, _>(csv_file)
            }
            x => panic!("Unknown file source {}", x), // yes I know should be an error
        }?;
        let mut trades = if self.group_by_day {
            crate::trades::group_trades_by_day(&trades)
        } else {
            trades
        };

        trades.sort_by(|t1, t2| t1.date_time.cmp(&t2.date_time));
        crate::trades::write_csv(trades, io::stdout())
    }

    fn csv_to_trades<CsvRecord, R, E>(reader: R) -> color_eyre::Result<Vec<Trade>>
    where
        CsvRecord: Clone + DeserializeOwned + TryInto<Trade, Error = E>,
        R: Read,
        E: std::error::Error + 'static + Send + Sync,
    {
        let mut rdr = csv::Reader::from_reader(reader);
        let result: Result<Vec<CsvRecord>, _> = rdr.deserialize().collect();
        let result = result?;
        log::info!("Read {} csv records", result.len());
        let mut trades = result
            .iter()
            .cloned()
            .map(|record: CsvRecord| TryInto::try_into(record).map_err(Into::into))
            .collect::<color_eyre::Result<Vec<Trade>>>()?;
        trades.sort_by(|tx1, tx2| tx1.date_time.cmp(&tx2.date_time));
        Ok(trades)
    }
}
