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
    let mut gains = year
        .and_then(|y| report.years.get(&y).map(|ty| ty.events.clone()))
        .unwrap_or(
            report
                .years
                .iter()
                .flat_map(|(_, y)| y.events.clone())
                .collect::<Vec<_>>(),
        );
    gains.sort_by(|g1, g2| g1.date_time().cmp(&g2.date_time()));

    let (total_proceeds, total_allowable_costs, total_gains) = gains.iter().fold(
        (Money::zero(GBP), Money::zero(GBP), Money::zero(GBP)),
        |(p, ac, gain), g| (p + g.proceeds(), ac + g.allowable_costs(), gain + g.gain()),
    );

    let estimated_liability = (total_gains - Money::of_major(GBP, 11_300)) * 0.2;

    log::info!("Disposals {}", gains.len());
    log::info!("Proceeds {}", total_proceeds);
    log::info!("Allowable Costs {}", total_allowable_costs);
    log::info!("Gains {}", total_gains);
    log::info!("Estimated Liability {}", estimated_liability);

    cgt::TaxEvent::write_csv(&gains, io::stdout())
}
