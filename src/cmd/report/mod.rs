use crate::cmd::prices::Prices;
use crate::trades;
use std::{error::Error, fs::File, io};
use steel_cent::{currency::GBP, Money};

mod cgt;

pub fn generate_report(file: &str, prices: &str, year: Option<&str>) -> Result<(), Box<dyn Error>> {
    let trades = trades::read_csv(File::open(file)?)?;
    let prices = Prices::read_csv(File::open(prices)?)?;
    let report = cgt::calculate(trades, &prices)?;
    let year = year.map(|y| y.parse::<i32>().expect("valid year"));
    let gains = report.gains(year);

    let estimated_liability = (gains.total_gain() - Money::of_major(GBP, 11_300)) * 0.2;

    log::info!("Disposals {}", gains.len());
    log::info!("Proceeds {}", gains.total_proceeds());
    log::info!("Allowable Costs {}", gains.total_allowable_costs());
    log::info!("Gains {}", gains.total_gain());
    log::info!("Estimated Liability {}", estimated_liability);

    cgt::TaxEvent::write_csv(gains, io::stdout())
}
