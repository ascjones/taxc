use std::error::Error;
use crate::trades;
use std::fs::File;
use crate::cmd::prices::Prices;
use steel_cent::{currency::GBP, Money};
use std::io;

mod cgt;

pub fn generate_report(file: &str, prices: &str, year: Option<&str>) -> Result<(), Box<dyn Error>> {
	let trades = trades::read_csv(File::open(file)?)?;
	let prices = Prices::read_csv(File::open(prices)?)?;
	let report = cgt::calculate(trades, &prices)?;
	let year = year.map(|y| y.parse::<i32>().expect("valid year"));
	let mut gains = year
		.and_then(|y| report.years.get(&y).map(|ty| ty.gains.clone()))
		.unwrap_or(
			report
				.years
				.iter()
				.flat_map(|(_, y)| y.gains.clone())
				.collect::<Vec<_>>(),
		);
	gains.sort_by(|g1, g2| g1.trade.date_time.cmp(&g2.trade.date_time));

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

	cgt::Gain::write_csv(&gains, io::stdout())
}
