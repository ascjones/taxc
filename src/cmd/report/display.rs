use super::{Disposal, Year};
use crate::money::display_amount;

use serde::{Deserialize, Serialize};
use std::io::Write;

pub fn write_csv<'a, D, W>(disposals: D, writer: W) -> color_eyre::Result<()>
where
    D: IntoIterator<Item = Disposal<'a>>,
    W: Write,
{
    let mut wtr = csv::Writer::from_writer(writer);
    for tax_event in disposals.into_iter() {
        let record: DisposalRecord = tax_event.into();
        wtr.serialize(record)?;
    }
    wtr.flush()?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct DisposalRecord {
    date_time: String,
    tax_year: Year,
    exchange: String,
    buy_asset: String,
    buy_amt: String,
    sell_asset: String,
    sell_amt: String,
    price: String,
    rate: String,
    buy_gbp: String,
    sell_gbp: String,
    fee: String,
    allowable_cost: String,
    gain: String,
    buy_pool_total: String,
    buy_pool_cost: String,
    sell_pool_total: String,
    sell_pool_cost: String,
}

impl<'a> From<Disposal<'a>> for DisposalRecord {
    fn from(disposal: Disposal) -> Self {
        DisposalRecord {
            date_time: disposal.trade.date_time.date().to_string(),
            tax_year: disposal.tax_year,
            exchange: disposal.trade.exchange.clone().unwrap_or(String::new()),
            buy_asset: disposal.trade.buy.currency().code.to_string(),
            buy_amt: display_amount(&disposal.trade.buy),
            sell_asset: disposal.trade.sell.currency().code.to_string(),
            sell_amt: display_amount(&disposal.trade.sell),
            price: disposal.price.pair.to_string(),
            rate: disposal.price.rate.to_string(),
            buy_gbp: display_amount(&disposal.buy_value),
            sell_gbp: display_amount(&disposal.sell_value),
            fee: display_amount(disposal.fee()),
            allowable_cost: display_amount(disposal.allowable_costs()),
            gain: display_amount(&disposal.gain()),
            buy_pool_total: disposal
                .buy_pool
                .as_ref()
                .map_or("".to_string(), |p| display_amount(&p.total())),
            buy_pool_cost: disposal
                .buy_pool
                .as_ref()
                .map_or("".to_string(), |p| format!("{:.2}", &p.cost_basis())),
            sell_pool_total: disposal
                .sell_pool
                .as_ref()
                .map_or("".to_string(), |p| display_amount(&p.total())),
            sell_pool_cost: disposal
                .sell_pool
                .as_ref()
                .map_or("".to_string(), |p| format!("{:.2}", &p.cost_basis())),
        }
    }
}
